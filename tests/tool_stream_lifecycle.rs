use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
    routing::post,
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tower::ServiceExt;
use zakoai_proxy::{
    Shared, app, new_app,
    store::{self, Account, ApiKey, Group},
};

const KEY: &str = "tool-lifecycle-proxy-key";
const MODEL: &str = "tool-model";
const WAIT: Duration = Duration::from_secs(3);
type Captured = Arc<Mutex<Vec<Value>>>;

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn serve(router: Router) -> (String, Server) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (endpoint, Server(task))
}

fn fixture(endpoint: &str) -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    let secret = state.store.seal("tool-upstream-secret").unwrap();
    state
        .store
        .update(|config| {
            config.groups.push(Group {
                id: "tools".into(),
                ..Default::default()
            });
            config.keys.push(ApiKey {
                id: "tools-key".into(),
                group_id: "tools".into(),
                hash: store::digest(KEY),
                enabled: true,
                ..Default::default()
            });
            config.accounts.push(Account {
                id: "upstream".into(),
                name: "Tool upstream".into(),
                endpoint: endpoint.into(),
                api_key: secret,
                group_ids: vec!["tools".into()],
                models: vec![MODEL.into()],
                protocols: vec!["responses".into(), "chat".into(), "anthropic".into()],
                enabled: true,
                ..Default::default()
            });
            config.settings.failure_threshold = 2;
            config.settings.cooldown_seconds = 60;
            config.settings.request_timeout_seconds = 10;
            Ok(())
        })
        .unwrap();
    (dir, state)
}

async fn request(state: &Shared, path: &str, body: &Value) -> Response {
    tokio::time::timeout(
        WAIT,
        app(state.clone()).oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {KEY}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        ),
    )
    .await
    .expect("proxy response headers timed out")
    .unwrap()
}

async fn read(response: Response) -> Result<Bytes, axum::Error> {
    tokio::time::timeout(WAIT, to_bytes(response.into_body(), 1024 * 1024))
        .await
        .expect("proxy response body timed out")
}

async fn error_json(response: Response) -> Value {
    serde_json::from_slice(&read(response).await.unwrap()).unwrap()
}

fn assert_error_code(body: &Value) {
    assert!(
        body["error"]["code"]
            .as_str()
            .is_some_and(|code| !code.is_empty()),
        "local admission errors need a machine-readable code: {body}"
    );
}

fn retry_seconds(response: &Response) -> i64 {
    response
        .headers()
        .get("retry-after")
        .expect("availability errors need Retry-After")
        .to_str()
        .unwrap()
        .parse()
        .expect("Retry-After should contain delta seconds")
}

#[derive(Clone, Copy, Debug)]
enum Protocol {
    Responses,
    Chat,
    Anthropic,
}

impl Protocol {
    fn path(self) -> &'static str {
        match self {
            Self::Responses => "/v1/responses",
            Self::Chat => "/v1/chat/completions",
            Self::Anthropic => "/v1/messages",
        }
    }

    fn body(self, continuation: bool) -> Value {
        let schema = json!({"type":"object","properties":{"q":{"type":"string"}}});
        match self {
            Self::Responses => json!({
                "model":MODEL, "stream":true,
                "tools":[{"type":"function","name":"lookup","parameters":schema}],
                "tool_choice":"auto", "parallel_tool_calls":false,
                "input":if continuation {
                    json!([
                        {"role":"user","content":"lookup"},
                        {"type":"function_call","call_id":"call-1","name":"lookup","arguments":"{\"q\":\"a\"}"},
                        {"type":"function_call_output","call_id":"call-1","output":"result"}
                    ])
                } else {
                    json!([{"role":"user","content":"lookup"}])
                }
            }),
            Self::Chat => json!({
                "model":MODEL, "stream":true,
                "tools":[{"type":"function","function":{"name":"lookup","parameters":schema}}],
                "tool_choice":"auto", "parallel_tool_calls":false,
                "messages":if continuation {
                    json!([
                        {"role":"user","content":"lookup"},
                        {"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"a\"}"}}]},
                        {"role":"tool","tool_call_id":"call-1","content":"result"}
                    ])
                } else {
                    json!([{"role":"user","content":"lookup"}])
                }
            }),
            Self::Anthropic => json!({
                "model":MODEL, "stream":true, "max_tokens":128,
                "tools":[{"name":"lookup","input_schema":schema}],
                "tool_choice":{"type":"auto"},
                "messages":if continuation {
                    json!([
                        {"role":"user","content":"lookup"},
                        {"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"lookup","input":{"q":"a"}}]},
                        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"result"}]}
                    ])
                } else {
                    json!([{"role":"user","content":"lookup"}])
                }
            }),
        }
    }

    fn terminal(self) -> &'static str {
        match self {
            Self::Responses => {
                "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"function_call\",\"call_id\":\"call-1\",\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":\\\"a\\\"}\"}]}}\n\n"
            }
            Self::Chat => {
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":\\\"a\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n"
            }
            Self::Anthropic => {
                "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call-1\",\"name\":\"lookup\",\"input\":{}}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":\\\"a\\\"}\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":7}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
            }
        }
    }

    fn partial(self) -> &'static str {
        match self {
            Self::Responses => "data: {\"type\":\"response.created\"}\n\n",
            Self::Chat => {
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\"}}]},\"finish_reason\":null}]}\n\n"
            }
            Self::Anthropic => {
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n"
            }
        }
    }
}

async fn sse_server(path: &str, text: &'static str, keep_open: bool) -> (String, Server, Captured) {
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));
    let received = captured.clone();
    let router = Router::new().route(
        path,
        post(move |Json(body): Json<Value>| {
            received.lock().unwrap().push(body);
            async move {
                let output = async_stream::stream! {
                    yield Ok::<Bytes, std::io::Error>(Bytes::from_static(text.as_bytes()));
                    if keep_open {
                        std::future::pending::<()>().await;
                    }
                };
                (
                    [("content-type", "text/event-stream")],
                    Body::from_stream(output),
                )
            }
        }),
    );
    let (endpoint, server) = serve(router).await;
    (endpoint, server, captured)
}

#[tokio::test]
async fn complete_chat_tool_calls_without_done_do_not_cool_down_the_account() {
    const SSE: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":\\\"a\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7}}\n\n";
    let (endpoint, _server, captured) = sse_server(Protocol::Chat.path(), SSE, false).await;
    let (_dir, state) = fixture(&endpoint);
    let mut expected = Vec::new();
    for round in 0..3 {
        let body = Protocol::Chat.body(round > 0);
        expected.push(body.clone());
        let response = request(&state, Protocol::Chat.path(), &body).await;
        assert_eq!(response.status(), StatusCode::OK, "tool round {round}");
        assert_eq!(read(response).await.unwrap().as_ref(), SSE.as_bytes());
    }
    assert_eq!(*captured.lock().unwrap(), expected);
    let runtime = state.runtime.lock().unwrap();
    assert_eq!(runtime["upstream"].failures, 0);
    assert_eq!(runtime["upstream"].cooldown_until, 0);
    assert_eq!(runtime["upstream"].active, 0);
    drop(runtime);
    let logs = state.store.logs(0, i64::MAX).unwrap();
    assert_eq!(logs.len(), 3);
    assert!(logs.iter().all(|log| log.status == 200));
}

#[tokio::test]
async fn chat_multiple_choices_require_every_requested_choice_to_finish() {
    for (text, succeeds) in [
        (
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            false,
        ),
        (
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"},{\"index\":1,\"delta\":{\"content\":\"unfinished\"},\"finish_reason\":null}]}\n\n",
            false,
        ),
        (
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: {\"choices\":[{\"index\":1,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            true,
        ),
    ] {
        let (endpoint, _server, _captured) = sse_server(Protocol::Chat.path(), text, false).await;
        let (_dir, state) = fixture(&endpoint);
        let mut body = Protocol::Chat.body(false);
        body["n"] = json!(2);
        let response = request(&state, Protocol::Chat.path(), &body).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(read(response).await.is_ok(), succeeds, "{text}");
        let logs = state.store.logs(0, i64::MAX).unwrap();
        assert_eq!(logs[0].status, if succeeds { 200 } else { 502 });
        assert_eq!(
            state.runtime.lock().unwrap()["upstream"].failures,
            u32::from(!succeeds)
        );
    }
}

#[tokio::test]
async fn terminal_tool_stream_releases_concurrency_before_upstream_eof() {
    for protocol in [Protocol::Responses, Protocol::Chat, Protocol::Anthropic] {
        let terminal = protocol.terminal();
        let (endpoint, _server, captured) = sse_server(protocol.path(), terminal, true).await;
        let (_dir, state) = fixture(&endpoint);
        state
            .store
            .update(|config| {
                config.accounts[0].concurrency = 1;
                Ok(())
            })
            .unwrap();
        let first_body = protocol.body(false);
        let first = request(&state, protocol.path(), &first_body).await;
        assert_eq!(first.status(), StatusCode::OK, "{protocol:?}");
        let mut first_stream = first.into_body().into_data_stream();
        let first_frame = tokio::time::timeout(WAIT, async {
            let mut bytes = Vec::new();
            while bytes.len() < terminal.len() {
                bytes.extend_from_slice(
                    &first_stream
                        .next()
                        .await
                        .expect("missing terminal event")
                        .unwrap(),
                );
            }
            bytes
        })
        .await
        .expect("terminal event was buffered until upstream EOF");
        assert_eq!(
            first_frame.as_slice(),
            terminal.as_bytes(),
            "{protocol:?}: terminal event must be delivered while upstream remains open"
        );
        assert_eq!(
            state.runtime.lock().unwrap()["upstream"].active,
            0,
            "{protocol:?}: completed operation should release its admission"
        );
        let next_body = protocol.body(true);
        let next = request(&state, protocol.path(), &next_body).await;
        assert_eq!(
            next.status(),
            StatusCode::OK,
            "{protocol:?}: tool continuation should not be blocked by completed response"
        );
        assert_eq!(*captured.lock().unwrap(), vec![first_body, next_body]);
        drop(next);
        drop(first_stream);
    }
}

#[tokio::test]
async fn unfinished_tool_stream_keeps_concurrency_reserved() {
    for protocol in [Protocol::Responses, Protocol::Chat, Protocol::Anthropic] {
        let (endpoint, _server, captured) =
            sse_server(protocol.path(), protocol.partial(), true).await;
        let (_dir, state) = fixture(&endpoint);
        state
            .store
            .update(|config| {
                config.accounts[0].concurrency = 1;
                Ok(())
            })
            .unwrap();
        let first = request(&state, protocol.path(), &protocol.body(false)).await;
        assert_eq!(first.status(), StatusCode::OK);
        let mut first_stream = first.into_body().into_data_stream();
        assert!(
            tokio::time::timeout(WAIT, first_stream.next())
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
        assert_eq!(state.runtime.lock().unwrap()["upstream"].active, 1);
        let blocked = request(&state, protocol.path(), &protocol.body(true)).await;
        assert_eq!(
            blocked.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "{protocol:?}"
        );
        assert!(retry_seconds(&blocked) >= 1);
        assert_error_code(&error_json(blocked).await);
        assert_eq!(captured.lock().unwrap().len(), 1);
        drop(first_stream);
        assert_eq!(state.runtime.lock().unwrap()["upstream"].active, 0);
    }
}

#[tokio::test]
async fn normal_response_length_stop_does_not_cool_down_account() {
    const SSE: &str = "event: response.incomplete\ndata: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"},\"usage\":{\"input_tokens\":11,\"output_tokens\":128}}}\n\n";
    let (endpoint, _server, captured) = sse_server(Protocol::Responses.path(), SSE, false).await;
    let (_dir, state) = fixture(&endpoint);
    for round in 0..3 {
        let body = Protocol::Responses.body(round > 0);
        let response = request(&state, Protocol::Responses.path(), &body).await;
        assert_eq!(response.status(), StatusCode::OK, "round {round}");
        assert_eq!(read(response).await.unwrap().as_ref(), SSE.as_bytes());
    }
    assert_eq!(captured.lock().unwrap().len(), 3);
    let runtime = state.runtime.lock().unwrap();
    assert_eq!(runtime["upstream"].failures, 0);
    assert_eq!(runtime["upstream"].cooldown_until, 0);
    assert_eq!(runtime["upstream"].active, 0);
    drop(runtime);
    assert!(
        state
            .store
            .logs(0, i64::MAX)
            .unwrap()
            .iter()
            .all(|log| log.status == 200)
    );
}

#[tokio::test]
async fn real_failed_response_still_cools_down_and_is_not_reported_as_rate_limit() {
    const SSE: &str = "event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"server_error\",\"message\":\"failure\"}}}\n\n";
    let (endpoint, _server, captured) = sse_server(Protocol::Responses.path(), SSE, false).await;
    let (_dir, state) = fixture(&endpoint);
    for _ in 0..2 {
        let response = request(
            &state,
            Protocol::Responses.path(),
            &Protocol::Responses.body(false),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(read(response).await.unwrap().as_ref(), SSE.as_bytes());
    }
    let response = request(
        &state,
        Protocol::Responses.path(),
        &Protocol::Responses.body(true),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!((1..=60).contains(&retry_seconds(&response)));
    assert_error_code(&error_json(response).await);
    assert_eq!(captured.lock().unwrap().len(), 2);
    let runtime = state.runtime.lock().unwrap();
    assert_eq!(runtime["upstream"].failures, 2);
    assert!(runtime["upstream"].cooldown_until > store::now());
    assert_eq!(runtime["upstream"].active, 0);
}

#[tokio::test]
async fn cooldown_reports_503_and_actual_remaining_retry_delay() {
    let (_dir, state) = fixture("http://127.0.0.1:1");
    let until = store::now() + 45;
    state
        .runtime
        .lock()
        .unwrap()
        .entry("upstream".into())
        .or_default()
        .cooldown_until = until;
    let response = request(&state, Protocol::Chat.path(), &Protocol::Chat.body(true)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let remaining = retry_seconds(&response);
    assert!(
        remaining >= until - store::now() && remaining <= 45,
        "retry delay {remaining} does not match cooldown until {until}"
    );
    assert_error_code(&error_json(response).await);
}

#[tokio::test]
async fn local_rpm_limit_reports_429_with_machine_readable_reason() {
    let (endpoint, _server, captured) =
        sse_server(Protocol::Chat.path(), Protocol::Chat.terminal(), false).await;
    let (_dir, state) = fixture(&endpoint);
    state
        .store
        .update(|config| {
            config.accounts[0].rpm = 1;
            Ok(())
        })
        .unwrap();
    let response = request(&state, Protocol::Chat.path(), &Protocol::Chat.body(false)).await;
    assert_eq!(response.status(), StatusCode::OK);
    read(response).await.unwrap();
    // Keep the bounded request away from the real wall-clock minute rollover.
    let until_boundary = 60 - store::now().rem_euclid(60);
    if until_boundary <= 3 {
        tokio::time::sleep(Duration::from_secs((until_boundary + 1) as u64)).await;
    }
    {
        let mut runtime = state.runtime.lock().unwrap();
        let account = runtime.get_mut("upstream").unwrap();
        account.window = store::now() / 60;
        account.requests = 1;
    }
    let blocked = request(&state, Protocol::Chat.path(), &Protocol::Chat.body(true)).await;
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!((1..=60).contains(&retry_seconds(&blocked)));
    assert_error_code(&error_json(blocked).await);
    assert_eq!(captured.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn actual_upstream_429_status_body_and_retry_after_are_preserved() {
    const UPSTREAM_ERROR: &str =
        "{\"error\":{\"code\":\"upstream_quota\",\"message\":\"quota exhausted\"}}";
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));
    let received = captured.clone();
    let (endpoint, _server) = serve(Router::new().route(
        Protocol::Chat.path(),
        post(move |Json(body): Json<Value>| {
            received.lock().unwrap().push(body);
            async {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("content-type", "application/json"), ("retry-after", "17")],
                    UPSTREAM_ERROR,
                )
            }
        }),
    ))
    .await;
    let (_dir, state) = fixture(&endpoint);
    let body = Protocol::Chat.body(true);
    let response = request(&state, Protocol::Chat.path(), &body).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(retry_seconds(&response), 17);
    assert_eq!(
        read(response).await.unwrap().as_ref(),
        UPSTREAM_ERROR.as_bytes()
    );
    assert_eq!(*captured.lock().unwrap(), vec![body]);
    assert_eq!(state.runtime.lock().unwrap()["upstream"].failures, 1);
}

#[tokio::test]
async fn explicit_error_after_chat_finish_is_counted_once() {
    const TERMINAL: &str =
        "data: {\"choices\":[{\"index\":0,\"finish_reason\":\"tool_calls\"}]}\n\n";
    const ERROR: &str = "event: error\ndata: {\"error\":{\"code\":\"server_error\"}}\n\n";
    let router = Router::new().route(
        Protocol::Chat.path(),
        post(|| async {
            let output = async_stream::stream! {
                yield Ok::<Bytes, std::io::Error>(Bytes::from_static(TERMINAL.as_bytes()));
                tokio::time::sleep(Duration::from_millis(20)).await;
                yield Ok(Bytes::from_static(ERROR.as_bytes()));
                yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));
            };
            (
                [("content-type", "text/event-stream")],
                Body::from_stream(output),
            )
        }),
    );
    let (endpoint, _server) = serve(router).await;
    let (_dir, state) = fixture(&endpoint);
    let response = request(&state, Protocol::Chat.path(), &Protocol::Chat.body(false)).await;
    assert_eq!(response.status(), StatusCode::OK);
    read(response).await.unwrap();
    assert_eq!(state.runtime.lock().unwrap()["upstream"].failures, 1);
    assert_eq!(state.store.logs(0, i64::MAX).unwrap()[0].status, 502);
}

#[tokio::test]
async fn malformed_incomplete_releases_slot_while_upstream_stays_open() {
    let (endpoint, _server, _captured) = sse_server(
        Protocol::Responses.path(),
        "event: response.incomplete\ndata: invalid-json\n\n",
        true,
    )
    .await;
    let (_dir, state) = fixture(&endpoint);
    state
        .store
        .update(|config| {
            config.accounts[0].concurrency = 1;
            Ok(())
        })
        .unwrap();
    let first = request(
        &state,
        Protocol::Responses.path(),
        &Protocol::Responses.body(false),
    )
    .await;
    let mut stream = first.into_body().into_data_stream();
    assert!(
        tokio::time::timeout(WAIT, stream.next())
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    assert_eq!(state.runtime.lock().unwrap()["upstream"].active, 0);
    assert_eq!(state.runtime.lock().unwrap()["upstream"].failures, 1);
    let next = request(
        &state,
        Protocol::Responses.path(),
        &Protocol::Responses.body(true),
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);
}

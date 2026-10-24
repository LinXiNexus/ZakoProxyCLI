use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
    routing::{get, post},
};
use bytes::Bytes;
use futures_util::stream;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;
use lxshai_proxy::{
    Shared, app, new_app, profiles,
    store::{self, Account, ApiKey, Group},
};

const PROXY_KEY: &str = "sk-lxsh-chat-integration";
const UPSTREAM_KEY: &str = "chat-upstream-secret";

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    state.store.session("chat-console-session").unwrap();
    state
        .store
        .update(|config| {
            config.groups.push(Group {
                id: "chat-group".into(),
                name: "Chat".into(),
                profile_ids: Vec::new(),
                default_strategy_enabled: false,
            });
            config.keys.push(ApiKey {
                id: "chat-key".into(),
                name: "Chat".into(),
                group_id: "chat-group".into(),
                hash: store::digest(PROXY_KEY),
                enabled: true,
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
    (dir, state)
}

fn add_account(
    state: &Shared,
    endpoint: &str,
    id: &str,
    model: &str,
    requested: Option<&str>,
    priority: i32,
) {
    let mut model_mappings = BTreeMap::new();
    if let Some(requested) = requested {
        model_mappings.insert(requested.into(), model.into());
    }
    let secret = state.store.seal(UPSTREAM_KEY).unwrap();
    state
        .store
        .update(|config| {
            config.accounts.push(Account {
                id: id.into(),
                name: id.into(),
                endpoint: endpoint.into(),
                api_key: secret,
                models: vec![model.into()],
                model_mappings,
                group_ids: vec!["chat-group".into()],
                protocols: vec!["chat".into()],
                enabled: true,
                priority,
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
}

fn select_profiles(state: &Shared, ids: Vec<String>) {
    state
        .store
        .update(|config| {
            config.groups[0].profile_ids = ids;
            Ok(())
        })
        .unwrap();
}

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

async fn request(
    state: &Shared,
    path: &str,
    body: Value,
    key: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(key) = key {
        builder = builder.header("authorization", format!("Bearer {key}"));
    }
    app(state.clone())
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn chat(state: &Shared, body: Value) -> axum::response::Response {
    request(state, "/v1/chat/completions", body, Some(PROXY_KEY)).await
}

async fn admin_request(
    state: &Shared,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("cookie", "lxsh_session=chat-console-session")
                .header("x-console-path", "/")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn admin(state: &Shared, body: Value) -> (StatusCode, Value) {
    admin_request(state, "POST", "/api/models/discover", body).await
}

async fn read_body(response: axum::response::Response) -> (StatusCode, Bytes) {
    let status = response.status();
    let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, body)
}

fn logs(state: &Shared) -> Vec<store::RequestLog> {
    state.store.logs(0, i64::MAX).unwrap()
}

#[tokio::test]
async fn chat_completions_requires_proxy_authentication() {
    let (_dir, state) = fixture();
    let response = request(
        &state,
        "/v1/chat/completions",
        json!({"model":"model-a", "messages":[]}),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn chat_completions_passes_messages_tools_images_and_options_through() {
    let (_dir, state) = fixture();
    let captured = Arc::new(Mutex::new(None));
    let upstream_capture = captured.clone();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let capture = upstream_capture.clone();
            async move {
                assert_eq!(headers["authorization"], format!("Bearer {UPSTREAM_KEY}"));
                *capture.lock().unwrap() = Some(body.clone());
                Json(body)
            }
        }),
    );
    let (endpoint, _server) = serve(router).await;
    add_account(&state, &endpoint, "echo", "model-a", None, 0);
    let original = json!({
        "model":"model-a",
        "messages":[
            {"role":"system", "content":"Client system"},
            {"role":"user", "content":[{"type":"text", "text":"What is in this picture?"}, {"type":"image_url", "image_url":{"url":"data:image/png;base64,AA=="}}]},
            {"role":"assistant", "tool_calls":[{"id":"call-1", "type":"function", "function":{"name":"lookup", "arguments":"{}"}}]},
            {"role":"tool", "tool_call_id":"call-1", "content":"previous result"}
        ],
        "tools":[{"type":"function", "function":{"name":"lookup", "parameters":{"type":"object"}}}],
        "tool_choice":"auto", "stream":false, "metadata":{"preserved":true}
    });
    let (status, bytes) = read_body(chat(&state, original.clone()).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), original);
    assert_eq!(*captured.lock().unwrap(), Some(original));
}

#[tokio::test]
async fn chat_alias_uses_actual_model_profile_and_preserves_history() {
    let (_dir, state) = fixture();
    let profile = profiles::all().first().unwrap();
    select_profiles(&state, vec![profile.id.clone()]);
    let captured = Arc::new(Mutex::new(None));
    let upstream_capture = captured.clone();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let capture = upstream_capture.clone();
            async move {
                *capture.lock().unwrap() = Some(body.clone());
                Json(body)
            }
        }),
    );
    let (endpoint, _server) = serve(router).await;
    let actual = &profile.models[0];
    add_account(&state, &endpoint, "mapped", actual, Some("public-chat"), 0);
    let original_messages = json!([
        {"role":"system", "content":"Keep existing system"},
        {"role":"developer", "content":"Keep existing developer"},
        {"role":"user", "content":[{"type":"image_url", "image_url":{"url":"https://example.test/img.png"}}]},
        {"role":"assistant", "content":null, "tool_calls":[{"id":"prior-call", "type":"function", "function":{"name":"lookup", "arguments":"{}"}}]},
        {"role":"tool", "tool_call_id":"prior-call", "content":"prior result"}
    ]);
    let input = json!({"model":"public-chat", "messages":original_messages, "tools":[{"type":"function"}], "stream":false});
    let (status, bytes) = read_body(chat(&state, input).await).await;
    assert_eq!(status, StatusCode::OK);
    let output: Value = serde_json::from_slice(&bytes).unwrap();
    let sent = captured.lock().unwrap().clone().unwrap();
    assert_eq!(output, sent);
    assert_eq!(sent["model"], actual.as_str());
    assert_eq!(
        sent["messages"][0],
        json!({"role":"system", "content":profile.instruction})
    );
    assert_eq!(
        sent["messages"].as_array().unwrap().len(),
        original_messages.as_array().unwrap().len() + 1
    );
    assert_eq!(
        &sent["messages"].as_array().unwrap()[1..],
        original_messages.as_array().unwrap()
    );
    assert_eq!(sent["tools"], json!([{"type":"function"}]));
}

#[tokio::test]
async fn chat_profile_does_not_match_a_different_actual_model() {
    let (_dir, state) = fixture();
    let profile = profiles::all().first().unwrap();
    select_profiles(&state, vec![profile.id.clone()]);
    let captured = Arc::new(Mutex::new(None));
    let upstream_capture = captured.clone();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let capture = upstream_capture.clone();
            async move {
                *capture.lock().unwrap() = Some(body.clone());
                Json(body)
            }
        }),
    );
    let (endpoint, _server) = serve(router).await;
    add_account(&state, &endpoint, "plain", "unmatched-model", None, 0);
    let original =
        json!({"model":"unmatched-model", "messages":[{"role":"user", "content":"hello"}]});
    let (status, bytes) = read_body(chat(&state, original.clone()).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), original);
    assert_eq!(*captured.lock().unwrap(), Some(original));
}

#[tokio::test]
async fn chat_failover_rebuilds_model_and_instruction_from_original_messages() {
    let (_dir, state) = fixture();
    let profiles = profiles::all();
    let first = profiles.first().unwrap();
    let second = profiles
        .iter()
        .find(|profile| profile.id != first.id)
        .unwrap();
    select_profiles(&state, vec![first.id.clone(), second.id.clone()]);
    let failing = Router::new().route(
        "/v1/chat/completions",
        post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "retry") }),
    );
    let (first_endpoint, _first_server) = serve(failing).await;
    let captured = Arc::new(Mutex::new(None));
    let upstream_capture = captured.clone();
    let succeeding = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let capture = upstream_capture.clone();
            async move {
                *capture.lock().unwrap() = Some(body.clone());
                Json(body)
            }
        }),
    );
    let (second_endpoint, _second_server) = serve(succeeding).await;
    add_account(
        &state,
        &first_endpoint,
        "first",
        &first.models[0],
        Some("public"),
        0,
    );
    add_account(
        &state,
        &second_endpoint,
        "second",
        &second.models[0],
        Some("public"),
        1,
    );
    let original_messages = json!([{"role":"user", "content":"hello"}]);
    let (status, bytes) = read_body(
        chat(
            &state,
            json!({"model":"public", "messages":original_messages}),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let output: Value = serde_json::from_slice(&bytes).unwrap();
    let sent = captured.lock().unwrap().clone().unwrap();
    assert_eq!(output, sent);
    assert_eq!(sent["model"], second.models[0].as_str());
    assert_eq!(sent["messages"][0]["content"], second.instruction);
    assert_eq!(
        &sent["messages"].as_array().unwrap()[1..],
        original_messages.as_array().unwrap()
    );
    let logs = logs(&state);
    assert_eq!(logs[0].protocol, "chat");
    assert_eq!(logs[0].attempts, 2);
}

#[tokio::test]
async fn chat_stream_preserves_sse_and_records_final_usage_at_done() {
    let (_dir, state) = fixture();
    let chunks = vec![
        Bytes::from_static(
            b"data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
        ),
        Bytes::from_static(
            b"data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[],\"usage\":{\"prompt_tokens\":17,\"completion_tokens\":4,\"prompt_tokens_details\":{\"cached_tokens\":3}}}\n\n",
        ),
        Bytes::from_static(b"data: [DONE]\n\n"),
    ];
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let chunks = chunks.clone();
            async move {
                (
                    [("content-type", "text/event-stream")],
                    Body::from_stream(stream::iter(
                        chunks.into_iter().map(Ok::<_, std::io::Error>),
                    )),
                )
            }
        }),
    );
    let (endpoint, _server) = serve(router).await;
    add_account(&state, &endpoint, "stream", "model-stream", None, 0);
    let (status, body) = read_body(
        chat(&state, json!({"model":"model-stream", "messages":[{"role":"user", "content":"hello"}], "stream":true, "stream_options":{"include_usage":true}})).await,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("data: [DONE]\n\n"));
    let log = &logs(&state)[0];
    assert_eq!(log.protocol, "chat");
    assert_eq!(log.status, 200);
    assert_eq!(log.usage.input_tokens, 17);
    assert_eq!(log.usage.output_tokens, 4);
    assert_eq!(log.usage.cached_tokens, 3);
}

#[tokio::test]
async fn chat_stream_without_done_is_logged_as_truncated() {
    let (_dir, state) = fixture();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            (
                [("content-type", "text/event-stream")],
                Body::from("data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n"),
            )
        }),
    );
    let (endpoint, _server) = serve(router).await;
    add_account(&state, &endpoint, "truncated", "model-truncated", None, 0);
    let response = chat(
        &state,
        json!({"model":"model-truncated", "messages":[{"role":"user", "content":"hello"}], "stream":true}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(to_bytes(response.into_body(), 1024 * 1024).await.is_err());
    let log = &logs(&state)[0];
    assert_eq!(log.protocol, "chat");
    assert_eq!(log.status, 502);
    assert_eq!(log.attempts, 1);
}

#[tokio::test]
async fn chat_is_accepted_for_model_discovery_and_uses_bearer_auth() {
    let (_dir, state) = fixture();
    let router = Router::new().route(
        "/v1/models",
        get(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], format!("Bearer {UPSTREAM_KEY}"));
            Json(json!({"data":[{"id":"chat-model"}]}))
        }),
    );
    let (endpoint, _server) = serve(router).await;
    let (status, output) = admin(
        &state,
        json!({"endpoint":endpoint, "api_key":UPSTREAM_KEY, "protocols":["chat"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{output}");
    assert_eq!(output["models"], json!(["chat-model"]));
}

#[tokio::test]
async fn chat_protocol_can_be_saved_and_is_enabled_by_default() {
    let (_dir, state) = fixture();
    for (name, protocols, expected) in [
        ("Explicit chat", Some(json!(["chat"])), json!(["chat"])),
        (
            "Default protocols",
            None,
            json!(["responses", "compact", "chat", "anthropic"]),
        ),
    ] {
        let mut body = json!({
            "name":name,
            "endpoint":"https://upstream.example/v1",
            "api_key":UPSTREAM_KEY,
            "models":["model-a"],
            "group_ids":["chat-group"]
        });
        if let Some(protocols) = protocols {
            body["protocols"] = protocols;
        }
        let (status, account) = admin_request(&state, "POST", "/api/accounts", body).await;
        assert_eq!(status, StatusCode::OK, "{account}");
        assert_eq!(account["protocols"], expected);
    }
}

#[tokio::test]
async fn group_profile_limit_uses_catalog_size_after_deduplication() {
    let (_dir, state) = fixture();
    let ids: Vec<_> = profiles::all()
        .iter()
        .map(|profile| profile.id.clone())
        .collect();
    let duplicate_ids: Vec<_> = ids.iter().chain(&ids).cloned().collect();
    let (status, group) = admin_request(
        &state,
        "POST",
        "/api/groups",
        json!({"name":"Catalog sized", "profile_ids":duplicate_ids}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{group}");
    assert_eq!(group["profile_ids"], json!(ids));
}

#[tokio::test]
async fn chat_stream_error_is_preserved_and_recorded_as_failure() {
    let (_dir, state) = fixture();
    let error_event =
        b"data: {\"error\":{\"message\":\"upstream rejected request\",\"type\":\"invalid_request_error\"}}\n\n";
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move || async move {
            (
                [("content-type", "text/event-stream")],
                Body::from(Bytes::from_static(error_event)),
            )
        }),
    );
    let (endpoint, _server) = serve(router).await;
    add_account(&state, &endpoint, "error-stream", "model-error", None, 0);
    let response = chat(
        &state,
        json!({"model":"model-error", "messages":[{"role":"user", "content":"hello"}], "stream":true}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(body.as_ref(), error_event);
    let log = &logs(&state)[0];
    assert_eq!(log.protocol, "chat");
    assert_eq!(log.status, 502);
}

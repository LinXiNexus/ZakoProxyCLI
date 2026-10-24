use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
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

const PROXY_KEY: &str = "sk-lxsh-anthropic-integration";
const UPSTREAM_KEY: &str = "anthropic-upstream-secret";

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    state.store.session("anthropic-console-session").unwrap();
    state
        .store
        .update(|config| {
            config.groups.push(Group {
                id: "anthropic-group".into(),
                name: "Anthropic".into(),
                profile_ids: vec![],
                default_strategy_enabled: false,
            });
            config.keys.push(ApiKey {
                id: "anthropic-key".into(),
                name: "Anthropic".into(),
                group_id: "anthropic-group".into(),
                hash: store::digest(PROXY_KEY),
                enabled: true,
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
    (dir, state)
}

fn add_account(state: &Shared, endpoint: &str, id: &str, model: &str, priority: i32) {
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
                model_mappings: BTreeMap::from([("public-model".into(), model.into())]),
                group_ids: vec!["anthropic-group".into()],
                protocols: vec!["anthropic".into()],
                enabled: true,
                priority,
                ..Default::default()
            });
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
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    app(state.clone())
        .oneshot(
            request
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn json_response(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn admin(state: &Shared, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    json_response(
        request(
            state,
            method,
            path,
            body,
            &[
                ("cookie", "lxsh_session=anthropic-console-session"),
                ("x-console-path", "/"),
            ],
        )
        .await,
    )
    .await
}

#[tokio::test]
async fn anthropic_account_protocol_saves_updates_and_survives_restart() {
    let (dir, state) = fixture();
    let body = json!({
        "name":"Anthropic only", "endpoint":"https://example.com/v1", "api_key":UPSTREAM_KEY,
        "models":["claude-test"], "group_ids":["anthropic-group"], "protocols":["anthropic"]
    });
    let (status, saved) = admin(&state, "POST", "/api/accounts", body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let id = saved["id"].as_str().unwrap();
    let (status, accounts) = admin(&state, "GET", "/api/accounts", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accounts[0]["protocols"], json!(["anthropic"]));
    assert_eq!(accounts[0]["has_api_key"], true);
    assert!(accounts[0].get("api_key").is_none());
    let mut updated = body.clone();
    updated["api_key"] = json!("");
    updated["protocols"] = json!(["responses", "compact", "anthropic"]);
    let (status, result) = admin(&state, "PUT", &format!("/api/accounts/{id}"), updated).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    for invalid in [
        json!([]),
        json!(["unknown"]),
        json!(["anthropic", "unknown"]),
    ] {
        let mut invalid_body = body.clone();
        invalid_body["protocols"] = invalid;
        assert_eq!(
            admin(&state, "POST", "/api/accounts", invalid_body).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    drop(state);
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    let account = state.store.config().unwrap().accounts.remove(0);
    assert_eq!(account.protocols, ["responses", "compact", "anthropic"]);
    assert_eq!(state.store.unseal(&account.api_key).unwrap(), UPSTREAM_KEY);
}

#[tokio::test]
async fn new_accounts_default_to_all_protocols_and_explicit_selections_are_preserved() {
    let (dir, state) = fixture();
    for (name, protocols, expected) in [
        (
            "Default protocols",
            None,
            json!(["responses", "compact", "chat", "anthropic"]),
        ),
        (
            "Explicit Responses only",
            Some(json!(["responses"])),
            json!(["responses"]),
        ),
    ] {
        let mut body = json!({
            "name":name, "endpoint":"https://example.com/v1", "api_key":UPSTREAM_KEY,
            "models":["model-test"], "group_ids":["anthropic-group"]
        });
        if let Some(protocols) = protocols {
            body["protocols"] = protocols;
        }
        let (status, saved) = admin(&state, "POST", "/api/accounts", body).await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        let id = saved["id"].as_str().unwrap();
        let (status, accounts) = admin(&state, "GET", "/api/accounts", Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        let account = accounts
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap();
        assert_eq!(account["protocols"], expected);
    }
    drop(state);
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    let accounts = state.store.config().unwrap().accounts;
    assert_eq!(
        accounts[0].protocols,
        ["responses", "compact", "chat", "anthropic"]
    );
    assert_eq!(accounts[1].protocols, ["responses"]);
}

#[tokio::test]
async fn anthropic_non_streaming_response_preserves_native_json_and_accounts_for_cache_tokens() {
    let (_dir, state) = fixture();
    let native_response = json!({
        "id":"msg_native", "type":"message", "role":"assistant", "model":"claude-test",
        "content":[{"type":"text","text":"Hello"}], "stop_reason":"end_turn", "stop_sequence":null,
        "usage":{"input_tokens":17,"output_tokens":7,"cache_read_input_tokens":11,"cache_creation_input_tokens":3}
    });
    let upstream_response = native_response.clone();
    let (endpoint, _server) = serve(Router::new().route(
        "/v1/messages",
        post(move || {
            let response = upstream_response.clone();
            async move { ([("request-id", "req-anthropic-native")], Json(response)) }
        }),
    ))
    .await;
    add_account(&state, &endpoint, "native-json", "claude-test", 0);
    let original = json!({"model":"public-model","max_tokens":32,"messages":[{"role":"user","content":"Hello"}]});
    let response = request(
        &state,
        "POST",
        "/v1/messages",
        original,
        &[("x-api-key", PROXY_KEY)],
    )
    .await;
    assert_eq!(
        response.headers().get("request-id").unwrap(),
        "req-anthropic-native"
    );
    let (status, output) = json_response(response).await;
    assert_eq!(status, StatusCode::OK, "{output}");
    assert_eq!(output, native_response);
    let logs = state.store.logs(0, store::now() + 1).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status, 200);
    assert_eq!(logs[0].protocol, "anthropic");
    assert_eq!(logs[0].usage.input_tokens, 31);
    assert_eq!(logs[0].usage.output_tokens, 7);
    assert_eq!(logs[0].usage.cached_tokens, 11);
    let (status, stats) = admin(&state, "GET", "/api/stats", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stats["summary"]["input_tokens"], 31);
    assert_eq!(stats["summary"]["output_tokens"], 7);
    assert_eq!(stats["summary"]["cached_tokens"], 11);
    assert_eq!(stats["summary"]["uncached_tokens"], 20);
    assert_eq!(stats["summary"]["total_tokens"], 38);
}

#[tokio::test]
async fn anthropic_forwards_credentials_headers_model_and_native_system_without_changing_messages()
{
    let (_dir, state) = fixture();
    let (endpoint, _server) = serve(Router::new().route("/v1/messages", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
        Json(json!({"body":body,"x_api_key":headers.get("x-api-key").and_then(|h|h.to_str().ok()),
            "authorization":headers.get("authorization").and_then(|h|h.to_str().ok()),
            "version":headers.get("anthropic-version").and_then(|h|h.to_str().ok()),
            "beta":headers.get("anthropic-beta").and_then(|h|h.to_str().ok())}))
    }))).await;
    let profile = &profiles::all()[0];
    add_account(
        &state,
        &format!("{endpoint}/v1/"),
        "native",
        &profile.models[0],
        0,
    );
    state
        .store
        .update(|config| {
            config.groups[0].profile_ids = vec![profile.id.clone()];
            Ok(())
        })
        .unwrap();
    let messages = json!([
        {"role":"user","content":[{"type":"text","text":"hello","cache_control":{"type":"ephemeral"}}]},
        {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"lookup","input":{"q":"test"}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"result"}]}
    ]);
    let systems = [
        Value::Null,
        json!("Original system"),
        json!([
            {"type":"text","text":"Original block","cache_control":{"type":"ephemeral"}}
        ]),
    ];
    for (index, system) in systems.into_iter().enumerate() {
        let mut original = json!({"model":"public-model","max_tokens":32,"messages":messages,
            "tools":[{"name":"lookup","input_schema":{"type":"object"}}], "metadata":{"user_id":"test"}});
        if !system.is_null() {
            original["system"] = system.clone();
        }
        let bearer = format!("Bearer {PROXY_KEY}");
        let mut headers = if index == 1 {
            vec![("authorization", bearer.as_str())]
        } else {
            vec![("x-api-key", PROXY_KEY)]
        };
        headers.push(("anthropic-beta", "prompt-caching-2024-07-31"));
        if index == 1 {
            headers.push(("anthropic-version", "2023-06-02"));
        }
        let (status, output) = json_response(
            request(&state, "POST", "/v1/messages", original.clone(), &headers).await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{output}");
        // The upstream credential must never be reflected back to clients.
        assert_eq!(output["x_api_key"], "[redacted]");
        assert!(output["authorization"].is_null());
        assert_eq!(
            output["version"],
            if index == 1 {
                "2023-06-02"
            } else {
                "2023-06-01"
            }
        );
        assert_eq!(output["beta"], "prompt-caching-2024-07-31");
        let mut expected = original;
        expected["model"] = json!(profile.models[0]);
        expected["system"] = match system {
            Value::Null => json!(profile.instruction),
            Value::String(existing) => json!(format!("{}\n\n{existing}", profile.instruction)),
            Value::Array(mut blocks) => {
                blocks.insert(0, json!({"type":"text","text":profile.instruction}));
                json!(blocks)
            }
            _ => unreachable!(),
        };
        assert_eq!(output["body"], expected);
        assert!(output["body"].get("instructions").is_none());
    }
    // Accounts without the selected protocol are ineligible for this endpoint.
    state
        .store
        .update(|config| {
            config.accounts[0].protocols = vec!["responses".into()];
            Ok(())
        })
        .unwrap();
    let response = request(
        &state,
        "POST",
        "/v1/messages",
        json!({"model":"public-model","max_tokens":1,"messages":[]}),
        &[("x-api-key", PROXY_KEY)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn anthropic_failover_rebuilds_mapping_and_system_from_the_original_request() {
    let (_dir, state) = fixture();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let first_capture = captured.clone();
    let (first, _first_server) = serve(Router::new().route(
        "/v1/messages",
        post(move |Json(body): Json<Value>| {
            let captured = first_capture.clone();
            async move {
                captured.lock().unwrap().push(body);
                StatusCode::SERVICE_UNAVAILABLE
            }
        }),
    ))
    .await;
    let second_capture = captured.clone();
    let (second, _second_server) = serve(Router::new().route(
        "/v1/messages",
        post(move |Json(body): Json<Value>| {
            let captured = second_capture.clone();
            async move {
                captured.lock().unwrap().push(body.clone());
                Json(body)
            }
        }),
    ))
    .await;
    let first_profile = &profiles::all()[0];
    let second_profile = &profiles::all()[1];
    add_account(&state, &first, "first", &first_profile.models[0], 0);
    add_account(&state, &second, "second", &second_profile.models[0], 1);
    state
        .store
        .update(|config| {
            config.groups[0].profile_ids =
                vec![first_profile.id.clone(), second_profile.id.clone()];
            Ok(())
        })
        .unwrap();
    let original = json!({"model":"public-model","max_tokens":16,"system":"Client instruction","messages":[{"role":"user","content":"hello"}]});
    let (status, output) = json_response(
        request(
            &state,
            "POST",
            "/v1/messages",
            original.clone(),
            &[("x-api-key", PROXY_KEY)],
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{output}");
    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2);
    for (body, profile) in captured.iter().zip([first_profile, second_profile]) {
        let mut expected = original.clone();
        expected["model"] = json!(profile.models[0]);
        expected["system"] = json!(format!("{}\n\nClient instruction", profile.instruction));
        assert_eq!(*body, expected);
    }
    assert_eq!(output, captured[1]);
    let logs = state.store.logs(0, store::now() + 1).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].attempts, 2);
    assert_eq!(logs[0].protocol, "anthropic");
    assert_eq!(logs[0].account_id, "second");
    assert_eq!(logs[0].model, "public-model");
}

#[tokio::test]
async fn anthropic_streaming_preserves_sse_and_records_usage_completion_errors_and_truncation() {
    const START: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"type\":\"message\",\"usage\":{\"input_tokens\":17,\"output_tokens\":1,\"cache_read_input_tokens\":11,\"cache_creation_input_tokens\":3}}}\n\n";
    const DELTA: &str = "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":7}}\n\n";
    const STOP: &str = "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    const ERROR: &str = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"overloaded\"}}\n\n";
    for (scenario, expected_status, body_succeeds) in [
        ("complete", 200, true),
        ("error", 502, true),
        ("truncated", 502, false),
    ] {
        let (_dir, state) = fixture();
        let stream = match scenario {
            "complete" => format!("{START}{DELTA}{STOP}"),
            "error" => format!("{START}{ERROR}"),
            _ => format!("{START}{DELTA}"),
        };
        let upstream_stream = stream.clone();
        let (endpoint, _server) = serve(Router::new().route(
            "/v1/messages",
            post(move || {
                let stream = upstream_stream.clone();
                async move {
                    // Split inside event names and JSON to exercise the streaming parser.
                    let chunks: Vec<_> = stream
                        .as_bytes()
                        .chunks(13)
                        .map(|part| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(part)))
                        .collect();
                    (
                        [("content-type", "text/event-stream")],
                        Body::from_stream(futures_util::stream::iter(chunks)),
                    )
                }
            }),
        ))
        .await;
        add_account(&state, &endpoint, "stream", "claude-test", 0);
        let response = request(&state, "POST", "/v1/messages", json!({"model":"public-model","max_tokens":16,"messages":[{"role":"user","content":"hello"}],"stream":true}), &[("x-api-key", PROXY_KEY)]).await;
        assert_eq!(response.status(), StatusCode::OK, "{scenario}");
        let received = to_bytes(response.into_body(), 1024 * 1024).await;
        assert_eq!(received.is_ok(), body_succeeds, "{scenario}");
        if let Ok(bytes) = received {
            assert_eq!(bytes.as_ref(), stream.as_bytes(), "{scenario}");
        }
        let logs = state.store.logs(0, store::now() + 1).unwrap();
        assert_eq!(logs.len(), 1, "{scenario}");
        assert_eq!(logs[0].status, expected_status, "{scenario}");
        assert_eq!(logs[0].usage.input_tokens, 31, "{scenario}");
        assert_eq!(logs[0].usage.cached_tokens, 11, "{scenario}");
        assert_eq!(
            logs[0].usage.output_tokens,
            if scenario == "error" { 1 } else { 7 },
            "{scenario}"
        );
        assert_eq!(state.runtime.lock().unwrap()["stream"].active, 0);
        assert_eq!(
            state.runtime.lock().unwrap()["stream"].failures,
            u32::from(expected_status == 502)
        );
    }
}

#[tokio::test]
async fn anthropic_discovery_uses_native_auth_for_explicit_and_saved_account_protocols() {
    let (_dir, state) = fixture();
    let (endpoint, _server) = serve(Router::new().route(
        "/v1/models",
        get(|headers: HeaderMap| async move {
            if headers.get("x-api-key").and_then(|h| h.to_str().ok()) != Some(UPSTREAM_KEY)
                || headers
                    .get("anthropic-version")
                    .and_then(|h| h.to_str().ok())
                    != Some("2023-06-01")
            {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            Json(json!({"data":[{"id":"claude-b"},{"id":"claude-a"},{"id":"claude-a"}]}))
                .into_response()
        }),
    ))
    .await;
    add_account(&state, &endpoint, "discovery", "claude-a", 0);
    for body in [
        json!({"endpoint":endpoint,"api_key":UPSTREAM_KEY,"protocols":["anthropic"]}),
        json!({"endpoint":endpoint,"account_id":"discovery"}),
    ] {
        let (status, output) = admin(&state, "POST", "/api/models/discover", body).await;
        assert_eq!(status, StatusCode::OK, "{output}");
        assert_eq!(output["models"], json!(["claude-a", "claude-b"]));
    }
}

#[tokio::test]
async fn mixed_protocol_discovery_preserves_bearer_auth_for_openai_upstreams() {
    let (_dir, state) = fixture();
    let (endpoint, _server) = serve(Router::new().route(
        "/v1/models",
        get(|headers: HeaderMap| async move {
            if headers.get("authorization").and_then(|h| h.to_str().ok())
                != Some(format!("Bearer {UPSTREAM_KEY}").as_str())
            {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            Json(json!({"data":[{"id":"openai-test"}]})).into_response()
        }),
    ))
    .await;
    let (status, output) = admin(
        &state,
        "POST",
        "/api/models/discover",
        json!({"endpoint":endpoint,"api_key":UPSTREAM_KEY,"protocols":["responses","compact","anthropic"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{output}");
    assert_eq!(output["models"], json!(["openai-test"]));
}

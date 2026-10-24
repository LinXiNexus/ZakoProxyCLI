use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use lxshai_proxy::{
    Shared, app, new_app, profiles,
    store::{self, Account, ApiKey, Group},
};

const DEFAULT: &str = include_str!("../profiles/default.md");
const SESSION: &str = "lxsh_session=default-strategy-session";
const PROTOCOLS: [&str; 4] = ["responses", "compact", "chat", "anthropic"];

fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    state.store.session("default-strategy-session").unwrap();
    state
        .store
        .update(|config| {
            config.groups.push(Group {
                id: "group".into(),
                name: "Default strategy".into(),
                ..Default::default()
            });
            config.keys.push(ApiKey {
                id: "key".into(),
                group_id: "group".into(),
                hash: store::digest("route-key"),
                enabled: true,
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
    (dir, state)
}

async fn console(state: &Shared, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("x-console-path", "/")
                .header("cookie", SESSION)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"error":String::from_utf8_lossy(&bytes)})),
    )
}

fn select(state: &Shared, enabled: bool, ids: Vec<String>) {
    state
        .store
        .update(|config| {
            config.groups[0].default_strategy_enabled = enabled;
            config.groups[0].profile_ids = ids;
            Ok(())
        })
        .unwrap();
}

fn add_account(state: &Shared, endpoint: &str, id: &str, model: &str, alias: &str, priority: i32) {
    let secret = state.store.seal("upstream-key").unwrap();
    state
        .store
        .update(|config| {
            config.accounts.push(Account {
                id: id.into(),
                name: id.into(),
                endpoint: endpoint.into(),
                api_key: secret,
                models: vec![model.into()],
                model_mappings: if alias == model {
                    Default::default()
                } else {
                    [(alias.into(), model.into())].into()
                },
                group_ids: vec!["group".into()],
                protocols: PROTOCOLS.iter().map(|p| (*p).into()).collect(),
                enabled: true,
                priority,
                ..Default::default()
            });
            Ok(())
        })
        .unwrap();
}

fn path(protocol: &str) -> &'static str {
    match protocol {
        "responses" => "/v1/responses",
        "compact" => "/v1/responses/compact",
        "chat" => "/v1/chat/completions",
        "anthropic" => "/v1/messages",
        _ => unreachable!(),
    }
}

async fn request(state: &Shared, protocol: &str, body: Value) -> Response {
    app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path(protocol))
                .header("content-type", "application/json")
                .header("authorization", "Bearer route-key")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
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
    (
        endpoint,
        Server(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        })),
    )
}

fn original(protocol: &str, model: &str, stream: bool) -> Value {
    let mut body = match protocol {
        "responses" | "compact" => json!({
            "instructions":"Keep client instructions.",
            "input":[{"role":"user","content":[{"type":"input_text","text":"Describe"},{"type":"input_image","image_url":"https://example.test/image.png"}]}],
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}],
            "store":false
        }),
        "chat" => json!({
            "messages":[
                {"role":"system","content":"Keep client instructions."},
                {"role":"developer","content":"Keep developer instructions."},
                {"role":"user","content":[{"type":"text","text":"Describe"},{"type":"image_url","image_url":{"url":"https://example.test/image.png"}}]},
                {"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"call-1","content":"Existing tool result"}
            ],
            "tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}],
            "tool_choice":"auto"
        }),
        "anthropic" => json!({
            "system":[{"type":"text","text":"Keep client instructions.","cache_control":{"type":"ephemeral"}}],
            "messages":[{"role":"user","content":[{"type":"text","text":"Describe"},{"type":"image","source":{"type":"url","url":"https://example.test/image.png"}}]}],
            "max_tokens":512,"tools":[{"name":"lookup","input_schema":{"type":"object"}}]
        }),
        _ => unreachable!(),
    };
    body["model"] = json!(model);
    body["stream"] = json!(stream);
    body["metadata"] = json!({"trace_id":"preserve-me"});
    body
}

fn assert_injection(
    protocol: &str,
    actual: &Value,
    original: &Value,
    model: &str,
    instruction: Option<&str>,
) {
    let mut expected = original.clone();
    expected["model"] = json!(model);
    if let Some(instruction) = instruction {
        match protocol {
            "responses" | "compact" => {
                expected["instructions"] =
                    json!(format!("{instruction}\n\nKeep client instructions."))
            }
            "chat" => expected["messages"]
                .as_array_mut()
                .unwrap()
                .insert(0, json!({"role":"system","content":instruction})),
            "anthropic" => expected["system"]
                .as_array_mut()
                .unwrap()
                .insert(0, json!({"type":"text","text":instruction})),
            _ => unreachable!(),
        };
    }
    assert_eq!(
        actual, &expected,
        "payload changed unexpectedly for {protocol}"
    );
}

#[tokio::test]
async fn old_groups_default_off_and_api_omission_preserves_explicit_toggle() {
    let legacy: Group =
        serde_json::from_value(json!({"id":"legacy","name":"Legacy","profile_ids":[]})).unwrap();
    assert!(!legacy.default_strategy_enabled);
    assert_eq!(
        serde_json::to_value(&legacy).unwrap()["default_strategy_enabled"],
        false
    );
    let (dir, state) = fixture();
    let (status, created) = console(
        &state,
        "POST",
        "/api/groups",
        json!({"name":"Default off","profile_ids":[]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["default_strategy_enabled"], false);
    let (status, created_enabled) = console(
        &state,
        "POST",
        "/api/groups",
        json!({"name":"Explicit opt in","profile_ids":[],"default_strategy_enabled":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created_enabled["default_strategy_enabled"], true);
    let (status, enabled) = console(
        &state,
        "PUT",
        "/api/groups/group",
        json!({"name":"Enabled","profile_ids":[],"default_strategy_enabled":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(enabled["default_strategy_enabled"], true);
    let (status, preserved) = console(
        &state,
        "PUT",
        "/api/groups/group",
        json!({"name":"Renamed by old client","profile_ids":[]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preserved["default_strategy_enabled"], true);
    let (_, listed) = console(&state, "GET", "/api/groups", Value::Null).await;
    assert_eq!(listed[0]["default_strategy_enabled"], true);
    let reopened = new_app(dir.path(), dir.path().join("web")).unwrap();
    assert!(reopened.store.config().unwrap().groups[0].default_strategy_enabled);
    drop(reopened);
    let (status, disabled) = console(
        &state,
        "PUT",
        "/api/groups/group",
        json!({"name":"Disabled","profile_ids":[],"default_strategy_enabled":false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(disabled["default_strategy_enabled"], false);
    drop(state);
    assert!(
        !new_app(dir.path(), dir.path().join("web"))
            .unwrap()
            .store
            .config()
            .unwrap()
            .groups[0]
            .default_strategy_enabled
    );
}

#[tokio::test]
async fn invalid_nonboolean_default_strategy_is_rejected_without_changing_groups() {
    let (_dir, state) = fixture();
    select(&state, true, vec![]);
    for invalid in [
        Value::Null,
        json!("true"),
        json!("false"),
        json!(1),
        json!(0),
        json!([]),
        json!({}),
    ] {
        let body = json!({"name":"Invalid","profile_ids":[],"default_strategy_enabled":invalid});
        assert!(serde_json::from_value::<Group>(body.clone()).is_err());
        for (method, path) in [("POST", "/api/groups"), ("PUT", "/api/groups/group")] {
            assert_eq!(
                console(&state, method, path, body.clone()).await.0,
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
        let groups = state.store.config().unwrap().groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Default strategy");
        assert!(groups[0].default_strategy_enabled);
    }
}

#[test]
fn selected_exact_strategy_wins_and_unknown_or_unselected_model_uses_opt_in_fallback() {
    assert!(!DEFAULT.trim().is_empty());
    let profile = &profiles::all()[0];
    let model = &profile.models[0];
    let group = Group {
        profile_ids: vec![profile.id.clone()],
        default_strategy_enabled: true,
        ..Default::default()
    };
    assert_eq!(
        profiles::instruction(&group, model),
        Some(profile.instruction.as_str())
    );
    assert_eq!(profiles::instruction(&group, "unknown-v1"), Some(DEFAULT));
    assert_eq!(
        profiles::instruction(&group, &format!("{model}-not-selected")),
        Some(DEFAULT)
    );
    let unselected = Group {
        default_strategy_enabled: true,
        ..Default::default()
    };
    assert_eq!(profiles::instruction(&unselected, model), Some(DEFAULT));
    let disabled = Group {
        default_strategy_enabled: false,
        ..group
    };
    assert!(profiles::instruction(&disabled, "unknown-v1").is_none());
    assert_eq!(
        profiles::instruction(&disabled, model),
        Some(profile.instruction.as_str())
    );
}

#[tokio::test]
async fn all_protocols_toggle_fallback_and_preserve_streaming_payloads() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let received = captured.clone();
    let (endpoint, _server) =
        serve(Router::new().fallback(post(move |Json(body): Json<Value>| {
            received.lock().unwrap().push(body.clone());
            async move {
                if body["stream"] == true {
                    (
                        [("content-type", "text/event-stream")],
                        "data: {\"text\":\"unchanged-output\"}\n\ndata: [DONE]\n\n",
                    )
                        .into_response()
                } else {
                    Json(body).into_response()
                }
            }
        })))
        .await;
    let (_dir, state) = fixture();
    add_account(&state, &endpoint, "account", "unknown-v1", "unknown-v1", 0);
    for enabled in [false, true] {
        select(&state, enabled, vec![]);
        for protocol in PROTOCOLS {
            for stream in [false, true] {
                let original = original(protocol, "unknown-v1", stream);
                let response = request(&state, protocol, original.clone()).await;
                assert_eq!(response.status(), StatusCode::OK);
                let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
                    .await
                    .unwrap();
                if stream {
                    assert_eq!(
                        &bytes[..],
                        b"data: {\"text\":\"unchanged-output\"}\n\ndata: [DONE]\n\n"
                    );
                }
                let actual = captured.lock().unwrap().last().unwrap().clone();
                assert_injection(
                    protocol,
                    &actual,
                    &original,
                    "unknown-v1",
                    enabled.then_some(DEFAULT),
                );
            }
        }
    }
    assert_eq!(captured.lock().unwrap().len(), 16);
}

#[tokio::test]
async fn mapped_actual_model_selects_specialized_strategy_before_default_in_every_protocol() {
    let (endpoint, _server) =
        serve(Router::new().fallback(post(|Json(body): Json<Value>| async { Json(body) }))).await;
    let profile = &profiles::all()[0];
    let model = &profile.models[0];
    let (_dir, state) = fixture();
    select(&state, true, vec![profile.id.clone()]);
    add_account(&state, &endpoint, "mapped", model, "client-alias", 0);
    for protocol in PROTOCOLS {
        let original = original(protocol, "client-alias", false);
        let response = request(&state, protocol, original.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let actual: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_injection(
            protocol,
            &actual,
            &original,
            model,
            Some(&profile.instruction),
        );
    }
    // The requested alias resembles a selected model, but routing uses its mapped target.
    state
        .store
        .update(|config| {
            config.accounts[0].models = vec!["unknown-v2".into()];
            config.accounts[0].model_mappings = [(model.clone(), "unknown-v2".into())].into();
            Ok(())
        })
        .unwrap();
    for protocol in PROTOCOLS {
        let original = original(protocol, model, false);
        let response = request(&state, protocol, original.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let actual: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_injection(protocol, &actual, &original, "unknown-v2", Some(DEFAULT));
    }
}

#[tokio::test]
async fn failover_rebuilds_default_or_specific_instruction_from_original_without_accumulating() {
    let profile = &profiles::all()[0];
    let model = &profile.models[0];
    for protocol in PROTOCOLS {
        for first_is_specific in [false, true] {
            let captures = Arc::new(Mutex::new(Vec::new()));
            let first_captures = captures.clone();
            let (first, _first_server) =
                serve(Router::new().fallback(post(move |Json(body): Json<Value>| {
                    first_captures.lock().unwrap().push(body);
                    async { (StatusCode::TOO_MANY_REQUESTS, "retry") }
                })))
                .await;
            let second_captures = captures.clone();
            let (second, _second_server) =
                serve(Router::new().fallback(post(move |Json(body): Json<Value>| {
                    second_captures.lock().unwrap().push(body.clone());
                    async { Json(body) }
                })))
                .await;
            let (_dir, state) = fixture();
            select(&state, true, vec![profile.id.clone()]);
            let (first_model, second_model) = if first_is_specific {
                (model.as_str(), "unknown-v1")
            } else {
                ("unknown-v1", model.as_str())
            };
            add_account(&state, &first, "first", first_model, "shared-alias", 0);
            add_account(&state, &second, "second", second_model, "shared-alias", 1);
            let original = original(protocol, "shared-alias", false);
            let response = request(&state, protocol, original.clone()).await;
            assert_eq!(response.status(), StatusCode::OK);
            to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap();
            let captures = captures.lock().unwrap();
            assert_eq!(captures.len(), 2);
            assert_injection(
                protocol,
                &captures[0],
                &original,
                first_model,
                Some(if first_is_specific {
                    &profile.instruction
                } else {
                    DEFAULT
                }),
            );
            assert_injection(
                protocol,
                &captures[1],
                &original,
                second_model,
                Some(if first_is_specific {
                    DEFAULT
                } else {
                    &profile.instruction
                }),
            );
        }
    }
}

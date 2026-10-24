use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tower::ServiceExt;
use lxshai_proxy::{
    Shared, app, new_app,
    store::{self, Account, ApiKey, Group},
};

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn serve(router: Router) -> (SocketAddr, Server) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    (
        addr,
        Server(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        })),
    )
}

#[derive(Clone, Debug)]
struct Connect {
    host: String,
    port: u16,
    username: String,
    password: String,
}
struct Socks {
    addr: SocketAddr,
    records: Arc<Mutex<Vec<Connect>>>,
    _server: Server,
}
impl Socks {
    fn url(&self) -> String {
        format!("socks5h://proxy-user:proxy%40password@{}", self.addr)
    }
}
async fn socks(target: SocketAddr) -> Socks {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let records = Arc::new(Mutex::new(Vec::new()));
    let captured = records.clone();
    let server = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                result = listener.accept() => {
                    let Ok((socket, _)) = result else { break };
                    let records = captured.clone();
                    connections.spawn(async move {
                        let _ = tokio::time::timeout(Duration::from_secs(15), socks_connection(socket, target, records)).await;
                    });
                }
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
    });
    Socks {
        addr,
        records,
        _server: Server(server),
    }
}
async fn read_string(socket: &mut TcpStream) -> std::io::Result<String> {
    let length = socket.read_u8().await? as usize;
    let mut value = vec![0; length];
    socket.read_exact(&mut value).await?;
    Ok(String::from_utf8_lossy(&value).into_owned())
}
async fn socks_connection(
    mut socket: TcpStream,
    target: SocketAddr,
    records: Arc<Mutex<Vec<Connect>>>,
) -> std::io::Result<()> {
    assert_eq!(socket.read_u8().await?, 5);
    let count = socket.read_u8().await? as usize;
    let mut methods = vec![0; count];
    socket.read_exact(&mut methods).await?;
    let authenticated = methods.contains(&2);
    socket
        .write_all(&[5, if authenticated { 2 } else { 0 }])
        .await?;
    let (username, password) = if authenticated {
        assert_eq!(socket.read_u8().await?, 1);
        let username = read_string(&mut socket).await?;
        let password = read_string(&mut socket).await?;
        let valid = username == "proxy-user" && password == "proxy@password";
        socket.write_all(&[1, u8::from(!valid)]).await?;
        if !valid {
            return Ok(());
        }
        (username, password)
    } else {
        (String::new(), String::new())
    };
    assert_eq!(socket.read_u8().await?, 5);
    assert_eq!(socket.read_u8().await?, 1);
    assert_eq!(socket.read_u8().await?, 0);
    let host = match socket.read_u8().await? {
        1 => {
            let mut ip = [0; 4];
            socket.read_exact(&mut ip).await?;
            std::net::Ipv4Addr::from(ip).to_string()
        }
        3 => read_string(&mut socket).await?,
        4 => {
            let mut ip = [0; 16];
            socket.read_exact(&mut ip).await?;
            std::net::Ipv6Addr::from(ip).to_string()
        }
        _ => panic!("unexpected SOCKS address type"),
    };
    let port = socket.read_u16().await?;
    records.lock().unwrap().push(Connect {
        host,
        port,
        username,
        password,
    });
    let mut remote = TcpStream::connect(target).await?;
    socket.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80]).await?;
    tokio::io::copy_bidirectional(&mut socket, &mut remote).await?;
    Ok(())
}
fn fixture() -> (tempfile::TempDir, Shared) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_app(dir.path(), dir.path().join("web")).unwrap();
    state.store.session("proxy-console-session").unwrap();
    state
        .store
        .update(|config| {
            config.groups.push(Group {
                id: "group".into(),
                name: "group".into(),
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
async fn console(
    state: &Shared,
    method: &str,
    path: &str,
    body: Value,
    authenticated: bool,
) -> (StatusCode, Value) {
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("x-console-path", "/")
                .header(
                    "cookie",
                    if authenticated {
                        "lxsh_session=proxy-console-session"
                    } else {
                        ""
                    },
                )
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn save(state: &Shared, endpoint: &str, proxy_url: &str) -> Value {
    let (status, body) = console(state, "POST", "/api/accounts", json!({"name":"proxied","endpoint":endpoint,"api_key":"upstream-key","proxy_url":proxy_url,"models":["chosen"],"group_ids":["group"]}), true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn account_proxy_is_encrypted_hidden_preserved_and_cleared() {
    let (_dir, state) = fixture();
    let old: Account =
        serde_json::from_value(json!({"name":"legacy","endpoint":"https://example.com"})).unwrap();
    assert!(old.proxy_url.is_empty());
    let proxy_url = "socks5h://proxy-user:proxy%40password@127.0.0.1:1080";
    let mut account = save(&state, "https://upstream.invalid", proxy_url).await;
    assert_eq!(account["has_proxy"], true);
    assert_eq!(account["proxy_display"], "socks5h://127.0.0.1:1080");
    assert!(account.get("proxy_url").is_none());
    let encrypted = state.store.config().unwrap().accounts[0].proxy_url.clone();
    assert!(!encrypted.contains("proxy-user"));
    assert_eq!(state.store.unseal(&encrypted).unwrap(), proxy_url);
    let path = format!("/api/accounts/{}", account["id"].as_str().unwrap());
    assert_eq!(
        console(&state, "PUT", &path, account.clone(), true).await.0,
        StatusCode::OK
    );
    assert_eq!(
        state.store.config().unwrap().accounts[0].proxy_url,
        encrypted
    );
    account["proxy_url"] = Value::Null;
    assert_eq!(
        console(&state, "PUT", &path, account.clone(), true).await.0,
        StatusCode::OK
    );
    assert_eq!(
        state.store.config().unwrap().accounts[0].proxy_url,
        encrypted
    );
    let (_, listed) = console(&state, "GET", "/api/accounts", Value::Null, true).await;
    assert!(!listed.to_string().contains("proxy-user"));
    assert!(!listed.to_string().contains(&encrypted));
    account["proxy_url"] = json!("https://secret:password@localhost:1080");
    let (status, error) = console(&state, "PUT", &path, account.clone(), true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!error.to_string().contains("secret"));
    assert_eq!(
        state.store.config().unwrap().accounts[0].proxy_url,
        encrypted
    );
    account["proxy_url"] = json!("");
    let (status, body) = console(&state, "PUT", &path, account, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["has_proxy"], false);
    assert!(
        state.store.config().unwrap().accounts[0]
            .proxy_url
            .is_empty()
    );
}

#[tokio::test]
async fn proxy_test_checks_auth_remote_dns_and_http_rejection_without_leaking_credentials() {
    let (_dir, state) = fixture();
    let (addr, _server) = serve(Router::new().route(
        "/v1/models",
        get(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], "Bearer upstream-key");
            assert!(headers.get("proxy-authorization").is_none());
            (
                StatusCode::UNAUTHORIZED,
                "upstream-key proxy@password should never be read",
            )
        }),
    ))
    .await;
    let proxy = socks(addr).await;
    let input = json!({"proxy_url":proxy.url(),"endpoint":"http://upstream.invalid:8080","api_key":"upstream-key"});
    assert_eq!(
        console(
            &state,
            "POST",
            "/api/accounts/proxy-test",
            input.clone(),
            false
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = console(&state, "POST", "/api/accounts/proxy-test", input, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["connected"], true);
    assert_eq!(body["ok"], false);
    assert_eq!(body["upstream_status"], 401);
    assert!(body["latency_ms"].is_u64());
    assert!(!body.to_string().contains("password"));
    let account = save(&state, "http://upstream.invalid:8080", &proxy.url()).await;
    let (status, body) = console(
        &state,
        "POST",
        "/api/accounts/proxy-test",
        json!({"account_id":account["id"]}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["upstream_status"], 401);
    let records = proxy.records.lock().unwrap();
    assert_eq!(records.len(), 2);
    for record in records.iter() {
        assert_eq!(record.host, "upstream.invalid");
        assert_eq!(record.port, 8080);
        assert_eq!(record.username, "proxy-user");
        assert_eq!(record.password, "proxy@password");
    }
}

#[tokio::test]
async fn every_account_protocol_and_model_discovery_use_the_saved_socks_proxy() {
    let (_dir, state) = fixture();
    let hits = Arc::new(AtomicUsize::new(0));
    let captured = hits.clone();
    let router = Router::new()
        .route(
            "/v1/models",
            get(|headers: HeaderMap| async move {
                assert_eq!(headers["authorization"], "Bearer upstream-key");
                Json(json!({"data":[{"id":"chosen"}]}))
            }),
        )
        .fallback(post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let hits = captured.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                assert!(headers.get("proxy-authorization").is_none());
                assert!(
                    headers
                        .get("authorization")
                        .is_some_and(|h| h == "Bearer upstream-key")
                        || headers
                            .get("x-api-key")
                            .is_some_and(|h| h == "upstream-key")
                );
                assert_eq!(body["model"], "chosen");
                if body["stream"] == true {
                    (
                        [("content-type", "text/event-stream")],
                        "data: {\"text\":\"via-socks\"}\n\ndata: [DONE]\n\n",
                    )
                        .into_response()
                } else {
                    Json(json!({"text":"via-socks"})).into_response()
                }
            }
        }));
    let (addr, _server) = serve(router).await;
    let proxy = socks(addr).await;
    let account = save(&state, "http://upstream.invalid:8080", &proxy.url()).await;
    for route in [
        "/v1/responses",
        "/v1/responses/compact",
        "/v1/chat/completions",
        "/v1/messages",
    ] {
        for stream in [false, true] {
            let response = route_request(&state, route, stream).await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{route}, stream={stream}"
            );
            let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            assert!(String::from_utf8_lossy(&body).contains("via-socks"));
        }
    }
    assert_eq!(hits.load(Ordering::SeqCst), 8);
    let (status, discovered) = console(&state, "POST", "/api/models/discover", json!({"account_id":account["id"],"endpoint":"http://upstream.invalid:8080","api_key":"upstream-key","protocols":["chat"]}), true).await;
    assert_eq!(status, StatusCode::OK, "{discovered}");
    assert_eq!(discovered["models"], json!(["chosen"]));
    let (status, discovered) = console(&state, "POST", "/api/models/discover", json!({"proxy_url":proxy.url(),"endpoint":"http://upstream.invalid:8080","api_key":"upstream-key"}), true).await;
    assert_eq!(status, StatusCode::OK, "{discovered}");
    assert!(
        proxy
            .records
            .lock()
            .unwrap()
            .iter()
            .all(|record| record.host == "upstream.invalid")
    );
}

async fn route_request(state: &Shared, route: &str, stream: bool) -> Response {
    app(state.clone()).oneshot(Request::builder().method("POST").uri(route)
        .header("content-type", "application/json").header("authorization", "Bearer route-key")
        .body(Body::from(json!({"model":"chosen","stream":stream,"input":"hello","messages":[{"role":"user","content":"hello"}]}).to_string())).unwrap()).await.unwrap()
}

#[tokio::test]
async fn proxy_failure_never_falls_back_to_a_direct_account_connection() {
    let (_dir, state) = fixture();
    let hits = Arc::new(AtomicUsize::new(0));
    let captured = hits.clone();
    let (addr, _server) = serve(Router::new().fallback(move || {
        let hits = captured.clone();
        async move {
            hits.fetch_add(1, Ordering::SeqCst);
            Json(json!({"direct":true}))
        }
    }))
    .await;
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!(
        "socks5://proxy-user:proxy%40password@{}",
        closed.local_addr().unwrap()
    );
    drop(closed);
    let endpoint = format!("http://{addr}");
    let account = save(&state, &endpoint, &proxy_url).await;
    assert_eq!(
        route_request(&state, "/v1/responses", false).await.status(),
        StatusCode::BAD_GATEWAY
    );
    let (status, result) = console(
        &state,
        "POST",
        "/api/accounts/proxy-test",
        json!({"account_id":account["id"]}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["connected"], false);
    assert!(result["upstream_status"].is_null());
    assert!(!result.to_string().contains("proxy@password"));
    assert_eq!(
        console(
            &state,
            "POST",
            "/api/models/discover",
            json!({"account_id":account["id"],"endpoint":endpoint}),
            true
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn retries_select_each_accounts_proxy_and_bad_proxy_auth_is_reported() {
    let (_dir, state) = fixture();
    let (addr, _server) =
        serve(Router::new().fallback(|| async { Json(json!({"text":"via-second-proxy"})) })).await;
    let proxy = socks(addr).await;
    let first = save(
        &state,
        "http://upstream.invalid:8080",
        &proxy.url().replace("proxy%40password", "incorrect"),
    )
    .await;
    save(&state, "http://upstream.invalid:8080", &proxy.url()).await;
    state
        .store
        .update(|config| {
            for account in &mut config.accounts {
                account.priority = if account.id == first["id"].as_str().unwrap() {
                    0
                } else {
                    1
                };
            }
            Ok(())
        })
        .unwrap();
    let response = route_request(&state, "/v1/responses", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        String::from_utf8_lossy(&to_bytes(response.into_body(), 1024).await.unwrap())
            .contains("via-second-proxy")
    );
    let (status, body) = console(
        &state,
        "POST",
        "/api/accounts/proxy-test",
        json!({"account_id":first["id"]}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["connected"], false);
    assert_eq!(proxy.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn proxy_test_does_not_follow_redirects_with_upstream_credentials() {
    let (_dir, state) = fixture();
    let hits = Arc::new(AtomicUsize::new(0));
    let captured = hits.clone();
    let (other, _other_server) = serve(Router::new().fallback(move || {
        let hits = captured.clone();
        async move {
            hits.fetch_add(1, Ordering::SeqCst);
            "unexpected"
        }
    }))
    .await;
    let (addr, _server) = serve(Router::new().fallback(move || async move {
        (
            StatusCode::FOUND,
            [("location", format!("http://{other}/secret"))],
        )
    }))
    .await;
    let proxy = socks(addr).await;
    let (status, body) = console(&state, "POST", "/api/accounts/proxy-test", json!({"proxy_url":proxy.url(),"endpoint":"http://upstream.invalid:8080","api_key":"upstream-key"}), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["connected"], true);
    assert_eq!(body["upstream_status"], 302);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn socks5_without_credentials_reports_success_and_resolves_locally() {
    let (_dir, state) = fixture();
    let (addr, _server) =
        serve(Router::new().route("/v1/models", get(|| async { Json(json!({"data":[]})) }))).await;
    let proxy = socks(addr).await;
    let (status, body) = console(
        &state,
        "POST",
        "/api/accounts/proxy-test",
        json!({"proxy_url":format!("socks5://{}", proxy.addr),"endpoint":"http://127.0.0.1:8080"}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["connected"], true);
    assert_eq!(body["ok"], true);
    assert_eq!(body["upstream_status"], 200);
    assert!(body["error"].is_null());
    let records = proxy.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].host, "127.0.0.1");
    assert!(records[0].username.is_empty());
}

#[tokio::test]
async fn editing_proxy_replaces_cached_transport_and_missing_account_cannot_go_direct() {
    let (_dir, state) = fixture();
    let (first_addr, _first_server) =
        serve(Router::new().fallback(|| async { Json(json!({"via":"first"})) })).await;
    let (second_addr, _second_server) =
        serve(Router::new().fallback(|| async { Json(json!({"via":"second"})) })).await;
    let first = socks(first_addr).await;
    let second = socks(second_addr).await;
    let mut account = save(&state, "http://upstream.invalid:8080", &first.url()).await;
    let response = route_request(&state, "/v1/responses", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["via"],
        "first"
    );
    account["proxy_url"] = json!(second.url());
    let path = format!("/api/accounts/{}", account["id"].as_str().unwrap());
    assert_eq!(
        console(&state, "PUT", &path, account, true).await.0,
        StatusCode::OK
    );
    let response = route_request(&state, "/v1/responses", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["via"],
        "second"
    );
    assert_eq!(first.records.lock().unwrap().len(), 1);
    assert_eq!(second.records.lock().unwrap().len(), 1);
    let (status, body) = console(&state, "POST", "/api/models/discover", json!({"account_id":"missing","endpoint":format!("http://{first_addr}"),"api_key":"key","protocols":["chat"]}), true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Account not found");
}

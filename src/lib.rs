pub mod agent;
pub mod api;
pub mod network;
pub mod profiles;
pub mod proxy;
pub mod store;
pub mod update;

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use store::Store;

#[derive(Default, Clone, serde::Serialize)]
pub struct Runtime {
    pub active: u32,
    pub failures: u32,
    pub cooldown_until: i64,
    #[serde(skip)]
    pub window: i64,
    #[serde(skip)]
    pub requests: u32,
}
pub struct App {
    pub store: Store,
    pub client: reqwest::Client,
    pub proxy_clients: Mutex<HashMap<String, (String, reqwest::Client)>>,
    pub runtime: Mutex<HashMap<String, Runtime>>,
    pub login_attempts: Mutex<HashMap<String, (i64, u32)>>,
    pub web_dir: std::path::PathBuf,
}
pub type Shared = Arc<App>;
pub type ApiResult<T> = Result<axum::Json<T>, ApiError>;
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, axum::Json(json!({"error":self.1}))).into_response()
    }
}
impl From<String> for ApiError {
    fn from(value: String) -> Self {
        tracing::error!(error=%value, "Storage or validation error");
        Self(StatusCode::BAD_REQUEST, value)
    }
}
pub fn bad(message: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}
pub fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "Please sign in".into())
}
pub fn app(state: Shared) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { axum::Json(json!({"status":"ok"})) }),
        )
        .route("/api/bootstrap", get(api::bootstrap))
        .route("/api/setup", post(api::setup))
        .route("/api/login", post(api::login))
        .route("/api/logout", post(api::logout))
        .route("/api/groups", get(api::groups).post(api::save_group))
        .route(
            "/api/groups/{id}",
            put(api::save_group_id).delete(api::delete_group),
        )
        .route("/api/accounts", get(api::accounts).post(api::save_account))
        .route("/api/accounts/proxy-test", post(api::test_proxy))
        .route(
            "/api/accounts/{id}",
            put(api::save_account_id).delete(api::delete_account),
        )
        .route("/api/keys", get(api::keys).post(api::create_key))
        .route(
            "/api/keys/{id}",
            put(api::update_key).delete(api::delete_key),
        )
        .route("/api/keys/{id}/reveal", post(api::reveal_key))
        .route("/api/settings", get(api::settings).put(api::save_settings))
        .route("/api/agent/config", get(agent::get_config).put(agent::put_config))
        .route("/api/agent/models", get(agent::models))
        .route("/api/agent/conversations", get(agent::conversations))
        .route(
            "/api/agent/conversations/{id}",
            get(agent::conversation)
                .put(agent::save_conversation)
                .delete(agent::delete_conversation),
        )
        .route("/api/agent/chat", post(agent::chat))
        .route("/api/password", post(api::password))
        .route("/api/me", get(api::me))
        .route("/api/register", post(api::register))
        .route("/api/admins", get(api::admins_list).post(api::admin_create))
        .route("/api/admins/{id}", axum::routing::delete(api::admin_delete))
        .route("/api/admins/{id}/password", post(api::admin_password))
        .route("/api/updates", get(update::status).post(update::start))
        .route("/api/updates/latest", get(update::latest))
        .route("/api/stats", get(api::stats))
        .route("/api/profiles", get(api::profiles))
        .route("/api/models/discover", post(api::discover))
        .route("/v1/models", get(proxy::models))
        .route("/v1/responses", post(proxy::responses))
        .route("/v1/responses/compact", post(proxy::compact))
        .route("/v1/chat/completions", post(proxy::chat_completions))
        .route("/v1/messages", post(proxy::messages))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .fallback(console)
        .with_state(state)
}
async fn console(State(state): State<Shared>, headers: HeaderMap, uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") || path.starts_with("/v1/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(cfg) = state.store.config() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let entry = format!("/{}", cfg.settings.security_path);
    let asset = path.starts_with("/assets/") || path == "/favicon.svg";
    if path == "/"
        && path != entry
        && api::valid_access_host(&headers)
        && state.store.authenticated(api::cookie(&headers))
    {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
        let Ok(location) = entry.parse() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        response
            .headers_mut()
            .insert(axum::http::header::LOCATION, location);
        return response;
    }
    if path != entry && !asset {
        return StatusCode::NOT_FOUND.into_response();
    }
    if asset && (path.contains("..") || path.contains('\\')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let file = state
        .web_dir
        .join(if asset { &path[1..] } else { "index.html" });
    match tokio::fs::read(file).await {
        Ok(bytes) => {
            let content = if path.ends_with(".js") {
                "application/javascript"
            } else if path.ends_with(".css") {
                "text/css"
            } else if path.ends_with(".svg") {
                "image/svg+xml"
            } else if path.ends_with(".woff2") {
                "font/woff2"
            } else if path.ends_with(".woff") {
                "font/woff"
            } else if path.ends_with(".txt") {
                "text/plain; charset=utf-8"
            } else {
                "text/html; charset=utf-8"
            };
            let secure = api::is_https(&headers);
            let mut response = Response::new(Body::from(bytes));
            let h = response.headers_mut();
            h.insert("content-type", content.parse().unwrap());
            h.insert("x-content-type-options", "nosniff".parse().unwrap());
            h.insert("x-frame-options", "DENY".parse().unwrap());
            h.insert("referrer-policy", "same-origin".parse().unwrap());
            h.insert(
                "cache-control",
                if asset {
                    "public, max-age=86400"
                } else {
                    "no-store"
                }
                .parse()
                .unwrap(),
            );
            h.insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'".parse().unwrap());
            if secure {
                h.insert(
                    "strict-transport-security",
                    "max-age=31536000".parse().unwrap(),
                );
            }
            response
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Frontend is not built. Run npm ci && npm run build in web/.",
        )
            .into_response(),
    }
}
pub fn new_app(data_dir: &std::path::Path, web_dir: std::path::PathBuf) -> Result<Shared, String> {
    Ok(Arc::new(App {
        store: Store::open(data_dir)?,
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?,
        runtime: Mutex::new(HashMap::new()),
        proxy_clients: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(HashMap::new()),
        web_dir,
    }))
}

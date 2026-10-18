use crate::{ApiError, ApiResult, Shared, api::auth, bad, store::now};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{convert::Infallible, time::Duration};

#[derive(Serialize, Deserialize, Clone)]
pub struct AgentConfig {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_prompt")]
    pub system_prompt: String,
    #[serde(default)]
    pub allow_root: bool,
    #[serde(default = "default_steps")]
    pub max_steps: u32,
    #[serde(default)]
    pub reasoning: String,
}

fn default_prompt() -> String {
    "你是运行在 Android 手机上的 AI 智能体。你可以调用工具完成任务：执行 shell 命令（可用 root）、读写文件、列目录、抓取网页、获取时间。\n\
     原则：\n\
     1. 先思考再行动，必要时分步调用工具。\n\
     2. 执行删除、格式化、修改系统等破坏性命令前，先说明意图。\n\
     3. 工具结果要如实转述；失败就报告失败原因，不要编造输出。\n\
     4. 默认用简体中文回答。"
        .into()
}

fn default_steps() -> u32 {
    6
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            api_key: String::new(),
            model: String::new(),
            system_prompt: default_prompt(),
            allow_root: false,
            max_steps: default_steps(),
            reasoning: String::new(),
        }
    }
}

fn config_path(s: &Shared) -> std::path::PathBuf {
    s.store.dir().join("agent.json")
}

fn load(s: &Shared) -> AgentConfig {
    std::fs::read_to_string(config_path(s))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn persist(s: &Shared, cfg: &AgentConfig) -> Result<(), String> {
    let path = config_path(s);
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub async fn get_config(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    Ok(Json(json!(load(&s))))
}

pub async fn put_config(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(mut cfg): Json<AgentConfig>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    cfg.endpoint = cfg.endpoint.trim().to_string();
    cfg.api_key = cfg.api_key.trim().to_string();
    cfg.model = cfg.model.trim().to_string();
    if cfg.model.is_empty() {
        return Err(bad("模型不能为空"));
    }
    if !cfg.endpoint.is_empty()
        && !(cfg.endpoint.starts_with("http://") || cfg.endpoint.starts_with("https://"))
    {
        return Err(bad("自定义端点必须以 http:// 或 https:// 开头"));
    }
    cfg.max_steps = cfg.max_steps.clamp(1, 20);
    cfg.reasoning = match cfg.reasoning.as_str() {
        "low" | "medium" | "high" => cfg.reasoning,
        _ => String::new(),
    };
    if cfg.system_prompt.trim().is_empty() {
        cfg.system_prompt = default_prompt();
    }
    persist(&s, &cfg)?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct ChatInput {
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub allow_root: Option<bool>,
}

fn endpoint_url(base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.ends_with("/chat/completions") {
        return b.to_string();
    }
    if b.ends_with("/v1") {
        return format!("{b}/chat/completions");
    }
    format!("{b}/v1/chat/completions")
}

fn ev(kind: &str, data: Value) -> Result<Event, Infallible> {
    Ok(Event::default().event(kind).data(data.to_string()))
}

pub async fn chat(State(s): State<Shared>, h: HeaderMap, Json(input): Json<ChatInput>) -> Response {
    if let Err(e) = auth(&s, &h) {
        return e.into_response();
    }
    let cfg = load(&s);
    if cfg.model.trim().is_empty() {
        return ApiError(StatusCode::BAD_REQUEST, "请先选择模型".into()).into_response();
    }
    let stream = run(s, cfg, input);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn run(
    s: Shared,
    cfg: AgentConfig,
    input: ChatInput,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    async_stream::stream! {
        let (client, url, secret, model) = if cfg.endpoint.trim().is_empty() {
            match resolve_account(&s, &cfg.model) {
                Ok(value) => value,
                Err(error) => {
                    yield ev("error", json!({"message": error}));
                    yield ev("done", json!({}));
                    return;
                }
            }
        } else {
            (
                s.client.clone(),
                endpoint_url(&cfg.endpoint),
                cfg.api_key.clone(),
                cfg.model.clone(),
            )
        };
        let root_allowed = input.allow_root.unwrap_or(true);
        let mut messages: Vec<Value> = Vec::with_capacity(input.messages.len() + 1);
        messages.push(json!({"role": "system", "content": cfg.system_prompt}));
        for m in input.messages {
            if m.get("role").and_then(Value::as_str).is_some() {
                messages.push(m);
            }
        }
        let tools = tools_json();
        let steps = cfg.max_steps.clamp(1, 20);

        for _ in 0..steps {
            let mut body = json!({
                "model": model,
                "messages": messages,
                "tools": tools,
                "tool_choice": "auto",
                "stream": true
            });
            if !cfg.reasoning.is_empty() {
                body["reasoning_effort"] = json!(cfg.reasoning);
            }
            let response = match client
                .post(&url)
                .bearer_auth(&secret)
                .timeout(Duration::from_secs(180))
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    yield ev("error", json!({"message": format!("请求模型失败: {e}")}));
                    yield ev("done", json!({}));
                    return;
                }
            };

            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                let msg = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| {
                        v.pointer("/error/message")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| text.chars().take(300).collect());
                yield ev("error", json!({"message": format!("HTTP {}: {}", status.as_u16(), msg)}));
                yield ev("done", json!({}));
                return;
            }

            let sse = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("text/event-stream"))
                .unwrap_or(false);

            let mut content = String::new();
            let mut accs: std::collections::BTreeMap<usize, (String, String, String)> =
                std::collections::BTreeMap::new();

            if sse {
                let mut stream = response.bytes_stream();
                let mut buffer = String::new();
                let mut stop = false;
                while let Some(chunk) = stream.next().await {
                    let bytes = match chunk {
                        Ok(b) => b,
                        Err(e) => {
                            yield ev("error", json!({"message": format!("读取流失败: {e}")}));
                            yield ev("done", json!({}));
                            return;
                        }
                    };
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].trim_end_matches('\r').to_string();
                        buffer.drain(..=pos);
                        let Some(data) = line.strip_prefix("data:") else {
                            continue;
                        };
                        let data = data.trim();
                        if data.is_empty() {
                            continue;
                        }
                        if data == "[DONE]" {
                            stop = true;
                            break;
                        }
                        let Ok(value) = serde_json::from_str::<Value>(data) else {
                            continue;
                        };
                        if let Some(err) = value.pointer("/error/message").and_then(Value::as_str) {
                            yield ev("error", json!({"message": err.to_string()}));
                        }
                        let Some(choice) = value.pointer("/choices/0") else {
                            continue;
                        };
                        if let Some(delta) = choice.get("delta") {
                            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                                if !text.is_empty() {
                                    content.push_str(text);
                                    yield ev("delta", json!({"text": text}));
                                }
                            }
                            if let Some(text) = delta
                                .get("reasoning_content")
                                .and_then(Value::as_str)
                                .or_else(|| delta.get("reasoning").and_then(Value::as_str))
                            {
                                if !text.is_empty() {
                                    yield ev("reasoning", json!({"text": text}));
                                }
                            }
                            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                                for call in calls {
                                    let idx = call
                                        .get("index")
                                        .and_then(Value::as_u64)
                                        .unwrap_or(0) as usize;
                                    let entry = accs.entry(idx).or_insert((
                                        String::new(),
                                        String::new(),
                                        String::new(),
                                    ));
                                    if let Some(id) = call.get("id").and_then(Value::as_str) {
                                        if !id.is_empty() {
                                            entry.0 = id.to_string();
                                        }
                                    }
                                    if let Some(name) =
                                        call.pointer("/function/name").and_then(Value::as_str)
                                    {
                                        if !name.is_empty() {
                                            entry.1 = name.to_string();
                                        }
                                    }
                                    if let Some(args) = call
                                        .pointer("/function/arguments")
                                        .and_then(Value::as_str)
                                    {
                                        entry.2.push_str(args);
                                    }
                                }
                            }
                        }
                        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                            if !reason.is_empty() {
                                stop = true;
                            }
                        }
                        if stop {
                            break;
                        }
                    }
                    if stop {
                        break;
                    }
                }
            } else {
                let value: Value = match response.json().await {
                    Ok(v) => v,
                    Err(e) => {
                        yield ev("error", json!({"message": format!("解析响应失败: {e}")}));
                        yield ev("done", json!({}));
                        return;
                    }
                };
                if let Some(message) = value.pointer("/choices/0/message") {
                    if let Some(text) = message
                        .get("reasoning_content")
                        .and_then(Value::as_str)
                        .or_else(|| message.get("reasoning").and_then(Value::as_str))
                    {
                        if !text.is_empty() {
                            yield ev("reasoning", json!({"text": text}));
                        }
                    }
                    if let Some(text) = message.get("content").and_then(Value::as_str) {
                        if !text.is_empty() {
                            content.push_str(text);
                            yield ev("delta", json!({"text": text}));
                        }
                    }
                    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                        for (index, call) in calls.iter().enumerate() {
                            let entry = accs.entry(index).or_insert((
                                String::new(),
                                String::new(),
                                String::new(),
                            ));
                            entry.0 = call
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            entry.1 = call
                                .pointer("/function/name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            entry.2 = call
                                .pointer("/function/arguments")
                                .and_then(Value::as_str)
                                .unwrap_or("{}")
                                .to_string();
                        }
                    }
                }
            }

            let mut calls: Vec<Value> = Vec::new();
            for (index, (id, name, args)) in accs.iter() {
                if name.is_empty() {
                    continue;
                }
                let id = if id.is_empty() {
                    format!("call_{index}")
                } else {
                    id.clone()
                };
                let args = if args.trim().is_empty() {
                    "{}".to_string()
                } else {
                    args.clone()
                };
                calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": args}
                }));
            }

            if calls.is_empty() {
                yield ev("done", json!({}));
                return;
            }

            messages.push(json!({
                "role": "assistant",
                "content": if content.is_empty() { Value::Null } else { Value::String(content.clone()) },
                "tool_calls": calls
            }));

            for call in &calls {
                let name = call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let raw = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let args: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
                yield ev("tool", json!({"id": id, "name": name, "args": args}));
                let result = execute(&s, &client, &name, &args, root_allowed).await;
                yield ev("tool_result", json!({"id": id, "name": name, "result": result}));
                messages.push(json!({"role": "tool", "tool_call_id": id, "content": result}));
            }
        }
        yield ev("done", json!({}));
    }
}

async fn execute(
    s: &Shared,
    client: &reqwest::Client,
    name: &str,
    args: &Value,
    root_allowed: bool,
) -> String {
    let _ = s;
    match name {
        "shell" => {
            let command = args
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if command.trim().is_empty() {
                return "缺少 command 参数".into();
            }
            let want_root = args.get("root").and_then(Value::as_bool).unwrap_or(false);
            if want_root && !root_allowed {
                return "未获授权使用 root。请在助手设置中开启“允许 root”，并在本次请求中勾选 root。".into();
            }
            let handle = tokio::task::spawn_blocking(move || run_shell(&command, want_root));
            match tokio::time::timeout(Duration::from_secs(75), handle).await {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => format!("执行线程失败: {e}"),
                Err(_) => "执行超时（75 秒）".into(),
            }
        }
        "read_file" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            if path.is_empty() {
                return "缺少 path".into();
            }
            match std::fs::read_to_string(path) {
                Ok(t) => truncate(t, 12000),
                Err(e) => format!("读取失败: {e}"),
            }
        }
        "write_file" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            if path.is_empty() {
                return "缺少 path".into();
            }
            match std::fs::write(path, content) {
                Ok(_) => format!("已写入 {} 字节到 {path}", content.len()),
                Err(e) => format!("写入失败: {e}"),
            }
        }
        "list_dir" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("/");
            match std::fs::read_dir(path) {
                Ok(rd) => {
                    let mut items: Vec<String> = rd
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            let kind = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                "d"
                            } else {
                                "-"
                            };
                            format!("{kind} {}", e.file_name().to_string_lossy())
                        })
                        .collect();
                    items.sort();
                    items.truncate(300);
                    if items.is_empty() {
                        format!("目录为空: {path}")
                    } else {
                        items.join("\n")
                    }
                }
                Err(e) => format!("列目录失败: {e}"),
            }
        }
        "web_fetch" => {
            let url = args.get("url").and_then(Value::as_str).unwrap_or("");
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return "url 必须是 http(s) 地址".into();
            }
            match client.get(url).timeout(Duration::from_secs(30)).send().await {
                Ok(r) => {
                    let status = r.status();
                    match r.text().await {
                        Ok(t) => truncate(format!("HTTP {}\n{t}", status.as_u16()), 12000),
                        Err(e) => format!("读取响应失败: {e}"),
                    }
                }
                Err(e) => format!("抓取失败: {e}"),
            }
        }
        "time" => chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z").to_string(),
        other => format!("未知工具: {other}"),
    }
}

fn run_shell(command: &str, root: bool) -> String {
    use std::process::Command;
    let mut cmd = if root {
        let mut c = Command::new(su_binary());
        c.arg("-c").arg(command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    match cmd.output() {
        Ok(o) => {
            let mut out = String::new();
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stdout.is_empty() {
                out.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str("[stderr]\n");
                out.push_str(&stderr);
            }
            out.push_str(&format!("\n[exit {}]", o.status.code().unwrap_or(-1)));
            truncate(out, 12000)
        }
        Err(e) => format!("无法启动命令: {e}"),
    }
}

fn su_binary() -> String {
    for p in [
        "/system/bin/su",
        "/system/xbin/su",
        "/sbin/su",
        "/debug_ramdisk/su",
    ] {
        if std::path::Path::new(p).exists() {
            return p.to_string();
        }
    }
    "su".to_string()
}

fn truncate(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut cut = max;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n…（已截断）");
    }
    text
}

fn resolve_account(
    s: &Shared,
    model: &str,
) -> Result<(reqwest::Client, String, String, String), String> {
    let cfg = s.store.config()?;
    let mut accounts: Vec<_> = cfg
        .accounts
        .iter()
        .filter(|account| {
            account.enabled
                && account.protocols.iter().any(|item| item == "chat")
                && account.actual_model(model).is_some()
        })
        .cloned()
        .collect();
    accounts.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    let account = accounts
        .first()
        .ok_or_else(|| "没有可用账号支持该模型（请在账号管理中启用账号并勾选 chat 协议）".to_string())?;
    let actual = account
        .actual_model(model)
        .ok_or_else(|| "模型映射失败".to_string())?
        .to_string();
    let url = crate::proxy::upstream_url(&account.endpoint, "chat/completions")?.to_string();
    let client = crate::network::account_client(s, account)?;
    let secret = s.store.unseal(&account.api_key)?;
    Ok((client, url, secret, actual))
}

pub async fn models(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    let cfg = s.store.config()?;
    let mut groups: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for account in cfg
        .accounts
        .iter()
        .filter(|account| account.enabled && account.protocols.iter().any(|item| item == "chat"))
    {
        let provider = if account.name.trim().is_empty() {
            reqwest::Url::parse(&account.endpoint)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| "其他".into())
        } else {
            account.name.trim().to_owned()
        };
        let entry = groups.entry(provider).or_default();
        for requested in account.models.iter().chain(account.model_mappings.keys()) {
            if account.actual_model(requested).is_some() {
                entry.insert(requested.clone());
            }
        }
    }
    let list: Vec<Value> = groups
        .into_iter()
        .map(|(provider, models)| {
            json!({
                "provider": provider,
                "models": models.into_iter().collect::<Vec<_>>()
            })
        })
        .collect();
    Ok(Json(json!(list)))
}

#[derive(Deserialize)]
pub struct ConversationInput {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub messages: Vec<Value>,
}

pub async fn conversations(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    Ok(Json(json!(s.store.agent_conversations()?)))
}

pub async fn conversation(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    match s.store.agent_conversation(&id)? {
        Some(value) => Ok(Json(value)),
        None => Err(ApiError(
            StatusCode::NOT_FOUND,
            "Conversation not found".into(),
        )),
    }
}

pub async fn save_conversation(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ConversationInput>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    if id.is_empty()
        || id.len() > 100
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(bad("Invalid conversation id"));
    }
    let title: String = if input.title.trim().is_empty() {
        "新对话".into()
    } else {
        input.title.trim().chars().take(80).collect()
    };
    let value = json!({
        "id": id,
        "title": title,
        "model": input.model,
        "messages": input.messages,
        "updated_at": now(),
    });
    s.store.save_agent_conversation(&id, &value)?;
    Ok(Json(value))
}

pub async fn clear_conversations(State(s): State<Shared>, h: HeaderMap) -> ApiResult<Value> {
    auth(&s, &h)?;
    s.store.clear_agent_conversations()?;
    Ok(Json(json!({"ok": true})))
}

pub async fn delete_conversation(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    auth(&s, &h)?;
    s.store.delete_agent_conversation(&id)?;
    Ok(Json(json!({"ok": true})))
}

fn tools_json() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "shell",
                "description": "在手机本地执行 shell 命令。root=true 时通过 su 以 root 身份执行（需用户授权）。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "要执行的命令"},
                        "root": {"type": "boolean", "description": "是否用 root 执行"}
                    },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "读取文本文件内容",
                "parameters": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "写入文本文件（覆盖）",
                "parameters": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}, "content": {"type": "string"}},
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "列出目录内容",
                "parameters": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "web_fetch",
                "description": "抓取网页并返回文本",
                "parameters": {
                    "type": "object",
                    "properties": {"url": {"type": "string"}},
                    "required": ["url"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "time",
                "description": "获取当前本地时间",
                "parameters": {"type": "object", "properties": {}}
            }
        }
    ])
}

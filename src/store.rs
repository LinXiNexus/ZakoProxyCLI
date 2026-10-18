use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, sync::Mutex};

pub type Result<T> = std::result::Result<T, String>;
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn token(prefix: &str) -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("{prefix}{}", hex::encode(bytes))
}
pub fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(from = "GroupInput")]
pub struct Group {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub profile_ids: Vec<String>,
    #[serde(default)]
    pub default_strategy_enabled: bool,
}

#[derive(Deserialize)]
struct GroupInput {
    #[serde(default)]
    id: String,
    name: String,
    #[serde(default, deserialize_with = "explicit_profile_ids")]
    profile_ids: Option<Vec<String>>,
    #[serde(default)]
    default_strategy_enabled: bool,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    instruction: String,
}

fn explicit_profile_ids<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<String>::deserialize(deserializer).map(Some)
}

impl From<GroupInput> for Group {
    fn from(input: GroupInput) -> Self {
        let profile_ids = input.profile_ids.unwrap_or_else(|| {
            crate::profiles::legacy_profile(&input.models, &input.instruction)
                .map(|profile| vec![profile.id.clone()])
                .unwrap_or_default()
        });
        Self {
            id: input.id,
            name: input.name,
            profile_ids,
            default_strategy_enabled: input.default_strategy_enabled,
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Account {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub endpoint: String,
    #[serde(default)]
    pub api_key: String,
    /// Encrypted SOCKS proxy URL; never returned by account APIs.
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub model_mappings: BTreeMap<String, String>,
    #[serde(default)]
    pub group_ids: Vec<String>,
    #[serde(default)]
    pub rpm: u32,
    #[serde(default)]
    pub concurrency: u32,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default = "protocols")]
    pub protocols: Vec<String>,
}
impl Account {
    pub fn actual_model<'a>(&'a self, requested: &'a str) -> Option<&'a str> {
        let actual = self
            .model_mappings
            .get(requested)
            .map(String::as_str)
            .unwrap_or(requested);
        self.models
            .iter()
            .any(|model| model == actual)
            .then_some(actual)
    }
}
fn enabled() -> bool {
    true
}
fn protocols() -> Vec<String> {
    vec![
        "responses".into(),
        "compact".into(),
        "chat".into(),
        "anthropic".into(),
    ]
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub group_id: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub security_path: String,
    pub max_retries: u32,
    pub failure_threshold: u32,
    pub cooldown_seconds: u32,
    pub request_timeout_seconds: u32,
    pub retention_days: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            security_path: String::new(),
            max_retries: 2,
            failure_threshold: 2,
            cooldown_seconds: 60,
            request_timeout_seconds: 120,
            retention_days: 7,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Admin {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    #[serde(default = "default_admin_role")]
    pub role: String,
    #[serde(default)]
    pub created_at: i64,
}
fn default_admin_role() -> String {
    "owner".into()
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub username: String,
    pub password_hash: String,
    #[serde(default)]
    pub admins: Vec<Admin>,
    pub settings: Settings,
    pub groups: Vec<Group>,
    pub accounts: Vec<Account>,
    pub keys: Vec<ApiKey>,
}
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
#[serde(from = "UsageInput")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}
#[derive(Deserialize)]
struct UsageInput {
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}
impl From<UsageInput> for Usage {
    fn from(input: UsageInput) -> Self {
        Self {
            input_tokens: input.input_tokens,
            output_tokens: input.output_tokens,
            cached_tokens: input.cached_tokens,
            cache_creation_input_tokens: input.cache_creation_input_tokens,
            cache_read_input_tokens: input.cache_read_input_tokens.unwrap_or(input.cached_tokens),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct RequestLog {
    pub id: String,
    pub timestamp: i64,
    pub key_id: String,
    pub account_id: String,
    pub model: String,
    pub protocol: String,
    pub status: u16,
    pub latency_ms: u64,
    #[serde(default)]
    pub first_token_ms: Option<u64>,
    pub usage: Usage,
    pub cyber: bool,
    pub attempts: u32,
    #[serde(default)]
    pub request_path: String,
    #[serde(default)]
    pub error: Option<String>,
}
pub const LOG_PAGE_SIZE: u64 = 20;
pub struct LogPage {
    pub items: Vec<RequestLog>,
    pub page: u64,
    pub total: u64,
    pub total_pages: u64,
    pub retention_days: u32,
}
pub struct Store {
    db: Mutex<Connection>,
    cipher: Aes256Gcm,
    dir: std::path::PathBuf,
}
fn read_config(db: &Connection) -> Result<Config> {
    db.query_row("SELECT value FROM config WHERE id=1", [], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .map_err(|e| e.to_string())?
    .map(|value| serde_json::from_str::<Config>(&value).map_err(|e| e.to_string()))
    .transpose()
    .map(Option::unwrap_or_default)
}
fn write_config(db: &Connection, config: &Config) -> Result<()> {
    db.execute(
        "INSERT INTO config(id,value) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
        [serde_json::to_string(config).map_err(|e| e.to_string())?],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn insert_session(db: &Connection, raw: &str, admin_id: Option<&str>) -> Result<()> {
    db.execute("DELETE FROM sessions WHERE expires <= ?1", [now()])
        .map_err(|e| e.to_string())?;
    db.execute(
        "INSERT OR REPLACE INTO sessions(hash, expires, admin_id) VALUES(?1,?2,?3)",
        params![digest(raw), now() + 86400, admin_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn prune_logs(db: &Connection, retention_days: u32) -> Result<()> {
    db.execute(
        "DELETE FROM requests WHERE timestamp < ?1",
        [now() - i64::from(retention_days.max(1)) * 86400],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
impl Store {
    pub fn dir(&self) -> &std::path::Path { &self.dir }
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join("master.key");
        let key = if path.exists() {
            std::fs::read(&path).map_err(|e| e.to_string())?
        } else {
            let mut key = vec![0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(&path)
                .and_then(|mut f| f.write_all(&key))
                .map_err(|e| e.to_string())?;
            key
        };
        let cipher =
            Aes256Gcm::new_from_slice(&key).map_err(|_| "Invalid master.key".to_owned())?;
        let db = Connection::open(dir.join("zako.db")).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS config (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions (hash TEXT PRIMARY KEY, expires INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS requests (id TEXT PRIMARY KEY, timestamp INTEGER NOT NULL, value TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS requests_time ON requests(timestamp);
            CREATE TABLE IF NOT EXISTS agent_conversations (id TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS agent_conversations_time ON agent_conversations(updated_at);") .map_err(|e| e.to_string())?;
        let has_admin_id: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='admin_id'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if has_admin_id == 0 {
            db.execute("ALTER TABLE sessions ADD COLUMN admin_id TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        let mut migrated = read_config(&db)?;
        if migrated.admins.is_empty()
            && !migrated.username.is_empty()
            && !migrated.password_hash.is_empty()
        {
            migrated.admins.push(Admin {
                id: id(),
                username: migrated.username.clone(),
                password_hash: migrated.password_hash.clone(),
                role: "owner".into(),
                created_at: now(),
            });
            write_config(&db, &migrated)?;
        }
        prune_logs(&db, read_config(&db)?.settings.retention_days)?;
        Ok(Self {
            db: Mutex::new(db),
            cipher,
            dir: dir.to_path_buf(),
        })
    }
    pub fn seal(&self, plain: &str) -> Result<String> {
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let data = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
            .map_err(|_| "Encryption failed")?;
        Ok(STANDARD.encode([nonce.to_vec(), data].concat()))
    }
    pub fn unseal(&self, encrypted: &str) -> Result<String> {
        let bytes = STANDARD
            .decode(encrypted)
            .map_err(|_| "Invalid encrypted secret")?;
        if bytes.len() < 28 {
            return Err("Invalid encrypted secret".into());
        }
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..])
            .map_err(|_| "Secret authentication failed")?;
        String::from_utf8(plain).map_err(|e| e.to_string())
    }
    pub fn config(&self) -> Result<Config> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        read_config(&db)
    }
    pub fn update<T>(&self, change: impl FnOnce(&mut Config) -> Result<T>) -> Result<T> {
        let mut db = self.db.lock().map_err(|e| e.to_string())?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let mut cfg = read_config(&tx)?;
        let result = change(&mut cfg)?;
        // Keep the legacy single-account fields in sync with the admins list,
        // mirroring the migration performed in `open`.
        if cfg.admins.is_empty() && !cfg.username.is_empty() && !cfg.password_hash.is_empty() {
            cfg.admins.push(Admin {
                id: id(),
                username: cfg.username.clone(),
                password_hash: cfg.password_hash.clone(),
                role: "owner".into(),
                created_at: now(),
            });
        }
        write_config(&tx, &cfg)?;
        prune_logs(&tx, cfg.settings.retention_days)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(result)
    }
    pub fn session(&self, raw: &str) -> Result<()> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        insert_session(&db, raw, None)
    }
    pub fn session_for_credentials(&self, raw: &str, username: &str, hash: &str) -> Result<bool> {
        let mut db = self.db.lock().map_err(|e| e.to_string())?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let cfg = read_config(&tx)?;
        let Some(admin) = cfg
            .admins
            .iter()
            .find(|a| a.username == username && a.password_hash == hash)
        else {
            return Ok(false);
        };
        insert_session(&tx, raw, Some(&admin.id))?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(true)
    }
    pub fn change_password(&self, session: &str, old_hash: &str, new_hash: String) -> Result<bool> {
        let mut db = self.db.lock().map_err(|e| e.to_string())?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let mut cfg = read_config(&tx)?;
        let row: Option<Option<String>> = tx
            .query_row(
                "SELECT admin_id FROM sessions WHERE hash=?1 AND expires>?2",
                params![digest(session), now()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(session_admin) = row else {
            return Ok(false);
        };
        let target = session_admin
            .as_deref()
            .and_then(|id| cfg.admins.iter().position(|a| a.id == id))
            .or_else(|| {
                if cfg.admins.len() == 1 {
                    Some(0)
                } else {
                    cfg.admins.iter().position(|a| a.username == cfg.username)
                }
            });
        let Some(index) = target else {
            return Ok(false);
        };
        if cfg.admins[index].password_hash != old_hash {
            return Ok(false);
        }
        cfg.admins[index].password_hash = new_hash.clone();
        if cfg.admins[index].username == cfg.username {
            cfg.password_hash = new_hash;
        }
        write_config(&tx, &cfg)?;
        tx.execute("DELETE FROM sessions", [])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(true)
    }
    pub fn authenticated(&self, raw: &str) -> bool {
        self.db
            .lock()
            .ok()
            .and_then(|db| {
                db.query_row(
                    "SELECT 1 FROM sessions WHERE hash=?1 AND expires>?2",
                    params![digest(raw), now()],
                    |_| Ok(()),
                )
                .ok()
            })
            .is_some()
    }
    pub fn logout(&self, raw: &str) {
        if let Ok(db) = self.db.lock() {
            let _ = db.execute("DELETE FROM sessions WHERE hash=?1", [digest(raw)]);
        }
    }
    pub fn revoke_sessions(&self) -> Result<()> {
        self.db
            .lock()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM sessions", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn admins(&self) -> Result<Vec<Admin>> {
        Ok(self.config()?.admins)
    }
    pub fn find_admin(&self, username: &str) -> Result<Option<Admin>> {
        Ok(self
            .config()?
            .admins
            .into_iter()
            .find(|a| a.username == username))
    }
    pub fn session_admin(&self, raw: &str) -> Option<Admin> {
        let db = self.db.lock().ok()?;
        let row: Option<Option<String>> = db
            .query_row(
                "SELECT admin_id FROM sessions WHERE hash=?1 AND expires>?2",
                params![digest(raw), now()],
                |row| row.get(0),
            )
            .optional()
            .ok()?;
        let admin_id = row.flatten()?;
        let cfg = read_config(&db).ok()?;
        cfg.admins.into_iter().find(|a| a.id == admin_id)
    }
    pub fn create_admin(&self, username: &str, hash: &str, role: &str) -> Result<Admin> {
        let admin = Admin {
            id: id(),
            username: username.to_owned(),
            password_hash: hash.to_owned(),
            role: role.to_owned(),
            created_at: now(),
        };
        let pending = admin.clone();
        self.update(|c| {
            if c.admins
                .iter()
                .any(|a| a.username.eq_ignore_ascii_case(username))
            {
                return Err("Username already exists".into());
            }
            c.admins.push(pending);
            Ok(())
        })?;
        Ok(admin)
    }
    pub fn delete_admin(&self, admin_id: &str) -> Result<()> {
        self.update(|c| {
            let position = c
                .admins
                .iter()
                .position(|a| a.id == admin_id)
                .ok_or("Administrator not found")?;
            c.admins.remove(position);
            Ok(())
        })
    }
    pub fn set_admin_password(&self, admin_id: &str, hash: String) -> Result<()> {
        self.update(|c| {
            let admin = c
                .admins
                .iter_mut()
                .find(|a| a.id == admin_id)
                .ok_or("Administrator not found")?;
            admin.password_hash = hash;
            Ok(())
        })
    }
    pub fn owner_count(&self) -> Result<usize> {
        Ok(self
            .config()?
            .admins
            .iter()
            .filter(|a| a.role == "owner")
            .count())
    }
    pub fn agent_conversations(&self) -> Result<Vec<Value>> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let mut statement = db
            .prepare("SELECT value FROM agent_conversations ORDER BY updated_at DESC LIMIT 100")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut list = Vec::new();
        for row in rows {
            let text = row.map_err(|e| e.to_string())?;
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                list.push(json!({
                    "id": value.get("id").cloned().unwrap_or(Value::Null),
                    "title": value.get("title").cloned().unwrap_or(Value::Null),
                    "model": value.get("model").cloned().unwrap_or(Value::Null),
                    "updated_at": value.get("updated_at").cloned().unwrap_or(Value::Null),
                    "message_count": value
                        .get("messages")
                        .and_then(Value::as_array)
                        .map(|messages| messages.len())
                        .unwrap_or(0)
                }));
            }
        }
        Ok(list)
    }
    pub fn agent_conversation(&self, id: &str) -> Result<Option<Value>> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let text: Option<String> = db
            .query_row(
                "SELECT value FROM agent_conversations WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(text.and_then(|text| serde_json::from_str(&text).ok()))
    }
    pub fn save_agent_conversation(&self, id: &str, value: &Value) -> Result<()> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
        db.execute(
            "INSERT INTO agent_conversations(id, value, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
            params![id, text, now()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn clear_agent_conversations(&self) -> Result<()> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.execute("DELETE FROM agent_conversations", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn delete_agent_conversation(&self, id: &str) -> Result<()> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.execute("DELETE FROM agent_conversations WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn log(&self, entry: &RequestLog, _retention: u32) -> Result<()> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.execute(
            "INSERT INTO requests VALUES(?1,?2,?3)",
            params![
                entry.id,
                entry.timestamp,
                serde_json::to_string(entry).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        // An in-flight request may have captured an older retention setting.
        prune_logs(&db, read_config(&db)?.settings.retention_days)?;
        Ok(())
    }
    pub fn logs(&self, from: i64, to: i64) -> Result<Vec<RequestLog>> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        prune_logs(&db, read_config(&db)?.settings.retention_days)?;
        let mut query = db.prepare("SELECT value FROM requests WHERE timestamp>=?1 AND timestamp<=?2 ORDER BY timestamp DESC, id DESC").map_err(|e| e.to_string())?;
        query
            .query_map(params![from, to], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .map(|row| {
                serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect()
    }
    pub fn logs_page(&self, requested_page: u64) -> Result<LogPage> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let retention_days = read_config(&db)?.settings.retention_days.max(1);
        prune_logs(&db, retention_days)?;
        let total: u64 = db
            .query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let total_pages = total.div_ceil(LOG_PAGE_SIZE).max(1);
        let page = requested_page.clamp(1, total_pages);
        let mut query = db
            .prepare(
                "SELECT value FROM requests ORDER BY timestamp DESC, id DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| e.to_string())?;
        let items = query
            .query_map(params![LOG_PAGE_SIZE, (page - 1) * LOG_PAGE_SIZE], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?
            .map(|row| {
                serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<RequestLog>>>()?;
        Ok(LogPage {
            items,
            page,
            total,
            total_pages,
            retention_days,
        })
    }
    pub fn export_logs(&self) -> Result<(u32, Vec<RequestLog>)> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let retention_days = read_config(&db)?.settings.retention_days.max(1);
        prune_logs(&db, retention_days)?;
        let mut query = db
            .prepare("SELECT value FROM requests ORDER BY timestamp DESC, id DESC")
            .map_err(|e| e.to_string())?;
        let logs = query
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .map(|row| {
                serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<RequestLog>>>()?;
        Ok((retention_days, logs))
    }
    pub fn delete_logs(&self) -> Result<usize> {
        self.db
            .lock()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM requests", [])
            .map_err(|e| e.to_string())
    }
}
pub fn usage(value: &Value) -> Usage {
    let u = value
        .get("usage")
        .or_else(|| value.pointer("/response/usage"))
        .or_else(|| value.pointer("/message/usage"))
        .unwrap_or(value);
    let cache_read_input_tokens = [
        "/cache_read_input_tokens",
        "/input_tokens_details/cached_tokens",
        "/prompt_tokens_details/cached_tokens",
        "/cached_tokens",
    ]
    .iter()
    .find_map(|path| u.pointer(path).and_then(Value::as_u64))
    .unwrap_or(0);
    let cache_creation_input_tokens = [
        "/cache_creation_input_tokens",
        "/input_tokens_details/cache_creation_tokens",
        "/input_tokens_details/cache_write_tokens",
        "/prompt_tokens_details/cache_creation_tokens",
        "/prompt_tokens_details/cache_write_tokens",
        "/cache_creation_tokens",
        "/cache_write_tokens",
    ]
    .iter()
    .find_map(|path| u.pointer(path).and_then(Value::as_u64))
    .unwrap_or(0);
    Usage {
        input_tokens: u["input_tokens"]
            .as_u64()
            .or(u["prompt_tokens"].as_u64())
            .unwrap_or(0)
            .saturating_add(u["cache_read_input_tokens"].as_u64().unwrap_or(0))
            .saturating_add(u["cache_creation_input_tokens"].as_u64().unwrap_or(0)),
        output_tokens: u["output_tokens"]
            .as_u64()
            .or(u["completion_tokens"].as_u64())
            .unwrap_or(0),
        cached_tokens: cache_read_input_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_mappings_default_empty_and_resolve_only_once_with_allowlist() {
        let mut account: Account = serde_json::from_value(serde_json::json!({
            "name":"Legacy", "endpoint":"https://example.com", "models":["actual", "alias"]
        }))
        .unwrap();
        assert!(account.model_mappings.is_empty());
        assert_eq!(account.actual_model("actual"), Some("actual"));
        account.model_mappings = BTreeMap::from([
            ("public".into(), "alias".into()),
            ("alias".into(), "actual".into()),
            ("another".into(), "actual".into()),
            ("invalid".into(), "unselected".into()),
        ]);
        assert_eq!(account.actual_model("public"), Some("alias"));
        assert_eq!(account.actual_model("alias"), Some("actual"));
        assert_eq!(account.actual_model("another"), Some("actual"));
        assert_eq!(account.actual_model("invalid"), None);
        assert_eq!(account.actual_model("unknown"), None);
        account.models.clear();
        assert_eq!(account.actual_model("alias"), None);
    }
    #[test]
    fn encrypted_storage_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let encrypted = store.seal("secret-upstream-token").unwrap();
        assert!(!encrypted.contains("secret-upstream-token"));
        assert_eq!(store.unseal(&encrypted).unwrap(), "secret-upstream-token");
        assert!(store.unseal("bad").is_err());
        drop(store);
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.unseal(&encrypted).unwrap(), "secret-upstream-token");
    }
    #[test]
    fn sessions_are_revocable() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path()).unwrap();
        s.session("abc").unwrap();
        assert!(s.authenticated("abc"));
        s.logout("abc");
        assert!(!s.authenticated("abc"));
    }
    #[test]
    fn credential_changes_atomically_revoke_and_reject_stale_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path()).unwrap();
        s.update(|c| {
            c.username = "admin".into();
            c.password_hash = "first-password-hash".into();
            Ok(())
        })
        .unwrap();
        assert!(
            s.session_for_credentials("first", "admin", "first-password-hash")
                .unwrap()
        );
        assert!(
            s.change_password(
                "first",
                "first-password-hash",
                "second-password-hash".into()
            )
            .unwrap()
        );
        assert!(!s.authenticated("first"));
        assert!(
            !s.session_for_credentials("stale", "admin", "first-password-hash")
                .unwrap()
        );
        assert!(!s.authenticated("stale"));
        assert!(
            !s.change_password("first", "first-password-hash", "stale-replacement".into())
                .unwrap()
        );
        assert!(
            s.session_for_credentials("second", "admin", "second-password-hash")
                .unwrap()
        );
        s.logout("second");
        assert!(
            !s.change_password(
                "second",
                "second-password-hash",
                "revoked-replacement".into()
            )
            .unwrap()
        );
        assert_eq!(s.config().unwrap().password_hash, "second-password-hash");
    }
    #[test]
    fn usage_includes_cache() {
        let u = usage(
            &serde_json::json!({"response":{"usage":{"input_tokens":100,"output_tokens":10,"input_tokens_details":{"cached_tokens":80}}}}),
        );
        assert_eq!(u.cached_tokens, 80);
        assert_eq!(u.cache_read_input_tokens, 80);
        assert_eq!(u.cache_creation_input_tokens, 0);
        assert_eq!(u.input_tokens, 100);
    }
    #[test]
    fn usage_preserves_anthropic_cache_creation_and_read_counts() {
        let u = usage(&serde_json::json!({"message":{"usage":{
            "input_tokens":20,"cache_creation_input_tokens":30,
            "cache_read_input_tokens":50,"output_tokens":7
        }}}));
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.cache_creation_input_tokens, 30);
        assert_eq!(u.cache_read_input_tokens, 50);
        assert_eq!(u.cached_tokens, 50);
        let old: Usage = serde_json::from_value(serde_json::json!({
            "input_tokens":100,"output_tokens":7,"cached_tokens":50
        }))
        .unwrap();
        assert_eq!(old.cache_creation_input_tokens, 0);
        assert_eq!(old.cache_read_input_tokens, 50);
        assert_eq!(old.cached_tokens, 50);
    }
}

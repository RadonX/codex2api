use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub upstream: UpstreamConfig,
    pub auth: AuthConfig,
    pub logging: LoggingConfig,
    pub models: ModelsConfig,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_request_body_bytes: usize,
    pub max_aggregate_bytes: usize,
    pub connect_timeout_seconds: u64,
    pub response_header_timeout_seconds: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UpstreamConfig {
    pub base_url: String,
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub credential_store: String,
    pub refresh_lead_seconds: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub request_bodies: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub static_models: Vec<String>,
    pub cache_seconds: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8318,
            max_request_body_bytes: 16 * 1024 * 1024,
            max_aggregate_bytes: 32 * 1024 * 1024,
            connect_timeout_seconds: 10,
            response_header_timeout_seconds: 60,
        }
    }
}
impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            base_url: "https://chatgpt.com/backend-api/codex".into(),
            auth_url: "https://auth.openai.com/oauth/authorize".into(),
            token_url: "https://auth.openai.com/oauth/token".into(),
            client_id: "app_EMoamEEZ73f0CkXaXp7hrann".into(),
        }
    }
}
impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            credential_store: "keychain".into(),
            refresh_lead_seconds: 300,
        }
    }
}
impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            request_bodies: false,
        }
    }
}
impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            static_models: vec![
                "gpt-5.5".into(),
                "gpt-5.4-mini".into(),
                "codex-auto-review".into(),
            ],
            cache_seconds: 300,
        }
    }
}

pub fn data_dir() -> Result<PathBuf> {
    if let Some(v) = std::env::var_os("CODEX2API_DATA_DIR") {
        return Ok(PathBuf::from(v));
    }
    let home =
        std::env::var_os("HOME").ok_or_else(|| AppError::Config("HOME is not set".into()))?;
    Ok(PathBuf::from(home).join("Library/Application Support/codex2api"))
}
pub fn config_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("config.toml"))
}
pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    set_mode(path, 0o700)?;
    Ok(())
}
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}
#[cfg(not(unix))]
fn set_mode(_: &Path, _: u32) -> Result<()> {
    Ok(())
}
pub fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Config("invalid storage path".into()))?;
    ensure_dir(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    let mut file = opts.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    set_mode(path, mode)?;
    Ok(())
}
impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(&config_path()?)
    }
    pub fn load_from(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("cannot read {}: {e}", path.display())))?;
        let mut c: Self =
            toml::from_str(&s).map_err(|e| AppError::Config(format!("invalid config: {e}")))?;
        if let Ok(value) = std::env::var("CODEX2API_AUTH_CREDENTIAL_STORE") {
            c.auth.credential_store = value;
        }
        if let Ok(value) = std::env::var("CODEX2API_SERVER_HOST") {
            c.server.host = value;
        }
        if let Ok(value) = std::env::var("CODEX2API_SERVER_PORT") {
            c.server.port = value
                .parse()
                .map_err(|_| AppError::Config("CODEX2API_SERVER_PORT must be a port".into()))?;
        }
        c.validate(true)?;
        Ok(c)
    }
    pub fn save_new(&self, path: &Path) -> Result<()> {
        if path.exists() {
            return Err(AppError::Config(format!(
                "{} already exists",
                path.display()
            )));
        };
        atomic_write(
            path,
            toml::to_string_pretty(self)
                .map_err(|e| AppError::Config(e.to_string()))?
                .as_bytes(),
            0o600,
        )
    }
    pub fn validate(&self, allow_non_loopback: bool) -> Result<()> {
        let ip: IpAddr = self
            .server
            .host
            .parse()
            .map_err(|_| AppError::Config("server.host must be an IP address".into()))?;
        if !ip.is_loopback() && !allow_non_loopback {
            return Err(AppError::Config(
                "refusing non-loopback bind; pass --allow-non-loopback explicitly".into(),
            ));
        }
        if self.server.max_request_body_bytes == 0 {
            return Err(AppError::Config(
                "max_request_body_bytes must be positive".into(),
            ));
        }
        Ok(())
    }
}

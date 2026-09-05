pub mod callback;
pub mod oauth;
pub mod pkce;
pub mod refresh;
pub mod store;

use crate::{
    config::Config,
    error::{AppError, Result},
};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct CodexCredentials {
    pub access_token: SecretString,
    pub refresh_token: SecretString,
    pub id_token: SecretString,
    pub expires_at: DateTime<Utc>,
    pub account_id: SecretString,
    pub email: Option<SecretString>,
}
impl Clone for CodexCredentials {
    fn clone(&self) -> Self {
        Self {
            access_token: SecretString::from(self.access_token.expose_secret().to_owned()),
            refresh_token: SecretString::from(self.refresh_token.expose_secret().to_owned()),
            id_token: SecretString::from(self.id_token.expose_secret().to_owned()),
            expires_at: self.expires_at,
            account_id: SecretString::from(self.account_id.expose_secret().to_owned()),
            email: self
                .email
                .as_ref()
                .map(|v| SecretString::from(v.expose_secret().to_owned())),
        }
    }
}
impl std::fmt::Debug for CodexCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexCredentials")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("id_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("account_id", &"[REDACTED]")
            .field("email", &self.email.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}
#[derive(Serialize, Deserialize)]
pub(crate) struct StoredCredentials {
    access_token: String,
    refresh_token: String,
    id_token: String,
    expires_at: DateTime<Utc>,
    account_id: String,
    email: Option<String>,
}
impl From<&CodexCredentials> for StoredCredentials {
    fn from(v: &CodexCredentials) -> Self {
        Self {
            access_token: v.access_token.expose_secret().to_owned(),
            refresh_token: v.refresh_token.expose_secret().to_owned(),
            id_token: v.id_token.expose_secret().to_owned(),
            expires_at: v.expires_at,
            account_id: v.account_id.expose_secret().to_owned(),
            email: v.email.as_ref().map(|s| s.expose_secret().to_owned()),
        }
    }
}
impl From<StoredCredentials> for CodexCredentials {
    fn from(v: StoredCredentials) -> Self {
        Self {
            access_token: SecretString::from(v.access_token),
            refresh_token: SecretString::from(v.refresh_token),
            id_token: SecretString::from(v.id_token),
            expires_at: v.expires_at,
            account_id: SecretString::from(v.account_id),
            email: v.email.map(SecretString::from),
        }
    }
}

#[derive(Clone)]
pub struct AuthManager {
    config: Config,
    store: store::CredentialStore,
    current: Arc<Mutex<Option<CodexCredentials>>>,
    client: reqwest::Client,
}
impl AuthManager {
    pub fn new(config: Config) -> Result<Self> {
        let store = store::CredentialStore::new(&config.auth.credential_store)?;
        let current = store.load_credentials()?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(
                config.server.connect_timeout_seconds,
            ))
            .timeout(std::time::Duration::from_secs(
                config.server.response_header_timeout_seconds,
            ))
            .user_agent(crate::codex::identity::USER_AGENT)
            .build()?;
        Ok(Self {
            config,
            store,
            current: Arc::new(Mutex::new(current)),
            client,
        })
    }
    pub async fn credentials(&self, force: bool) -> Result<CodexCredentials> {
        let mut guard = self.current.lock().await;
        let mut creds = guard.clone().ok_or(AppError::MissingCredentials)?;
        let needs = force
            || Utc::now()
                >= creds.expires_at
                    - chrono::Duration::seconds(self.config.auth.refresh_lead_seconds);
        if needs {
            creds = refresh::refresh(&self.client, &self.config, &creds).await?;
            self.store.save_credentials(&creds)?;
            *guard = Some(creds.clone());
        }
        Ok(creds)
    }
    pub async fn refresh_after_401(&self, rejected_access_token: &str) -> Result<CodexCredentials> {
        let mut guard = self.current.lock().await;
        let current = guard.clone().ok_or(AppError::MissingCredentials)?;
        if current.access_token.expose_secret() != rejected_access_token {
            return Ok(current);
        }
        let fresh = refresh::refresh(&self.client, &self.config, &current).await?;
        self.store.save_credentials(&fresh)?;
        *guard = Some(fresh.clone());
        Ok(fresh)
    }
    pub async fn configured(&self) -> bool {
        self.current.lock().await.is_some()
    }
    pub async fn replace(&self, c: CodexCredentials) -> Result<()> {
        self.store.save_credentials(&c)?;
        *self.current.lock().await = Some(c);
        Ok(())
    }
    pub async fn logout(&self) -> Result<()> {
        self.store.delete_credentials()?;
        *self.current.lock().await = None;
        Ok(())
    }
    pub fn store(&self) -> &store::CredentialStore {
        &self.store
    }
}

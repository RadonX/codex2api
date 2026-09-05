use crate::{
    auth::CodexCredentials,
    codex::identity,
    config::Config,
    error::{AppError, Result},
};
use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}
pub async fn refresh(
    client: &reqwest::Client,
    cfg: &Config,
    old: &CodexCredentials,
) -> Result<CodexCredentials> {
    let response = client
        .post(&cfg.upstream.token_url)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", old.refresh_token.expose_secret()),
            ("client_id", &cfg.upstream.client_id),
        ])
        .send()
        .await?;
    if !response.status().is_success() {
        return if response.status().as_u16() == 400 || response.status().as_u16() == 401 {
            Err(AppError::ReloginRequired)
        } else {
            Err(AppError::Auth(format!(
                "token refresh failed with status {}",
                response.status()
            )))
        };
    }
    let t: Tokens = response
        .json()
        .await
        .map_err(|_| AppError::Auth("invalid token refresh response".into()))?;
    let id = t
        .id_token
        .unwrap_or_else(|| old.id_token.expose_secret().to_owned());
    let account =
        identity::account_id(&id).unwrap_or_else(|| old.account_id.expose_secret().to_owned());
    Ok(CodexCredentials {
        access_token: SecretString::from(t.access_token),
        refresh_token: SecretString::from(
            t.refresh_token
                .unwrap_or_else(|| old.refresh_token.expose_secret().to_owned()),
        ),
        id_token: SecretString::from(id),
        expires_at: Utc::now() + Duration::seconds(t.expires_in.unwrap_or(3600)),
        account_id: SecretString::from(account),
        email: old.email.clone(),
    })
}

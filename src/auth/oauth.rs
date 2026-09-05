use crate::{
    auth::{callback, pkce, CodexCredentials},
    codex::identity,
    config::Config,
    error::{AppError, Result},
};
use chrono::{Duration, Utc};
use secrecy::SecretString;
use serde::Deserialize;
use url::Url;
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}
pub async fn login(cfg: &Config, no_open: bool) -> Result<CodexCredentials> {
    let listener = callback::bind(1455).await?;
    let pkce = pkce::generate();
    let state = pkce::state();
    let redirect = "http://localhost:1455/auth/callback";
    let mut url =
        Url::parse(&cfg.upstream.auth_url).map_err(|e| AppError::Config(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.upstream.client_id)
        .append_pair("redirect_uri", redirect)
        .append_pair(
            "scope",
            "openid profile email offline_access api.connectors.read api.connectors.invoke",
        )
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", crate::codex::identity::ORIGINATOR);
    println!("Complete authentication as Account B in a separate browser profile.\nAuthorization URL:\n{url}");
    if !no_open {
        webbrowser::open(url.as_str()).map_err(|e| {
            AppError::Auth(format!("could not open browser: {e}; retry with --no-open"))
        })?;
    }
    let code = callback::wait(listener, &state).await?;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(
            cfg.server.connect_timeout_seconds,
        ))
        .timeout(std::time::Duration::from_secs(
            cfg.server.response_header_timeout_seconds,
        ))
        .build()?;
    let response = client
        .post(&cfg.upstream.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", redirect),
            ("client_id", &cfg.upstream.client_id),
            ("code_verifier", &pkce.verifier),
        ])
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::Auth(format!(
            "authorization code exchange failed with status {}",
            response.status()
        )));
    }
    let t: Tokens = response
        .json()
        .await
        .map_err(|_| AppError::Auth("invalid token response".into()))?;
    let refresh = t
        .refresh_token
        .ok_or_else(|| AppError::Auth("token response did not include a refresh token".into()))?;
    let id = t
        .id_token
        .ok_or_else(|| AppError::Auth("token response did not include an ID token".into()))?;
    let account = identity::account_id(&id).ok_or_else(|| {
        AppError::Auth("ID token did not contain a ChatGPT account identifier".into())
    })?;
    let email = identity::email(&id).map(SecretString::from);
    Ok(CodexCredentials {
        access_token: SecretString::from(t.access_token),
        refresh_token: SecretString::from(refresh),
        id_token: SecretString::from(id),
        expires_at: Utc::now() + Duration::seconds(t.expires_in.unwrap_or(3600)),
        account_id: SecretString::from(account),
        email,
    })
}

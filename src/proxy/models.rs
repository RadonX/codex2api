use super::routes::AppState;
use crate::{
    codex::identity,
    error::{AppError, Result},
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use secrecy::ExposeSecret;
use serde_json::{json, Value};

pub async fn list(State(state): State<AppState>) -> Result<Response> {
    let first = state.auth.credentials(false).await?;
    let response = discover(&state, &first).await?;
    let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let fresh = state
            .auth
            .refresh_after_401(first.access_token.expose_secret())
            .await?;
        discover(&state, &fresh).await?
    } else {
        response
    };
    if response.status().is_success() {
        let value: Value = response
            .json()
            .await
            .map_err(|_| AppError::Upstream("model discovery returned invalid JSON".into()))?;
        return Ok(Json(value).into_response());
    }
    let data = state
        .config
        .models
        .static_models
        .iter()
        .map(|id| json!({"id":id,"object":"model","owned_by":"openai"}))
        .collect::<Vec<_>>();
    let mut fallback = Json(json!({"object":"list","data":data})).into_response();
    fallback.headers_mut().insert(
        "x-codex2api-model-source",
        http::HeaderValue::from_static("static-fallback"),
    );
    Ok(fallback)
}
async fn discover(
    state: &AppState,
    credentials: &crate::auth::CodexCredentials,
) -> Result<reqwest::Response> {
    let url = format!(
        "{}/models?client_version=0.137.0",
        state.config.upstream.base_url.trim_end_matches('/')
    );
    let request = state
        .client
        .get(url)
        .bearer_auth(credentials.access_token.expose_secret())
        .header("chatgpt-account-id", credentials.account_id.expose_secret())
        .header("originator", identity::ORIGINATOR)
        .header("user-agent", identity::USER_AGENT)
        .send();
    tokio::time::timeout(
        std::time::Duration::from_secs(state.config.server.response_header_timeout_seconds),
        request,
    )
    .await
    .map_err(|_| AppError::Upstream("model discovery response-header timeout".into()))?
    .map_err(Into::into)
}

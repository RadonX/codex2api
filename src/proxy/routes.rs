use crate::{
    auth::AuthManager,
    config::Config,
    error::Result,
    proxy::{models, responses},
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use secrecy::ExposeSecret;
use serde_json::json;
use subtle::ConstantTimeEq;
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub auth: AuthManager,
    pub client: reqwest::Client,
}
impl AppState {
    pub fn new(config: Config) -> Result<Self> {
        let auth = AuthManager::new(config.clone())?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(
                config.server.connect_timeout_seconds,
            ))
            .user_agent(crate::codex::identity::USER_AGENT)
            .build()?;
        Ok(Self {
            config,
            auth,
            client,
        })
    }
}
pub fn router(state: AppState) -> Router {
    let limit = state.config.server.max_request_body_bytes;
    let protected = Router::new()
        .route("/responses", post(responses::post))
        .route("/models", get(models::list))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate));
    Router::new()
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/readyz", get(ready))
        .nest("/v1", protected)
        .layer(DefaultBodyLimit::max(limit))
        .with_state(state)
}
async fn authenticate(State(state): State<AppState>, req: Request<Body>, next: Next) -> Response {
    let expected = match state.auth.store().local_key() {
        Ok(Some(v)) => v,
        _ => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "local_key_missing",
                "codex2api local API key is not configured",
            )
        }
    };
    let supplied = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let valid: bool = supplied
        .as_bytes()
        .ct_eq(expected.expose_secret().as_bytes())
        .into();
    if !valid {
        return error(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid codex2api API key",
        );
    }
    next.run(req).await
}
async fn ready(State(state): State<AppState>) -> Response {
    if state.config.validate(false).is_err() {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "invalid_config",
            "Configuration is invalid",
        );
    }
    if !state.auth.configured().await {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "account_not_configured",
            "Account B is not configured",
        );
    }
    Json(json!({"status":"ready"})).into_response()
}
fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({"error":{"type":"authentication_error","code":code,"message":message}})),
    )
        .into_response()
}

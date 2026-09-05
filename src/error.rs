use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Auth(String),
    #[error("Account B is not configured; run `codex2api login`")]
    MissingCredentials,
    #[error("Account B requires re-login; run `codex2api login`")]
    ReloginRequired,
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

impl AppError {
    pub fn response(&self) -> Response {
        let (status, code, message) = match self {
            Self::MissingCredentials => (
                StatusCode::SERVICE_UNAVAILABLE,
                "account_not_configured",
                self.to_string(),
            ),
            Self::ReloginRequired => (
                StatusCode::UNAUTHORIZED,
                "account_relogin_required",
                self.to_string(),
            ),
            Self::InvalidRequest(_) => {
                (StatusCode::BAD_REQUEST, "invalid_request", self.to_string())
            }
            Self::Auth(_) => (
                StatusCode::UNAUTHORIZED,
                "account_authentication_failed",
                self.to_string(),
            ),
            Self::Upstream(_) | Self::Http(_) => {
                (StatusCode::BAD_GATEWAY, "upstream_error", self.to_string())
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                self.to_string(),
            ),
        };
        (
            status,
            Json(json!({"error":{"type":"api_error","code":code,"message":message}})),
        )
            .into_response()
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        self.response()
    }
}
pub type Result<T> = std::result::Result<T, AppError>;

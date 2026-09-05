use super::{
    headers,
    sse::{Aggregator, Parser},
};
use crate::{
    codex::{compatibility, identity},
    error::{AppError, Result},
    proxy::routes::AppState,
};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;
use secrecy::ExposeSecret;
use serde_json::{json, Value};
pub async fn post(
    State(state): State<AppState>,
    headers_in: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response> {
    let stream_requested = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let upstream_body = compatibility::transform(&body)
        .ok_or_else(|| AppError::InvalidRequest("request body must be a JSON object".into()))?;
    let first_creds = state.auth.credentials(false).await?;
    let first = send(&state, &headers_in, &upstream_body, &first_creds).await?;
    let response = if first.status() == reqwest::StatusCode::UNAUTHORIZED {
        drop(first);
        let fresh = state
            .auth
            .refresh_after_401(first_creds.access_token.expose_secret())
            .await?;
        send(&state, &headers_in, &upstream_body, &fresh).await?
    } else {
        first
    };
    let status = response.status();
    if !status.is_success() {
        let mapped = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let code = match status.as_u16() {
            400 => "invalid_request",
            401 => "account_relogin_required",
            429 => "rate_limit_exceeded",
            503 => "upstream_unavailable",
            504 => "upstream_timeout",
            _ => "upstream_error",
        };
        let detail = structured_upstream_error(response).await;
        return Ok((mapped,Json(json!({"error":{"type":"api_error","code":code,"message":format!("Codex upstream returned {status}: {detail}")}}))).into_response());
    }
    let out_headers = headers::upstream_response_headers(response.headers());
    if stream_requested {
        let stream = validated_stream(response.bytes_stream());
        let mut out = Response::new(Body::from_stream(stream));
        *out.status_mut() = StatusCode::OK;
        *out.headers_mut() = out_headers;
        out.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("text/event-stream"),
        );
        Ok(out)
    } else {
        aggregate(
            response,
            state.config.server.max_aggregate_bytes,
            out_headers,
        )
        .await
    }
}

async fn structured_upstream_error(response: reqwest::Response) -> String {
    const LIMIT: usize = 64 * 1024;
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            return "upstream error body could not be read".into();
        };
        if bytes.len().saturating_add(chunk.len()) > LIMIT {
            return "upstream error body exceeded the diagnostic limit".into();
        }
        bytes.extend_from_slice(&chunk);
    }
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return "upstream returned a non-JSON error".into();
    };
    value
        .pointer("/error/message")
        .or_else(|| value.get("detail"))
        .or_else(|| value.get("error"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(|message| message.chars().take(500).collect())
        .unwrap_or_else(|| "upstream returned an error without a message".into())
}

fn validated_stream(
    stream: impl futures_util::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
) -> impl futures_util::Stream<Item = std::io::Result<bytes::Bytes>> + Send {
    futures_util::stream::try_unfold(
        (Box::pin(stream), Parser::default(), false),
        |(mut stream, mut parser, terminated)| async move {
            if terminated {
                return Ok(None);
            }
            match stream.next().await {
                Some(Ok(chunk)) => {
                    let events = parser.push(&chunk).map_err(std::io::Error::other)?;
                    let terminal = events.iter().any(|event| event.is_terminal_error());
                    Ok(Some((chunk, (stream, parser, terminal))))
                }
                Some(Err(error)) => Err(std::io::Error::other(error)),
                None => {
                    parser.finish().map_err(std::io::Error::other)?;
                    Ok(None)
                }
            }
        },
    )
}
async fn send(
    state: &AppState,
    _client_headers: &HeaderMap,
    body: &Value,
    c: &crate::auth::CodexCredentials,
) -> Result<reqwest::Response> {
    let url = format!(
        "{}/responses",
        state.config.upstream.base_url.trim_end_matches('/')
    );
    let fut = state
        .client
        .post(url)
        .bearer_auth(c.access_token.expose_secret())
        .header("chatgpt-account-id", c.account_id.expose_secret())
        .header("originator", identity::ORIGINATOR)
        .header("user-agent", identity::USER_AGENT)
        .header("accept", "text/event-stream")
        .json(body)
        .send();
    tokio::time::timeout(
        std::time::Duration::from_secs(state.config.server.response_header_timeout_seconds),
        fut,
    )
    .await
    .map_err(|_| AppError::Upstream("upstream response-header timeout".into()))?
    .map_err(Into::into)
}
async fn aggregate(
    response: reqwest::Response,
    limit: usize,
    headers: HeaderMap,
) -> Result<Response> {
    let mut parser = Parser::default();
    let mut agg = Aggregator::new();
    let mut seen = 0usize;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        seen = seen.saturating_add(chunk.len());
        if seen > limit {
            return Err(AppError::Upstream(
                "aggregated upstream response exceeded configured limit".into(),
            ));
        }
        for e in parser.push(&chunk)? {
            agg.accept(e)?
        }
    }
    parser.finish()?;
    let value = agg.finish()?;
    let mut out = (StatusCode::OK, Json(value)).into_response();
    for (k, v) in headers.iter() {
        if k.as_str() != "content-type" {
            out.headers_mut().insert(k, v.clone());
        }
    }
    Ok(out)
}

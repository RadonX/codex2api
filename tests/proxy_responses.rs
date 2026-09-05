use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::{Duration, Utc};
use codex2api::{
    auth::{AuthManager, CodexCredentials},
    config::Config,
    proxy::routes::{self, AppState},
};
use http_body_util::BodyExt;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
type Capture = Arc<Mutex<Option<(http::HeaderMap, Value)>>>;

#[tokio::test]
async fn authenticates_filters_headers_and_aggregates_upstream_sse() {
    let captured = Arc::new(Mutex::new(None));
    let mock=Router::new().route("/responses",post(|State(captured):State<Capture>,headers:http::HeaderMap,Json(body):Json<Value>|async move{*captured.lock().unwrap()=Some((headers,body));([("content-type","text/event-stream"),("x-request-id","up-r1")],concat!("event: response.created\n","data: {\"response\":{\"id\":\"r1\",\"status\":\"in_progress\",\"output\":[]}}\n\n","event: response.output_item.done\n","data: {\"item\":{\"id\":\"m1\",\"type\":\"message\"}}\n\n","event: response.completed\n","data: {\"response\":{\"id\":\"r1\",\"status\":\"completed\"}}\n\n")).into_response()})).with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let temp = tempfile::tempdir().unwrap();
    std::env::set_var("CODEX2API_DATA_DIR", temp.path());
    let mut cfg = Config::default();
    cfg.auth.credential_store = "file".into();
    cfg.upstream.base_url = format!("http://{addr}");
    let manager = AuthManager::new(cfg.clone()).unwrap();
    manager
        .replace(CodexCredentials {
            access_token: SecretString::from("account-b-access".to_owned()),
            refresh_token: SecretString::from("account-b-refresh".to_owned()),
            id_token: SecretString::from("id-token".to_owned()),
            expires_at: Utc::now() + Duration::hours(1),
            account_id: SecretString::from("account-b-id".to_owned()),
            email: None,
        })
        .await
        .unwrap();
    let key = manager.store().create_local_key(false).unwrap();
    let app = routes::router(AppState::new(cfg).unwrap());
    let unauthorized = app
        .clone()
        .oneshot(
            Request::post("/v1/responses")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .oneshot(
            Request::post("/v1/responses")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", key.expose_secret()))
                .header("chatgpt-account-id", "attacker")
                .body(Body::from(
                    r#"{"model":"gpt-5","input":"hi","stream":false,"store":true,"temperature":1}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-request-id"], "up-r1");
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "completed");
    assert_eq!(body["output"][0]["id"], "m1");
    let (headers, upstream) = captured.lock().unwrap().take().unwrap();
    assert_eq!(headers["chatgpt-account-id"], "account-b-id");
    assert_eq!(headers["originator"], "codex_cli_rs");
    assert_eq!(upstream["stream"], true);
    assert_eq!(upstream["store"], false);
    assert!(upstream.get("temperature").is_none());
}

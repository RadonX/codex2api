use axum::{routing::post, Json, Router};
use chrono::{Duration, Utc};
use codex2api::{
    auth::{refresh, CodexCredentials},
    config::Config,
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
#[tokio::test]
async fn retains_refresh_token_when_rotation_is_omitted() {
    let app = Router::new().route(
        "/token",
        post(|| async { Json(json!({"access_token":"new-access","expires_in":3600})) }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(l, app).await.unwrap();
    });
    let mut cfg = Config::default();
    cfg.upstream.token_url = format!("http://{addr}/token");
    let old = CodexCredentials {
        access_token: SecretString::from("old-access".to_owned()),
        refresh_token: SecretString::from("keep-refresh".to_owned()),
        id_token: SecretString::from("x.e30.y".to_owned()),
        expires_at: Utc::now() - Duration::seconds(1),
        account_id: SecretString::from("acct-b".to_owned()),
        email: None,
    };
    let got = refresh::refresh(&reqwest::Client::new(), &cfg, &old)
        .await
        .unwrap();
    assert_eq!(got.refresh_token.expose_secret(), "keep-refresh");
    assert_eq!(got.access_token.expose_secret(), "new-access");
}

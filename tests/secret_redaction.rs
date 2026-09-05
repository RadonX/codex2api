use chrono::Utc;
use codex2api::auth::CodexCredentials;
use secrecy::SecretString;
#[test]
fn credential_debug_is_redacted() {
    let secrets = [
        "access-fixture",
        "refresh-fixture",
        "id-fixture",
        "account-fixture",
        "mail-fixture@example.test",
    ];
    let c = CodexCredentials {
        access_token: SecretString::from(secrets[0].to_owned()),
        refresh_token: SecretString::from(secrets[1].to_owned()),
        id_token: SecretString::from(secrets[2].to_owned()),
        expires_at: Utc::now(),
        account_id: SecretString::from(secrets[3].to_owned()),
        email: Some(SecretString::from(secrets[4].to_owned())),
    };
    let log = format!("{c:?}");
    for secret in secrets {
        assert!(!log.contains(secret));
    }
}

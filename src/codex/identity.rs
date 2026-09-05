use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;

pub const ORIGINATOR: &str = "codex_cli_rs";
pub const USER_AGENT: &str = "codex_cli_rs/0.137.0";
pub fn jwt_claim(token: &str, claim: &str) -> Option<String> {
    let payload = token.trim().split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value.pointer(claim)?.as_str().map(str::to_owned)
}
pub fn account_id(token: &str) -> Option<String> {
    jwt_claim(token, "/https:~1~1api.openai.com~1auth/chatgpt_account_id")
}
pub fn email(token: &str) -> Option<String> {
    jwt_claim(token, "/email")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_account() {
        let p=URL_SAFE_NO_PAD.encode(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct_b"},"email":"b@example.com"}"#);
        let t = format!("x.{p}.y");
        assert_eq!(account_id(&t).as_deref(), Some("acct_b"));
        assert_eq!(email(&t).as_deref(), Some("b@example.com"));
    }
}

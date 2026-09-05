use codex2api::auth::callback;
#[test]
fn state_is_parsed_exactly() {
    let c = callback::parse("GET /auth/callback?code=fixture-code&state=expected HTTP/1.1\r\n\r\n");
    assert_eq!(c.state.as_deref(), Some("expected"));
    assert_ne!(c.state.as_deref(), Some("wrong"));
}
#[test]
fn callback_errors_do_not_expose_code() {
    let c =
        callback::parse("GET /auth/callback?error=access_denied&code=secret-code HTTP/1.1\r\n\r\n");
    assert_eq!(c.error.as_deref(), Some("access_denied"));
}

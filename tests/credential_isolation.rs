use codex2api::{auth::store::CredentialStore, config};
use std::fs;
#[test]
fn dedicated_storage_leaves_codex_sentinels_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let codex = temp.path().join(".codex");
    fs::create_dir(&codex).unwrap();
    let auth = codex.join("auth.json");
    let cfg = codex.join("config.toml");
    fs::write(&auth, b"AUTH_SENTINEL").unwrap();
    fs::write(&cfg, b"CONFIG_SENTINEL").unwrap();
    std::env::set_var(
        "CODEX2API_DATA_DIR",
        temp.path().join("Library/Application Support/codex2api"),
    );
    let store = CredentialStore::new("file").unwrap();
    store.create_local_key(false).unwrap();
    assert_eq!(fs::read(&auth).unwrap(), b"AUTH_SENTINEL");
    assert_eq!(fs::read(&cfg).unwrap(), b"CONFIG_SENTINEL");
    assert!(config::data_dir().unwrap().ends_with("codex2api"));
}

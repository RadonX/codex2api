use crate::{
    auth::{CodexCredentials, StoredCredentials},
    config::{atomic_write, data_dir, ensure_dir},
    error::{AppError, Result},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
const SERVICE: &str = "dev.codex2api.credentials";
const CREDS: &str = "account-b-oauth";
const LOCAL_KEY: &str = "local-api-key";
#[derive(Clone)]
pub struct CredentialStore {
    kind: String,
    dir: PathBuf,
}
impl CredentialStore {
    pub fn new(kind: &str) -> Result<Self> {
        if kind != "keychain" && kind != "file" {
            return Err(AppError::Config(
                "credential_store must be keychain or file".into(),
            ));
        }
        let dir = data_dir()?;
        ensure_dir(&dir)?;
        Ok(Self {
            kind: kind.into(),
            dir,
        })
    }
    fn get(&self, name: &str) -> Result<Option<String>> {
        if self.kind == "keychain" {
            match keyring::Entry::new(SERVICE, name)
                .map_err(|e| AppError::Auth(format!("Keychain unavailable: {e}")))?
                .get_password()
            {
                Ok(v) => Ok(Some(v)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(AppError::Auth(format!("Keychain read failed: {e}"))),
            }
        } else {
            let p = self.secret_path(name);
            match fs::read_to_string(p) {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            }
        }
    }
    fn set(&self, name: &str, value: &str) -> Result<()> {
        if self.kind == "keychain" {
            keyring::Entry::new(SERVICE, name)
                .map_err(|e| AppError::Auth(format!("Keychain unavailable: {e}")))?
                .set_password(value)
                .map_err(|e| AppError::Auth(format!("Keychain write failed: {e}")))
        } else {
            atomic_write(&self.secret_path(name), value.as_bytes(), 0o600)
        }
    }
    fn delete(&self, name: &str) -> Result<()> {
        if self.kind == "keychain" {
            match keyring::Entry::new(SERVICE, name)
                .map_err(|e| AppError::Auth(e.to_string()))?
                .delete_credential()
            {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(AppError::Auth(format!("Keychain delete failed: {e}"))),
            }
        } else {
            match fs::remove_file(self.secret_path(name)) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
    }
    fn secret_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }
    pub fn load_credentials(&self) -> Result<Option<CodexCredentials>> {
        self.get(CREDS)?
            .map(|v| {
                serde_json::from_str::<StoredCredentials>(&v)
                    .map(Into::into)
                    .map_err(Into::into)
            })
            .transpose()
    }
    pub fn save_credentials(&self, c: &CodexCredentials) -> Result<()> {
        self.set(CREDS, &serde_json::to_string(&StoredCredentials::from(c))?)
    }
    pub fn delete_credentials(&self) -> Result<()> {
        self.delete(CREDS)
    }
    pub fn local_key(&self) -> Result<Option<SecretString>> {
        Ok(self.get(LOCAL_KEY)?.map(SecretString::from))
    }
    pub fn create_local_key(&self, replace: bool) -> Result<SecretString> {
        if !replace && self.get(LOCAL_KEY)?.is_some() {
            return Err(AppError::Auth("local API key already exists".into()));
        }
        let mut b = [0u8; 32];
        rand::rng().fill_bytes(&mut b);
        let key = SecretString::from(format!("c2a_{}", URL_SAFE_NO_PAD.encode(b)));
        self.set(LOCAL_KEY, key.expose_secret())?;
        Ok(key)
    }
    pub fn delete_local_key(&self) -> Result<()> {
        self.delete(LOCAL_KEY)
    }
    pub fn fingerprint(key: &SecretString) -> String {
        let h = Sha256::digest(key.expose_secret().as_bytes());
        format!("sha256:{}", hex8(&h[..6]))
    }
}
fn hex8(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

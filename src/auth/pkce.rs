use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}
pub fn generate() -> Pkce {
    let mut b = [0u8; 64];
    rand::rng().fill_bytes(&mut b);
    let verifier = URL_SAFE_NO_PAD.encode(b);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Pkce {
        verifier,
        challenge,
    }
}
pub fn state() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid() {
        let a = generate();
        let b = generate();
        assert_ne!(a.verifier, b.verifier);
        assert_eq!(
            a.challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(a.verifier.as_bytes()))
        );
        assert_ne!(state(), state());
    }
}

// Envelope encryption for provider tokens at rest.
use std::env;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::random;

const TOKEN_PREFIX: &str = "v1:";

#[derive(Clone)]
pub struct TokenVault {
    cipher: Aes256Gcm,
}

impl TokenVault {
    pub fn from_env() -> Result<Option<Self>> {
        let raw = match env::var("REPLAYCORE_TOKEN_ENCRYPTION_KEY")
            .or_else(|_| env::var("TOKEN_ENCRYPTION_KEY"))
        {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };

        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(None);
        }

        Ok(Some(Self::from_base64_key(raw)?))
    }

    pub fn from_base64_key(encoded: &str) -> Result<Self> {
        let key = STANDARD
            .decode(encoded.trim())
            .context("failed to decode token encryption key")?;

        if key.len() != 32 {
            anyhow::bail!(
                "token encryption key must be 32 bytes after base64 decoding, got {}",
                key.len()
            );
        }

        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| anyhow::anyhow!("failed to initialize token encryption cipher"))?;

        Ok(Self { cipher })
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let nonce_bytes: [u8; 12] = random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("failed to encrypt token"))?;

        let mut payload = nonce_bytes.to_vec();
        payload.extend_from_slice(&ciphertext);

        Ok(format!("{}{}", TOKEN_PREFIX, STANDARD.encode(payload)))
    }
}

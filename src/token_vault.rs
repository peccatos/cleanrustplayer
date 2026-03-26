use std::env;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::de::DeserializeOwned;
use serde::Serialize;

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
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("failed to encrypt token"))?;

        let mut payload = nonce_bytes.to_vec();
        payload.extend_from_slice(&ciphertext);

        Ok(format!("{}{}", TOKEN_PREFIX, STANDARD.encode(payload)))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<String> {
        let bytes = self.decode_payload(encoded)?;
        if bytes.len() < 12 {
            anyhow::bail!("encrypted token payload is too short");
        }

        let (nonce_bytes, ciphertext) = bytes.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("failed to decrypt token"))?;

        String::from_utf8(plaintext).context("decrypted token is not valid UTF-8")
    }

    pub fn encrypt_json<T: Serialize>(&self, value: &T) -> Result<String> {
        let serialized = serde_json::to_string(value).context("failed to serialize token data")?;
        self.encrypt(&serialized)
    }

    pub fn decrypt_json<T: DeserializeOwned>(&self, encoded: &str) -> Result<T> {
        let plaintext = self.decrypt(encoded)?;
        serde_json::from_str(&plaintext).context("failed to deserialize token data")
    }

    fn decode_payload(&self, encoded: &str) -> Result<Vec<u8>> {
        let encoded = encoded.trim();
        let encoded = encoded
            .strip_prefix(TOKEN_PREFIX)
            .with_context(|| format!("encrypted token is missing {} prefix", TOKEN_PREFIX))?;

        STANDARD
            .decode(encoded)
            .context("failed to decode encrypted token payload")
    }
}

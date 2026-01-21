use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
use rand::RngCore;

use crate::error::{PostUrbitError, Result};

const SECRET_MAGIC: &[u8; 4] = b"PUNS";
const SECRET_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

pub fn load_app_secrets(data_dir: &Path, app_id: &str, key: &[u8]) -> Result<HashMap<String, String>> {
    let path = secrets_path(data_dir, app_id);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = std::fs::read(&path).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    let plaintext = decrypt_payload(&data, key)?;
    serde_json::from_slice(&plaintext).map_err(|_| PostUrbitError::InvalidInput("secrets json"))
}

pub fn save_app_secrets(
    data_dir: &Path,
    app_id: &str,
    key: &[u8],
    secrets: &HashMap<String, String>,
) -> Result<()> {
    let path = secrets_path(data_dir, app_id);
    if secrets.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    }
    let payload = serde_json::to_vec(secrets)
        .map_err(|_| PostUrbitError::InvalidInput("secrets json"))?;
    let encrypted = encrypt_payload(&payload, key)?;
    std::fs::write(&path, encrypted).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

pub fn delete_app_secrets(data_dir: &Path, app_id: &str) -> Result<()> {
    let path = secrets_path(data_dir, app_id);
    let _ = std::fs::remove_file(&path);
    Ok(())
}

fn secrets_path(data_dir: &Path, app_id: &str) -> PathBuf {
    let safe = app_id.replace(['/', '\\'], "_");
    data_dir
        .join("admin")
        .join("app_secrets")
        .join(format!("{safe}.bin"))
}

fn encrypt_payload(payload: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    if key.len() != 32 {
        return Err(PostUrbitError::InvalidInput("secrets key"));
    }
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher = ChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt((&nonce).into(), payload)
        .map_err(|_| PostUrbitError::Crypto("secrets encrypt"))?;
    let mut out = Vec::with_capacity(SECRET_MAGIC.len() + 1 + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(SECRET_MAGIC);
    out.push(SECRET_VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_payload(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    if data.len() < SECRET_MAGIC.len() + 1 + NONCE_LEN {
        return Err(PostUrbitError::InvalidInput("secrets data"));
    }
    if &data[..SECRET_MAGIC.len()] != SECRET_MAGIC {
        return Err(PostUrbitError::InvalidInput("secrets magic"));
    }
    if data[SECRET_MAGIC.len()] != SECRET_VERSION {
        return Err(PostUrbitError::InvalidInput("secrets version"));
    }
    if key.len() != 32 {
        return Err(PostUrbitError::InvalidInput("secrets key"));
    }
    let mut idx = SECRET_MAGIC.len() + 1;
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&data[idx..idx + NONCE_LEN]);
    idx += NONCE_LEN;
    let ciphertext = &data[idx..];
    let cipher = ChaCha20Poly1305::new(key.into());
    cipher
        .decrypt((&nonce).into(), ciphertext)
        .map_err(|_| PostUrbitError::Crypto("secrets decrypt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let key = [7u8; 32];
        let mut secrets = HashMap::new();
        secrets.insert("token".to_string(), "value".to_string());
        save_app_secrets(dir.path(), "app.id", &key, &secrets).unwrap();
        let loaded = load_app_secrets(dir.path(), "app.id", &key).unwrap();
        assert_eq!(loaded.get("token"), Some(&"value".to_string()));
    }
}

use std::path::Path;

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;

use crate::error::{PostUrbitError, Result};

const BACKUP_MAGIC: &[u8; 4] = b"PUSB";
const BACKUP_VERSION: u8 = 1;
const PBKDF2_ITERATIONS: u32 = 100_000;

pub fn create_backup(dir: &Path, passphrase: &str) -> Result<Vec<u8>> {
    let mut archive = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut archive);
        builder.append_dir_all(".", dir).map_err(|err| PostUrbitError::Io(err.to_string()))?;
        builder.finish().map_err(|err| PostUrbitError::Io(err.to_string()))?;
    }

    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &salt, PBKDF2_ITERATIONS, &mut key);

    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let cipher = ChaCha20Poly1305::new((&key).into());
    let ciphertext = cipher
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: &archive,
                aad: BACKUP_MAGIC,
            },
        )
        .map_err(|_| PostUrbitError::Crypto("backup encrypt"))?;

    let mut out = Vec::new();
    out.extend_from_slice(BACKUP_MAGIC);
    out.push(BACKUP_VERSION);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn restore_backup(data: &[u8], passphrase: &str, target_dir: &Path) -> Result<()> {
    if data.len() < 4 + 1 + 16 + 12 {
        return Err(PostUrbitError::InvalidInput("backup data"));
    }
    if &data[..4] != BACKUP_MAGIC {
        return Err(PostUrbitError::InvalidInput("backup magic"));
    }
    if data[4] != BACKUP_VERSION {
        return Err(PostUrbitError::InvalidInput("backup version"));
    }

    let mut idx = 5;
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&data[idx..idx + 16]);
    idx += 16;

    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&data[idx..idx + 12]);
    idx += 12;

    let ciphertext = &data[idx..];
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &salt, PBKDF2_ITERATIONS, &mut key);

    let cipher = ChaCha20Poly1305::new((&key).into());
    let plaintext = cipher
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: ciphertext,
                aad: BACKUP_MAGIC,
            },
        )
        .map_err(|_| PostUrbitError::Crypto("backup decrypt"))?;

    let mut archive = tar::Archive::new(plaintext.as_slice());
    archive
        .unpack(target_dir)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, b"hello").unwrap();

        let backup = create_backup(dir.path(), "password").unwrap();
        let restore_dir = tempfile::tempdir().unwrap();
        restore_backup(&backup, "password", restore_dir.path()).unwrap();

        let restored = std::fs::read(restore_dir.path().join("file.txt")).unwrap();
        assert_eq!(restored, b"hello");
    }

    #[test]
    fn backup_wrong_passphrase_fails() {
        let dir = tempfile::tempdir().unwrap();
        let backup = create_backup(dir.path(), "password").unwrap();
        let restore_dir = tempfile::tempdir().unwrap();
        let err = restore_backup(&backup, "wrong", restore_dir.path()).unwrap_err();
        assert!(matches!(err, PostUrbitError::Crypto(_)));
    }
}

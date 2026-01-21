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

    // Sanitize each entry path to prevent path traversal attacks
    let canonical_target = target_dir
        .canonicalize()
        .or_else(|_| {
            std::fs::create_dir_all(target_dir)?;
            target_dir.canonicalize()
        })
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;

    for entry in archive.entries().map_err(|err| PostUrbitError::Io(err.to_string()))? {
        let mut entry = entry.map_err(|err| PostUrbitError::Io(err.to_string()))?;
        let entry_path = entry.path().map_err(|err| PostUrbitError::Io(err.to_string()))?;

        // Reject absolute paths
        if entry_path.is_absolute() {
            return Err(PostUrbitError::InvalidInput("backup contains absolute path"));
        }

        // Reject paths with parent directory components
        for component in entry_path.components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(PostUrbitError::InvalidInput("backup contains path traversal"));
                }
                std::path::Component::Normal(_) | std::path::Component::CurDir => {}
                _ => {
                    return Err(PostUrbitError::InvalidInput("backup contains invalid path component"));
                }
            }
        }

        // Construct the final path and verify it's within target_dir
        let dest_path = canonical_target.join(&entry_path);
        let canonical_dest = if dest_path.exists() {
            dest_path.canonicalize().map_err(|err| PostUrbitError::Io(err.to_string()))?
        } else {
            // For non-existent paths, canonicalize the parent and append the filename
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| PostUrbitError::Io(err.to_string()))?;
                let canonical_parent = parent.canonicalize().map_err(|err| PostUrbitError::Io(err.to_string()))?;
                if let Some(file_name) = dest_path.file_name() {
                    canonical_parent.join(file_name)
                } else {
                    canonical_parent
                }
            } else {
                dest_path.clone()
            }
        };

        if !canonical_dest.starts_with(&canonical_target) {
            return Err(PostUrbitError::InvalidInput("backup entry escapes target directory"));
        }

        // Only allow regular files and directories; reject all special entry types
        // (symlinks, hardlinks, block/char devices, FIFOs) to prevent privilege escalation
        let entry_type = entry.header().entry_type();
        match entry_type {
            tar::EntryType::Regular | tar::EntryType::Directory => {
                // Allowed entry types
            }
            tar::EntryType::Symlink => {
                return Err(PostUrbitError::InvalidInput("backup contains symlink"));
            }
            tar::EntryType::Link => {
                return Err(PostUrbitError::InvalidInput("backup contains hardlink"));
            }
            _ => {
                return Err(PostUrbitError::InvalidInput("backup contains unsupported entry type"));
            }
        }

        entry.unpack(&dest_path).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    }

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

    #[test]
    fn backup_tamper_detects() {
        let dir = tempfile::tempdir().unwrap();
        let backup = create_backup(dir.path(), "password").unwrap();
        let mut tampered = backup.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let restore_dir = tempfile::tempdir().unwrap();
        let err = restore_backup(&tampered, "password", restore_dir.path()).unwrap_err();
        assert!(matches!(err, PostUrbitError::Crypto(_)));
    }

    #[test]
    fn backup_rejects_symlink_entry() {
        // Build a tar with a symlink entry
        let mut archive = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_path("malicious_symlink").unwrap();
            header.set_link_name("/etc/passwd").unwrap();
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
            builder.finish().unwrap();
        }

        // Encrypt it as a backup
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(b"password", &salt, PBKDF2_ITERATIONS, &mut key);
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
            .unwrap();

        let mut backup = Vec::new();
        backup.extend_from_slice(BACKUP_MAGIC);
        backup.push(BACKUP_VERSION);
        backup.extend_from_slice(&salt);
        backup.extend_from_slice(&nonce);
        backup.extend_from_slice(&ciphertext);

        // Verify restore_backup returns Err with "symlink" in the message
        let restore_dir = tempfile::tempdir().unwrap();
        let err = restore_backup(&backup, "password", restore_dir.path()).unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(err_msg.contains("symlink"), "Expected error containing 'symlink', got: {}", err_msg);
    }

    #[test]
    fn backup_rejects_hardlink_entry() {
        // Build a tar with a hardlink entry
        let mut archive = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Link);
            header.set_size(0);
            header.set_path("malicious_hardlink").unwrap();
            header.set_link_name("/etc/passwd").unwrap();
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
            builder.finish().unwrap();
        }

        // Encrypt it as a backup
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(b"password", &salt, PBKDF2_ITERATIONS, &mut key);
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
            .unwrap();

        let mut backup = Vec::new();
        backup.extend_from_slice(BACKUP_MAGIC);
        backup.push(BACKUP_VERSION);
        backup.extend_from_slice(&salt);
        backup.extend_from_slice(&nonce);
        backup.extend_from_slice(&ciphertext);

        // Verify restore_backup returns Err with "hardlink" in the message
        let restore_dir = tempfile::tempdir().unwrap();
        let err = restore_backup(&backup, "password", restore_dir.path()).unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(err_msg.contains("hardlink"), "Expected error containing 'hardlink', got: {}", err_msg);
    }

    #[test]
    fn backup_rejects_special_entry_types() {
        // Build a tar with a FIFO entry (special device type)
        let mut archive = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Fifo);
            header.set_size(0);
            header.set_path("malicious_fifo").unwrap();
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
            builder.finish().unwrap();
        }

        // Encrypt it as a backup
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(b"password", &salt, PBKDF2_ITERATIONS, &mut key);
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
            .unwrap();

        let mut backup = Vec::new();
        backup.extend_from_slice(BACKUP_MAGIC);
        backup.push(BACKUP_VERSION);
        backup.extend_from_slice(&salt);
        backup.extend_from_slice(&nonce);
        backup.extend_from_slice(&ciphertext);

        // Verify restore_backup returns Err with "unsupported" in the message
        let restore_dir = tempfile::tempdir().unwrap();
        let err = restore_backup(&backup, "password", restore_dir.path()).unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(err_msg.contains("unsupported"), "Expected error containing 'unsupported', got: {}", err_msg);
    }

    #[test]
    fn backup_restores_nested_paths() {
        // Create a directory with nested structure
        let dir = tempfile::tempdir().unwrap();
        let nested_dir = dir.path().join("nested").join("dir");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let file_path = nested_dir.join("file.txt");
        std::fs::write(&file_path, b"nested content").unwrap();

        // Create backup
        let backup = create_backup(dir.path(), "password").unwrap();

        // Restore to new directory
        let restore_dir = tempfile::tempdir().unwrap();
        restore_backup(&backup, "password", restore_dir.path()).unwrap();

        // Verify nested file was restored correctly
        let restored_path = restore_dir.path().join("nested").join("dir").join("file.txt");
        assert!(restored_path.exists(), "Nested file should exist at {:?}", restored_path);
        let restored_content = std::fs::read(&restored_path).unwrap();
        assert_eq!(restored_content, b"nested content");
    }
}

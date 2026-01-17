use hmac::{Hmac, Mac};
use argon2::password_hash::PasswordHash;
use argon2::PasswordVerifier;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::{PostUrbitError, Result};

pub type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub password_hash: Option<String>,
    pub admin_token_hash: Option<String>,
    pub session_secret: Vec<u8>,
    pub session_timeout_hours: u32,
}

pub fn hash_token(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(digest)
}

pub fn generate_token_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn verify_password(hash: &str, password: &str) -> Result<()> {
    let parsed = PasswordHash::new(hash)
        .map_err(|_| PostUrbitError::InvalidInput("password hash"))?;
    argon2::Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| PostUrbitError::InvalidInput("password"))?;
    Ok(())
}

pub fn create_session_cookie(session_id: &str, secret: &[u8]) -> Result<String> {
    let signature = hmac_sign(session_id, secret)?;
    Ok(format!("{session_id}.{signature}"))
}

pub fn verify_session_cookie(value: &str, secret: &[u8]) -> Result<String> {
    let mut parts = value.splitn(2, '.');
    let id = parts.next().ok_or(PostUrbitError::InvalidInput("session cookie"))?;
    let sig = parts.next().ok_or(PostUrbitError::InvalidInput("session cookie"))?;
    let expected = hmac_sign(id, secret)?;
    if !constant_time_eq(sig.as_bytes(), expected.as_bytes()) {
        return Err(PostUrbitError::InvalidInput("session cookie"));
    }
    Ok(id.to_string())
}

fn hmac_sign(value: &str, secret: &[u8]) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|_| PostUrbitError::InvalidInput("session secret"))?;
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut out = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        out |= x ^ y;
    }
    out == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_round_trip() {
        let secret = vec![42u8; 32];
        let cookie = create_session_cookie("abc", &secret).unwrap();
        let id = verify_session_cookie(&cookie, &secret).unwrap();
        assert_eq!(id, "abc");
    }

    #[test]
    fn session_cookie_rejects_tamper() {
        let secret = vec![42u8; 32];
        let cookie = create_session_cookie("abc", &secret).unwrap();
        let tampered = format!("{}0", cookie);
        let err = verify_session_cookie(&tampered, &secret).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }
}

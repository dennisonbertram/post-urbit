use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::canonical_json::canonical_json_from;
use crate::encoding::{base64_decode, base64_encode, crockford_base32_decode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};

const MAILBOX_DOMAIN: &[u8] = b"post-urbit-mailbox-token-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxToken {
    pub iid: String,
    pub mailbox_url: String,
    pub expires_at: String,
    pub nonce: String,
    pub signature: String,
}

pub fn create_mailbox_token(
    iid: &str,
    mailbox_url: &str,
    expires_at: DateTime<Utc>,
    nonce: [u8; 16],
    signing_key: &SigningKey,
) -> Result<String> {
    validate_crockford_base32_lower(iid)?;
    let mailbox_url = canonicalize_mailbox_url(mailbox_url)?;
    let expires_at_str = expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let signature = mailbox_signature(
        iid,
        &mailbox_url,
        &expires_at_str,
        &nonce,
        signing_key,
    )?;

    let token = MailboxToken {
        iid: iid.to_string(),
        mailbox_url,
        expires_at: expires_at_str,
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        signature,
    };

    let canonical = canonical_json_from(&token)?;
    Ok(URL_SAFE_NO_PAD.encode(canonical.as_bytes()))
}

pub fn verify_mailbox_token(token_b64: &str, signing_keys: &[String]) -> Result<MailboxToken> {
    let token_bytes = URL_SAFE_NO_PAD
        .decode(token_b64.as_bytes())
        .map_err(|_| PostUrbitError::InvalidEncoding("token base64url"))?;
    let token: MailboxToken = serde_json::from_slice(&token_bytes)
        .map_err(|_| PostUrbitError::InvalidInput("token json"))?;

    validate_crockford_base32_lower(&token.iid)?;
    let mailbox_url = canonicalize_mailbox_url(&token.mailbox_url)?;
    if mailbox_url != token.mailbox_url {
        return Err(PostUrbitError::InvalidInput("mailbox url canonicalization"));
    }

    let nonce = URL_SAFE_NO_PAD
        .decode(token.nonce.as_bytes())
        .map_err(|_| PostUrbitError::InvalidEncoding("token nonce"))?;
    let nonce: [u8; 16] = nonce
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("token nonce length"))?;

    verify_mailbox_signature(&token, &nonce, signing_keys)?;
    Ok(token)
}

fn mailbox_signature(
    iid: &str,
    mailbox_url: &str,
    expires_at: &str,
    nonce: &[u8; 16],
    signing_key: &SigningKey,
) -> Result<String> {
    let signature_input = mailbox_signature_input(iid, mailbox_url, expires_at, nonce)?;
    let digest = Sha256::digest(&signature_input);
    let signature: Signature = signing_key.sign(&digest);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

fn verify_mailbox_signature(
    token: &MailboxToken,
    nonce: &[u8; 16],
    signing_keys: &[String],
) -> Result<()> {
    let signature_input = mailbox_signature_input(
        &token.iid,
        &token.mailbox_url,
        &token.expires_at,
        nonce,
    )?;
    let digest = Sha256::digest(&signature_input);

    let signature_bytes = base64_decode(&token.signature)?;
    let signature = Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );
    for key in signing_keys {
        let key_bytes = base64_decode(key)?;
        if key_bytes.len() != 32 {
            continue;
        }
        let verifying_key = VerifyingKey::from_bytes(
            key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| PostUrbitError::InvalidInput("signing key length"))?,
        )
        .map_err(|_| PostUrbitError::InvalidInput("signing key parse"))?;
        if verifying_key.verify_strict(&digest, &signature).is_ok() {
            return Ok(());
        }
    }
    Err(PostUrbitError::Crypto("mailbox signature invalid"))
}

fn mailbox_signature_input(
    iid: &str,
    mailbox_url: &str,
    expires_at: &str,
    nonce: &[u8; 16],
) -> Result<Vec<u8>> {
    let iid_raw = crockford_base32_decode(iid)?;
    if iid_raw.len() != 20 {
        return Err(PostUrbitError::InvalidInput("iid decode"));
    }

    let mut out = Vec::new();
    out.extend_from_slice(MAILBOX_DOMAIN);
    out.extend_from_slice(&iid_raw);
    out.extend_from_slice(mailbox_url.as_bytes());
    out.extend_from_slice(expires_at.as_bytes());
    out.extend_from_slice(nonce);
    Ok(out)
}

pub fn canonicalize_mailbox_url(input: &str) -> Result<String> {
    let url = Url::parse(input).map_err(|_| PostUrbitError::InvalidInput("mailbox url"))?;
    if url.scheme() != "https" {
        return Err(PostUrbitError::InvalidInput("mailbox url scheme"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(PostUrbitError::InvalidInput("mailbox url userinfo"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(PostUrbitError::InvalidInput("mailbox url query"));
    }
    let host = url.host_str().ok_or(PostUrbitError::InvalidInput("mailbox url host"))?;
    if !host.is_ascii() {
        return Err(PostUrbitError::InvalidInput("mailbox url host ascii"));
    }
    let host = host.to_ascii_lowercase();

    let mut path = url.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    if path.len() > 1 {
        while path.ends_with('/') {
            path.pop();
        }
        if path.is_empty() {
            path.push('/');
        }
    }

    let mut out = String::new();
    out.push_str("https://");
    if host.contains(':') && !host.starts_with('[') {
        out.push('[');
        out.push_str(&host);
        out.push(']');
    } else {
        out.push_str(&host);
    }
    if let Some(port) = url.port() {
        if port != 443 {
            out.push_str(&format!(":{port}"));
        }
    }
    out.push_str(&path);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_mailbox_url_basic() {
        let url = canonicalize_mailbox_url("HTTPS://Mailbox.Example.COM:443/").unwrap();
        assert_eq!(url, "https://mailbox.example.com/");
    }
}

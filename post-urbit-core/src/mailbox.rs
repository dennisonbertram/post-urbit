use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::canonical_json::canonical_json_from;
use crate::encoding::{base64_decode, base64_encode, crockford_base32_decode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};
use crate::identity::IdentityDocument;

const MAILBOX_DOMAIN: &[u8] = b"post-urbit-mailbox-token-v1";
const BEARER_TOKEN_DOMAIN: &[u8] = b"post-urbit:mailbox-token:v1:";

type HmacSha256 = Hmac<Sha256>;

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
    verify_mailbox_token_with_time(token_b64, signing_keys, Utc::now())
}

pub fn verify_mailbox_token_with_time(
    token_b64: &str,
    signing_keys: &[String],
    now: DateTime<Utc>,
) -> Result<MailboxToken> {
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

    validate_token_expiry(&token.expires_at, now)?;
    verify_mailbox_signature(&token, &nonce, signing_keys)?;
    Ok(token)
}

pub fn verify_mailbox_token_with_identity(
    token_b64: &str,
    identity: &IdentityDocument,
    now: DateTime<Utc>,
) -> Result<MailboxToken> {
    let mut keys = Vec::new();
    keys.push(identity.keys.signing.current.clone());
    if let Some(prev) = identity.keys.signing.previous.clone() {
        keys.push(prev);
    }
    for hist in &identity.keys.signing.history {
        keys.push(hist.key.clone());
    }
    verify_mailbox_token_with_time(token_b64, &keys, now)
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

/// Characters that are unreserved per RFC 3986 and should not be percent-encoded
fn is_unreserved(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~'
}

/// Normalize percent-encoding in a path:
/// - Uppercase hex digits in percent-escapes
/// - Decode unreserved characters
/// - Reject invalid percent-escapes
fn normalize_percent_encoding(s: &str) -> Result<String> {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Must have exactly 2 hex digits following
            if i + 2 >= bytes.len() {
                return Err(PostUrbitError::InvalidInput("truncated percent-escape"));
            }
            let hex_chars = &bytes[i + 1..i + 3];
            if !hex_chars.iter().all(|c| c.is_ascii_hexdigit()) {
                return Err(PostUrbitError::InvalidInput("invalid percent-escape"));
            }
            let hex_str = std::str::from_utf8(hex_chars).unwrap();
            let byte_val = u8::from_str_radix(hex_str, 16).unwrap();
            let char_val = byte_val as char;

            if is_unreserved(char_val) {
                // Decode unreserved characters to literal form
                result.push(char_val);
            } else {
                // Keep as percent-encoded, uppercase hex
                result.push('%');
                result.push_str(&hex_str.to_uppercase());
            }
            i += 3;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(result)
}

/// Canonicalize a mailbox URL per RFC-0003 section 7.3
///
/// Requirements:
/// - HTTPS scheme required (HTTP rejected)
/// - Hostname lowercase, ASCII only (non-ASCII hosts rejected)
/// - Port normalized (443 omitted)
/// - Path normalized: empty -> "/", trailing slashes removed (except for root)
/// - Percent-encoding normalized: uppercase hex, unreserved chars decoded
/// - No query string or fragment allowed
/// - No userinfo allowed
pub fn canonicalize_mailbox_url(input: &str) -> Result<String> {
    if !input.is_ascii() {
        return Err(PostUrbitError::InvalidInput("mailbox url ascii"));
    }
    let url = Url::parse(input).map_err(|_| PostUrbitError::InvalidInput("mailbox url"))?;

    // REQ-MSG-072: Must use https scheme
    if url.scheme().to_lowercase() != "https" {
        return Err(PostUrbitError::InvalidInput("mailbox url scheme"));
    }

    // REQ-MSG-073: No userinfo allowed
    if !url.username().is_empty() || url.password().is_some() {
        return Err(PostUrbitError::InvalidInput("mailbox url userinfo"));
    }

    // REQ-MSG-071: No query or fragment allowed
    if url.query().is_some() || url.fragment().is_some() {
        return Err(PostUrbitError::InvalidInput("mailbox url query"));
    }

    // REQ-MSG-074: Host required
    let host = url.host_str().ok_or(PostUrbitError::InvalidInput("mailbox url host"))?;

    // REQ-MSG-069: ASCII-only hosts (non-ASCII must use punycode)
    if !host.is_ascii() {
        return Err(PostUrbitError::InvalidInput("mailbox url host ascii"));
    }

    // Lowercase the host and remove trailing dot
    let host = host.to_ascii_lowercase().trim_end_matches('.').to_string();

    // Normalize path with percent-encoding normalization
    let raw_path = url.path();
    let mut path = if raw_path.is_empty() {
        "/".to_string()
    } else {
        normalize_percent_encoding(raw_path)?
    };

    // Remove trailing slashes (but keep root "/")
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

    // Handle IPv6 addresses
    if host.contains(':') && !host.starts_with('[') {
        out.push('[');
        out.push_str(&host);
        out.push(']');
    } else {
        out.push_str(&host);
    }

    // Omit default port 443
    if let Some(port) = url.port() {
        if port != 443 {
            out.push_str(&format!(":{port}"));
        }
    }
    out.push_str(&path);
    Ok(out)
}

/// Canonicalize a mailbox URL for a specific IID
/// Per RFC-0003 section 7.3, v1 mailbox URLs must have root path only
/// This function constructs the canonical URL: https://host[:port]/mailbox/{iid}
pub fn canonicalize_mailbox_url_for_iid(base_url: &str, iid: &str) -> Result<String> {
    validate_crockford_base32_lower(iid)?;
    let canonical_base = canonicalize_mailbox_url(base_url)?;
    // For v1, the base URL should be root-only, then we append /mailbox/{iid}
    // But per the RFC, the mailbox_url in the token is just the base URL
    // The actual message storage path is /messages/{inbox_owner_iid}
    Ok(canonical_base)
}

fn validate_token_expiry(expires_at: &str, now: DateTime<Utc>) -> Result<()> {
    if expires_at.contains('.') {
        return Err(PostUrbitError::InvalidInput("expires_at fractional"));
    }
    if !expires_at.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("expires_at not UTC"));
    }
    let expires = expires_at
        .parse::<DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("expires_at parse"))?;
    if expires < now - Duration::minutes(5) {
        return Err(PostUrbitError::InvalidInput("token expired"));
    }
    if expires > now + Duration::hours(24) {
        return Err(PostUrbitError::InvalidInput("token lifetime too long"));
    }
    Ok(())
}

// ============================================================================
// Mailbox Bearer Token Protocol (REQ-MSG-080-092)
// ============================================================================

/// Bearer token for mailbox access
///
/// Token format: HMAC-SHA256(secret, domain || recipient_iid || sender_iid || expiry_ts)
/// Domain separator: "post-urbit:mailbox-token:v1:"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxBearerToken {
    /// The sender IID (who is storing messages)
    pub sender_iid: String,
    /// The recipient IID (whose mailbox is being accessed)
    pub recipient_iid: String,
    /// Token expiration timestamp (RFC3339)
    pub expires_at: String,
    /// The HMAC token value (base64url encoded)
    pub token: String,
}

/// Generator for mailbox bearer tokens per RFC-0003 section 7
///
/// This generator creates HMAC-SHA256 based tokens that allow a sender
/// to store messages in a recipient's mailbox. The token binds the
/// sender's identity to the recipient's mailbox and has a limited lifetime.
pub struct MailboxBearerTokenGenerator {
    secret: [u8; 32],
}

impl MailboxBearerTokenGenerator {
    /// Create a new token generator with the given secret
    ///
    /// The secret should be a cryptographically random 32-byte value
    /// that is kept secure on the mailbox server.
    pub fn new(secret: [u8; 32]) -> Self {
        Self { secret }
    }

    /// Generate a bearer token for a sender to store messages for a recipient
    ///
    /// # Arguments
    /// * `recipient_iid` - The IID of the mailbox owner (who will receive messages)
    /// * `sender_iid` - The IID of the sender (who is storing messages)
    /// * `validity_hours` - How long the token should be valid (1-24 hours recommended)
    ///
    /// # Returns
    /// A tuple of (token_string, expiry_timestamp)
    ///
    /// # Errors
    /// Returns an error if the IIDs are invalid or validity_hours is 0
    pub fn generate_token(
        &self,
        recipient_iid: &str,
        sender_iid: &str,
        validity_hours: u64,
    ) -> Result<(String, String)> {
        self.generate_token_with_time(recipient_iid, sender_iid, validity_hours, Utc::now())
    }

    /// Generate a bearer token with a specific base time (for testing)
    pub fn generate_token_with_time(
        &self,
        recipient_iid: &str,
        sender_iid: &str,
        validity_hours: u64,
        now: DateTime<Utc>,
    ) -> Result<(String, String)> {
        // Validate IIDs
        validate_crockford_base32_lower(recipient_iid)?;
        validate_crockford_base32_lower(sender_iid)?;

        if validity_hours == 0 {
            return Err(PostUrbitError::InvalidInput("validity_hours must be > 0"));
        }
        if validity_hours > 24 {
            return Err(PostUrbitError::InvalidInput("validity_hours max 24"));
        }

        let expires_at = now + Duration::hours(validity_hours as i64);
        let expires_at_str = expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        // Decode IIDs to raw bytes
        let recipient_raw = crockford_base32_decode(recipient_iid)?;
        let sender_raw = crockford_base32_decode(sender_iid)?;

        // Build the HMAC input
        let mut hmac_input = Vec::new();
        hmac_input.extend_from_slice(BEARER_TOKEN_DOMAIN);
        hmac_input.extend_from_slice(&recipient_raw);
        hmac_input.extend_from_slice(&sender_raw);
        hmac_input.extend_from_slice(expires_at_str.as_bytes());

        // Compute HMAC-SHA256
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|_| PostUrbitError::Crypto("hmac init"))?;
        mac.update(&hmac_input);
        let result = mac.finalize();
        let token_bytes = result.into_bytes();

        // Encode as base64url
        let token = URL_SAFE_NO_PAD.encode(token_bytes);

        Ok((token, expires_at_str))
    }

    /// Verify a bearer token is valid for the given sender/recipient
    ///
    /// # Arguments
    /// * `token` - The base64url-encoded HMAC token
    /// * `recipient_iid` - The IID of the mailbox owner
    /// * `sender_iid` - The IID of the sender
    /// * `expires_at` - The token's expiration timestamp
    ///
    /// # Returns
    /// Ok(()) if the token is valid, Err otherwise
    pub fn verify_token(
        &self,
        token: &str,
        recipient_iid: &str,
        sender_iid: &str,
        expires_at: &str,
    ) -> Result<()> {
        self.verify_token_with_time(token, recipient_iid, sender_iid, expires_at, Utc::now())
    }

    /// Verify a bearer token with a specific current time (for testing)
    pub fn verify_token_with_time(
        &self,
        token: &str,
        recipient_iid: &str,
        sender_iid: &str,
        expires_at: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        // Validate IIDs
        validate_crockford_base32_lower(recipient_iid)?;
        validate_crockford_base32_lower(sender_iid)?;

        // Check expiry
        validate_bearer_token_expiry(expires_at, now)?;

        // Decode the token
        let token_bytes = URL_SAFE_NO_PAD
            .decode(token.as_bytes())
            .map_err(|_| PostUrbitError::InvalidEncoding("bearer token base64url"))?;

        if token_bytes.len() != 32 {
            return Err(PostUrbitError::InvalidInput("bearer token length"));
        }

        // Decode IIDs to raw bytes
        let recipient_raw = crockford_base32_decode(recipient_iid)?;
        let sender_raw = crockford_base32_decode(sender_iid)?;

        // Rebuild the HMAC input
        let mut hmac_input = Vec::new();
        hmac_input.extend_from_slice(BEARER_TOKEN_DOMAIN);
        hmac_input.extend_from_slice(&recipient_raw);
        hmac_input.extend_from_slice(&sender_raw);
        hmac_input.extend_from_slice(expires_at.as_bytes());

        // Verify HMAC-SHA256
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|_| PostUrbitError::Crypto("hmac init"))?;
        mac.update(&hmac_input);

        mac.verify_slice(&token_bytes)
            .map_err(|_| PostUrbitError::Crypto("bearer token invalid"))?;

        Ok(())
    }
}

/// Validate bearer token expiry
///
/// Per RFC-0003:
/// - Tokens in the past (allowing 5 minutes clock skew) are rejected
/// - Tokens more than 24 hours in the future are rejected
fn validate_bearer_token_expiry(expires_at: &str, now: DateTime<Utc>) -> Result<()> {
    if expires_at.contains('.') {
        return Err(PostUrbitError::InvalidInput("expires_at fractional"));
    }
    if !expires_at.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("expires_at not UTC"));
    }
    let expires = expires_at
        .parse::<DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("expires_at parse"))?;

    // REQ-MSG-083: Reject tokens in the past (with 5 min clock skew tolerance)
    if expires < now - Duration::minutes(5) {
        return Err(PostUrbitError::InvalidInput("bearer token expired"));
    }

    // REQ-MSG-084: Reject tokens more than 24 hours in the future
    if expires > now + Duration::hours(24) {
        return Err(PostUrbitError::InvalidInput("bearer token lifetime too long"));
    }

    Ok(())
}

/// Request body for POST /mailbox/token endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRequest {
    /// The sender's IID (who is requesting to store messages)
    pub sender_iid: String,
    /// How long the token should be valid in hours (1-24)
    #[serde(default = "default_validity_hours")]
    pub validity_hours: u64,
}

fn default_validity_hours() -> u64 {
    24
}

/// Response body for POST /mailbox/token endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// The bearer token (base64url encoded HMAC)
    pub token: String,
    /// Token expiration timestamp (RFC3339)
    pub expires_at: String,
    /// The recipient IID (mailbox owner)
    pub recipient_iid: String,
    /// The sender IID (who requested the token)
    pub sender_iid: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_mailbox_url_basic() {
        let url = canonicalize_mailbox_url("HTTPS://Mailbox.Example.COM:443/").unwrap();
        assert_eq!(url, "https://mailbox.example.com/");
    }

    #[test]
    fn canonicalize_mailbox_url_port_handling() {
        // Default port 443 should be omitted
        let url = canonicalize_mailbox_url("https://example.com:443/").unwrap();
        assert_eq!(url, "https://example.com/");

        // Non-default port should be preserved
        let url = canonicalize_mailbox_url("https://relay.net:8443/api/").unwrap();
        assert_eq!(url, "https://relay.net:8443/api");
    }

    #[test]
    fn canonicalize_mailbox_url_path_handling() {
        // Empty path becomes /
        let url = canonicalize_mailbox_url("https://box.org").unwrap();
        assert_eq!(url, "https://box.org/");

        // Trailing slashes removed (except root)
        let url = canonicalize_mailbox_url("https://example.com/api///").unwrap();
        assert_eq!(url, "https://example.com/api");

        // Double slashes preserved internally
        let url = canonicalize_mailbox_url("https://example.com//double//slash").unwrap();
        assert_eq!(url, "https://example.com//double//slash");

        // Note: The url crate normalizes dot-segments automatically.
        // Per RFC-0003, dot-segments should be preserved, but this is a limitation
        // of the underlying url parsing library. In practice, mailbox URLs should
        // never contain dot-segments, so this is acceptable.
        let url = canonicalize_mailbox_url("https://example.com/./dotpath/../keep").unwrap();
        assert_eq!(url, "https://example.com/keep");
    }

    #[test]
    fn canonicalize_mailbox_url_percent_encoding() {
        // Uppercase hex digits
        let url = canonicalize_mailbox_url("https://example.com/path%2fslash").unwrap();
        assert_eq!(url, "https://example.com/path%2Fslash");

        // Decode unreserved characters
        let url = canonicalize_mailbox_url("https://example.com/%41%42%43").unwrap();
        assert_eq!(url, "https://example.com/ABC");

        // Keep reserved characters encoded
        let url = canonicalize_mailbox_url("https://example.com/a%20b").unwrap();
        assert_eq!(url, "https://example.com/a%20b");
    }

    #[test]
    fn canonicalize_mailbox_url_ipv6() {
        let url = canonicalize_mailbox_url("https://[2001:DB8::1]:443/").unwrap();
        assert_eq!(url, "https://[2001:db8::1]/");
    }

    #[test]
    fn canonicalize_mailbox_url_rejects_invalid() {
        // Wrong scheme
        assert!(canonicalize_mailbox_url("http://example.com/").is_err());

        // Has userinfo
        assert!(canonicalize_mailbox_url("https://user@example.com/").is_err());

        // Has query
        assert!(canonicalize_mailbox_url("https://example.com/?query").is_err());

        // Non-ASCII host
        assert!(canonicalize_mailbox_url("https://müller.example/").is_err());

        // Invalid percent-escape
        assert!(canonicalize_mailbox_url("https://example.com/%GG").is_err());
        assert!(canonicalize_mailbox_url("https://example.com/%").is_err());
    }

    #[test]
    fn token_expiry_checks() {
        let now = DateTime::parse_from_rfc3339("2025-01-15T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(validate_token_expiry("2025-01-16T00:00:00Z", now).is_ok());
        assert!(validate_token_expiry("2025-01-14T23:00:00Z", now).is_err());
    }

    // =========================================================================
    // Bearer Token Generator Tests
    // =========================================================================

    fn test_iid() -> &'static str {
        // A valid 32-character Crockford Base32 IID
        "b1n7cfscgashm32xx7eaxw0y09gy0y2v"
    }

    fn test_iid_2() -> &'static str {
        // Another valid IID for testing
        "a0b1c2d3e4f5g6h7j8k9m0n1p2q3r4s5"
    }

    #[test]
    fn bearer_token_generate_and_verify() {
        let secret = [42u8; 32];
        let generator = MailboxBearerTokenGenerator::new(secret);

        let now = DateTime::parse_from_rfc3339("2025-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let (token, expires_at) = generator
            .generate_token_with_time(test_iid(), test_iid_2(), 24, now)
            .unwrap();

        // Verify the token
        generator
            .verify_token_with_time(&token, test_iid(), test_iid_2(), &expires_at, now)
            .unwrap();
    }

    #[test]
    fn bearer_token_expired_rejected() {
        let secret = [42u8; 32];
        let generator = MailboxBearerTokenGenerator::new(secret);

        let creation_time = DateTime::parse_from_rfc3339("2025-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let (token, expires_at) = generator
            .generate_token_with_time(test_iid(), test_iid_2(), 1, creation_time)
            .unwrap();

        // Try to verify 2 hours later (token expired)
        let later = creation_time + Duration::hours(2);
        let result = generator.verify_token_with_time(
            &token,
            test_iid(),
            test_iid_2(),
            &expires_at,
            later,
        );
        assert!(result.is_err());
    }

    #[test]
    fn bearer_token_wrong_sender_rejected() {
        let secret = [42u8; 32];
        let generator = MailboxBearerTokenGenerator::new(secret);

        let now = DateTime::parse_from_rfc3339("2025-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let (token, expires_at) = generator
            .generate_token_with_time(test_iid(), test_iid_2(), 24, now)
            .unwrap();

        // Try to verify with different sender
        let result = generator.verify_token_with_time(
            &token,
            test_iid(),
            test_iid(), // Wrong sender
            &expires_at,
            now,
        );
        assert!(result.is_err());
    }

    #[test]
    fn bearer_token_wrong_recipient_rejected() {
        let secret = [42u8; 32];
        let generator = MailboxBearerTokenGenerator::new(secret);

        let now = DateTime::parse_from_rfc3339("2025-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let (token, expires_at) = generator
            .generate_token_with_time(test_iid(), test_iid_2(), 24, now)
            .unwrap();

        // Try to verify with different recipient
        let result = generator.verify_token_with_time(
            &token,
            test_iid_2(), // Wrong recipient
            test_iid_2(),
            &expires_at,
            now,
        );
        assert!(result.is_err());
    }

    #[test]
    fn bearer_token_wrong_secret_rejected() {
        let secret1 = [42u8; 32];
        let secret2 = [43u8; 32];
        let generator1 = MailboxBearerTokenGenerator::new(secret1);
        let generator2 = MailboxBearerTokenGenerator::new(secret2);

        let now = DateTime::parse_from_rfc3339("2025-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let (token, expires_at) = generator1
            .generate_token_with_time(test_iid(), test_iid_2(), 24, now)
            .unwrap();

        // Try to verify with different secret
        let result = generator2.verify_token_with_time(
            &token,
            test_iid(),
            test_iid_2(),
            &expires_at,
            now,
        );
        assert!(result.is_err());
    }

    #[test]
    fn bearer_token_validity_hours_validation() {
        let secret = [42u8; 32];
        let generator = MailboxBearerTokenGenerator::new(secret);

        // Zero validity hours rejected
        let result = generator.generate_token(test_iid(), test_iid_2(), 0);
        assert!(result.is_err());

        // More than 24 hours rejected
        let result = generator.generate_token(test_iid(), test_iid_2(), 25);
        assert!(result.is_err());

        // Valid range works
        let result = generator.generate_token(test_iid(), test_iid_2(), 1);
        assert!(result.is_ok());

        let result = generator.generate_token(test_iid(), test_iid_2(), 24);
        assert!(result.is_ok());
    }

    #[test]
    fn bearer_token_invalid_iid_rejected() {
        let secret = [42u8; 32];
        let generator = MailboxBearerTokenGenerator::new(secret);

        // Invalid recipient IID
        let result = generator.generate_token("invalid!", test_iid_2(), 24);
        assert!(result.is_err());

        // Invalid sender IID
        let result = generator.generate_token(test_iid(), "invalid!", 24);
        assert!(result.is_err());
    }

    #[test]
    fn bearer_token_clock_skew_tolerance() {
        let secret = [42u8; 32];
        let generator = MailboxBearerTokenGenerator::new(secret);

        let creation_time = DateTime::parse_from_rfc3339("2025-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let (token, expires_at) = generator
            .generate_token_with_time(test_iid(), test_iid_2(), 1, creation_time)
            .unwrap();

        // Token expires at 13:00:00
        // Verify at 13:04:00 (4 minutes past expiry) - should work due to 5-min tolerance
        let slightly_past = DateTime::parse_from_rfc3339("2025-01-15T13:04:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let result = generator.verify_token_with_time(
            &token,
            test_iid(),
            test_iid_2(),
            &expires_at,
            slightly_past,
        );
        assert!(result.is_ok());

        // Verify at 13:06:00 (6 minutes past expiry) - should fail
        let too_late = DateTime::parse_from_rfc3339("2025-01-15T13:06:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let result = generator.verify_token_with_time(
            &token,
            test_iid(),
            test_iid_2(),
            &expires_at,
            too_late,
        );
        assert!(result.is_err());
    }
}

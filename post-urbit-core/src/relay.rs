use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};
use sha2::{Digest, Sha256};

use crate::encoding::{base64_decode, base64_encode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};

const PURL_MAGIC: &[u8; 4] = b"PURL";
const PURL_VERSION: u8 = 1;
const RELAY_ALLOC_DOMAIN: &[u8] = b"post-urbit-relay-alloc-v1";
const RELAY_REBIND_DOMAIN: &[u8] = b"post-urbit-rebind-v1";
const PURL_MAX_PAYLOAD: usize = 1200;

pub const PURL_TYPE_DATA: u8 = 0x01;
pub const PURL_TYPE_PING: u8 = 0x02;
pub const PURL_TYPE_PONG: u8 = 0x03;
pub const PURL_TYPE_REFRESH: u8 = 0x05;
pub const PURL_TYPE_RELEASE: u8 = 0x06;
pub const PURL_TYPE_ERROR: u8 = 0x07;
pub const PURL_TYPE_REBIND: u8 = 0x08;
pub const PURL_TYPE_COORDINATE: u8 = 0x09;

#[derive(Debug, Clone)]
pub struct PurlPacket {
    pub packet_type: u8,
    pub allocation_token: [u8; 16],
    pub destination_iid: [u8; 20],
    pub payload: Vec<u8>,
}

pub fn encode_purl(packet: &PurlPacket) -> Result<Vec<u8>> {
    validate_purl_packet(packet)?;
    let len: u16 = packet
        .payload
        .len()
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("purl payload length"))?;
    let mut out = Vec::with_capacity(4 + 1 + 1 + 16 + 20 + 2 + packet.payload.len());
    out.extend_from_slice(PURL_MAGIC);
    out.push(PURL_VERSION);
    out.push(packet.packet_type);
    out.extend_from_slice(&packet.allocation_token);
    out.extend_from_slice(&packet.destination_iid);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&packet.payload);
    Ok(out)
}

pub fn decode_purl(bytes: &[u8]) -> Result<PurlPacket> {
    if bytes.len() < 4 + 1 + 1 + 16 + 20 + 2 {
        return Err(PostUrbitError::InvalidInput("purl length"));
    }
    if &bytes[..4] != PURL_MAGIC {
        return Err(PostUrbitError::InvalidInput("purl magic"));
    }
    if bytes[4] != PURL_VERSION {
        return Err(PostUrbitError::InvalidInput("purl version"));
    }
    let packet_type = bytes[5];
    validate_purl_type(packet_type)?;
    let mut idx = 6;

    let mut allocation_token = [0u8; 16];
    allocation_token.copy_from_slice(&bytes[idx..idx + 16]);
    idx += 16;

    let mut destination_iid = [0u8; 20];
    destination_iid.copy_from_slice(&bytes[idx..idx + 20]);
    idx += 20;

    let payload_len = u16::from_be_bytes([bytes[idx], bytes[idx + 1]]) as usize;
    idx += 2;
    if payload_len > PURL_MAX_PAYLOAD {
        return Err(PostUrbitError::InvalidInput("purl payload too large"));
    }
    if bytes.len() != idx + payload_len {
        return Err(PostUrbitError::InvalidInput("purl payload length"));
    }
    let payload = bytes[idx..].to_vec();

    let packet = PurlPacket {
        packet_type,
        allocation_token,
        destination_iid,
        payload,
    };
    validate_purl_packet(&packet)?;
    Ok(packet)
}

pub fn sign_relay_allocation(
    signing_key: &SigningKey,
    iid: &str,
    lifetime: u32,
    timestamp: &str,
    nonce: &[u8; 16],
) -> Result<String> {
    let payload = relay_allocation_signature_input(iid, lifetime, timestamp, nonce)?;
    let digest = Sha256::digest(&payload);
    let signature: Signature = signing_key.sign(&digest);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

pub fn verify_relay_allocation(
    signature_base64: &str,
    signing_key_base64: &str,
    iid: &str,
    lifetime: u32,
    timestamp: &str,
    nonce: &[u8; 16],
) -> Result<()> {
    let payload = relay_allocation_signature_input(iid, lifetime, timestamp, nonce)?;
    let digest = Sha256::digest(&payload);
    let signature_bytes = base64_decode(signature_base64)?;
    if signature_bytes.len() != 64 {
        return Err(PostUrbitError::InvalidInput("signature length"));
    }
    let signature = Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );
    let key_bytes = base64_decode(signing_key_base64)?;
    if key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("signing key length"));
    }
    let verifying_key = VerifyingKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signing key length"))?,
    )
    .map_err(|_| PostUrbitError::InvalidInput("signing key parse"))?;
    verifying_key
        .verify_strict(&digest, &signature)
        .map_err(|_| PostUrbitError::Crypto("relay allocation signature invalid"))
}

pub fn sign_relay_rebind(
    signing_key: &SigningKey,
    allocation_id: &str,
    token_base64url: &str,
    timestamp: &str,
) -> Result<String> {
    let payload = relay_rebind_signature_input(allocation_id, token_base64url, timestamp)?;
    let digest = Sha256::digest(&payload);
    let signature: Signature = signing_key.sign(&digest);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

pub fn verify_relay_rebind(
    signature_base64: &str,
    signing_key_base64: &str,
    allocation_id: &str,
    token_base64url: &str,
    timestamp: &str,
) -> Result<()> {
    let payload = relay_rebind_signature_input(allocation_id, token_base64url, timestamp)?;
    let digest = Sha256::digest(&payload);
    let signature_bytes = base64_decode(signature_base64)?;
    if signature_bytes.len() != 64 {
        return Err(PostUrbitError::InvalidInput("signature length"));
    }
    let signature = Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );
    let key_bytes = base64_decode(signing_key_base64)?;
    if key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("signing key length"));
    }
    let verifying_key = VerifyingKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signing key length"))?,
    )
    .map_err(|_| PostUrbitError::InvalidInput("signing key parse"))?;
    verifying_key
        .verify_strict(&digest, &signature)
        .map_err(|_| PostUrbitError::Crypto("relay rebind signature invalid"))
}

fn relay_allocation_signature_input(
    iid: &str,
    lifetime: u32,
    timestamp: &str,
    nonce: &[u8; 16],
) -> Result<Vec<u8>> {
    validate_crockford_base32_lower(iid)?;
    validate_timestamp(timestamp)?;
    let mut out = Vec::with_capacity(
        RELAY_ALLOC_DOMAIN.len() + iid.len() + 4 + timestamp.len() + nonce.len(),
    );
    out.extend_from_slice(RELAY_ALLOC_DOMAIN);
    out.extend_from_slice(iid.as_bytes());
    out.extend_from_slice(&lifetime.to_be_bytes());
    out.extend_from_slice(timestamp.as_bytes());
    out.extend_from_slice(nonce);
    Ok(out)
}

fn relay_rebind_signature_input(
    allocation_id: &str,
    token_base64url: &str,
    timestamp: &str,
) -> Result<Vec<u8>> {
    validate_timestamp(timestamp)?;
    let token = URL_SAFE_NO_PAD
        .decode(token_base64url.as_bytes())
        .map_err(|_| PostUrbitError::InvalidEncoding("token base64url"))?;
    if token.len() != 16 {
        return Err(PostUrbitError::InvalidInput("token length"));
    }
    let mut out = Vec::with_capacity(
        RELAY_REBIND_DOMAIN.len() + allocation_id.len() + token.len() + timestamp.len(),
    );
    out.extend_from_slice(RELAY_REBIND_DOMAIN);
    out.extend_from_slice(allocation_id.as_bytes());
    out.extend_from_slice(&token);
    out.extend_from_slice(timestamp.as_bytes());
    Ok(out)
}

fn validate_timestamp(value: &str) -> Result<()> {
    if value.contains('.') {
        return Err(PostUrbitError::InvalidInput("timestamp fractional"));
    }
    if !value.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("timestamp utc"));
    }
    let _: DateTime<Utc> = value
        .parse()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))?;
    Ok(())
}

fn validate_purl_type(packet_type: u8) -> Result<()> {
    match packet_type {
        PURL_TYPE_DATA
        | PURL_TYPE_PING
        | PURL_TYPE_PONG
        | PURL_TYPE_REFRESH
        | PURL_TYPE_RELEASE
        | PURL_TYPE_ERROR
        | PURL_TYPE_REBIND
        | PURL_TYPE_COORDINATE => Ok(()),
        _ => Err(PostUrbitError::InvalidInput("purl packet type")),
    }
}

fn validate_purl_packet(packet: &PurlPacket) -> Result<()> {
    validate_purl_type(packet.packet_type)?;
    if packet.payload.len() > PURL_MAX_PAYLOAD {
        return Err(PostUrbitError::InvalidInput("purl payload too large"));
    }
    let dest_zero = packet.destination_iid.iter().all(|b| *b == 0);
    if is_control_packet(packet.packet_type) {
        if !dest_zero {
            return Err(PostUrbitError::InvalidInput("purl control destination"));
        }
    } else if dest_zero {
        return Err(PostUrbitError::InvalidInput("purl data destination"));
    }
    Ok(())
}

fn is_control_packet(packet_type: u8) -> bool {
    matches!(
        packet_type,
        PURL_TYPE_PING
            | PURL_TYPE_PONG
            | PURL_TYPE_REFRESH
            | PURL_TYPE_RELEASE
            | PURL_TYPE_ERROR
            | PURL_TYPE_REBIND
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purl_round_trip() {
        let packet = PurlPacket {
            packet_type: PURL_TYPE_DATA,
            allocation_token: [7u8; 16],
            destination_iid: [9u8; 20],
            payload: vec![1, 2, 3],
        };
        let encoded = encode_purl(&packet).unwrap();
        let decoded = decode_purl(&encoded).unwrap();
        assert_eq!(decoded.packet_type, PURL_TYPE_DATA);
        assert_eq!(decoded.payload, vec![1, 2, 3]);
    }

    #[test]
    fn purl_rejects_bad_magic() {
        let err = decode_purl(b"PURX").unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn purl_rejects_oversize_payload() {
        let packet = PurlPacket {
            packet_type: PURL_TYPE_DATA,
            allocation_token: [1u8; 16],
            destination_iid: [2u8; 20],
            payload: vec![0u8; PURL_MAX_PAYLOAD + 1],
        };
        let err = encode_purl(&packet).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn purl_control_requires_zero_destination() {
        let packet = PurlPacket {
            packet_type: PURL_TYPE_PING,
            allocation_token: [1u8; 16],
            destination_iid: [9u8; 20],
            payload: vec![],
        };
        let err = encode_purl(&packet).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn relay_allocation_signature_round_trip() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let nonce = [7u8; 16];
        let signature = sign_relay_allocation(
            &signing_key,
            iid,
            3600,
            "2025-01-15T00:00:00Z",
            &nonce,
        )
        .unwrap();
        let key_b64 = base64_encode(signing_key.verifying_key().as_bytes());
        verify_relay_allocation(
            &signature,
            &key_b64,
            iid,
            3600,
            "2025-01-15T00:00:00Z",
            &nonce,
        )
        .unwrap();
    }

    #[test]
    fn relay_rebind_signature_round_trip() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let allocation_id = "alloc-123";
        let token = URL_SAFE_NO_PAD.encode([9u8; 16]);
        let signature = sign_relay_rebind(
            &signing_key,
            allocation_id,
            &token,
            "2025-01-15T00:00:00Z",
        )
        .unwrap();
        let key_b64 = base64_encode(signing_key.verifying_key().as_bytes());
        verify_relay_rebind(
            &signature,
            &key_b64,
            allocation_id,
            &token,
            "2025-01-15T00:00:00Z",
        )
        .unwrap();
    }
}

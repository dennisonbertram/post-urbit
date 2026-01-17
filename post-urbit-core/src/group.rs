use chrono::{DateTime, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::encoding::{crockford_base32_encode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};
use crate::ratchet::kdf_sender_key;

#[derive(Debug, Clone)]
pub struct SenderKey {
    pub key_id: [u8; 16],
    pub sender_iid: [u8; 20],
    pub chain_key: [u8; 32],
    pub created_at: String,
    pub iteration: u32,
}

impl SenderKey {
    pub fn advance(&mut self, group_id: &[u8; 20]) -> Result<[u8; 32]> {
        let (new_chain, message_key) =
            kdf_sender_key(&self.chain_key, group_id, &self.sender_iid, &self.key_id);
        self.chain_key = new_chain;
        self.iteration = self
            .iteration
            .checked_add(1)
            .ok_or(PostUrbitError::InvalidInput("iteration overflow"))?;
        Ok(message_key)
    }
}

pub fn generate_sender_key(sender_iid: [u8; 20], created_at: &str) -> Result<SenderKey> {
    validate_timestamp(created_at)?;
    let mut key_id = [0u8; 16];
    let mut chain_key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key_id);
    rand::rngs::OsRng.fill_bytes(&mut chain_key);
    Ok(SenderKey {
        key_id,
        sender_iid,
        chain_key,
        created_at: created_at.to_string(),
        iteration: 0,
    })
}

pub fn should_rotate_sender_key(key: &SenderKey, now: DateTime<Utc>) -> Result<bool> {
    let created_at = key
        .created_at
        .parse::<DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))?;
    let too_many_messages = key.iteration >= 100;
    let too_old = now.signed_duration_since(created_at).num_days() >= 7;
    Ok(too_many_messages || too_old)
}

pub fn derive_group_id(
    creator_iid_raw: &[u8; 20],
    random: &[u8; 32],
    created_at: &str,
) -> Result<String> {
    validate_timestamp(created_at)?;
    let mut hasher = Sha256::new();
    hasher.update(creator_iid_raw);
    hasher.update(random);
    hasher.update(created_at.as_bytes());
    let digest = hasher.finalize();
    Ok(crockford_base32_encode(&digest[..20]).to_lowercase())
}

fn validate_timestamp(value: &str) -> Result<()> {
    if value.contains('.') {
        return Err(PostUrbitError::InvalidInput("timestamp fractional"));
    }
    if value.len() != 20 || !value.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("timestamp format"));
    }
    let _: DateTime<Utc> = value
        .parse()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))?;
    Ok(())
}

pub fn validate_group_id(group_id: &str) -> Result<()> {
    validate_crockford_base32_lower(group_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_id_derivation_is_deterministic() {
        let creator = [1u8; 20];
        let random = [2u8; 32];
        let created_at = "2025-01-13T12:00:00Z";
        let id1 = derive_group_id(&creator, &random, created_at).unwrap();
        let id2 = derive_group_id(&creator, &random, created_at).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 32);
    }

    #[test]
    fn sender_key_iteration_increments() {
        let mut key = SenderKey {
            key_id: [7u8; 16],
            sender_iid: [8u8; 20],
            chain_key: [9u8; 32],
            created_at: "2025-01-13T12:00:00Z".to_string(),
            iteration: 0,
        };
        let group_id = [1u8; 20];
        let _ = key.advance(&group_id).unwrap();
        assert_eq!(key.iteration, 1);
    }

    #[test]
    fn sender_key_rotation_triggers() {
        let key = SenderKey {
            key_id: [7u8; 16],
            sender_iid: [8u8; 20],
            chain_key: [9u8; 32],
            created_at: "2025-01-01T00:00:00Z".to_string(),
            iteration: 100,
        };
        let now = "2025-01-10T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert!(should_rotate_sender_key(&key, now).unwrap());
    }

    #[test]
    fn sender_key_generation_validates_timestamp() {
        let key = generate_sender_key([1u8; 20], "2025-01-13T12:00:00Z").unwrap();
        assert_eq!(key.iteration, 0);
    }
}

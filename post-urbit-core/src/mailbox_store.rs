use std::collections::{BTreeMap, HashMap};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::encoding::{crockford_base32_decode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};
use crate::messaging::decode_puse_envelope;

#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub message_id: String,
    pub stored_at: String,
    pub sender_iid: String,
    pub size: u64,
    pub envelope: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct MailboxStore {
    messages: HashMap<String, BTreeMap<String, StoredMessage>>,
}

impl MailboxStore {
    pub fn new() -> Self {
        Self { messages: HashMap::new() }
    }

    pub fn store(
        &mut self,
        inbox_owner_iid: &str,
        token_iid: &str,
        envelope: &[u8],
    ) -> Result<StoredMessage> {
        validate_crockford_base32_lower(inbox_owner_iid)?;
        validate_crockford_base32_lower(token_iid)?;

        if envelope.len() > 1_048_576 {
            return Err(PostUrbitError::InvalidInput("envelope too large"));
        }
        let parsed = decode_puse_envelope(envelope)?;
        let sender_raw = parsed.header.sender_iid;

        let token_raw = crockford_base32_decode(token_iid)?;
        if token_raw.len() != 20 {
            return Err(PostUrbitError::InvalidInput("token iid length"));
        }
        if !constant_time_eq(&sender_raw, &token_raw) {
            return Err(PostUrbitError::InvalidInput("sender iid mismatch"));
        }

        let message_id = Uuid::from_bytes(parsed.header.message_id).to_string();
        let stored_at = DateTime::<Utc>::from(SystemTime::now())
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let stored = StoredMessage {
            message_id: message_id.clone(),
            stored_at,
            sender_iid: token_iid.to_string(),
            size: envelope.len() as u64,
            envelope: envelope.to_vec(),
        };

        let inbox = self
            .messages
            .entry(inbox_owner_iid.to_string())
            .or_default();
        if let Some(existing) = inbox.get(&message_id) {
            return Ok(existing.clone());
        }
        inbox.insert(message_id, stored.clone());
        Ok(stored)
    }

    pub fn retrieve(&self, inbox_owner_iid: &str) -> Result<Vec<StoredMessage>> {
        validate_crockford_base32_lower(inbox_owner_iid)?;
        let mut values = self
            .messages
            .get(inbox_owner_iid)
            .map(|m| m.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        values.sort_by(|a, b| {
            let order = a.stored_at.cmp(&b.stored_at);
            if order == std::cmp::Ordering::Equal {
                a.message_id.cmp(&b.message_id)
            } else {
                order
            }
        });
        Ok(values)
    }

    pub fn delete(&mut self, inbox_owner_iid: &str, ids: &[String]) -> Result<u64> {
        validate_crockford_base32_lower(inbox_owner_iid)?;
        let Some(inbox) = self.messages.get_mut(inbox_owner_iid) else {
            return Ok(0);
        };
        let mut deleted = 0;
        for id in ids {
            if inbox.remove(id).is_some() {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

fn constant_time_eq(a: &[u8; 20], b: &[u8]) -> bool {
    if b.len() != 20 {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::{build_puse_envelope, PUSEHeader};
    use ed25519_dalek::SigningKey;

    #[test]
    fn mailbox_store_idempotent() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let token_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let sender_raw = crockford_base32_decode(token_iid).unwrap();
        let sender_raw: [u8; 20] = sender_raw.try_into().unwrap();
        let header = PUSEHeader {
            flags: 0,
            sender_iid: sender_raw,
            recipient_iid: [2u8; 20],
            message_id: [3u8; 16],
            header_extension: vec![0x00; 33],
            nonce: [4u8; 12],
            ciphertext_length: 0,
        };
        let message_key = [7u8; 32];
        let envelope = build_puse_envelope(&signing_key, header, &message_key, b"hi").unwrap();
        let mut store = MailboxStore::new();
        let first = store
            .store(token_iid, token_iid, &envelope)
            .unwrap();
        let second = store
            .store(token_iid, token_iid, &envelope)
            .unwrap();
        assert_eq!(first.message_id, second.message_id);
    }
}

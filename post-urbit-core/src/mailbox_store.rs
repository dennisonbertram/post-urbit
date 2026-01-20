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

    /// Retrieve messages filtered by sender IID
    ///
    /// Per REQ-MSG-090-092, recipients can filter messages by sender.
    /// This allows recipients to retrieve only messages from specific senders.
    pub fn retrieve_by_sender(
        &self,
        inbox_owner_iid: &str,
        sender_iid: &str,
    ) -> Result<Vec<StoredMessage>> {
        validate_crockford_base32_lower(inbox_owner_iid)?;
        validate_crockford_base32_lower(sender_iid)?;

        let mut values = self
            .messages
            .get(inbox_owner_iid)
            .map(|m| {
                m.values()
                    .filter(|msg| msg.sender_iid == sender_iid)
                    .cloned()
                    .collect::<Vec<_>>()
            })
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

    /// Get all unique sender IIDs that have messages in this inbox
    pub fn get_senders(&self, inbox_owner_iid: &str) -> Result<Vec<String>> {
        validate_crockford_base32_lower(inbox_owner_iid)?;

        let mut senders: Vec<String> = self
            .messages
            .get(inbox_owner_iid)
            .map(|m| {
                let mut seen = std::collections::HashSet::new();
                m.values()
                    .filter_map(|msg| {
                        if seen.insert(msg.sender_iid.clone()) {
                            Some(msg.sender_iid.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        senders.sort();
        Ok(senders)
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

    /// Store a group message to multiple recipients' mailboxes
    ///
    /// Per REQ-MSG-089, when sending a group message to offline members,
    /// the sender fans out the same PUSE envelope to each member's mailbox.
    ///
    /// # Arguments
    /// * `group_id` - The group identifier (for logging/tracking purposes)
    /// * `member_iids` - The IIDs of group members to receive the message
    /// * `sender_iid` - The sender's IID (must match envelope sender)
    /// * `envelope` - The PUSE envelope (contains group_id as recipient)
    ///
    /// # Returns
    /// A vector of message IDs, one per recipient that was successfully stored
    pub fn store_group_message(
        &mut self,
        _group_id: &str,
        member_iids: &[String],
        sender_iid: &str,
        envelope: &[u8],
    ) -> Result<Vec<String>> {
        validate_crockford_base32_lower(sender_iid)?;

        if envelope.len() > 1_048_576 {
            return Err(PostUrbitError::InvalidInput("envelope too large"));
        }

        // Validate envelope and extract sender
        let parsed = decode_puse_envelope(envelope)?;
        let envelope_sender_raw = parsed.header.sender_iid;

        // Verify sender_iid matches envelope sender
        let sender_raw = crockford_base32_decode(sender_iid)?;
        if sender_raw.len() != 20 {
            return Err(PostUrbitError::InvalidInput("sender iid length"));
        }
        if !constant_time_eq(&envelope_sender_raw, &sender_raw) {
            return Err(PostUrbitError::InvalidInput("sender iid mismatch"));
        }

        // Store to each member's inbox
        let mut message_ids = Vec::with_capacity(member_iids.len());
        let message_id = Uuid::from_bytes(parsed.header.message_id).to_string();

        for member_iid in member_iids {
            validate_crockford_base32_lower(member_iid)?;

            let stored_at = DateTime::<Utc>::from(SystemTime::now())
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();

            let stored = StoredMessage {
                message_id: message_id.clone(),
                stored_at,
                sender_iid: sender_iid.to_string(),
                size: envelope.len() as u64,
                envelope: envelope.to_vec(),
            };

            let inbox = self
                .messages
                .entry(member_iid.to_string())
                .or_default();

            // Idempotent - if already stored, skip
            if inbox.get(&message_id).is_none() {
                inbox.insert(message_id.clone(), stored);
            }
            message_ids.push(message_id.clone());
        }

        Ok(message_ids)
    }

    /// Get statistics about a mailbox
    pub fn get_inbox_stats(&self, inbox_owner_iid: &str) -> Result<InboxStats> {
        validate_crockford_base32_lower(inbox_owner_iid)?;

        let inbox = self.messages.get(inbox_owner_iid);
        let (message_count, total_size, unique_senders) = match inbox {
            Some(m) => {
                let count = m.len() as u64;
                let size = m.values().map(|msg| msg.size).sum();
                let senders: std::collections::HashSet<_> =
                    m.values().map(|msg| msg.sender_iid.clone()).collect();
                (count, size, senders.len() as u64)
            }
            None => (0, 0, 0),
        };

        Ok(InboxStats {
            message_count,
            total_size,
            unique_senders,
        })
    }
}

/// Statistics about an inbox
#[derive(Debug, Clone)]
pub struct InboxStats {
    /// Number of messages in the inbox
    pub message_count: u64,
    /// Total size of all envelopes in bytes
    pub total_size: u64,
    /// Number of unique senders
    pub unique_senders: u64,
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
    use crate::messaging::{build_puse_envelope, build_group_extension, PUSEHeader};
    use ed25519_dalek::SigningKey;

    fn test_iid_1() -> &'static str {
        "b1n7cfscgashm32xx7eaxw0y09gy0y2v"
    }

    fn test_iid_2() -> &'static str {
        "a0b1c2d3e4f5g6h7j8k9m0n1p2q3r4s5"
    }

    fn create_envelope(
        signing_key: &SigningKey,
        sender_iid: &str,
        recipient_iid: [u8; 20],
        message_id: [u8; 16],
    ) -> Vec<u8> {
        let sender_raw = crockford_base32_decode(sender_iid).unwrap();
        let sender_raw: [u8; 20] = sender_raw.try_into().unwrap();
        let header = PUSEHeader {
            flags: 0,
            sender_iid: sender_raw,
            recipient_iid,
            message_id,
            header_extension: vec![0x00; 33],
            nonce: [4u8; 12],
            ciphertext_length: 0,
        };
        let message_key = [7u8; 32];
        build_puse_envelope(signing_key, header, &message_key, b"hi").unwrap()
    }

    fn create_group_envelope(
        signing_key: &SigningKey,
        sender_iid: &str,
        group_id: [u8; 20],
        message_id: [u8; 16],
    ) -> Vec<u8> {
        let sender_raw = crockford_base32_decode(sender_iid).unwrap();
        let sender_raw: [u8; 20] = sender_raw.try_into().unwrap();
        let group_ext = build_group_extension([1u8; 16], 1).unwrap();
        let header = PUSEHeader {
            flags: 0x01, // Group message flag
            sender_iid: sender_raw,
            recipient_iid: group_id,
            message_id,
            header_extension: group_ext,
            nonce: [4u8; 12],
            ciphertext_length: 0,
        };
        let message_key = [7u8; 32];
        build_puse_envelope(signing_key, header, &message_key, b"group message").unwrap()
    }

    #[test]
    fn mailbox_store_idempotent() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let token_iid = test_iid_1();
        let envelope = create_envelope(&signing_key, token_iid, [2u8; 20], [3u8; 16]);
        let mut store = MailboxStore::new();
        let first = store.store(token_iid, token_iid, &envelope).unwrap();
        let second = store.store(token_iid, token_iid, &envelope).unwrap();
        assert_eq!(first.message_id, second.message_id);
    }

    #[test]
    fn retrieve_by_sender_filters_correctly() {
        let signing_key1 = SigningKey::generate(&mut rand::rngs::OsRng);
        let signing_key2 = SigningKey::generate(&mut rand::rngs::OsRng);

        let sender1_iid = test_iid_1();
        let sender2_iid = test_iid_2();
        let recipient_iid = test_iid_1();

        let envelope1 = create_envelope(&signing_key1, sender1_iid, [2u8; 20], [1u8; 16]);
        let envelope2 = create_envelope(&signing_key2, sender2_iid, [2u8; 20], [2u8; 16]);

        let mut store = MailboxStore::new();
        store.store(recipient_iid, sender1_iid, &envelope1).unwrap();
        store.store(recipient_iid, sender2_iid, &envelope2).unwrap();

        // All messages
        let all = store.retrieve(recipient_iid).unwrap();
        assert_eq!(all.len(), 2);

        // Filter by sender1
        let from_sender1 = store.retrieve_by_sender(recipient_iid, sender1_iid).unwrap();
        assert_eq!(from_sender1.len(), 1);
        assert_eq!(from_sender1[0].sender_iid, sender1_iid);

        // Filter by sender2
        let from_sender2 = store.retrieve_by_sender(recipient_iid, sender2_iid).unwrap();
        assert_eq!(from_sender2.len(), 1);
        assert_eq!(from_sender2[0].sender_iid, sender2_iid);
    }

    #[test]
    fn get_senders_returns_unique_senders() {
        let signing_key1 = SigningKey::generate(&mut rand::rngs::OsRng);
        let signing_key2 = SigningKey::generate(&mut rand::rngs::OsRng);

        let sender1_iid = test_iid_1();
        let sender2_iid = test_iid_2();
        let recipient_iid = test_iid_1();

        let envelope1 = create_envelope(&signing_key1, sender1_iid, [2u8; 20], [1u8; 16]);
        let envelope2 = create_envelope(&signing_key1, sender1_iid, [2u8; 20], [2u8; 16]);
        let envelope3 = create_envelope(&signing_key2, sender2_iid, [2u8; 20], [3u8; 16]);

        let mut store = MailboxStore::new();
        store.store(recipient_iid, sender1_iid, &envelope1).unwrap();
        store.store(recipient_iid, sender1_iid, &envelope2).unwrap();
        store.store(recipient_iid, sender2_iid, &envelope3).unwrap();

        let senders = store.get_senders(recipient_iid).unwrap();
        assert_eq!(senders.len(), 2);
        assert!(senders.contains(&sender1_iid.to_string()));
        assert!(senders.contains(&sender2_iid.to_string()));
    }

    #[test]
    fn store_group_message_fans_out() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let sender_iid = test_iid_1();
        let member1_iid = test_iid_1();
        let member2_iid = test_iid_2();
        let group_id = [5u8; 20];
        let message_id = [6u8; 16];

        let envelope = create_group_envelope(&signing_key, sender_iid, group_id, message_id);

        let mut store = MailboxStore::new();
        let member_iids = vec![member1_iid.to_string(), member2_iid.to_string()];
        let message_ids = store
            .store_group_message("test-group", &member_iids, sender_iid, &envelope)
            .unwrap();

        // Should return same message ID for both
        assert_eq!(message_ids.len(), 2);
        assert_eq!(message_ids[0], message_ids[1]);

        // Both members should have the message
        let member1_messages = store.retrieve(member1_iid).unwrap();
        let member2_messages = store.retrieve(member2_iid).unwrap();

        assert_eq!(member1_messages.len(), 1);
        assert_eq!(member2_messages.len(), 1);
        assert_eq!(member1_messages[0].sender_iid, sender_iid);
        assert_eq!(member2_messages[0].sender_iid, sender_iid);
    }

    #[test]
    fn store_group_message_idempotent() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let sender_iid = test_iid_1();
        let member_iid = test_iid_1();
        let group_id = [5u8; 20];
        let message_id = [6u8; 16];

        let envelope = create_group_envelope(&signing_key, sender_iid, group_id, message_id);

        let mut store = MailboxStore::new();
        let member_iids = vec![member_iid.to_string()];

        // Store twice
        store
            .store_group_message("test-group", &member_iids, sender_iid, &envelope)
            .unwrap();
        store
            .store_group_message("test-group", &member_iids, sender_iid, &envelope)
            .unwrap();

        // Should only have one message
        let messages = store.retrieve(member_iid).unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn inbox_stats_are_correct() {
        let signing_key1 = SigningKey::generate(&mut rand::rngs::OsRng);
        let signing_key2 = SigningKey::generate(&mut rand::rngs::OsRng);

        let sender1_iid = test_iid_1();
        let sender2_iid = test_iid_2();
        let recipient_iid = test_iid_1();

        let envelope1 = create_envelope(&signing_key1, sender1_iid, [2u8; 20], [1u8; 16]);
        let envelope2 = create_envelope(&signing_key2, sender2_iid, [2u8; 20], [2u8; 16]);

        let mut store = MailboxStore::new();

        // Empty inbox
        let stats = store.get_inbox_stats(recipient_iid).unwrap();
        assert_eq!(stats.message_count, 0);
        assert_eq!(stats.total_size, 0);
        assert_eq!(stats.unique_senders, 0);

        // Add messages
        store.store(recipient_iid, sender1_iid, &envelope1).unwrap();
        store.store(recipient_iid, sender2_iid, &envelope2).unwrap();

        let stats = store.get_inbox_stats(recipient_iid).unwrap();
        assert_eq!(stats.message_count, 2);
        assert!(stats.total_size > 0);
        assert_eq!(stats.unique_senders, 2);
    }

    #[test]
    fn sender_iid_mismatch_rejected() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let actual_sender_iid = test_iid_1();
        let claimed_sender_iid = test_iid_2();
        let recipient_iid = test_iid_1();

        let envelope = create_envelope(&signing_key, actual_sender_iid, [2u8; 20], [1u8; 16]);

        let mut store = MailboxStore::new();
        let result = store.store(recipient_iid, claimed_sender_iid, &envelope);

        assert!(result.is_err());
        match result {
            Err(PostUrbitError::InvalidInput(msg)) => {
                assert_eq!(msg, "sender iid mismatch");
            }
            _ => panic!("Expected sender iid mismatch error"),
        }
    }
}

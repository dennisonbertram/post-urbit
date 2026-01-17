use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{PostUrbitError, Result};

#[derive(Debug, Serialize)]
pub struct Summary {
    pub run_id: String,
    pub status: String,
    pub scenarios: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Event {
    pub step_id: String,
    pub detail: String,
}

pub struct EvidenceBundle {
    base_dir: PathBuf,
    run_id: String,
    events: Vec<Event>,
    scenarios: Vec<String>,
}

impl EvidenceBundle {
    pub fn new(base_dir: &Path, run_id: &str) -> Result<Self> {
        let run_dir = base_dir.join(run_id);
        fs::create_dir_all(run_dir.join("artifacts"))
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        Ok(Self {
            base_dir: run_dir,
            run_id: run_id.to_string(),
            events: Vec::new(),
            scenarios: Vec::new(),
        })
    }

    pub fn add_scenario(&mut self, scenario: &str) {
        self.scenarios.push(scenario.to_string());
    }

    pub fn record_event(&mut self, step_id: &str, detail: &str) {
        self.events.push(Event {
            step_id: step_id.to_string(),
            detail: detail.to_string(),
        });
    }

    pub fn finalize(self, status: &str) -> Result<()> {
        let summary = Summary {
            run_id: self.run_id.clone(),
            status: status.to_string(),
            scenarios: self.scenarios,
        };
        let summary_json = serde_json::to_vec_pretty(&summary)
            .map_err(|_| PostUrbitError::InvalidInput("summary json"))?;
        let summary_md = format!(
            "# Run {run_id}\n\nStatus: {status}\n",
            run_id = summary.run_id,
            status = summary.status
        );

        let summary_path = self.base_dir.join("summary.md");
        fs::write(summary_path, summary_md)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        let summary_json_path = self.base_dir.join("summary.json");
        fs::write(summary_json_path, summary_json)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        let mut events_file = File::create(self.base_dir.join("events.ndjson"))
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        for event in self.events {
            let line = serde_json::to_string(&event)
                .map_err(|_| PostUrbitError::InvalidInput("event json"))?;
            writeln!(events_file, "{line}")
                .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dht::MemoryDht;
    use crate::identity::{fetch_identity, publish_genesis, IdentityManager};
    use crate::mailbox_store::MailboxStore;
    use crate::messaging::{build_puse_envelope, decode_puse_envelope, PUSEHeader};
    use crate::encoding::crockford_base32_decode;
    use ed25519_dalek::SigningKey;
    use crate::sync::{sign_sync_operation, SyncSession, SyncStore};

    #[test]
    fn evidence_bundle_writes_files() {
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-test").unwrap();
        bundle.add_scenario("SCEN-ID-01");
        bundle.record_event("S1", "created identity");
        bundle.finalize("ok").unwrap();

        assert!(temp.path().join("run-test/summary.md").exists());
        assert!(temp.path().join("run-test/summary.json").exists());
        assert!(temp.path().join("run-test/events.ndjson").exists());
    }

    #[test]
    fn harness_single_node_identity_publish() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-e2e").unwrap();
        bundle.add_scenario("SCEN-ID-01");
        bundle.record_event("S1", "create identity");

        rt.block_on(async {
            let dht = MemoryDht::new();
            let identity = IdentityManager::new(temp.path().to_str().unwrap())
                .await
                .unwrap();
            publish_genesis(&dht, identity.identity_document()).await.unwrap();
            let fetched = fetch_identity(&dht, identity.iid()).await.unwrap();
            assert!(fetched.is_some());
        });

        bundle.record_event("S2", "publish identity");
        bundle.finalize("ok").unwrap();
    }

    #[test]
    fn harness_mailbox_store_retrieve() {
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-mailbox").unwrap();
        bundle.add_scenario("SCEN-JOURNEY-04");
        bundle.record_event("S1", "store envelope");

        let sender_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let sender_raw = crockford_base32_decode(sender_iid).unwrap();
        let sender_raw: [u8; 20] = sender_raw.try_into().unwrap();
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let header = PUSEHeader {
            flags: 0,
            sender_iid: sender_raw,
            recipient_iid: [3u8; 20],
            message_id: [7u8; 16],
            header_extension: vec![0x00; 33],
            nonce: [8u8; 12],
            ciphertext_length: 0,
        };
        let message_key = [9u8; 32];
        let envelope = build_puse_envelope(&signing_key, header, &message_key, b"hello").unwrap();
        let mut store = MailboxStore::new();
        let _ = store.store(sender_iid, sender_iid, &envelope).unwrap();

        let messages = store.retrieve(sender_iid).unwrap();
        assert_eq!(messages.len(), 1);
        let decoded = decode_puse_envelope(&messages[0].envelope).unwrap();
        assert_eq!(decoded.header.message_id, [7u8; 16]);

        bundle.record_event("S2", "retrieve envelope");
        bundle.finalize("ok").unwrap();
    }

    #[test]
    fn harness_two_node_identity_exchange() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-identity-exchange").unwrap();
        bundle.add_scenario("SCEN-CONN-01");
        bundle.record_event("S1", "create identities");

        rt.block_on(async {
            let dht = MemoryDht::new();
            let node_a = IdentityManager::new(temp.path().to_str().unwrap()).await.unwrap();
            let node_b = IdentityManager::new(temp.path().to_str().unwrap()).await.unwrap();
            publish_genesis(&dht, node_a.identity_document()).await.unwrap();
            publish_genesis(&dht, node_b.identity_document()).await.unwrap();

            let fetched_a = fetch_identity(&dht, node_a.iid()).await.unwrap();
            let fetched_b = fetch_identity(&dht, node_b.iid()).await.unwrap();
            assert!(fetched_a.is_some());
            assert!(fetched_b.is_some());
        });

        bundle.record_event("S2", "exchange identity docs");
        bundle.finalize("ok").unwrap();
    }

    #[test]
    fn harness_sync_converges() {
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-sync").unwrap();
        bundle.add_scenario("SCEN-SYNC-01");
        bundle.record_event("S1", "prepare sync state");

        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let keys = vec![signing_key.verifying_key().to_bytes().to_vec()];
        let document_id = [4u8; 32];
        let (op_id, signature) = sign_sync_operation(
            &document_id,
            &[7u8; 20],
            10,
            1,
            b"op",
            &[],
            &signing_key,
        );
        let record = crate::sync::SyncOperationRecord {
            id: op_id,
            origin: [7u8; 20],
            document_id,
            physical_ms: 10,
            logical: 1,
            operation: b"op".to_vec(),
            dependencies: Vec::new(),
            signature: signature.to_vec(),
        };

        let mut sender_store = SyncStore::new();
        sender_store.add_operation(record, &keys).unwrap();
        let sender = SyncSession::new(document_id, sender_store);

        let receiver_store = SyncStore::new();
        let mut receiver = SyncSession::new(document_id, receiver_store);

        let request = receiver.request();
        let offer = sender.handle_request(&request).unwrap();
        let accept = receiver.handle_offer(&offer).unwrap();
        let operations = sender.handle_accept(&accept).unwrap();
        let ack = receiver.handle_operations(&operations, &keys).unwrap();
        assert_eq!(ack.operation_ids.len(), 1);

        bundle.record_event("S2", "sync converged");
        bundle.finalize("ok").unwrap();
    }
}

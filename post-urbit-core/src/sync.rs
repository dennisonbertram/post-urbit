use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};

use crate::error::{PostUrbitError, Result};

const MERKLE_LEAF_PREFIX: &[u8] = b"post-urbit:merkle-leaf:";
const MERKLE_NODE_PREFIX: &[u8] = b"post-urbit:merkle-node:";
const MERKLE_EMPTY_PREFIX: &[u8] = b"post-urbit:merkle-empty:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub document_id: Vec<u8>,
    pub merkle_root: Vec<u8>,
    pub depth: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOffer {
    pub document_id: Vec<u8>,
    pub operation_ids: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAccept {
    pub document_id: Vec<u8>,
    pub wanted_ids: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperations {
    pub document_id: Vec<u8>,
    pub operations: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAck {
    pub document_id: Vec<u8>,
    pub operation_ids: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSubscribe {
    pub document_id: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncUnsubscribe {
    pub document_id: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncError {
    pub error_code: u64,
    pub message: Option<String>,
    pub operation_id: Option<Vec<u8>>,
    pub document_id: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum SyncMessage {
    Request(SyncRequest),
    Offer(SyncOffer),
    Accept(SyncAccept),
    Operations(SyncOperations),
    Ack(SyncAck),
    Subscribe(SyncSubscribe),
    Unsubscribe(SyncUnsubscribe),
    Error(SyncError),
}

#[derive(Debug, Clone)]
pub struct SyncOperationRecord {
    pub id: [u8; 32],
    pub origin: [u8; 20],
    pub document_id: [u8; 32],
    pub physical_ms: u64,
    pub logical: u32,
    pub operation: Vec<u8>,
    pub dependencies: Vec<[u8; 32]>,
    pub signature: [u8; 64],
}

pub fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_cbor::to_vec(value).map_err(|_| PostUrbitError::InvalidInput("cbor encode"))
}

pub fn decode_cbor<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    serde_cbor::from_slice(bytes).map_err(|_| PostUrbitError::InvalidInput("cbor decode"))
}

pub fn encode_sync_message(message: &SyncMessage) -> Result<Vec<u8>> {
    let (msg_type, payload) = match message {
        SyncMessage::Request(req) => (0x01u8, encode_cbor(req)?),
        SyncMessage::Offer(req) => (0x02u8, encode_cbor(req)?),
        SyncMessage::Accept(req) => (0x03u8, encode_cbor(req)?),
        SyncMessage::Operations(req) => (0x04u8, encode_cbor(req)?),
        SyncMessage::Ack(req) => (0x05u8, encode_cbor(req)?),
        SyncMessage::Subscribe(req) => (0x06u8, encode_cbor(req)?),
        SyncMessage::Unsubscribe(req) => (0x07u8, encode_cbor(req)?),
        SyncMessage::Error(req) => (0x08u8, encode_cbor(req)?),
    };

    let mut out = Vec::with_capacity(1 + payload.len());
    out.push(msg_type);
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_sync_message(bytes: &[u8]) -> Result<SyncMessage> {
    if bytes.is_empty() {
        return Err(PostUrbitError::InvalidInput("sync message empty"));
    }
    let msg_type = bytes[0];
    let payload = &bytes[1..];
    match msg_type {
        0x01 => Ok(SyncMessage::Request(decode_cbor(payload)?)),
        0x02 => Ok(SyncMessage::Offer(decode_cbor(payload)?)),
        0x03 => Ok(SyncMessage::Accept(decode_cbor(payload)?)),
        0x04 => Ok(SyncMessage::Operations(decode_cbor(payload)?)),
        0x05 => Ok(SyncMessage::Ack(decode_cbor(payload)?)),
        0x06 => Ok(SyncMessage::Subscribe(decode_cbor(payload)?)),
        0x07 => Ok(SyncMessage::Unsubscribe(decode_cbor(payload)?)),
        0x08 => Ok(SyncMessage::Error(decode_cbor(payload)?)),
        _ => Err(PostUrbitError::InvalidInput("sync message type")),
    }
}

pub fn merkle_leaf_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(MERKLE_LEAF_PREFIX);
    hasher.update(data);
    hasher.finalize().as_slice().try_into().expect("sha256 length")
}

pub fn merkle_node_hash(left: &[u8], right: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(MERKLE_NODE_PREFIX);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().as_slice().try_into().expect("sha256 length")
}

pub fn merkle_empty_hash() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(MERKLE_EMPTY_PREFIX);
    hasher.finalize().as_slice().try_into().expect("sha256 length")
}

pub fn timestamp_bytes(physical_ms: u64, logical: u32, origin: &[u8; 20]) -> [u8; 20] {
    let mut out = [0u8; 20];
    out[..8].copy_from_slice(&physical_ms.to_be_bytes());
    out[8..12].copy_from_slice(&logical.to_be_bytes());
    let hash = Sha256::digest(origin);
    out[12..20].copy_from_slice(&hash[..8]);
    out
}

pub fn operation_id(
    origin: &[u8; 20],
    physical_ms: u64,
    logical: u32,
    operation_bytes: &[u8],
    dependencies: &[ [u8; 32] ],
) -> [u8; 32] {
    let timestamp = timestamp_bytes(physical_ms, logical, origin);
    let deps = dependencies_bytes(dependencies);
    let mut hasher = Sha256::new();
    hasher.update(origin);
    hasher.update(&timestamp);
    hasher.update(operation_bytes);
    hasher.update(&deps);
    hasher.finalize().as_slice().try_into().expect("sha256 length")
}

pub fn sign_sync_operation(
    document_id: &[u8; 32],
    origin: &[u8; 20],
    physical_ms: u64,
    logical: u32,
    operation_bytes: &[u8],
    dependencies: &[ [u8; 32] ],
    signing_key: &SigningKey,
) -> ([u8; 32], [u8; 64]) {
    let op_id = operation_id(origin, physical_ms, logical, operation_bytes, dependencies);
    let timestamp = timestamp_bytes(physical_ms, logical, origin);
    let deps = dependencies_bytes(dependencies);
    let mut signature_input = Vec::new();
    signature_input.extend_from_slice(b"post-urbit:sync-op:v1:");
    signature_input.extend_from_slice(&op_id);
    signature_input.extend_from_slice(document_id);
    signature_input.extend_from_slice(&timestamp);
    signature_input.extend_from_slice(operation_bytes);
    signature_input.extend_from_slice(&deps);
    let signature: Signature = signing_key.sign(&signature_input);
    (op_id, signature.to_bytes())
}

pub fn verify_sync_operation(
    record: &SyncOperationRecord,
    signing_keys: &[Vec<u8>],
) -> Result<()> {
    let timestamp = timestamp_bytes(record.physical_ms, record.logical, &record.origin);
    let deps = dependencies_bytes(&record.dependencies);
    let mut signature_input = Vec::new();
    signature_input.extend_from_slice(b"post-urbit:sync-op:v1:");
    signature_input.extend_from_slice(&record.id);
    signature_input.extend_from_slice(&record.document_id);
    signature_input.extend_from_slice(&timestamp);
    signature_input.extend_from_slice(&record.operation);
    signature_input.extend_from_slice(&deps);

    let signature = Signature::from_bytes(&record.signature);
    for key_bytes in signing_keys {
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
        if verifying_key.verify_strict(&signature_input, &signature).is_ok() {
            return Ok(());
        }
    }
    Err(PostUrbitError::Crypto("sync signature invalid"))
}

fn dependencies_bytes(dependencies: &[ [u8; 32] ]) -> Vec<u8> {
    let mut deps = dependencies.to_vec();
    deps.sort();
    let mut out = Vec::with_capacity(deps.len() * 32);
    for dep in deps {
        out.extend_from_slice(&dep);
    }
    out
}

#[derive(Debug, Clone)]
pub struct ORSet<T: Eq + Hash + Clone> {
    adds: HashMap<T, HashSet<u64>>,
    removes: HashMap<T, HashSet<u64>>,
}

#[derive(Debug, Clone)]
pub struct ReplicationFilter {
    allowlist: Option<HashSet<String>>,
    denylist: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct SyncStateMachine {
    local_root: Vec<u8>,
    remote_root: Vec<u8>,
}

impl SyncStateMachine {
    pub fn new(local_root: Vec<u8>) -> Self {
        Self {
            local_root: local_root.clone(),
            remote_root: local_root,
        }
    }

    pub fn request_diff(&self, remote_root: &[u8]) -> bool {
        self.local_root != remote_root
    }

    pub fn apply_remote_root(&mut self, remote_root: Vec<u8>) {
        self.remote_root = remote_root;
    }

    pub fn apply_local_root(&mut self, local_root: Vec<u8>) {
        self.local_root = local_root;
    }

    pub fn converged(&self) -> bool {
        self.local_root == self.remote_root
    }
}

impl ReplicationFilter {
    pub fn new(allowlist: Option<HashSet<String>>, denylist: HashSet<String>) -> Self {
        Self { allowlist, denylist }
    }

    pub fn allows(&self, dataset: &str) -> bool {
        if self.denylist.contains(dataset) {
            return false;
        }
        match &self.allowlist {
            Some(list) => list.contains(dataset),
            None => true,
        }
    }
}

impl<T: Eq + Hash + Clone> ORSet<T> {
    pub fn new() -> Self {
        Self {
            adds: HashMap::new(),
            removes: HashMap::new(),
        }
    }

    pub fn add(&mut self, value: T, tag: u64) {
        self.adds.entry(value).or_default().insert(tag);
    }

    pub fn remove(&mut self, value: &T, tags: &[u64]) {
        if tags.is_empty() {
            return;
        }
        self.removes
            .entry(value.clone())
            .or_default()
            .extend(tags.iter().copied());
    }

    pub fn merge(&mut self, other: &Self) {
        for (value, tags) in &other.adds {
            self.adds
                .entry(value.clone())
                .or_default()
                .extend(tags.iter().copied());
        }
        for (value, tags) in &other.removes {
            self.removes
                .entry(value.clone())
                .or_default()
                .extend(tags.iter().copied());
        }
    }

    pub fn values(&self) -> HashSet<T> {
        let mut out = HashSet::new();
        for (value, add_tags) in &self.adds {
            let removed = self.removes.get(value);
            let mut present = false;
            for tag in add_tags {
                if removed.map_or(true, |set| !set.contains(tag)) {
                    present = true;
                    break;
                }
            }
            if present {
                out.insert(value.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::base64_encode;

    #[test]
    fn cbor_round_trip() {
        let req = SyncRequest {
            document_id: vec![1u8; 32],
            merkle_root: vec![2u8; 32],
            depth: 3,
        };
        let encoded = encode_cbor(&req).unwrap();
        let decoded: SyncRequest = decode_cbor(&encoded).unwrap();
        assert_eq!(decoded.document_id, req.document_id);
    }

    #[test]
    fn sync_message_round_trip() {
        let msg = SyncMessage::Subscribe(SyncSubscribe {
            document_id: vec![9u8; 32],
        });
        let encoded = encode_sync_message(&msg).unwrap();
        let decoded = decode_sync_message(&encoded).unwrap();
        match decoded {
            SyncMessage::Subscribe(inner) => assert_eq!(inner.document_id, vec![9u8; 32]),
            _ => panic!("unexpected message"),
        }
    }

    #[test]
    fn merkle_hashes_stable() {
        let leaf = merkle_leaf_hash(b"abc");
        let empty = merkle_empty_hash();
        let node = merkle_node_hash(&leaf, &empty);
        assert_eq!(leaf.len(), 32);
        assert_eq!(empty.len(), 32);
        assert_eq!(node.len(), 32);
    }

    #[test]
    fn orset_add_remove_merge() {
        let mut a = ORSet::new();
        a.add("hello", 1);
        a.add("hello", 2);
        a.remove(&"hello", &[1]);

        let mut b = ORSet::new();
        b.add("hello", 3);

        a.merge(&b);
        let values = a.values();
        assert!(values.contains("hello"));
    }

    #[test]
    fn replication_filter_allows() {
        let mut allow = HashSet::new();
        allow.insert("docs".to_string());
        let filter = ReplicationFilter::new(Some(allow), HashSet::new());
        assert!(filter.allows("docs"));
        assert!(!filter.allows("photos"));
    }

    #[test]
    fn replication_filter_denies() {
        let mut deny = HashSet::new();
        deny.insert("secret".to_string());
        let filter = ReplicationFilter::new(None, deny);
        assert!(!filter.allows("secret"));
        assert!(filter.allows("public"));
    }

    #[test]
    fn sync_state_machine_converges() {
        let mut sm = SyncStateMachine::new(vec![1u8; 32]);
        assert!(sm.converged());
        assert!(sm.request_diff(&[2u8; 32]));
        sm.apply_remote_root(vec![2u8; 32]);
        assert!(!sm.converged());
        sm.apply_local_root(vec![2u8; 32]);
        assert!(sm.converged());
    }

    #[test]
    fn sync_operation_signature_round_trip() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let document_id = [1u8; 32];
        let origin = [2u8; 20];
        let operation = vec![0x01, 0x02];
        let deps = vec![[9u8; 32]];
        let (op_id, signature) = sign_sync_operation(
            &document_id,
            &origin,
            123,
            1,
            &operation,
            &deps,
            &signing_key,
        );
        let record = SyncOperationRecord {
            id: op_id,
            origin,
            document_id,
            physical_ms: 123,
            logical: 1,
            operation,
            dependencies: deps,
            signature,
        };
        let keys = vec![signing_key.verifying_key().to_bytes().to_vec()];
        verify_sync_operation(&record, &keys).unwrap();
    }

    #[test]
    fn sync_operation_matches_test_vector() {
        let signing_seed = hex::decode(
            "033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40",
        )
        .unwrap();
        let signing_key = SigningKey::from_bytes(signing_seed.as_slice().try_into().unwrap());

        let document_id: [u8; 32] = hex::decode(
            "550e8400e29b41d4a71644665544000000000000000000000000000000000000",
        )
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
        let origin: [u8; 20] = hex::decode("586a763f2c82b31a0c5de9dcaef01e0261e0785b")
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();
        let operation = hex::decode("a20000016b416c69636520536d697468").unwrap();

        let (op_id, signature) = sign_sync_operation(
            &document_id,
            &origin,
            1_700_000_000_000,
            7,
            &operation,
            &[],
            &signing_key,
        );

        assert_eq!(
            hex::encode(op_id),
            "27bff0b3171025eef73c81edb1c88bf61f902b30eef342b0e65ce847d65c2314"
        );
        assert_eq!(
            base64_encode(&signature),
            "q/5rBz+Pr7SiFvUJn2/q7HsqJXMJ4pvbMc1kexQJqqtMCBngpbxBIuo1Ab2QqZN0F8bQ5h0XnUu5sByjUgM/Cw"
        );
    }
}

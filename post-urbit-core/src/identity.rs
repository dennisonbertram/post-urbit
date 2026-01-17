use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::canonical_json::canonical_json_from;
use crate::dht::{
    dht_key_device, dht_key_device_revocation, dht_key_devices, dht_key_genesis,
    dht_key_identity, dht_key_revocation, Dht,
};
use crate::encoding::{base64_decode, base64_encode, crockford_base32_encode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};
use crate::admin_types::KeyRotationResult;

const IDOC_MAGIC: &[u8; 4] = b"IDOC";
const IDOC_VERSION: u8 = 1;
const IDOC_DOMAIN_SEPARATOR: &[u8] = b"post-urbit:idoc:v1:";
const KEY_REVOCATION_DOMAIN: &[u8] = b"post-urbit:key-revocation:v1:";
const IDENTITY_REVOCATION_DOMAIN: &[u8] = b"post-urbit:identity-revocation:v1:";
const DEVICE_REVOCATION_DOMAIN: &[u8] = b"post-urbit:device-revocation:v1:";
const DEVICE_DOC_DOMAIN: &[u8] = b"post-urbit:device-doc:v1:";
const DEVICE_INDEX_DOMAIN: &[u8] = b"post-urbit:device-index:v1:";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IdentityDocument {
    pub version: u8,
    pub iid: String,
    pub sequence: String,
    pub timestamp: String,
    pub keys: Keys,
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    #[serde(default)]
    pub claims: Claims,
    pub recovery: Recovery,
    #[serde(default = "empty_object")]
    pub extensions: serde_json::Value,
    pub recovery_proof: Option<serde_json::Value>,
    pub signatures: Signatures,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Keys {
    pub signing: SigningKeys,
    pub encryption: EncryptionKeys,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SigningKeys {
    pub genesis: String,
    pub current: String,
    pub previous: Option<String>,
    #[serde(default)]
    pub history: Vec<SigningKeyHistory>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SigningKeyHistory {
    pub key: String,
    pub valid_from: String,
    pub valid_until: String,
    pub expires_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EncryptionKeys {
    pub current: String,
    #[serde(default)]
    pub previous: Vec<EncryptionKeyHistory>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EncryptionKeyHistory {
    pub key: String,
    pub valid_from: String,
    pub valid_until: String,
    pub expires_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Endpoint {
    #[serde(rename = "type")]
    pub endpoint_type: String,
    pub host: String,
    pub port: u16,
    pub priority: u16,
    pub transport: String,
    #[serde(default)]
    pub relay_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Claims {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Recovery {
    pub method: String,
    #[serde(default = "empty_object")]
    pub config: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Signatures {
    pub current: String,
    pub previous: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum RevocationDocument {
    #[serde(rename = "key_revocation")]
    Key(KeyRevocation),
    #[serde(rename = "identity_revocation")]
    Identity(IdentityRevocation),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyRevocation {
    pub iid: String,
    pub revoked_key: String,
    pub revoked_key_type: String,
    pub reason: String,
    pub effective_at: String,
    pub replacement_document: IdentityDocument,
    pub signatures: KeyRevocationSignatures,
    pub recovery_proof: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyRevocationSignatures {
    pub by_current_signing_key: Option<String>,
    pub by_new_signing_key: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IdentityRevocation {
    pub iid: String,
    pub reason: String,
    pub message: Option<String>,
    pub effective_at: String,
    pub successor_iid: Option<String>,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceRevocation {
    pub did: String,
    pub iid: String,
    pub revoked_at: String,
    pub reason: String,
    pub signature_by_identity: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceDocument {
    pub version: u8,
    pub did: String,
    pub iid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    pub device_signing_key: String,
    pub endpoints: Vec<Endpoint>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub capabilities: Vec<String>,
    pub signature_by_identity: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceIndexEntry {
    pub did: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    pub last_seen: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceIndex {
    pub iid: String,
    pub devices: Vec<DeviceIndexEntry>,
    pub updated_at: String,
    pub signature: String,
}

pub struct IdentityManager {
    data_dir: String,
    inner: tokio::sync::RwLock<IdentityState>,
}

impl IdentityManager {
    pub async fn new(data_dir: &str) -> Result<Self> {
        let key_path = Path::new(data_dir).join("identity_signing.key");
        let enc_key_path = Path::new(data_dir).join("identity_encryption.key");
        let doc_path = Path::new(data_dir).join("identity.json");

        let signing_key = if key_path.exists() {
            let key_bytes = tokio::fs::read(&key_path).await?;
            SigningKey::from_bytes(&key_bytes.try_into().map_err(|_| {
                PostUrbitError::InvalidInput("signing key length")
            })?)
        } else {
            let key = SigningKey::generate(&mut rand::rngs::OsRng);
            tokio::fs::create_dir_all(data_dir).await?;
            tokio::fs::write(&key_path, key.to_bytes()).await?;
            key
        };

        let encryption_key = if enc_key_path.exists() {
            let key_bytes = tokio::fs::read(&enc_key_path).await?;
            let key_array: [u8; 32] = key_bytes
                .try_into()
                .map_err(|_| PostUrbitError::InvalidInput("encryption key length"))?;
            StaticSecret::from(key_array)
        } else {
            let key = StaticSecret::random_from_rng(rand::rngs::OsRng);
            tokio::fs::create_dir_all(data_dir).await?;
            tokio::fs::write(&enc_key_path, key.to_bytes()).await?;
            key
        };

        let document = if doc_path.exists() {
            let doc_json = tokio::fs::read_to_string(&doc_path).await?;
            serde_json::from_str(&doc_json)
                .map_err(|_| PostUrbitError::InvalidInput("identity document json"))?
        } else {
            Self::create_genesis_document(&signing_key, &encryption_key, &doc_path).await?
        };

        let meta_path = Path::new(data_dir).join("identity_meta.json");
        let (signing_key_valid_from, encryption_key_valid_from) = if meta_path.exists() {
            let meta_json = tokio::fs::read_to_string(&meta_path).await?;
            let meta: IdentityMeta = serde_json::from_str(&meta_json)
                .map_err(|_| PostUrbitError::InvalidInput("identity meta json"))?;
            (meta.signing_key_valid_from, meta.encryption_key_valid_from)
        } else {
            let seq = parse_sequence(&document.sequence)? as u64;
            (seq, seq)
        };

        let state = IdentityState {
            document,
            signing_key,
            encryption_key,
            signing_key_valid_from,
            encryption_key_valid_from,
        };

        Ok(Self {
            data_dir: data_dir.to_string(),
            inner: tokio::sync::RwLock::new(state),
        })
    }

    async fn create_genesis_document(
        signing_key: &SigningKey,
        encryption_key: &StaticSecret,
        doc_path: &Path,
    ) -> Result<IdentityDocument> {
        let verifying_key = signing_key.verifying_key();
        let iid = derive_iid(&verifying_key);

        let encryption_pub = PublicKey::from(encryption_key);

        let mut document = IdentityDocument {
            version: 1,
            iid,
            sequence: "0".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            keys: Keys {
                signing: SigningKeys {
                    genesis: base64_encode(verifying_key.as_bytes()),
                    current: base64_encode(verifying_key.as_bytes()),
                    previous: None,
                    history: Vec::new(),
                },
                encryption: EncryptionKeys {
                    current: base64_encode(encryption_pub.as_bytes()),
                    previous: Vec::new(),
                },
            },
            endpoints: Vec::new(),
            claims: Claims::default(),
            recovery: Recovery {
                method: "none".to_string(),
                config: empty_object(),
            },
            extensions: empty_object(),
            recovery_proof: None,
            signatures: Signatures {
                current: String::new(),
                previous: None,
            },
        };

        document.signatures.current = sign_idoc(&document, signing_key)?;

        let doc_json = serde_json::to_string_pretty(&document)
            .map_err(|_| PostUrbitError::InvalidInput("serialize identity document"))?;
        tokio::fs::write(doc_path, &doc_json).await?;

        Ok(document)
    }

    pub async fn iid(&self) -> String {
        self.inner.read().await.document.iid.clone()
    }

    pub async fn identity_document(&self) -> IdentityDocument {
        self.inner.read().await.document.clone()
    }

    pub async fn update_claims(
        &self,
        name: Option<String>,
        avatar: Option<String>,
        bio: Option<String>,
    ) -> Result<IdentityDocument> {
        let mut state = self.inner.write().await;
        let mut document = state.document.clone();
        let next_sequence = parse_sequence(&document.sequence)? + 1;
        document.sequence = next_sequence.to_string();
        document.timestamp = Utc::now().to_rfc3339();
        document.claims.name = name;
        document.claims.avatar = avatar;
        document.claims.bio = bio;
        document.signatures.current = sign_idoc(&document, &state.signing_key)?;
        document.signatures.previous = None;
        state.document = document.clone();
        self.persist_state(&state).await?;
        Ok(document)
    }

    pub async fn update_recovery(&self, recovery: Recovery) -> Result<IdentityDocument> {
        let mut state = self.inner.write().await;
        let mut document = state.document.clone();
        let next_sequence = parse_sequence(&document.sequence)? + 1;
        document.sequence = next_sequence.to_string();
        document.timestamp = Utc::now().to_rfc3339();
        document.recovery = recovery;
        document.signatures.current = sign_idoc(&document, &state.signing_key)?;
        document.signatures.previous = None;
        state.document = document.clone();
        self.persist_state(&state).await?;
        Ok(document)
    }

    pub async fn rotate_signing_key(&self) -> Result<KeyRotationResult> {
        let mut state = self.inner.write().await;
        let mut document = state.document.clone();
        let old_signing_key = state.signing_key.clone();
        let previous_key = document.keys.signing.current.clone();

        let new_signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let new_public = new_signing_key.verifying_key();
        let next_sequence = parse_sequence(&document.sequence)? + 1;
        document.sequence = next_sequence.to_string();
        document.timestamp = Utc::now().to_rfc3339();
        document.keys.signing.previous = Some(previous_key.clone());
        document.keys.signing.current = base64_encode(new_public.as_bytes());
        document.signatures.current = sign_idoc(&document, &new_signing_key)?;
        document.signatures.previous = Some(sign_idoc(&document, &old_signing_key)?);

        state.signing_key = new_signing_key;
        state.signing_key_valid_from = next_sequence as u64;
        state.document = document.clone();
        self.persist_state(&state).await?;

        Ok(KeyRotationResult {
            success: true,
            new_key_fingerprint: fingerprint_key(&document.keys.signing.current)?,
            previous_key_fingerprint: fingerprint_key(&previous_key)?,
            rotated_at: document.timestamp.clone(),
        })
    }

    pub async fn rotate_encryption_key(&self) -> Result<KeyRotationResult> {
        let mut state = self.inner.write().await;
        let mut document = state.document.clone();
        let previous_key = document.keys.encryption.current.clone();

        let new_secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let new_public = PublicKey::from(&new_secret);
        let next_sequence = parse_sequence(&document.sequence)? + 1;
        document.sequence = next_sequence.to_string();
        document.timestamp = Utc::now().to_rfc3339();

        let expires_at = (Utc::now() + chrono::Duration::days(30)).to_rfc3339();
        document.keys.encryption.previous.push(EncryptionKeyHistory {
            key: previous_key.clone(),
            valid_from: state.encryption_key_valid_from.to_string(),
            valid_until: next_sequence.to_string(),
            expires_at,
        });
        document.keys.encryption.current = base64_encode(new_public.as_bytes());
        document.signatures.current = sign_idoc(&document, &state.signing_key)?;
        document.signatures.previous = None;

        state.encryption_key = new_secret;
        state.encryption_key_valid_from = next_sequence as u64;
        state.document = document.clone();
        self.persist_state(&state).await?;

        Ok(KeyRotationResult {
            success: true,
            new_key_fingerprint: fingerprint_key(&document.keys.encryption.current)?,
            previous_key_fingerprint: fingerprint_key(&previous_key)?,
            rotated_at: document.timestamp.clone(),
        })
    }

    pub fn verify_document(document: &IdentityDocument) -> Result<()> {
        validate_crockford_base32_lower(&document.iid)?;

        let current_key = base64_decode(&document.keys.signing.current)?;
        if current_key.len() != 32 {
            return Err(PostUrbitError::InvalidInput("signing key length"));
        }
        let verifying_key = VerifyingKey::from_bytes(
            current_key
                .as_slice()
                .try_into()
                .map_err(|_| PostUrbitError::InvalidInput("signing key length"))?,
        )
        .map_err(|_| PostUrbitError::InvalidInput("invalid signing key"))?;

        let signed_payload = signature_payload(document)?;
        let signature_bytes = base64_decode(&document.signatures.current)?;
        if signature_bytes.len() != 64 {
            return Err(PostUrbitError::InvalidInput("signature length"));
        }
        let signature = Signature::from_bytes(
            signature_bytes
                .as_slice()
                .try_into()
                .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
        );
        verifying_key
            .verify_strict(&signed_payload, &signature)
            .map_err(|_| PostUrbitError::Crypto("signature verification failed"))
    }

    pub async fn persist(&self) -> Result<()> {
        let state = self.inner.read().await;
        self.persist_state(&state).await
    }

    async fn persist_state(&self, state: &IdentityState) -> Result<()> {
        let doc_path = Path::new(&self.data_dir).join("identity.json");
        let doc_json = serde_json::to_string_pretty(&state.document)
            .map_err(|_| PostUrbitError::InvalidInput("serialize identity document"))?;
        tokio::fs::write(&doc_path, &doc_json).await?;
        let signing_path = Path::new(&self.data_dir).join("identity_signing.key");
        tokio::fs::write(&signing_path, state.signing_key.to_bytes()).await?;
        let enc_path = Path::new(&self.data_dir).join("identity_encryption.key");
        tokio::fs::write(&enc_path, state.encryption_key.to_bytes()).await?;
        let meta = IdentityMeta {
            signing_key_valid_from: state.signing_key_valid_from,
            encryption_key_valid_from: state.encryption_key_valid_from,
        };
        let meta_path = Path::new(&self.data_dir).join("identity_meta.json");
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|_| PostUrbitError::InvalidInput("serialize identity meta"))?;
        tokio::fs::write(&meta_path, &meta_json).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentityMeta {
    signing_key_valid_from: u64,
    encryption_key_valid_from: u64,
}

struct IdentityState {
    document: IdentityDocument,
    signing_key: SigningKey,
    encryption_key: StaticSecret,
    signing_key_valid_from: u64,
    encryption_key_valid_from: u64,
}

pub fn derive_iid(verifying_key: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifying_key.as_bytes());
    let hash = hasher.finalize();
    crockford_base32_encode(&hash[..20]).to_lowercase()
}

pub fn derive_did(verifying_key: &VerifyingKey) -> String {
    derive_iid(verifying_key)
}

fn fingerprint_key(base64_key: &str) -> Result<String> {
    let raw = base64_decode(base64_key)?;
    if raw.len() != 32 {
        return Err(PostUrbitError::InvalidInput("key length"));
    }
    let hash = Sha256::digest(raw.as_slice());
    Ok(format!("sha256:{}", hex::encode(hash)))
}

pub async fn publish_identity(dht: &dyn Dht, document: &IdentityDocument) -> Result<()> {
    let ttl = Duration::from_secs(60 * 60 * 24);
    let idoc = encode_idoc_envelope(document)?;
    let key = dht_key_identity(&document.iid);
    dht.put(&key, idoc, ttl).await?;
    Ok(())
}

pub async fn publish_genesis(dht: &dyn Dht, document: &IdentityDocument) -> Result<()> {
    if document.sequence != "0" {
        return Err(PostUrbitError::InvalidInput("genesis sequence must be 0"));
    }
    let idoc = encode_idoc_envelope(document)?;
    let key = dht_key_genesis(&document.iid);
    let existing = dht.get_all(&key).await?;
    if !existing.is_empty() && existing.iter().all(|value| value != &idoc) {
        return Err(PostUrbitError::InvalidInput("genesis key immutable"));
    }
    let ttl = Duration::from_secs(60 * 60 * 24);
    dht.put(&key, idoc.clone(), ttl).await?;
    publish_identity(dht, document).await?;
    Ok(())
}

pub async fn fetch_identity(dht: &dyn Dht, iid: &str) -> Result<Option<IdentityDocument>> {
    let key = dht_key_identity(iid);
    let values = dht.get_all(&key).await?;
    if values.is_empty() {
        return Ok(None);
    }

    let mut best: Option<IdentityDocument> = None;
    let mut best_seq: u64 = 0;
    let mut best_raw: Vec<u8> = Vec::new();

    for value in values {
        let doc = decode_idoc_envelope(&value)?;
        if IdentityManager::verify_document(&doc).is_err() {
            continue;
        }
        let seq = parse_sequence(&doc.sequence)?;
        if best.is_none() || seq > best_seq {
            best_seq = seq;
            best_raw = value;
            best = Some(doc);
            continue;
        }
        if seq == best_seq && best_raw != value {
            return Err(PostUrbitError::InvalidInput(
                "identity conflict at same sequence",
            ));
        }
    }

    let Some(best_doc) = best else {
        return Ok(None);
    };

    let revocations = fetch_revocations(dht, iid).await?;
    let mut chosen: Option<(chrono::DateTime<Utc>, RevocationDocument)> = None;
    for revocation in revocations {
        let result = match &revocation {
            RevocationDocument::Key(doc) => verify_key_revocation(doc),
            RevocationDocument::Identity(doc) => {
                verify_identity_revocation(doc, &best_doc.keys.signing.current)
            }
        };
        if result.is_err() {
            continue;
        }
        let effective_at = match &revocation {
            RevocationDocument::Key(doc) => parse_rfc3339(&doc.effective_at),
            RevocationDocument::Identity(doc) => parse_rfc3339(&doc.effective_at),
        };
        let Ok(effective_at) = effective_at else {
            continue;
        };
        let replace = match &chosen {
            Some((current, _)) => effective_at < *current,
            None => true,
        };
        if replace {
            chosen = Some((effective_at, revocation));
        }
    }

    if let Some((_ts, revocation)) = chosen {
        match revocation {
            RevocationDocument::Identity(_) => {
                return Err(PostUrbitError::InvalidInput("identity revoked"));
            }
            RevocationDocument::Key(doc) => {
                return Ok(Some(doc.replacement_document.clone()));
            }
        }
    }

    Ok(Some(best_doc))
}

async fn fetch_revocations(dht: &dyn Dht, iid: &str) -> Result<Vec<RevocationDocument>> {
    let key = dht_key_revocation(iid);
    let values = dht.get_all(&key).await?;
    let mut out = Vec::new();
    for value in values {
        if let Ok(doc) = serde_json::from_slice::<RevocationDocument>(&value) {
            out.push(doc);
        }
    }
    Ok(out)
}

pub fn encode_idoc_envelope(document: &IdentityDocument) -> Result<Vec<u8>> {
    let json = canonical_json_from(document)?;
    let payload = json.as_bytes();
    let length: u32 = payload
        .len()
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("idoc length"))?;

    let mut out = Vec::with_capacity(9 + payload.len());
    out.extend_from_slice(IDOC_MAGIC);
    out.push(IDOC_VERSION);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode_idoc_envelope(bytes: &[u8]) -> Result<IdentityDocument> {
    if bytes.len() < 9 {
        return Err(PostUrbitError::InvalidInput("idoc envelope too short"));
    }
    if &bytes[..4] != IDOC_MAGIC {
        return Err(PostUrbitError::InvalidInput("idoc magic"));
    }
    if bytes[4] != IDOC_VERSION {
        return Err(PostUrbitError::InvalidInput("idoc version"));
    }
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&bytes[5..9]);
    let length = u32::from_be_bytes(len_bytes) as usize;
    if bytes.len() != 9 + length {
        return Err(PostUrbitError::InvalidInput("idoc length mismatch"));
    }
    let payload = &bytes[9..];
    let doc: IdentityDocument = serde_json::from_slice(payload)
        .map_err(|_| PostUrbitError::InvalidInput("idoc json"))?;
    Ok(doc)
}

pub fn sign_idoc(document: &IdentityDocument, signing_key: &SigningKey) -> Result<String> {
    let signature_input = signature_payload(document)?;
    let signature = signing_key.sign(&signature_input);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

fn signature_payload(document: &IdentityDocument) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(document)
        .map_err(|_| PostUrbitError::InvalidInput("serialize idoc"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signatures");
    }
    let canonical = crate::canonical_json::canonical_json_value(&value)?;
    let mut out = Vec::with_capacity(IDOC_DOMAIN_SEPARATOR.len() + canonical.len());
    out.extend_from_slice(IDOC_DOMAIN_SEPARATOR);
    out.extend_from_slice(canonical.as_bytes());
    Ok(out)
}

pub fn key_revocation_signature_input(revocation: &KeyRevocation) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(revocation)
        .map_err(|_| PostUrbitError::InvalidInput("serialize revocation"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signatures");
    }
    revocation_signature_payload(&value, KEY_REVOCATION_DOMAIN)
}

pub fn identity_revocation_signature_input(revocation: &IdentityRevocation) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(revocation)
        .map_err(|_| PostUrbitError::InvalidInput("serialize revocation"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature");
    }
    revocation_signature_payload(&value, IDENTITY_REVOCATION_DOMAIN)
}

pub fn sign_key_revocation(revocation: &KeyRevocation, signing_key: &SigningKey) -> Result<String> {
    let payload = key_revocation_signature_input(revocation)?;
    let signature = signing_key.sign(&payload);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

pub fn sign_identity_revocation(
    revocation: &IdentityRevocation,
    signing_key: &SigningKey,
) -> Result<String> {
    let payload = identity_revocation_signature_input(revocation)?;
    let signature = signing_key.sign(&payload);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

pub fn device_revocation_signature_input(revocation: &DeviceRevocation) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(revocation)
        .map_err(|_| PostUrbitError::InvalidInput("serialize revocation"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature_by_identity");
    }
    revocation_signature_payload(&value, DEVICE_REVOCATION_DOMAIN)
}

pub fn sign_device_revocation(
    revocation: &DeviceRevocation,
    signing_key: &SigningKey,
) -> Result<String> {
    let payload = device_revocation_signature_input(revocation)?;
    let signature = signing_key.sign(&payload);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

pub fn verify_device_revocation(
    revocation: &DeviceRevocation,
    signing_key_base64: &str,
) -> Result<()> {
    validate_crockford_base32_lower(&revocation.iid)?;
    validate_crockford_base32_lower(&revocation.did)?;
    let payload = device_revocation_signature_input(revocation)?;
    verify_signature_with_key(&payload, &revocation.signature_by_identity, signing_key_base64)
}

pub fn verify_key_revocation(revocation: &KeyRevocation) -> Result<()> {
    validate_crockford_base32_lower(&revocation.iid)?;
    if revocation.replacement_document.iid != revocation.iid {
        return Err(PostUrbitError::InvalidInput("revocation iid mismatch"));
    }
    IdentityManager::verify_document(&revocation.replacement_document)?;

    let payload = key_revocation_signature_input(revocation)?;
    let new_key = revocation.replacement_document.keys.signing.current.clone();

    if revocation.revoked_key_type == "signing" {
        let Some(sig_new) = &revocation.signatures.by_new_signing_key else {
            return Err(PostUrbitError::InvalidInput("missing new signature"));
        };
        verify_signature_with_key(&payload, sig_new, &new_key)?;
        if let Some(sig_old) = &revocation.signatures.by_current_signing_key {
            verify_signature_with_key(&payload, sig_old, &revocation.revoked_key)?;
        }
        return Ok(());
    }

    if revocation.revoked_key_type == "encryption" {
        let Some(sig_current) = &revocation.signatures.by_current_signing_key else {
            return Err(PostUrbitError::InvalidInput("missing current signature"));
        };
        verify_signature_with_key(&payload, sig_current, &new_key)?;
        return Ok(());
    }

    Err(PostUrbitError::InvalidInput("revocation key type"))
}

pub fn verify_identity_revocation(
    revocation: &IdentityRevocation,
    signing_key: &str,
) -> Result<()> {
    validate_crockford_base32_lower(&revocation.iid)?;
    let payload = identity_revocation_signature_input(revocation)?;
    verify_signature_with_key(&payload, &revocation.signature, signing_key)
}

pub fn derive_device_did(device_signing_key_b64: &str) -> Result<String> {
    let key_bytes = base64_decode(device_signing_key_b64)?;
    if key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("device signing key length"));
    }
    let mut hasher = Sha256::new();
    hasher.update(&key_bytes);
    let hash = hasher.finalize();
    Ok(crockford_base32_encode(&hash[..20]).to_lowercase())
}

pub fn device_document_signature_input(doc: &DeviceDocument) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(doc)
        .map_err(|_| PostUrbitError::InvalidInput("serialize device doc"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature_by_identity");
    }
    revocation_signature_payload(&value, DEVICE_DOC_DOMAIN)
}

pub fn sign_device_document(doc: &DeviceDocument, signing_key: &SigningKey) -> Result<String> {
    let payload = device_document_signature_input(doc)?;
    let signature = signing_key.sign(&payload);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

pub fn verify_device_document_with_keys(
    doc: &DeviceDocument,
    signing_keys: &[String],
    now: DateTime<Utc>,
) -> Result<()> {
    validate_crockford_base32_lower(&doc.did)?;
    validate_crockford_base32_lower(&doc.iid)?;
    let derived = derive_device_did(&doc.device_signing_key)?;
    if derived != doc.did {
        return Err(PostUrbitError::InvalidInput("device did mismatch"));
    }
    if let Some(expires_at) = &doc.expires_at {
        let expires = parse_rfc3339(expires_at)?;
        if expires < now {
            return Err(PostUrbitError::InvalidInput("device doc expired"));
        }
    }
    let payload = device_document_signature_input(doc)?;
    for key in signing_keys {
        if verify_signature_with_key(&payload, &doc.signature_by_identity, key).is_ok() {
            return Ok(());
        }
    }
    Err(PostUrbitError::Crypto("device doc signature invalid"))
}

pub fn verify_device_document(
    doc: &DeviceDocument,
    identity: &IdentityDocument,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut keys = Vec::new();
    keys.push(identity.keys.signing.current.clone());
    if let Some(prev) = identity.keys.signing.previous.clone() {
        keys.push(prev);
    }
    for hist in &identity.keys.signing.history {
        keys.push(hist.key.clone());
    }
    verify_device_document_with_keys(doc, &keys, now)
}

pub fn device_index_signature_input(index: &DeviceIndex) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(index)
        .map_err(|_| PostUrbitError::InvalidInput("serialize device index"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature");
    }
    revocation_signature_payload(&value, DEVICE_INDEX_DOMAIN)
}

pub fn sign_device_index(index: &DeviceIndex, signing_key: &SigningKey) -> Result<String> {
    let payload = device_index_signature_input(index)?;
    let signature = signing_key.sign(&payload);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

pub fn verify_device_index_with_keys(
    index: &DeviceIndex,
    signing_keys: &[String],
    now: DateTime<Utc>,
) -> Result<()> {
    validate_crockford_base32_lower(&index.iid)?;
    let _ = parse_rfc3339(&index.updated_at)?;
    for entry in &index.devices {
        validate_crockford_base32_lower(&entry.did)?;
        let last_seen = parse_rfc3339(&entry.last_seen)?;
        if last_seen > now + chrono::Duration::minutes(5) {
            return Err(PostUrbitError::InvalidInput("device index future"));
        }
    }
    let payload = device_index_signature_input(index)?;
    for key in signing_keys {
        if verify_signature_with_key(&payload, &index.signature, key).is_ok() {
            return Ok(());
        }
    }
    Err(PostUrbitError::Crypto("device index signature invalid"))
}

pub async fn publish_device_document(dht: &dyn Dht, doc: &DeviceDocument) -> Result<()> {
    let canonical = canonical_json_from(doc)?;
    let ttl = Duration::from_secs(60 * 60 * 24);
    let key = dht_key_device(&doc.did);
    dht.put(&key, canonical.into_bytes(), ttl).await?;
    Ok(())
}

pub async fn publish_device_index(dht: &dyn Dht, index: &DeviceIndex) -> Result<()> {
    let canonical = canonical_json_from(index)?;
    let ttl = Duration::from_secs(60 * 60 * 24);
    let key = dht_key_devices(&index.iid);
    dht.put(&key, canonical.into_bytes(), ttl).await?;
    Ok(())
}

pub async fn fetch_device_document(
    dht: &dyn Dht,
    did: &str,
    signing_keys: &[String],
    now: DateTime<Utc>,
) -> Result<Option<DeviceDocument>> {
    let key = dht_key_device(did);
    let values = dht.get_all(&key).await?;
    let mut chosen: Option<(DateTime<Utc>, DeviceDocument)> = None;
    for value in values {
        let Ok(doc) = serde_json::from_slice::<DeviceDocument>(&value) else {
            continue;
        };
        if verify_device_document_with_keys(&doc, signing_keys, now).is_err() {
            continue;
        }
        let Ok(updated_at) = parse_rfc3339(&doc.updated_at) else {
            continue;
        };
        let replace = match &chosen {
            Some((current, _)) => updated_at > *current,
            None => true,
        };
        if replace {
            chosen = Some((updated_at, doc));
        }
    }
    Ok(chosen.map(|(_, doc)| doc))
}

pub async fn fetch_device_index(
    dht: &dyn Dht,
    iid: &str,
    signing_keys: &[String],
    now: DateTime<Utc>,
) -> Result<Option<DeviceIndex>> {
    let key = dht_key_devices(iid);
    let values = dht.get_all(&key).await?;
    let mut chosen: Option<(DateTime<Utc>, DeviceIndex)> = None;
    for value in values {
        let Ok(index) = serde_json::from_slice::<DeviceIndex>(&value) else {
            continue;
        };
        if verify_device_index_with_keys(&index, signing_keys, now).is_err() {
            continue;
        }
        let Ok(updated_at) = parse_rfc3339(&index.updated_at) else {
            continue;
        };
        let replace = match &chosen {
            Some((current, _)) => updated_at > *current,
            None => true,
        };
        if replace {
            chosen = Some((updated_at, index));
        }
    }
    Ok(chosen.map(|(_, doc)| doc))
}

pub async fn publish_revocation(
    dht: &dyn Dht,
    revocation: &RevocationDocument,
) -> Result<()> {
    let canonical = canonical_json_from(revocation)?;
    let ttl = Duration::from_secs(60 * 60 * 24 * 365);
    let iid = match revocation {
        RevocationDocument::Key(doc) => doc.iid.as_str(),
        RevocationDocument::Identity(doc) => doc.iid.as_str(),
    };
    let key = dht_key_revocation(iid);
    dht.put(&key, canonical.into_bytes(), ttl).await?;
    Ok(())
}

pub async fn publish_device_revocation(
    dht: &dyn Dht,
    revocation: &DeviceRevocation,
) -> Result<()> {
    let canonical = canonical_json_from(revocation)?;
    let ttl = Duration::from_secs(60 * 60 * 24 * 365);
    let key = dht_key_device_revocation(&revocation.did);
    dht.put(&key, canonical.into_bytes(), ttl).await?;
    Ok(())
}

pub async fn fetch_device_revocation(
    dht: &dyn Dht,
    did: &str,
    signing_key_base64: &str,
) -> Result<Option<DeviceRevocation>> {
    let key = dht_key_device_revocation(did);
    let values = dht.get_all(&key).await?;
    let mut chosen: Option<(chrono::DateTime<Utc>, DeviceRevocation)> = None;
    for value in values {
        let Ok(doc) = serde_json::from_slice::<DeviceRevocation>(&value) else {
            continue;
        };
        if verify_device_revocation(&doc, signing_key_base64).is_err() {
            continue;
        }
        let Ok(ts) = parse_rfc3339(&doc.revoked_at) else {
            continue;
        };
        let replace = match &chosen {
            Some((current, _)) => ts < *current,
            None => true,
        };
        if replace {
            chosen = Some((ts, doc));
        }
    }
    Ok(chosen.map(|(_, doc)| doc))
}

fn revocation_signature_payload(
    value: &serde_json::Value,
    domain: &[u8],
) -> Result<Vec<u8>> {
    let canonical = crate::canonical_json::canonical_json_value(value)?;
    let mut out = Vec::with_capacity(domain.len() + canonical.len());
    out.extend_from_slice(domain);
    out.extend_from_slice(canonical.as_bytes());
    Ok(out)
}

fn verify_signature_with_key(
    payload: &[u8],
    signature_base64: &str,
    key_base64: &str,
) -> Result<()> {
    let signature_bytes = base64_decode(signature_base64)?;
    if signature_bytes.len() != 64 {
        return Err(PostUrbitError::InvalidInput("signature length"));
    }
    let key_bytes = base64_decode(key_base64)?;
    if key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("signing key length"));
    }
    let signature = Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );
    let verifying_key = VerifyingKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signing key length"))?,
    )
    .map_err(|_| PostUrbitError::InvalidInput("signing key parse"))?;
    verifying_key
        .verify_strict(payload, &signature)
        .map_err(|_| PostUrbitError::Crypto("signature invalid"))
}

fn parse_rfc3339(value: &str) -> Result<chrono::DateTime<Utc>> {
    value
        .parse::<chrono::DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))
}

fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

fn parse_sequence(value: &str) -> Result<u64> {
    if value.starts_with('0') && value != "0" {
        return Err(PostUrbitError::InvalidInput("sequence leading zeros"));
    }
    value
        .parse::<u64>()
        .map_err(|_| PostUrbitError::InvalidInput("sequence parse"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dht::MemoryDht;

    #[test]
    fn derive_iid_matches_test_vector() {
        let pubkey_hex = "e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();
        let verifying_key = VerifyingKey::from_bytes(
            pubkey_bytes.as_slice().try_into().unwrap(),
        )
        .unwrap();
        let iid = derive_iid(&verifying_key);
        // Base32 per RFC-0002 §2.1 (MSB-first 5-bit groups).
        assert_eq!(iid, "b1n7cfscgashm32xx7eaxw0y09gy0y2v");
    }

    #[test]
    fn derive_did_matches_test_vector() {
        let pubkey_hex = "ea0757f2720fa3459633c30eb2e0ab737656321c4803d849aa7f614239c28652";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();
        let verifying_key = VerifyingKey::from_bytes(
            pubkey_bytes.as_slice().try_into().unwrap(),
        )
        .unwrap();
        let did = derive_did(&verifying_key);
        assert_eq!(did, "42kbzq2tyab939amybd76bm8kfpzgn95");
    }

    #[test]
    fn idoc_signature_matches_test_vector() {
        let signing_seed = hex::decode(
            "033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40",
        )
        .unwrap();
        let signing_key = SigningKey::from_bytes(signing_seed.as_slice().try_into().unwrap());
        let verifying_key = signing_key.verifying_key();

        let enc_priv = hex::decode(
            "7ff8c1a741fd3c5253f5d6953cd78f5411f36507f8f653b498e19d381bf7877b",
        )
        .unwrap();
        let enc_priv: [u8; 32] = enc_priv.as_slice().try_into().unwrap();
        let enc_key = StaticSecret::from(enc_priv);
        let enc_pub = PublicKey::from(&enc_key);

        let mut doc = IdentityDocument {
            version: 1,
            iid: "b1anasr5h0bj3832xqexwy0f0987e1xb".to_string(),
            sequence: "0".to_string(),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            keys: Keys {
                signing: SigningKeys {
                    genesis: base64_encode(verifying_key.as_bytes()),
                    current: base64_encode(verifying_key.as_bytes()),
                    previous: None,
                    history: Vec::new(),
                },
                encryption: EncryptionKeys {
                    current: base64_encode(enc_pub.as_bytes()),
                    previous: Vec::new(),
                },
            },
            endpoints: Vec::new(),
            claims: Claims {
                name: Some("Alice".to_string()),
                avatar: None,
                bio: None,
            },
            recovery: Recovery {
                method: "none".to_string(),
                config: empty_object(),
            },
            extensions: empty_object(),
            recovery_proof: None,
            signatures: Signatures {
                current: String::new(),
                previous: None,
            },
        };

        let signature = sign_idoc(&doc, &signing_key).unwrap();
        doc.signatures.current = signature.clone();
        assert_eq!(
            signature,
            "mScYPiZ8NTMXk+TnOh/6gQph+MAmV9nUnX6GirzDCM2kVqFmY4DCuTAYdMfM3Mh043oQfPv7V7tvEnlC4yUNCQ"
        );
    }

    #[test]
    fn idoc_envelope_round_trip() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let tmp = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let doc = rt
            .block_on(IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            ))
            .unwrap();

        let envelope = encode_idoc_envelope(&doc).unwrap();
        let decoded = decode_idoc_envelope(&envelope).unwrap();
        assert_eq!(decoded.iid, doc.iid);
    }

    #[test]
    fn publish_genesis_prevents_overwrite() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();
        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let doc = IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
            .unwrap();
            publish_genesis(&dht, &doc).await.unwrap();

            let mut altered = doc.clone();
            altered.timestamp = "2025-01-16T00:00:00Z".to_string();
            let err = publish_genesis(&dht, &altered).await.unwrap_err();
            assert!(matches!(err, PostUrbitError::InvalidInput(_)));
        });
    }

    #[test]
    fn fetch_identity_applies_key_revocation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();
        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let doc = IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
            .unwrap();

            publish_identity(&dht, &doc).await.unwrap();

            let new_signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let new_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let new_enc_pub = PublicKey::from(&new_enc);

            let mut replacement = doc.clone();
            replacement.sequence = "1".to_string();
            replacement.timestamp = "2025-01-16T00:00:00Z".to_string();
            replacement.keys.signing.previous = Some(replacement.keys.signing.current.clone());
            replacement.keys.signing.current =
                base64_encode(new_signing_key.verifying_key().as_bytes());
            replacement.keys.encryption.current = base64_encode(new_enc_pub.as_bytes());
            replacement.signatures.current = sign_idoc(&replacement, &new_signing_key).unwrap();

            let mut revocation = KeyRevocation {
                iid: doc.iid.clone(),
                revoked_key: doc.keys.signing.current.clone(),
                revoked_key_type: "signing".to_string(),
                reason: "compromised".to_string(),
                effective_at: "2025-01-16T00:00:00Z".to_string(),
                replacement_document: replacement.clone(),
                signatures: KeyRevocationSignatures {
                    by_current_signing_key: None,
                    by_new_signing_key: None,
                },
                recovery_proof: None,
            };

            let sig_old = sign_key_revocation(&revocation, &signing_key).unwrap();
            let sig_new = sign_key_revocation(&revocation, &new_signing_key).unwrap();
            revocation.signatures.by_current_signing_key = Some(sig_old);
            revocation.signatures.by_new_signing_key = Some(sig_new);

            publish_revocation(&dht, &RevocationDocument::Key(revocation))
                .await
                .unwrap();

            let fetched = fetch_identity(&dht, doc.iid.as_str())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fetched.iid, replacement.iid);
            assert_eq!(fetched.keys.signing.current, replacement.keys.signing.current);
        });
    }

    #[test]
    fn device_revocation_selects_earliest() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();
        rt.block_on(async {
            let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let signing_key_b64 = base64_encode(signing_key.verifying_key().as_bytes());
            let did = "42kbzq2tyab939amybd76bm8kfpzgn95";
            let iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";

            let mut first = DeviceRevocation {
                did: did.to_string(),
                iid: iid.to_string(),
                revoked_at: "2025-01-10T00:00:00Z".to_string(),
                reason: "lost".to_string(),
                signature_by_identity: String::new(),
            };
            first.signature_by_identity = sign_device_revocation(&first, &signing_key).unwrap();

            let mut second = DeviceRevocation {
                did: did.to_string(),
                iid: iid.to_string(),
                revoked_at: "2025-01-12T00:00:00Z".to_string(),
                reason: "stolen".to_string(),
                signature_by_identity: String::new(),
            };
            second.signature_by_identity = sign_device_revocation(&second, &signing_key).unwrap();

            publish_device_revocation(&dht, &second).await.unwrap();
            publish_device_revocation(&dht, &first).await.unwrap();

            let chosen = fetch_device_revocation(&dht, did, &signing_key_b64)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(chosen.revoked_at, "2025-01-10T00:00:00Z");
        });
    }

    #[test]
    fn fetch_identity_conflict_same_sequence() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();
        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let doc = IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
            .unwrap();
            publish_identity(&dht, &doc).await.unwrap();

            let mut conflicting = doc.clone();
            conflicting.timestamp = "2025-01-16T00:00:00Z".to_string();
            conflicting.signatures.current = sign_idoc(&conflicting, &signing_key).unwrap();
            publish_identity(&dht, &conflicting).await.unwrap();

            let err = fetch_identity(&dht, doc.iid.as_str()).await.unwrap_err();
            assert!(matches!(err, PostUrbitError::InvalidInput(_)));
        });
    }

    #[test]
    fn device_document_signature_round_trip() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let doc = rt
            .block_on(async {
                let tmp = tempfile::tempdir().unwrap();
                IdentityManager::create_genesis_document(
                    &signing_key,
                    &enc_key,
                    &tmp.path().join("idoc.json"),
                )
                .await
            })
            .unwrap();

        let device_signing = SigningKey::generate(&mut rand::rngs::OsRng);
        let device_key_b64 = base64_encode(device_signing.verifying_key().as_bytes());
        let did = derive_device_did(&device_key_b64).unwrap();

        let mut device_doc = DeviceDocument {
            version: 1,
            did: did.clone(),
            iid: doc.iid.clone(),
            device_name: Some("laptop".to_string()),
            device_signing_key: device_key_b64,
            endpoints: Vec::new(),
            created_at: "2025-01-15T00:00:00Z".to_string(),
            updated_at: "2025-01-15T00:00:00Z".to_string(),
            expires_at: None,
            capabilities: vec!["messaging".to_string()],
            signature_by_identity: String::new(),
        };
        device_doc.signature_by_identity = sign_device_document(&device_doc, &signing_key).unwrap();
        verify_device_document(&device_doc, &doc, "2025-01-15T00:00:00Z".parse().unwrap())
            .unwrap();
    }

    #[test]
    fn device_index_signature_round_trip() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let doc = rt
            .block_on(async {
                let tmp = tempfile::tempdir().unwrap();
                IdentityManager::create_genesis_document(
                    &signing_key,
                    &enc_key,
                    &tmp.path().join("idoc.json"),
                )
                .await
            })
            .unwrap();

        let mut index = DeviceIndex {
            iid: doc.iid.clone(),
            devices: vec![DeviceIndexEntry {
                did: "42kbzq2tyab939amybd76bm8kfpzgn95".to_string(),
                device_name: Some("phone".to_string()),
                last_seen: "2025-01-15T00:00:00Z".to_string(),
            }],
            updated_at: "2025-01-15T00:00:00Z".to_string(),
            signature: String::new(),
        };
        index.signature = sign_device_index(&index, &signing_key).unwrap();
        let keys = vec![doc.keys.signing.current.clone()];
        verify_device_index_with_keys(&index, &keys, "2025-01-15T00:00:00Z".parse().unwrap())
            .unwrap();
    }

    #[test]
    fn fetch_device_document_prefers_latest_updated_at() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();
        rt.block_on(async {
            let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let tmp = tempfile::tempdir().unwrap();
            let identity = IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
            .unwrap();
            let keys = vec![identity.keys.signing.current.clone()];

            let device_signing = SigningKey::generate(&mut rand::rngs::OsRng);
            let device_key_b64 = base64_encode(device_signing.verifying_key().as_bytes());
            let did = derive_device_did(&device_key_b64).unwrap();

            let mut older = DeviceDocument {
                version: 1,
                did: did.clone(),
                iid: identity.iid.clone(),
                device_name: None,
                device_signing_key: device_key_b64.clone(),
                endpoints: Vec::new(),
                created_at: "2025-01-10T00:00:00Z".to_string(),
                updated_at: "2025-01-10T00:00:00Z".to_string(),
                expires_at: None,
                capabilities: Vec::new(),
                signature_by_identity: String::new(),
            };
            older.signature_by_identity = sign_device_document(&older, &signing_key).unwrap();
            publish_device_document(&dht, &older).await.unwrap();

            let mut newer = older.clone();
            newer.updated_at = "2025-01-12T00:00:00Z".to_string();
            newer.signature_by_identity = sign_device_document(&newer, &signing_key).unwrap();
            publish_device_document(&dht, &newer).await.unwrap();

            let fetched = fetch_device_document(
                &dht,
                &did,
                &keys,
                "2025-01-15T00:00:00Z".parse().unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(fetched.updated_at, "2025-01-12T00:00:00Z");
        });
    }
}

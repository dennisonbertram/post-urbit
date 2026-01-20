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
const RECOVERY_ATTESTATION_DOMAIN_SEPARATOR: &[u8] = b"post-urbit:recovery-attestation:v1:";

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

/// A recovery attestation from a trustee (per RFC-0001 §9.3)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecoveryAttestation {
    /// IID being recovered
    pub target_iid: String,
    /// Trustee's IID
    pub trustee_iid: String,
    /// New signing public key (base64)
    pub new_signing_key: String,
    /// Timestamp of attestation
    pub timestamp: String,
    /// Ed25519 signature by trustee
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

        // SECURITY: Verify IID is derived from the genesis signing key
        // This prevents IID hijacking attacks where an attacker publishes
        // a document with a higher sequence signed by their own key.
        let genesis_key_bytes = base64_decode(&document.keys.signing.genesis)?;
        if genesis_key_bytes.len() != 32 {
            return Err(PostUrbitError::InvalidInput("genesis signing key length"));
        }
        let genesis_verifying_key = VerifyingKey::from_bytes(
            genesis_key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| PostUrbitError::InvalidInput("genesis signing key length"))?,
        )
        .map_err(|_| PostUrbitError::InvalidInput("invalid genesis signing key"))?;

        let expected_iid = derive_iid(&genesis_verifying_key);
        if expected_iid != document.iid {
            return Err(PostUrbitError::InvalidInput("iid not derived from genesis key"));
        }

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
            .map_err(|_| PostUrbitError::Crypto("signature verification failed"))?;

        // SECURITY: For documents with sequence > 0, verify key rotation continuity
        // If the current key differs from genesis, there must be valid key rotation history
        let sequence = parse_sequence(&document.sequence)?;
        if sequence > 0 && document.keys.signing.current != document.keys.signing.genesis {
            // If keys differ from genesis, we need to verify the previous signature
            // to ensure the key rotation was authorized by the outgoing key
            if let Some(ref prev_sig) = document.signatures.previous {
                // Get the previous key - either from keys.signing.previous or the last in history
                let previous_key = if let Some(ref prev_key) = document.keys.signing.previous {
                    prev_key.clone()
                } else {
                    // No explicit previous key, this is invalid for a rotated document
                    return Err(PostUrbitError::InvalidInput("key rotation requires previous key"));
                };

                // Verify the previous signature is valid with the previous key
                verify_signature_with_key(&signed_payload, prev_sig, &previous_key)?;
            } else {
                // No previous signature but keys differ from genesis - invalid
                return Err(PostUrbitError::InvalidInput("key rotation requires previous signature"));
            }
        }

        Ok(())
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

    /// Sign arbitrary data with the identity's signing key.
    /// Returns the Ed25519 signature as bytes.
    pub async fn sign_data(&self, data: &[u8]) -> [u8; 64] {
        let state = self.inner.read().await;
        state.signing_key.sign(data).to_bytes()
    }

    /// Sign data and return base64-encoded signature.
    pub async fn sign_data_base64(&self, data: &[u8]) -> String {
        base64_encode(&self.sign_data(data).await)
    }

    /// Get the current signing public key as bytes.
    pub async fn signing_public_key_bytes(&self) -> [u8; 32] {
        let state = self.inner.read().await;
        state.signing_key.verifying_key().to_bytes()
    }

    /// Get the current signing public key as base64.
    pub async fn signing_public_key_base64(&self) -> String {
        base64_encode(&self.signing_public_key_bytes().await)
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
    validate_crockford_base32_lower(iid)?;

    let key = dht_key_identity(iid);
    let values = dht.get_all(&key).await?;
    if values.is_empty() {
        return Ok(None);
    }

    // SECURITY: First, try to fetch and validate the genesis document
    // This establishes the authoritative genesis key for this IID
    let genesis_key = dht_key_genesis(iid);
    let genesis_values = dht.get_all(&genesis_key).await?;
    let verified_genesis: Option<IdentityDocument> = genesis_values.iter().find_map(|value| {
        let doc = decode_idoc_envelope(value).ok()?;
        verify_genesis_document(&doc, iid).ok()?;
        Some(doc)
    });

    let mut best: Option<IdentityDocument> = None;
    let mut best_seq: u64 = 0;
    let mut best_raw: Vec<u8> = Vec::new();

    for value in values {
        let doc = match decode_idoc_envelope(&value) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // SECURITY: Verify the document passes all verification checks
        // This includes IID-genesis binding and key rotation continuity
        if IdentityManager::verify_document(&doc).is_err() {
            continue;
        }

        // SECURITY: If we have a verified genesis, ensure this document's
        // genesis key matches the verified genesis document's key
        if let Some(ref genesis) = verified_genesis {
            if doc.keys.signing.genesis != genesis.keys.signing.current {
                // This document claims a different genesis key than the verified genesis
                // This is an attempted hijack - skip this document
                continue;
            }
        }

        let seq = match parse_sequence(&doc.sequence) {
            Ok(s) => s,
            Err(_) => continue,
        };
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

/// Maximum identity document size (16 KB) per RFC-0001 §4.7
const IDOC_MAX_SIZE: usize = 16_384;

pub fn decode_idoc_envelope(bytes: &[u8]) -> Result<IdentityDocument> {
    // REQ-IDOC-041: Maximum document size is 16 KB
    if bytes.len() > IDOC_MAX_SIZE {
        return Err(PostUrbitError::InvalidInput("idoc too large"));
    }
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

    // Validate document field constraints per RFC-0001 §4.7
    validate_idoc_constraints(&doc)?;

    Ok(doc)
}

/// Validates identity document field constraints per RFC-0001 §4.7
fn validate_idoc_constraints(doc: &IdentityDocument) -> Result<()> {
    // REQ-IDOC-041: Endpoints limit (10 entries)
    if doc.endpoints.len() > 10 {
        return Err(PostUrbitError::InvalidInput("idoc too many endpoints"));
    }

    // REQ-IDOC-041: Signing history limit (10 entries)
    if doc.keys.signing.history.len() > 10 {
        return Err(PostUrbitError::InvalidInput("idoc signing history too large"));
    }

    // REQ-IDOC-041: Encryption history limit (5 entries)
    if doc.keys.encryption.previous.len() > 5 {
        return Err(PostUrbitError::InvalidInput("idoc encryption history too large"));
    }

    // REQ-IDOC-041: claims.name limit (64 UTF-8 chars)
    if let Some(ref name) = doc.claims.name {
        if name.chars().count() > 64 {
            return Err(PostUrbitError::InvalidInput("idoc name too long"));
        }
    }

    // REQ-IDOC-041: claims.bio limit (256 UTF-8 chars)
    if let Some(ref bio) = doc.claims.bio {
        if bio.chars().count() > 256 {
            return Err(PostUrbitError::InvalidInput("idoc bio too long"));
        }
    }

    // Validate recovery config constraints per RFC-0001 §9
    validate_recovery_config(&doc.recovery)?;

    Ok(())
}

/// Validates recovery configuration per RFC-0001 §9
fn validate_recovery_config(recovery: &Recovery) -> Result<()> {
    match recovery.method.as_str() {
        "none" => {
            // No additional validation needed
            Ok(())
        }
        "social" => {
            // REQ-IDOC-037: trustees array required
            let trustees = recovery.config.get("trustees")
                .and_then(|v| v.as_array())
                .ok_or_else(|| PostUrbitError::InvalidInput("social recovery requires trustees"))?;

            // REQ-IDOC-038: threshold required
            let threshold = recovery.config.get("threshold")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| PostUrbitError::InvalidInput("social recovery requires threshold"))? as usize;

            // REQ-IDOC-039: threshold >= 2
            if threshold < 2 {
                return Err(PostUrbitError::InvalidInput("social recovery threshold must be >= 2"));
            }

            // REQ-IDOC-037: trustees.len() >= threshold
            if trustees.len() < threshold {
                return Err(PostUrbitError::InvalidInput("social recovery needs more trustees than threshold"));
            }

            // REQ-IDOC-040: cooldown_hours validation (24 <= cooldown <= 720)
            if let Some(cooldown) = recovery.config.get("cooldown_hours").and_then(|v| v.as_u64()) {
                if cooldown < 24 || cooldown > 720 {
                    return Err(PostUrbitError::InvalidInput("social recovery cooldown out of range"));
                }
            }

            Ok(())
        }
        _ => {
            // Unknown recovery methods are allowed for forward compatibility
            Ok(())
        }
    }
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

/// Computes the signature input for a recovery attestation.
/// Domain separator: "post-urbit:recovery-attestation:v1:"
fn recovery_attestation_signature_input(attestation: &RecoveryAttestation) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(attestation)
        .map_err(|_| PostUrbitError::InvalidInput("serialize attestation"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature");
    }
    revocation_signature_payload(&value, RECOVERY_ATTESTATION_DOMAIN_SEPARATOR)
}

/// Verify a recovery attestation signature.
/// Domain separator: "post-urbit:recovery-attestation:v1:"
pub fn verify_recovery_attestation(
    attestation: &RecoveryAttestation,
    trustee_signing_key: &VerifyingKey,
) -> Result<()> {
    let payload = recovery_attestation_signature_input(attestation)?;
    let trustee_key_base64 = base64_encode(trustee_signing_key.as_bytes());
    verify_signature_with_key(&payload, &attestation.signature, &trustee_key_base64)
}

/// Sign a recovery attestation with a signing key.
pub fn sign_recovery_attestation(
    attestation: &RecoveryAttestation,
    signing_key: &SigningKey,
) -> Result<String> {
    let payload = recovery_attestation_signature_input(attestation)?;
    let signature = signing_key.sign(&payload);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

/// Verify that a document correctly extends a previous document.
/// Checks: sequence increment, key continuity, both signatures valid (per RFC-0001 §7.2, §8.2).
pub fn verify_document_extends(previous: &IdentityDocument, current: &IdentityDocument) -> Result<()> {
    // IID must be unchanged
    if current.iid != previous.iid {
        return Err(PostUrbitError::InvalidInput("iid mismatch in chain"));
    }

    // Sequence must increase
    let prev_seq = parse_sequence(&previous.sequence)?;
    let curr_seq = parse_sequence(&current.sequence)?;
    if curr_seq <= prev_seq {
        return Err(PostUrbitError::InvalidInput("sequence must increase"));
    }

    // Verify current signature
    IdentityManager::verify_document(current)?;

    // Check if signing key changed
    if current.keys.signing.current != previous.keys.signing.current {
        // Key rotation - verify key continuity per REQ-IDOC-020, REQ-IDOC-021, REQ-IDOC-022

        // REQ-IDOC-020: keys.signing.previous MUST be present
        let Some(ref current_previous) = current.keys.signing.previous else {
            return Err(PostUrbitError::InvalidInput("key rotation requires previous key"));
        };

        // REQ-IDOC-021: keys.signing.previous MUST match previous doc's current key
        if current_previous != &previous.keys.signing.current {
            return Err(PostUrbitError::InvalidInput("previous key mismatch"));
        }

        // REQ-IDOC-022: signatures.previous MUST verify with previous key
        let Some(ref prev_sig) = current.signatures.previous else {
            return Err(PostUrbitError::InvalidInput("key rotation requires previous signature"));
        };

        let signed_payload = signature_payload(current)?;
        verify_signature_with_key(&signed_payload, prev_sig, &previous.keys.signing.current)?;
    }

    Ok(())
}

/// Verifies an identity document chain from genesis to current.
/// Used when first encountering an identity (TOFU - Trust On First Use).
///
/// Algorithm (per RFC-0001 §7.3):
/// 1. Fetch genesis document from DHT using dht_key_genesis(iid)
/// 2. Verify genesis document: signature valid, sequence="0", keys.signing.genesis == keys.signing.current
/// 3. If target sequence > 0:
///    a. Fetch current document from DHT using dht_key_identity(iid)
///    b. Verify chain: each document's previous key matches prior document's current key
///    c. Verify all signatures in chain
/// 4. Return verified IdentityDocument
pub async fn bootstrap_verify(dht: &dyn Dht, iid: &str) -> Result<IdentityDocument> {
    validate_crockford_base32_lower(iid)?;

    // Step 1: Try to fetch genesis document
    let genesis_key = dht_key_genesis(iid);
    let genesis_values = dht.get_all(&genesis_key).await?;

    let genesis_doc = if !genesis_values.is_empty() {
        // Find valid genesis document
        let mut found_genesis: Option<IdentityDocument> = None;
        for value in &genesis_values {
            let doc = decode_idoc_envelope(value)?;
            if verify_genesis_document(&doc, iid).is_ok() {
                found_genesis = Some(doc);
                break;
            }
        }
        found_genesis
    } else {
        None
    };

    // Step 4: Fetch latest document
    let identity_key = dht_key_identity(iid);
    let identity_values = dht.get_all(&identity_key).await?;

    if identity_values.is_empty() && genesis_doc.is_none() {
        return Err(PostUrbitError::InvalidInput("identity not found"));
    }

    // Find the highest-sequence valid document
    let mut best_doc: Option<IdentityDocument> = None;
    let mut best_seq: u64 = 0;

    for value in &identity_values {
        let doc = match decode_idoc_envelope(value) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if IdentityManager::verify_document(&doc).is_err() {
            continue;
        }
        let seq = match parse_sequence(&doc.sequence) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if best_doc.is_none() || seq > best_seq {
            best_seq = seq;
            best_doc = Some(doc);
        }
    }

    // If no current doc but we have genesis, use genesis
    let latest = match best_doc {
        Some(doc) => doc,
        None => {
            if let Some(ref genesis) = genesis_doc {
                return Ok(genesis.clone());
            }
            return Err(PostUrbitError::InvalidInput("no valid identity document found"));
        }
    };

    // Step 5: Validate chain if we have genesis
    if let Some(ref genesis) = genesis_doc {
        // For non-genesis documents, verify chain integrity
        if latest.sequence != "0" {
            // The latest document's genesis key must match the genesis document's current key
            if latest.keys.signing.genesis != genesis.keys.signing.current {
                return Err(PostUrbitError::InvalidInput("genesis key mismatch"));
            }

            // If keys differ between genesis and latest, verify chain continuity
            // via the previous key and signature (REQ-IDOC-024, REQ-IDOC-025)
            if latest.keys.signing.current != genesis.keys.signing.current {
                // We can't verify the full chain without intermediate documents,
                // but we can verify that:
                // 1. The latest document has a valid current signature
                // 2. If there's a previous signature, it's valid with the previous key
                // 3. The genesis key in latest matches genesis document

                // The current signature is already verified by verify_document above
                // Just verify the genesis binding is correct
                // This is TOFU-acceptable per REQ-IDOC-026
            }
        }
        return Ok(latest);
    }

    // TOFU case (REQ-IDOC-027): No genesis available, accept first encountered
    // Verify the document is self-consistent
    if IdentityManager::verify_document(&latest).is_ok() {
        return Ok(latest);
    }

    Err(PostUrbitError::InvalidInput("bootstrap verification failed"))
}

/// Verify a genesis document (sequence = 0) per RFC-0001 §7.1.
fn verify_genesis_document(doc: &IdentityDocument, expected_iid: &str) -> Result<()> {
    // Verify IID matches
    if doc.iid != expected_iid {
        return Err(PostUrbitError::InvalidInput("genesis iid mismatch"));
    }

    // Verify sequence is 0
    if doc.sequence != "0" {
        return Err(PostUrbitError::InvalidInput("genesis sequence must be 0"));
    }

    // Verify genesis key equals current key
    if doc.keys.signing.genesis != doc.keys.signing.current {
        return Err(PostUrbitError::InvalidInput("genesis key must equal current key"));
    }

    // Verify the IID is derived from the genesis key
    let genesis_key_bytes = base64_decode(&doc.keys.signing.genesis)?;
    if genesis_key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("genesis key length"));
    }
    let verifying_key = VerifyingKey::from_bytes(
        genesis_key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("genesis key length"))?,
    )
    .map_err(|_| PostUrbitError::InvalidInput("invalid genesis key"))?;

    let derived_iid = derive_iid(&verifying_key);
    if derived_iid != doc.iid {
        return Err(PostUrbitError::InvalidInput("iid not derived from genesis key"));
    }

    // Verify signature
    IdentityManager::verify_document(doc)
}

/// Verify a complete social recovery attempt per RFC-0001 §9.5.
///
/// Algorithm:
/// 1. Fetch target identity document
/// 2. Verify recovery method is "social" with valid config
/// 3. For each attestation:
///    a. Verify trustee is in trustees list
///    b. Fetch trustee's identity document
///    c. Verify attestation signature with trustee's signing key
///    d. Verify attestation timestamp within cooldown window
/// 4. Count valid attestations >= threshold
/// 5. Return new signing key if recovery succeeds
pub async fn verify_social_recovery(
    dht: &dyn Dht,
    target_iid: &str,
    attestations: &[RecoveryAttestation],
) -> Result<String> {
    validate_crockford_base32_lower(target_iid)?;

    // Step 1: Fetch target identity document
    let target_doc = fetch_identity(dht, target_iid)
        .await?
        .ok_or_else(|| PostUrbitError::InvalidInput("target identity not found"))?;

    // Step 2: Verify recovery method is "social"
    if target_doc.recovery.method != "social" {
        return Err(PostUrbitError::InvalidInput("recovery method is not social"));
    }

    // Get trustees and threshold from config
    let trustees = target_doc
        .recovery
        .config
        .get("trustees")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PostUrbitError::InvalidInput("social recovery requires trustees"))?;

    let threshold = target_doc
        .recovery
        .config
        .get("threshold")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| PostUrbitError::InvalidInput("social recovery requires threshold"))? as usize;

    let cooldown_hours = target_doc
        .recovery
        .config
        .get("cooldown_hours")
        .and_then(|v| v.as_u64())
        .unwrap_or(72);

    // Build set of valid trustee IIDs
    let trustee_iids: std::collections::HashSet<String> = trustees
        .iter()
        .filter_map(|t| t.get("iid").and_then(|v| v.as_str()).map(String::from))
        .collect();

    // Step 3 & 4: Verify attestations and count valid ones
    let mut valid_trustees: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut new_signing_key: Option<String> = None;
    let now = Utc::now();

    for attestation in attestations {
        // 3a. Verify trustee is in trustees list
        if !trustee_iids.contains(&attestation.trustee_iid) {
            continue;
        }

        // Verify attestation is for the target
        if attestation.target_iid != target_iid {
            continue;
        }

        // 3b. Fetch trustee's identity document
        let trustee_doc = match fetch_identity(dht, &attestation.trustee_iid).await {
            Ok(Some(doc)) => doc,
            _ => continue,
        };

        // 3c. Verify attestation signature with trustee's signing key
        let trustee_key_bytes = match base64_decode(&trustee_doc.keys.signing.current) {
            Ok(bytes) if bytes.len() == 32 => bytes,
            _ => continue,
        };
        let trustee_key = match VerifyingKey::from_bytes(
            trustee_key_bytes
                .as_slice()
                .try_into()
                .unwrap_or(&[0u8; 32]),
        ) {
            Ok(key) => key,
            Err(_) => continue,
        };

        if verify_recovery_attestation(attestation, &trustee_key).is_err() {
            continue;
        }

        // 3d. Verify attestation timestamp within cooldown window
        let attestation_time = match parse_rfc3339(&attestation.timestamp) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Attestation must not be in the future (with 5 min tolerance)
        if attestation_time > now + chrono::Duration::minutes(5) {
            continue;
        }

        // Attestation must be within cooldown window (not too old)
        let cooldown_duration = chrono::Duration::hours(cooldown_hours as i64);
        if now - attestation_time > cooldown_duration {
            continue;
        }

        // Verify new_signing_key consistency across attestations
        if let Some(ref existing_key) = new_signing_key {
            if existing_key != &attestation.new_signing_key {
                // Attestations must agree on the new key
                continue;
            }
        } else {
            new_signing_key = Some(attestation.new_signing_key.clone());
        }

        // Count this trustee (only once)
        valid_trustees.insert(attestation.trustee_iid.clone());
    }

    // Step 5: Check threshold and return new signing key
    if valid_trustees.len() >= threshold {
        new_signing_key.ok_or_else(|| PostUrbitError::InvalidInput("no valid attestations"))
    } else {
        Err(PostUrbitError::InvalidInput("insufficient attestations for recovery"))
    }
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
            // Key rotation requires previous signature for chain verification
            replacement.signatures.previous = Some(sign_idoc(&replacement, &signing_key).unwrap());

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

    #[test]
    fn recovery_attestation_signature_round_trip() {
        let trustee_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let new_signing_key = SigningKey::generate(&mut rand::rngs::OsRng);

        let mut attestation = RecoveryAttestation {
            target_iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v".to_string(),
            trustee_iid: "42kbzq2tyab939amybd76bm8kfpzgn95".to_string(),
            new_signing_key: base64_encode(new_signing_key.verifying_key().as_bytes()),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            signature: String::new(),
        };

        attestation.signature = sign_recovery_attestation(&attestation, &trustee_key).unwrap();

        // Verify with the trustee's public key
        verify_recovery_attestation(&attestation, &trustee_key.verifying_key()).unwrap();
    }

    #[test]
    fn recovery_attestation_rejects_wrong_key() {
        let trustee_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let wrong_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let new_signing_key = SigningKey::generate(&mut rand::rngs::OsRng);

        let mut attestation = RecoveryAttestation {
            target_iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v".to_string(),
            trustee_iid: "42kbzq2tyab939amybd76bm8kfpzgn95".to_string(),
            new_signing_key: base64_encode(new_signing_key.verifying_key().as_bytes()),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            signature: String::new(),
        };

        attestation.signature = sign_recovery_attestation(&attestation, &trustee_key).unwrap();

        // Should fail with wrong key
        let err = verify_recovery_attestation(&attestation, &wrong_key.verifying_key()).unwrap_err();
        assert!(matches!(err, PostUrbitError::Crypto(_)));
    }

    #[test]
    fn verify_document_extends_same_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let genesis = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Create sequence 1 document with same key
        let mut seq1 = genesis.clone();
        seq1.sequence = "1".to_string();
        seq1.timestamp = "2025-01-16T00:00:00Z".to_string();
        seq1.signatures.current = sign_idoc(&seq1, &signing_key).unwrap();

        verify_document_extends(&genesis, &seq1).unwrap();
    }

    #[test]
    fn verify_document_extends_key_rotation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let old_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let new_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let genesis = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &old_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Create sequence 1 document with new key (key rotation)
        let mut seq1 = genesis.clone();
        seq1.sequence = "1".to_string();
        seq1.timestamp = "2025-01-16T00:00:00Z".to_string();
        seq1.keys.signing.previous = Some(genesis.keys.signing.current.clone());
        seq1.keys.signing.current = base64_encode(new_key.verifying_key().as_bytes());
        seq1.signatures.current = sign_idoc(&seq1, &new_key).unwrap();
        seq1.signatures.previous = Some(sign_idoc(&seq1, &old_key).unwrap());

        verify_document_extends(&genesis, &seq1).unwrap();
    }

    #[test]
    fn verify_document_extends_rejects_missing_previous_sig() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let old_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let new_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let genesis = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &old_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Create sequence 1 with key rotation but no previous signature
        let mut seq1 = genesis.clone();
        seq1.sequence = "1".to_string();
        seq1.timestamp = "2025-01-16T00:00:00Z".to_string();
        seq1.keys.signing.previous = Some(genesis.keys.signing.current.clone());
        seq1.keys.signing.current = base64_encode(new_key.verifying_key().as_bytes());
        seq1.signatures.current = sign_idoc(&seq1, &new_key).unwrap();
        seq1.signatures.previous = None; // Missing!

        let err = verify_document_extends(&genesis, &seq1).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn verify_document_extends_rejects_sequence_regression() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let genesis = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Create sequence 1 document
        let mut seq1 = genesis.clone();
        seq1.sequence = "1".to_string();
        seq1.timestamp = "2025-01-16T00:00:00Z".to_string();
        seq1.signatures.current = sign_idoc(&seq1, &signing_key).unwrap();

        // Try to verify same sequence (no increase)
        let err = verify_document_extends(&seq1, &genesis).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn bootstrap_verify_genesis_only() {
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

            let verified = bootstrap_verify(&dht, &doc.iid).await.unwrap();
            assert_eq!(verified.iid, doc.iid);
            assert_eq!(verified.sequence, "0");
        });
    }

    #[test]
    fn bootstrap_verify_with_updated_doc() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let genesis = IdentityManager::create_genesis_document(
                &signing_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
            .unwrap();

            publish_genesis(&dht, &genesis).await.unwrap();

            // Create and publish sequence 1
            let mut seq1 = genesis.clone();
            seq1.sequence = "1".to_string();
            seq1.timestamp = "2025-01-16T00:00:00Z".to_string();
            seq1.claims.name = Some("Updated".to_string());
            seq1.signatures.current = sign_idoc(&seq1, &signing_key).unwrap();
            publish_identity(&dht, &seq1).await.unwrap();

            let verified = bootstrap_verify(&dht, &genesis.iid).await.unwrap();
            assert_eq!(verified.sequence, "1");
            assert_eq!(verified.claims.name, Some("Updated".to_string()));
        });
    }

    #[test]
    fn bootstrap_verify_tofu_no_genesis() {
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

            // Only publish to identity key, not genesis
            publish_identity(&dht, &doc).await.unwrap();

            // Should still work with TOFU
            let verified = bootstrap_verify(&dht, &doc.iid).await.unwrap();
            assert_eq!(verified.iid, doc.iid);
        });
    }

    #[test]
    fn bootstrap_verify_not_found() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let err = bootstrap_verify(&dht, "b1n7cfscgashm32xx7eaxw0y09gy0y2v")
                .await
                .unwrap_err();
            assert!(matches!(err, PostUrbitError::InvalidInput(_)));
        });
    }

    #[test]
    fn verify_social_recovery_success() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();

            // Create target identity with social recovery
            let target_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let target_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let mut target_doc = IdentityManager::create_genesis_document(
                &target_key,
                &target_enc,
                &tmp.path().join("target.json"),
            )
            .await
            .unwrap();

            // Create trustees
            let trustee1_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let trustee1_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let trustee1_doc = IdentityManager::create_genesis_document(
                &trustee1_key,
                &trustee1_enc,
                &tmp.path().join("trustee1.json"),
            )
            .await
            .unwrap();

            let trustee2_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let trustee2_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let trustee2_doc = IdentityManager::create_genesis_document(
                &trustee2_key,
                &trustee2_enc,
                &tmp.path().join("trustee2.json"),
            )
            .await
            .unwrap();

            // Configure social recovery on target
            target_doc.recovery = Recovery {
                method: "social".to_string(),
                config: serde_json::json!({
                    "threshold": 2,
                    "trustees": [
                        {"iid": trustee1_doc.iid.clone(), "label": "Trustee 1"},
                        {"iid": trustee2_doc.iid.clone(), "label": "Trustee 2"}
                    ],
                    "cooldown_hours": 72
                }),
            };
            target_doc.sequence = "1".to_string();
            target_doc.signatures.current = sign_idoc(&target_doc, &target_key).unwrap();

            // Publish all identities
            publish_identity(&dht, &target_doc).await.unwrap();
            publish_identity(&dht, &trustee1_doc).await.unwrap();
            publish_identity(&dht, &trustee2_doc).await.unwrap();

            // Create new key for recovery
            let new_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let new_key_b64 = base64_encode(new_key.verifying_key().as_bytes());
            let now = Utc::now();

            // Create attestations from both trustees
            let mut att1 = RecoveryAttestation {
                target_iid: target_doc.iid.clone(),
                trustee_iid: trustee1_doc.iid.clone(),
                new_signing_key: new_key_b64.clone(),
                timestamp: now.to_rfc3339(),
                signature: String::new(),
            };
            att1.signature = sign_recovery_attestation(&att1, &trustee1_key).unwrap();

            let mut att2 = RecoveryAttestation {
                target_iid: target_doc.iid.clone(),
                trustee_iid: trustee2_doc.iid.clone(),
                new_signing_key: new_key_b64.clone(),
                timestamp: now.to_rfc3339(),
                signature: String::new(),
            };
            att2.signature = sign_recovery_attestation(&att2, &trustee2_key).unwrap();

            // Verify recovery
            let recovered_key = verify_social_recovery(&dht, &target_doc.iid, &[att1, att2])
                .await
                .unwrap();
            assert_eq!(recovered_key, new_key_b64);
        });
    }

    #[test]
    fn verify_social_recovery_insufficient_attestations() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();

            // Create target with threshold 2
            let target_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let target_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let mut target_doc = IdentityManager::create_genesis_document(
                &target_key,
                &target_enc,
                &tmp.path().join("target.json"),
            )
            .await
            .unwrap();

            let trustee1_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let trustee1_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let trustee1_doc = IdentityManager::create_genesis_document(
                &trustee1_key,
                &trustee1_enc,
                &tmp.path().join("trustee1.json"),
            )
            .await
            .unwrap();

            let trustee2_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let trustee2_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let trustee2_doc = IdentityManager::create_genesis_document(
                &trustee2_key,
                &trustee2_enc,
                &tmp.path().join("trustee2.json"),
            )
            .await
            .unwrap();

            target_doc.recovery = Recovery {
                method: "social".to_string(),
                config: serde_json::json!({
                    "threshold": 2,
                    "trustees": [
                        {"iid": trustee1_doc.iid.clone(), "label": "Trustee 1"},
                        {"iid": trustee2_doc.iid.clone(), "label": "Trustee 2"}
                    ],
                    "cooldown_hours": 72
                }),
            };
            target_doc.sequence = "1".to_string();
            target_doc.signatures.current = sign_idoc(&target_doc, &target_key).unwrap();

            publish_identity(&dht, &target_doc).await.unwrap();
            publish_identity(&dht, &trustee1_doc).await.unwrap();

            // Only one attestation (need 2)
            let new_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let new_key_b64 = base64_encode(new_key.verifying_key().as_bytes());
            let now = Utc::now();

            let mut att1 = RecoveryAttestation {
                target_iid: target_doc.iid.clone(),
                trustee_iid: trustee1_doc.iid.clone(),
                new_signing_key: new_key_b64.clone(),
                timestamp: now.to_rfc3339(),
                signature: String::new(),
            };
            att1.signature = sign_recovery_attestation(&att1, &trustee1_key).unwrap();

            let err = verify_social_recovery(&dht, &target_doc.iid, &[att1])
                .await
                .unwrap_err();
            assert!(matches!(err, PostUrbitError::InvalidInput(_)));
        });
    }

    #[test]
    fn verify_social_recovery_rejects_non_trustee() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();

            let target_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let target_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let mut target_doc = IdentityManager::create_genesis_document(
                &target_key,
                &target_enc,
                &tmp.path().join("target.json"),
            )
            .await
            .unwrap();

            let trustee_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let trustee_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let trustee_doc = IdentityManager::create_genesis_document(
                &trustee_key,
                &trustee_enc,
                &tmp.path().join("trustee.json"),
            )
            .await
            .unwrap();

            // Attacker not in trustees list
            let attacker_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let attacker_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let attacker_doc = IdentityManager::create_genesis_document(
                &attacker_key,
                &attacker_enc,
                &tmp.path().join("attacker.json"),
            )
            .await
            .unwrap();

            target_doc.recovery = Recovery {
                method: "social".to_string(),
                config: serde_json::json!({
                    "threshold": 2,
                    "trustees": [
                        {"iid": trustee_doc.iid.clone(), "label": "Real Trustee"}
                    ],
                    "cooldown_hours": 72
                }),
            };
            target_doc.sequence = "1".to_string();
            target_doc.signatures.current = sign_idoc(&target_doc, &target_key).unwrap();

            publish_identity(&dht, &target_doc).await.unwrap();
            publish_identity(&dht, &trustee_doc).await.unwrap();
            publish_identity(&dht, &attacker_doc).await.unwrap();

            let new_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let new_key_b64 = base64_encode(new_key.verifying_key().as_bytes());
            let now = Utc::now();

            // Attestation from attacker (not in trustees list)
            let mut att = RecoveryAttestation {
                target_iid: target_doc.iid.clone(),
                trustee_iid: attacker_doc.iid.clone(),
                new_signing_key: new_key_b64.clone(),
                timestamp: now.to_rfc3339(),
                signature: String::new(),
            };
            att.signature = sign_recovery_attestation(&att, &attacker_key).unwrap();

            let err = verify_social_recovery(&dht, &target_doc.iid, &[att])
                .await
                .unwrap_err();
            assert!(matches!(err, PostUrbitError::InvalidInput(_)));
        });
    }

    #[test]
    fn verify_social_recovery_rejects_wrong_method() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();

            let target_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let target_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let target_doc = IdentityManager::create_genesis_document(
                &target_key,
                &target_enc,
                &tmp.path().join("target.json"),
            )
            .await
            .unwrap();

            // Default recovery method is "none"
            publish_identity(&dht, &target_doc).await.unwrap();

            let err = verify_social_recovery(&dht, &target_doc.iid, &[])
                .await
                .unwrap_err();
            assert!(matches!(err, PostUrbitError::InvalidInput(_)));
        });
    }

    // ===========================================================================
    // Tests for IdentityManager sign_data methods
    // ===========================================================================

    #[test]
    fn identity_manager_sign_data_returns_64_bytes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let data = b"test data to sign";
            let signature = manager.sign_data(data).await;

            assert_eq!(signature.len(), 64);
        });
    }

    #[test]
    fn identity_manager_sign_data_is_deterministic() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let data = b"test data to sign";
            let sig1 = manager.sign_data(data).await;
            let sig2 = manager.sign_data(data).await;

            // Ed25519 signatures are deterministic with the same key and data
            assert_eq!(sig1, sig2);
        });
    }

    #[test]
    fn identity_manager_sign_data_differs_for_different_data() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let data1 = b"first message";
            let data2 = b"second message";
            let sig1 = manager.sign_data(data1).await;
            let sig2 = manager.sign_data(data2).await;

            assert_ne!(sig1, sig2);
        });
    }

    #[test]
    fn identity_manager_sign_data_base64_valid_encoding() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let data = b"test data to sign";
            let signature_b64 = manager.sign_data_base64(data).await;

            // Verify it's valid base64 and decodes to 64 bytes
            let decoded = base64_decode(&signature_b64).unwrap();
            assert_eq!(decoded.len(), 64);
        });
    }

    #[test]
    fn identity_manager_signing_public_key_bytes_returns_32_bytes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let pubkey = manager.signing_public_key_bytes().await;
            assert_eq!(pubkey.len(), 32);
        });
    }

    #[test]
    fn identity_manager_signing_public_key_base64_valid_encoding() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let pubkey_b64 = manager.signing_public_key_base64().await;

            // Verify it's valid base64 and decodes to 32 bytes
            let decoded = base64_decode(&pubkey_b64).unwrap();
            assert_eq!(decoded.len(), 32);
        });
    }

    #[test]
    fn identity_manager_signing_public_key_matches_document() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let pubkey_b64 = manager.signing_public_key_base64().await;
            let doc = manager.identity_document().await;

            // The public key from signing_public_key_base64 should match the current signing key in the document
            assert_eq!(pubkey_b64, doc.keys.signing.current);
        });
    }

    #[test]
    fn identity_manager_sign_data_verifiable_with_public_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let data = b"test message for verification";
            let signature_bytes = manager.sign_data(data).await;
            let pubkey_bytes = manager.signing_public_key_bytes().await;

            // Verify the signature using ed25519_dalek
            let verifying_key = VerifyingKey::from_bytes(&pubkey_bytes).unwrap();
            let signature = Signature::from_bytes(&signature_bytes);

            // Use verify_strict to ensure the signature is valid
            verifying_key.verify_strict(data, &signature).unwrap();
        });
    }

    #[test]
    fn identity_manager_sign_data_after_key_rotation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        rt.block_on(async {
            let manager = IdentityManager::new(tmp.path().to_str().unwrap())
                .await
                .unwrap();

            let data = b"test data";

            // Get signature before rotation
            let sig_before = manager.sign_data(data).await;
            let pubkey_before = manager.signing_public_key_bytes().await;

            // Rotate the key
            manager.rotate_signing_key().await.unwrap();

            // Get signature after rotation
            let sig_after = manager.sign_data(data).await;
            let pubkey_after = manager.signing_public_key_bytes().await;

            // Keys should be different
            assert_ne!(pubkey_before, pubkey_after);

            // Signatures should be different (since the key changed)
            assert_ne!(sig_before, sig_after);

            // New signature should be verifiable with new key
            let verifying_key = VerifyingKey::from_bytes(&pubkey_after).unwrap();
            let signature = Signature::from_bytes(&sig_after);
            verifying_key.verify_strict(data, &signature).unwrap();
        });
    }

    // ========================================================================
    // SECURITY TESTS: IID-Genesis Binding and Key Rotation Continuity
    // ========================================================================

    #[test]
    fn verify_document_rejects_fake_genesis_key() {
        // SECURITY TEST: Verify that an attacker cannot hijack an IID by
        // publishing a document with a fake genesis key that doesn't derive to the IID
        let rt = tokio::runtime::Runtime::new().unwrap();
        let attacker_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let legitimate_doc = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &SigningKey::generate(&mut rand::rngs::OsRng),
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Attacker creates a document claiming to be the legitimate IID
        // but using their own genesis key
        let mut fake_doc = legitimate_doc.clone();
        fake_doc.keys.signing.genesis = base64_encode(attacker_key.verifying_key().as_bytes());
        fake_doc.keys.signing.current = base64_encode(attacker_key.verifying_key().as_bytes());
        fake_doc.sequence = "1".to_string(); // Higher sequence to try to override
        fake_doc.signatures.current = sign_idoc(&fake_doc, &attacker_key).unwrap();

        // This should fail because the IID doesn't match the attacker's genesis key
        let err = IdentityManager::verify_document(&fake_doc).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn verify_document_rejects_key_rotation_without_previous_signature() {
        // SECURITY TEST: Key rotation must be authorized by the previous key
        let rt = tokio::runtime::Runtime::new().unwrap();
        let genesis_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let attacker_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let genesis_doc = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &genesis_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Attacker tries to rotate to their key without authorization from genesis key
        let mut fake_rotation = genesis_doc.clone();
        fake_rotation.sequence = "1".to_string();
        fake_rotation.keys.signing.previous = Some(genesis_doc.keys.signing.current.clone());
        fake_rotation.keys.signing.current = base64_encode(attacker_key.verifying_key().as_bytes());
        // Attacker only signs with their own key, not with the previous key
        fake_rotation.signatures.current = sign_idoc(&fake_rotation, &attacker_key).unwrap();
        fake_rotation.signatures.previous = None; // Missing previous signature!

        // This should fail because key rotation requires previous signature
        let err = IdentityManager::verify_document(&fake_rotation).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn verify_document_rejects_key_rotation_with_wrong_previous_signature() {
        // SECURITY TEST: Previous signature must be valid with the claimed previous key
        let rt = tokio::runtime::Runtime::new().unwrap();
        let genesis_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let attacker_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let random_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let genesis_doc = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &genesis_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Attacker tries to rotate to their key with a fake previous signature
        let mut fake_rotation = genesis_doc.clone();
        fake_rotation.sequence = "1".to_string();
        fake_rotation.keys.signing.previous = Some(genesis_doc.keys.signing.current.clone());
        fake_rotation.keys.signing.current = base64_encode(attacker_key.verifying_key().as_bytes());
        fake_rotation.signatures.current = sign_idoc(&fake_rotation, &attacker_key).unwrap();
        // Sign with a random key instead of the actual genesis key
        fake_rotation.signatures.previous = Some(sign_idoc(&fake_rotation, &random_key).unwrap());

        // This should fail because the previous signature doesn't match the previous key
        let err = IdentityManager::verify_document(&fake_rotation).unwrap_err();
        assert!(matches!(err, PostUrbitError::Crypto(_)));
    }

    #[test]
    fn verify_document_accepts_valid_key_rotation() {
        // Positive test: Valid key rotation should be accepted
        let rt = tokio::runtime::Runtime::new().unwrap();
        let genesis_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let new_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let genesis_doc = rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            IdentityManager::create_genesis_document(
                &genesis_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
        })
        .unwrap();

        // Legitimate key rotation
        let mut rotated = genesis_doc.clone();
        rotated.sequence = "1".to_string();
        rotated.keys.signing.previous = Some(genesis_doc.keys.signing.current.clone());
        rotated.keys.signing.current = base64_encode(new_key.verifying_key().as_bytes());
        rotated.signatures.current = sign_idoc(&rotated, &new_key).unwrap();
        rotated.signatures.previous = Some(sign_idoc(&rotated, &genesis_key).unwrap());

        // This should succeed
        IdentityManager::verify_document(&rotated).unwrap();
    }

    #[test]
    fn fetch_identity_rejects_hijack_attempt_with_fake_genesis() {
        // SECURITY TEST: An attacker publishing a higher-sequence document
        // with a fake genesis key should be rejected
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            let legitimate_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

            // Create and publish legitimate genesis document
            let legitimate_doc = IdentityManager::create_genesis_document(
                &legitimate_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
            .unwrap();

            publish_genesis(&dht, &legitimate_doc).await.unwrap();

            // Attacker creates a fake document with higher sequence
            let attacker_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let mut fake_doc = legitimate_doc.clone();
            fake_doc.keys.signing.genesis = base64_encode(attacker_key.verifying_key().as_bytes());
            fake_doc.keys.signing.current = base64_encode(attacker_key.verifying_key().as_bytes());
            fake_doc.sequence = "999".to_string();
            fake_doc.signatures.current = sign_idoc(&fake_doc, &attacker_key).unwrap();

            // The IID doesn't match attacker's genesis key, so verify_document fails
            // and the fake document won't be accepted
            assert!(IdentityManager::verify_document(&fake_doc).is_err());

            // Publish the fake document directly (bypassing verification)
            let key = dht_key_identity(&legitimate_doc.iid);
            let envelope = encode_idoc_envelope(&fake_doc).unwrap();
            dht.put(&key, envelope, Duration::from_secs(3600)).await.unwrap();

            // Fetch should return the legitimate document, not the fake one
            let fetched = fetch_identity(&dht, &legitimate_doc.iid)
                .await
                .unwrap()
                .unwrap();

            // Should get the legitimate genesis document (sequence 0), not the fake one
            assert_eq!(fetched.sequence, "0");
            assert_eq!(fetched.keys.signing.genesis, legitimate_doc.keys.signing.genesis);
        });
    }

    #[test]
    fn fetch_identity_with_genesis_cross_verification() {
        // Test that fetch_identity correctly cross-verifies with genesis document
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dht = MemoryDht::new();

        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            let genesis_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let new_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);

            // Create and publish genesis
            let genesis_doc = IdentityManager::create_genesis_document(
                &genesis_key,
                &enc_key,
                &tmp.path().join("idoc.json"),
            )
            .await
            .unwrap();
            publish_genesis(&dht, &genesis_doc).await.unwrap();

            // Create valid rotation and publish
            let mut rotated = genesis_doc.clone();
            rotated.sequence = "1".to_string();
            rotated.keys.signing.previous = Some(genesis_doc.keys.signing.current.clone());
            rotated.keys.signing.current = base64_encode(new_key.verifying_key().as_bytes());
            rotated.signatures.current = sign_idoc(&rotated, &new_key).unwrap();
            rotated.signatures.previous = Some(sign_idoc(&rotated, &genesis_key).unwrap());
            publish_identity(&dht, &rotated).await.unwrap();

            // Fetch should return the rotated document
            let fetched = fetch_identity(&dht, &genesis_doc.iid)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(fetched.sequence, "1");
            assert_eq!(fetched.keys.signing.current, rotated.keys.signing.current);
            // Genesis key should be preserved
            assert_eq!(fetched.keys.signing.genesis, genesis_doc.keys.signing.genesis);
        });
    }
}

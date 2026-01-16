use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::canonical_json::canonical_json_from;
use crate::dht::{dht_key_genesis, dht_key_identity, Dht};
use crate::encoding::{base64_decode, base64_encode, crockford_base32_encode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};

const IDOC_MAGIC: &[u8; 4] = b"IDOC";
const IDOC_VERSION: u8 = 1;
const IDOC_DOMAIN_SEPARATOR: &[u8] = b"post-urbit:idoc:v1:";

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
    pub name: Option<String>,
    pub avatar: Option<String>,
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

pub struct IdentityManager {
    document: IdentityDocument,
    signing_key: SigningKey,
    encryption_key: StaticSecret,
    data_dir: String,
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

        Ok(Self {
            document,
            signing_key,
            encryption_key,
            data_dir: data_dir.to_string(),
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

    pub fn iid(&self) -> &str {
        &self.document.iid
    }

    pub fn identity_document(&self) -> &IdentityDocument {
        &self.document
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
        let doc_path = Path::new(&self.data_dir).join("identity.json");
        let doc_json = serde_json::to_string_pretty(&self.document)
            .map_err(|_| PostUrbitError::InvalidInput("serialize identity document"))?;
        tokio::fs::write(&doc_path, &doc_json).await?;
        Ok(())
    }
}

pub fn derive_iid(verifying_key: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifying_key.as_bytes());
    let hash = hasher.finalize();
    crockford_base32_encode(&hash[..20]).to_lowercase()
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

    Ok(best)
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
}

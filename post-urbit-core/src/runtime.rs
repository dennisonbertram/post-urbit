use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};

use crate::error::{PostUrbitError, Result};
use crate::canonical_json::canonical_json_from;
use crate::encoding::{base64_decode, base64_encode};
use crate::identity::{IdentityDocument, RevocationDocument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u8,
    pub app: AppMetadata,
    pub runtime: RuntimeConfig,
    pub capabilities: CapabilitiesConfig,
    #[serde(default)]
    pub secrets: Option<HashMap<String, SecretDeclaration>>,
    #[serde(default)]
    pub network: Option<NetworkConfig>,
    pub dependencies: DependenciesConfig,
    pub files: FilesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Author,
    pub license: String,
    pub homepage: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub iid: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub entry: String,
    pub memory: Option<RuntimeMemory>,
    pub fuel: Option<RuntimeFuel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMemory {
    pub initial_pages: u32,
    pub maximum_pages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeFuel {
    pub user_action: Option<u64>,
    pub background_task: Option<u64>,
    pub app_start: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesConfig {
    pub required: Vec<String>,
    pub optional: Option<Vec<String>>,
    pub reasons: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretDeclaration {
    pub description: String,
    pub required: bool,
    pub inject: SecretInjection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretInjection {
    pub domains: Vec<String>,
    pub header: Option<String>,
    pub header_prefix: Option<String>,
    pub query_param: Option<String>,
    pub basic_auth: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub rate_limits: Option<HashMap<String, NetworkRateLimit>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRateLimit {
    pub requests_per_minute: Option<u32>,
    pub requests_per_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependenciesConfig {
    pub node_version: Option<String>,
    pub api_version: String,
    pub apps: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesConfig {
    pub hashes: HashMap<String, String>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSignature {
    pub author_iid: String,
    pub timestamp: String,
    pub signature: String,
    pub signed_manifest_hash: String,
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest> {
    serde_json::from_slice(bytes).map_err(|_| PostUrbitError::InvalidInput("manifest json"))
}

pub fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.manifest_version != 1 {
        return Err(PostUrbitError::InvalidInput("manifest version"));
    }
    validate_app_id(&manifest.app.id)?;
    if manifest.app.name.trim().is_empty() {
        return Err(PostUrbitError::InvalidInput("app name"));
    }
    if manifest.app.description.trim().is_empty() {
        return Err(PostUrbitError::InvalidInput("app description"));
    }
    validate_semver(&manifest.app.version)?;
    if manifest.app.author.name.trim().is_empty() {
        return Err(PostUrbitError::InvalidInput("author name"));
    }
    if manifest.app.license.trim().is_empty() {
        return Err(PostUrbitError::InvalidInput("app license"));
    }
    if !manifest.runtime.entry.ends_with(".wasm") {
        return Err(PostUrbitError::InvalidInput("runtime entry"));
    }
    if manifest.dependencies.api_version != "1" {
        return Err(PostUrbitError::InvalidInput("api version"));
    }
    for cap in &manifest.capabilities.required {
        if cap.trim().is_empty() {
            return Err(PostUrbitError::InvalidInput("capability format"));
        }
        validate_network_capability(cap)?;
    }
    if let Some(optional) = manifest.capabilities.optional.as_ref() {
        for cap in optional {
            if cap.trim().is_empty() {
                return Err(PostUrbitError::InvalidInput("capability format"));
            }
            validate_network_capability(cap)?;
        }
    }
    if let Some(secrets) = manifest.secrets.as_ref() {
        validate_secret_declarations(secrets, &manifest.capabilities)?;
    }
    if let Some(network) = manifest.network.as_ref() {
        validate_network_config(network)?;
    }
    if !manifest.files.hashes.contains_key(&manifest.runtime.entry) {
        return Err(PostUrbitError::InvalidInput("entry hash missing"));
    }
    for (path, hash) in &manifest.files.hashes {
        if path.trim().is_empty() {
            return Err(PostUrbitError::InvalidInput("file path"));
        }
        validate_sha256_hash(hash)?;
    }
    Ok(())
}

fn validate_network_capability(cap: &str) -> Result<()> {
    if !cap.starts_with("network:") {
        return Ok(());
    }
    let mut parts = cap.splitn(3, ':');
    let _ = parts.next();
    let protocol = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if protocol != "https" && protocol != "http" && protocol != "http+https" {
        return Err(PostUrbitError::InvalidInput("capability format"));
    }
    validate_domain_pattern(domain)?;
    Ok(())
}

fn validate_secret_declarations(
    secrets: &HashMap<String, SecretDeclaration>,
    caps: &CapabilitiesConfig,
) -> Result<()> {
    for (name, secret) in secrets {
        if name.trim().is_empty() {
            return Err(PostUrbitError::InvalidInput("secret name"));
        }
        if secret.description.trim().is_empty() {
            return Err(PostUrbitError::InvalidInput("secret description"));
        }
        validate_secret_injection(&secret.inject, caps)?;
    }
    Ok(())
}

fn validate_secret_injection(inject: &SecretInjection, caps: &CapabilitiesConfig) -> Result<()> {
    if inject.domains.is_empty() {
        return Err(PostUrbitError::InvalidInput("secret domains"));
    }
    for domain in &inject.domains {
        validate_domain_pattern(domain)?;
        if !caps_include_domain(caps, domain) {
            return Err(PostUrbitError::InvalidInput("secret domain not allowed"));
        }
    }
    let mut methods = 0;
    if inject.header.is_some() {
        methods += 1;
    }
    if inject.query_param.is_some() {
        methods += 1;
    }
    if inject.basic_auth.unwrap_or(false) {
        methods += 1;
    }
    if methods != 1 {
        return Err(PostUrbitError::InvalidInput("secret injection method"));
    }
    if inject.header_prefix.is_some() && inject.header.is_none() {
        return Err(PostUrbitError::InvalidInput("secret header prefix"));
    }
    Ok(())
}

fn validate_network_config(network: &NetworkConfig) -> Result<()> {
    if let Some(rate_limits) = network.rate_limits.as_ref() {
        for (domain, limits) in rate_limits {
            validate_domain_pattern(domain)?;
            if limits.requests_per_minute.is_none() && limits.requests_per_day.is_none() {
                return Err(PostUrbitError::InvalidInput("rate limit empty"));
            }
        }
    }
    Ok(())
}

fn validate_domain_pattern(pattern: &str) -> Result<()> {
    if pattern.is_empty() {
        return Err(PostUrbitError::InvalidInput("domain pattern"));
    }
    if pattern.contains('/') || pattern.contains(':') {
        return Err(PostUrbitError::InvalidInput("domain pattern"));
    }
    let (wildcard, domain) = if let Some(rest) = pattern.strip_prefix("*.") {
        (true, rest)
    } else {
        (false, pattern)
    };
    if wildcard && !domain.contains('.') {
        return Err(PostUrbitError::InvalidInput("domain pattern"));
    }
    if domain.parse::<std::net::IpAddr>().is_ok() {
        return Err(PostUrbitError::InvalidInput("domain pattern"));
    }
    for label in domain.split('.') {
        if label.is_empty() {
            return Err(PostUrbitError::InvalidInput("domain pattern"));
        }
        let mut chars = label.chars();
        let first = chars.next().unwrap();
        if first == '-' || !first.is_ascii_alphanumeric() {
            return Err(PostUrbitError::InvalidInput("domain pattern"));
        }
        let mut last = first;
        for ch in chars {
            if !(ch.is_ascii_alphanumeric() || ch == '-') {
                return Err(PostUrbitError::InvalidInput("domain pattern"));
            }
            last = ch;
        }
        if last == '-' {
            return Err(PostUrbitError::InvalidInput("domain pattern"));
        }
    }
    Ok(())
}

fn caps_include_domain(caps: &CapabilitiesConfig, domain: &str) -> bool {
    let mut all = caps.required.clone();
    if let Some(optional) = caps.optional.as_ref() {
        all.extend(optional.iter().cloned());
    }
    all.iter().any(|cap| {
        if !cap.starts_with("network:") {
            return false;
        }
        cap.splitn(3, ':')
            .nth(2)
            .map(|pattern| domain_pattern_matches(pattern, domain))
            .unwrap_or(false)
    })
}

fn domain_pattern_matches(pattern: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();
    if let Some(rest) = pattern.strip_prefix("*.") {
        if host == rest {
            return false;
        }
        return host.ends_with(&format!(".{rest}"));
    }
    host == pattern
}

pub fn verify_package(manifest: &Manifest, files: &HashMap<String, Vec<u8>>) -> Result<()> {
    for (path, expected) in &manifest.files.hashes {
        let data = files
            .get(path)
            .ok_or(PostUrbitError::InvalidInput("missing package file"))?;
        let hash = Sha256::digest(data);
        let actual = format!("sha256:{}", hex::encode(hash));
        if &actual != expected {
            return Err(PostUrbitError::InvalidInput("package hash mismatch"));
        }
    }
    Ok(())
}

pub fn sign_package_signature(
    manifest: &Manifest,
    author_iid: &str,
    timestamp: &str,
    signing_key: &SigningKey,
) -> Result<PackageSignature> {
    let manifest_hash_hex = manifest_hash_hex(manifest)?;
    let payload = format!("postapp-signature-v1:{manifest_hash_hex}:{timestamp}");
    let signature: Signature = signing_key.sign(payload.as_bytes());
    Ok(PackageSignature {
        author_iid: author_iid.to_string(),
        timestamp: timestamp.to_string(),
        signature: base64_encode(signature.to_bytes().as_slice()),
        signed_manifest_hash: format!("sha256:{manifest_hash_hex}"),
    })
}

pub fn verify_package_signature(
    manifest: &Manifest,
    signature: &PackageSignature,
    identity: &IdentityDocument,
) -> Result<()> {
    verify_package_signature_with_revocations(manifest, signature, identity, &[])
}

pub fn verify_package_signature_with_revocations(
    manifest: &Manifest,
    signature: &PackageSignature,
    identity: &IdentityDocument,
    revocations: &[RevocationDocument],
) -> Result<()> {
    if signature.author_iid != identity.iid {
        return Err(PostUrbitError::InvalidInput("author iid mismatch"));
    }

    let signed_at = parse_timestamp(&signature.timestamp)?;
    let now = Utc::now() + Duration::minutes(5);
    if signed_at > now {
        return Err(PostUrbitError::InvalidInput("signature timestamp in future"));
    }
    let genesis = parse_timestamp(&identity.timestamp)?;
    if signed_at < genesis {
        return Err(PostUrbitError::InvalidInput("signature before genesis"));
    }

    let manifest_hash_hex = manifest_hash_hex(manifest)?;
    let expected = format!("sha256:{manifest_hash_hex}");
    if signature.signed_manifest_hash != expected {
        return Err(PostUrbitError::InvalidInput("manifest hash mismatch"));
    }

    let payload = format!("postapp-signature-v1:{manifest_hash_hex}:{}", signature.timestamp);
    let verified_key = verify_with_signing_keys(payload.as_bytes(), signature, identity)?;
    check_revocations(&verified_key, signed_at, revocations)?;
    Ok(())
}

fn manifest_hash_hex(manifest: &Manifest) -> Result<String> {
    let canonical = canonical_json_from(manifest)?;
    let hash = Sha256::digest(canonical.as_bytes());
    Ok(hex::encode(hash))
}

fn verify_with_signing_keys(
    payload: &[u8],
    signature: &PackageSignature,
    identity: &IdentityDocument,
) -> Result<Vec<u8>> {
    let signature_bytes = base64_decode(&signature.signature)?;
    if signature_bytes.len() != 64 {
        return Err(PostUrbitError::InvalidInput("signature length"));
    }
    let signature = Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );

    let mut keys = Vec::new();
    keys.push(identity.keys.signing.current.clone());
    if let Some(prev) = identity.keys.signing.previous.clone() {
        keys.push(prev);
    }
    for hist in &identity.keys.signing.history {
        keys.push(hist.key.clone());
    }

    for key in keys {
        let key_bytes = base64_decode(&key)?;
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
        if verifying_key.verify_strict(payload, &signature).is_ok() {
            return Ok(key_bytes);
        }
    }

    Err(PostUrbitError::Crypto("package signature invalid"))
}

fn check_revocations(
    verified_key: &[u8],
    signed_at: DateTime<Utc>,
    revocations: &[RevocationDocument],
) -> Result<()> {
    for revocation in revocations {
        match revocation {
            RevocationDocument::Identity(doc) => {
                let effective = parse_timestamp(&doc.effective_at)?;
                if effective <= signed_at {
                    return Err(PostUrbitError::InvalidInput("identity revoked"));
                }
            }
            RevocationDocument::Key(doc) => {
                let effective = parse_timestamp(&doc.effective_at)?;
                if effective <= signed_at {
                    let revoked_key = base64_decode(&doc.revoked_key)?;
                    if revoked_key == verified_key {
                        return Err(PostUrbitError::InvalidInput("signing key revoked"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    value
        .parse::<DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))
}

fn validate_app_id(id: &str) -> Result<()> {
    if id.len() > 64 {
        return Err(PostUrbitError::InvalidInput("app id length"));
    }
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() < 2 {
        return Err(PostUrbitError::InvalidInput("app id format"));
    }
    for part in parts {
        let mut chars = part.chars();
        let first = chars
            .next()
            .ok_or(PostUrbitError::InvalidInput("app id format"))?;
        if !first.is_ascii_lowercase() {
            return Err(PostUrbitError::InvalidInput("app id format"));
        }
        for ch in chars {
            if !(ch.is_ascii_lowercase() || ch.is_ascii_digit()) {
                return Err(PostUrbitError::InvalidInput("app id format"));
            }
        }
    }
    Ok(())
}

fn validate_semver(version: &str) -> Result<()> {
    let mut parts = version.split('-');
    let main = parts.next().unwrap_or("");
    let nums: Vec<&str> = main.split('.').collect();
    if nums.len() != 3 {
        return Err(PostUrbitError::InvalidInput("version format"));
    }
    for num in nums {
        if num.is_empty() || num.chars().any(|c| !c.is_ascii_digit()) {
            return Err(PostUrbitError::InvalidInput("version format"));
        }
    }
    Ok(())
}

fn validate_sha256_hash(value: &str) -> Result<()> {
    let Some(hex_part) = value.strip_prefix("sha256:") else {
        return Err(PostUrbitError::InvalidInput("hash format"));
    };
    if hex_part.len() != 64 || hex_part.chars().any(|ch| !ch.is_ascii_hexdigit()) {
        return Err(PostUrbitError::InvalidInput("hash format"));
    }
    Ok(())
}

pub struct CapabilityRegistry {
    method_to_cap: HashMap<String, String>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            method_to_cap: HashMap::new(),
        }
    }

    pub fn register(&mut self, method: &str, cap: &str) {
        self.method_to_cap
            .insert(method.to_string(), cap.to_string());
    }

    pub fn capability_for(&self, method: &str) -> Option<&str> {
        self.method_to_cap.get(method).map(|cap| cap.as_str())
    }

    pub fn require(&self, grants: &[String], method: &str) -> Result<()> {
        let cap = self
            .method_to_cap
            .get(method)
            .ok_or(PostUrbitError::InvalidInput("unknown method"))?;
        if cap.is_empty() {
            return Ok(());
        }
        if !grants.iter().any(|g| g == cap) {
            return Err(PostUrbitError::InvalidInput("capability denied"));
        }
        Ok(())
    }
}

pub trait Storage {
    fn get(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>>;
    fn put(&mut self, namespace: &str, key: &str, value: Vec<u8>) -> Result<()>;
    fn delete_namespace(&mut self, namespace: &str) -> Result<()>;
}

#[derive(Default)]
pub struct MemoryStorage {
    data: HashMap<String, HashMap<String, Vec<u8>>>,
}

impl Storage for MemoryStorage {
    fn get(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .data
            .get(namespace)
            .and_then(|ns| ns.get(key).cloned()))
    }

    fn put(&mut self, namespace: &str, key: &str, value: Vec<u8>) -> Result<()> {
        self.data
            .entry(namespace.to_string())
            .or_default()
            .insert(key.to_string(), value);
        Ok(())
    }

    fn delete_namespace(&mut self, namespace: &str) -> Result<()> {
        self.data.remove(namespace);
        Ok(())
    }
}

pub trait MessagingHost {
    fn send(&self, to: &str, payload: &[u8]) -> Result<()>;
    fn subscribe(&self, handler: &str) -> Result<()>;
}

pub trait ContactsHost {
    fn resolve(&self, iid: &str) -> Result<Option<String>>;
}

pub trait NotificationsHost {
    fn notify(&self, title: &str, body: &str) -> Result<()>;
}

pub trait SyncHost {
    fn request_sync(&self, doc_id: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{
        derive_iid, sign_idoc, Claims, EncryptionKeys, IdentityDocument, Keys, Recovery,
        RevocationDocument, Signatures, SigningKeys,
    };
    use x25519_dalek::{PublicKey, StaticSecret};

    fn sample_manifest() -> Manifest {
        Manifest {
            manifest_version: 1,
            app: AppMetadata {
                id: "com.example.app".to_string(),
                name: "Example".to_string(),
                version: "1.0.0".to_string(),
                description: "desc".to_string(),
                author: Author {
                    name: "dev".to_string(),
                    iid: None,
                    url: None,
                },
                license: "MIT".to_string(),
                homepage: None,
                repository: None,
            },
            runtime: RuntimeConfig {
                entry: "main.wasm".to_string(),
                memory: None,
                fuel: None,
            },
            capabilities: CapabilitiesConfig {
                required: Vec::new(),
                optional: None,
                reasons: None,
            },
            dependencies: DependenciesConfig {
                node_version: None,
                api_version: "1".to_string(),
                apps: None,
            },
            files: FilesConfig {
                hashes: HashMap::from([(
                    "main.wasm".to_string(),
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                        .to_string(),
                )]),
                total_size: 0,
            },
            secrets: None,
            network: None,
        }
    }

    #[test]
    fn manifest_validation_ok() {
        let manifest = sample_manifest();
        validate_manifest(&manifest).unwrap();
    }

    #[test]
    fn manifest_requires_entry_hash() {
        let mut manifest = sample_manifest();
        manifest.files.hashes.clear();
        let err = validate_manifest(&manifest).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn manifest_requires_api_version() {
        let mut manifest = sample_manifest();
        manifest.dependencies.api_version = "2".to_string();
        let err = validate_manifest(&manifest).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn manifest_rejects_bad_hash() {
        let mut manifest = sample_manifest();
        manifest.files.hashes.insert("main.wasm".to_string(), "sha256:bad".to_string());
        let err = validate_manifest(&manifest).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn capabilities_enforce() {
        let mut registry = CapabilityRegistry::new();
        registry.register("messaging.send", "messaging:send");
        let grants = vec!["messaging:send".to_string()];
        registry.require(&grants, "messaging.send").unwrap();
    }

    #[test]
    fn storage_isolated() {
        let mut storage = MemoryStorage::default();
        storage
            .put("app.a", "key", b"value".to_vec())
            .unwrap();
        let value = storage.get("app.a", "key").unwrap();
        assert_eq!(value, Some(b"value".to_vec()));
        let missing = storage.get("app.b", "key").unwrap();
        assert!(missing.is_none());
    }

    fn sample_identity(signing_key: &SigningKey, enc_key: &StaticSecret) -> IdentityDocument {
        let verifying_key = signing_key.verifying_key();
        let iid = derive_iid(&verifying_key);
        let enc_pub = PublicKey::from(enc_key);
        let mut doc = IdentityDocument {
            version: 1,
            iid,
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
            claims: Claims::default(),
            recovery: Recovery {
                method: "none".to_string(),
                config: serde_json::Value::Object(serde_json::Map::new()),
            },
            extensions: serde_json::Value::Object(serde_json::Map::new()),
            recovery_proof: None,
            signatures: Signatures {
                current: String::new(),
                previous: None,
            },
        };
        doc.signatures.current = sign_idoc(&doc, signing_key).unwrap();
        doc
    }

    #[test]
    fn package_signature_round_trip() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let identity = sample_identity(&signing_key, &enc_key);
        let manifest = sample_manifest();

        let signature = sign_package_signature(
            &manifest,
            identity.iid.as_str(),
            "2025-01-15T12:00:00Z",
            &signing_key,
        )
        .unwrap();
        verify_package_signature(&manifest, &signature, &identity).unwrap();
    }

    #[test]
    fn package_signature_rejected_on_revocation() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let enc_key = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let identity = sample_identity(&signing_key, &enc_key);
        let manifest = sample_manifest();

        let signature = sign_package_signature(
            &manifest,
            identity.iid.as_str(),
            "2025-01-15T12:00:00Z",
            &signing_key,
        )
        .unwrap();

        let revocation = RevocationDocument::Identity(crate::identity::IdentityRevocation {
            iid: identity.iid.clone(),
            reason: "compromised".to_string(),
            message: None,
            effective_at: "2025-01-14T12:00:00Z".to_string(),
            successor_iid: None,
            signature: "ignored".to_string(),
        });

        let err = verify_package_signature_with_revocations(
            &manifest,
            &signature,
            &identity,
            &[revocation],
        )
        .unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }
}

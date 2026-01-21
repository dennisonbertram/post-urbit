use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, VerifyingKey};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::canonical_json::canonical_json_value;
use crate::encoding::base64_decode;
use crate::error::{PostUrbitError, Result};
use crate::identity::{fetch_identity, RevocationDocument};
use crate::runtime::{
    parse_manifest, validate_manifest, verify_package, verify_package_signature_with_revocations,
    Manifest, PackageSignature,
};
use crate::dht::Dht;

const MAX_PACKAGE_BYTES: usize = 100 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_MAIN_WASM_BYTES: usize = 50 * 1024 * 1024;
const MAX_SINGLE_ASSET_BYTES: usize = 10 * 1024 * 1024;
const MAX_UI_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug)]
pub struct ParsedPackage {
    pub manifest: Manifest,
    pub signature: PackageSignature,
    pub files: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryManifest {
    pub repository: RepositoryInfo,
    pub apps: Vec<RepositoryApp>,
    pub signature: RepositorySignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInfo {
    pub name: String,
    pub id: String,
    pub operator_iid: String,
    pub url: String,
    pub description: String,
    pub policies: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryApp {
    pub id: String,
    pub name: String,
    pub author_iid: String,
    pub latest_version: String,
    pub download_url: String,
    pub listing: serde_json::Value,
    pub versions: Vec<RepositoryVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryVersion {
    pub version: String,
    pub download_url: String,
    pub size: u64,
    pub released_at: String,
    pub changelog: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySignature {
    pub operator_iid: String,
    pub timestamp: String,
    pub sig: String,
}

pub fn parse_postapp(bytes: &[u8]) -> Result<ParsedPackage> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(PostUrbitError::InvalidInput("package too large"));
    }

    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|_| PostUrbitError::InvalidInput("invalid package zip"))?;

    let mut files = HashMap::new();
    let mut manifest_bytes: Option<Vec<u8>> = None;
    let mut signature_bytes: Option<Vec<u8>> = None;
    let mut ui_total = 0usize;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|_| PostUrbitError::InvalidInput("invalid package entry"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        // Validate path safety: reject absolute paths and traversal attempts
        let path = std::path::Path::new(&name);
        if path.is_absolute() {
            return Err(PostUrbitError::InvalidInput("package contains absolute path"));
        }
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(PostUrbitError::InvalidInput("package contains path traversal"));
                }
                std::path::Component::Normal(_) | std::path::Component::CurDir => {}
                _ => {
                    return Err(PostUrbitError::InvalidInput("package contains invalid path component"));
                }
            }
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)
            .map_err(|_| PostUrbitError::InvalidInput("package read"))?;

        if name == "manifest.json" {
            if buf.len() > MAX_MANIFEST_BYTES {
                return Err(PostUrbitError::InvalidInput("manifest too large"));
            }
            manifest_bytes = Some(buf.clone());
        } else if name == "SIGNATURE" {
            signature_bytes = Some(buf.clone());
        } else {
            if name == "main.wasm" && buf.len() > MAX_MAIN_WASM_BYTES {
                return Err(PostUrbitError::InvalidInput("main.wasm too large"));
            }
            if name.starts_with("assets/") && buf.len() > MAX_SINGLE_ASSET_BYTES {
                return Err(PostUrbitError::InvalidInput("asset too large"));
            }
            if name.starts_with("ui/") {
                ui_total = ui_total.saturating_add(buf.len());
                if ui_total > MAX_UI_BYTES {
                    return Err(PostUrbitError::InvalidInput("ui assets too large"));
                }
            }
        }
        files.insert(name, buf);
    }

    let manifest_bytes = manifest_bytes.ok_or(PostUrbitError::InvalidInput("missing manifest"))?;
    let signature_bytes = signature_bytes.ok_or(PostUrbitError::InvalidInput("missing signature"))?;

    let manifest = parse_manifest(&manifest_bytes)?;
    validate_manifest(&manifest)?;

    let signature: PackageSignature = serde_json::from_slice(&signature_bytes)
        .map_err(|_| PostUrbitError::InvalidInput("signature json"))?;

    Ok(ParsedPackage { manifest, signature, files })
}

pub async fn verify_package_with_dht(
    dht: &dyn Dht,
    package: &ParsedPackage,
) -> Result<()> {
    let author = fetch_identity(dht, &package.signature.author_iid)
        .await?
        .ok_or(PostUrbitError::InvalidInput("author identity not found"))?;
    let revocations: Vec<RevocationDocument> = fetch_revocations(dht, &package.signature.author_iid).await?;
    verify_package(&package.manifest, &package.files)?;
    verify_package_signature_with_revocations(&package.manifest, &package.signature, &author, &revocations)?;
    Ok(())
}

pub fn extract_package(package: &ParsedPackage, target_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(target_dir).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    let canonical_target = target_dir
        .canonicalize()
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;

    for (path, data) in &package.files {
        if path == "SIGNATURE" {
            continue;
        }

        // Validate path safety: reject absolute paths and traversal attempts
        let file_path = std::path::Path::new(path);
        if file_path.is_absolute() {
            return Err(PostUrbitError::InvalidInput("package contains absolute path"));
        }
        for component in file_path.components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(PostUrbitError::InvalidInput("package contains path traversal"));
                }
                std::path::Component::Normal(_) | std::path::Component::CurDir => {}
                _ => {
                    return Err(PostUrbitError::InvalidInput("package contains invalid path component"));
                }
            }
        }

        let full = target_dir.join(path);

        // Verify the resolved path is within the target directory
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|err| PostUrbitError::Io(err.to_string()))?;
            let canonical_parent = parent
                .canonicalize()
                .map_err(|err| PostUrbitError::Io(err.to_string()))?;
            if !canonical_parent.starts_with(&canonical_target) {
                return Err(PostUrbitError::InvalidInput("package entry escapes target directory"));
            }
        }

        std::fs::write(&full, data).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    }
    Ok(())
}

pub fn install_package(package: &ParsedPackage, apps_dir: &Path) -> Result<PathBuf> {
    let app_dir = apps_dir.join(&package.manifest.app.id);
    if app_dir.exists() {
        return Err(PostUrbitError::InvalidInput("app already installed"));
    }
    extract_package(package, &app_dir)?;
    Ok(app_dir)
}

pub async fn fetch_repository(url: &str) -> Result<RepositoryManifest> {
    let response = reqwest::get(url).await
        .map_err(|_| PostUrbitError::InvalidInput("repository fetch"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(PostUrbitError::InvalidInput("repository status"));
    }
    let bytes = response.bytes().await
        .map_err(|_| PostUrbitError::InvalidInput("repository read"))?;
    let manifest: RepositoryManifest = serde_json::from_slice(&bytes)
        .map_err(|_| PostUrbitError::InvalidInput("repository json"))?;
    Ok(manifest)
}

pub async fn verify_repository(dht: &dyn Dht, manifest: &RepositoryManifest) -> Result<Vec<u8>> {
    // Verify operator IID consistency: signature must match repository metadata
    if manifest.signature.operator_iid != manifest.repository.operator_iid {
        return Err(PostUrbitError::InvalidInput("repository operator iid mismatch"));
    }

    let mut value = serde_json::to_value(manifest)
        .map_err(|_| PostUrbitError::InvalidInput("repository json"))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature");
    }
    let canonical = canonical_json_value(&value)?;
    let hash = Sha256::digest(canonical.as_bytes());
    let hash_hex = hex::encode(hash);
    let payload = format!("postnode-repo-v1:{hash_hex}:{}", manifest.signature.timestamp);

    let author = fetch_identity(dht, &manifest.signature.operator_iid)
        .await?
        .ok_or(PostUrbitError::InvalidInput("operator identity not found"))?;
    let sig_bytes = base64_decode(&manifest.signature.sig)?;
    if sig_bytes.len() != 64 {
        return Err(PostUrbitError::InvalidInput("repository signature length"));
    }
    let signature = Signature::from_bytes(sig_bytes.as_slice().try_into().map_err(|_| PostUrbitError::InvalidInput("signature length"))?);

    let candidates = signing_keys_for_identity(&author);
    let mut verified_key: Option<Vec<u8>> = None;
    for key in candidates {
        if let Ok(verifying) = VerifyingKey::from_bytes(key.as_slice().try_into().map_err(|_| PostUrbitError::InvalidInput("signing key length"))?) {
            if verifying.verify_strict(payload.as_bytes(), &signature).is_ok() {
                verified_key = Some(key);
                break;
            }
        }
    }
    let Some(verified_key) = verified_key else {
        return Err(PostUrbitError::Crypto("repository signature"));
    };

    let signed_at = parse_timestamp(&manifest.signature.timestamp)?;

    // Validate timestamp is not too far in the future (5 minute skew allowed)
    let now = chrono::Utc::now() + Duration::minutes(5);
    if signed_at > now {
        return Err(PostUrbitError::InvalidInput("repository signature timestamp in future"));
    }

    // Validate signature is not before the identity's genesis timestamp
    // Fetch the actual genesis document (sequence 0) with full validation
    let genesis_key = crate::dht::dht_key_genesis(&author.iid);
    let genesis_values = dht.get_all(&genesis_key).await?;
    let genesis_doc = pick_valid_genesis_doc(&author.iid, &genesis_values)?;
    let genesis = parse_timestamp(&genesis_doc.timestamp)?;
    if signed_at < genesis {
        return Err(PostUrbitError::InvalidInput("repository signature before genesis"));
    }

    let revocations = fetch_revocations(dht, &manifest.signature.operator_iid).await?;
    check_revocations(&verified_key, signed_at, &revocations)?;

    Ok(verified_key)
}

fn signing_keys_for_identity(identity: &crate::identity::IdentityDocument) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Ok(current) = base64_decode(&identity.keys.signing.current) {
        out.push(current);
    }
    if let Some(prev) = identity.keys.signing.previous.as_ref() {
        if let Ok(decoded) = base64_decode(prev) {
            out.push(decoded);
        }
    }
    for entry in &identity.keys.signing.history {
        if let Ok(decoded) = base64_decode(&entry.key) {
            out.push(decoded);
        }
    }
    out
}

/// Pick a validated genesis document from DHT values, resilient to poisoned entries.
fn pick_valid_genesis_doc(
    iid: &str,
    values: &[Vec<u8>],
) -> Result<crate::identity::IdentityDocument> {
    for value in values {
        let doc = match crate::identity::decode_idoc_envelope(value) {
            Ok(doc) => doc,
            Err(_) => continue,
        };
        // verify_genesis_document checks: sequence == "0", iid match, genesis key == current key,
        // IID derived from genesis key, and signature validity
        if crate::identity::verify_genesis_document(&doc, iid).is_err() {
            continue;
        }
        return Ok(doc);
    }
    Err(PostUrbitError::InvalidInput("genesis identity not found"))
}

async fn fetch_revocations(dht: &dyn Dht, iid: &str) -> Result<Vec<RevocationDocument>> {
    let key = crate::dht::dht_key_revocation(iid);
    let values = dht.get_all(&key).await?;
    let mut out = Vec::new();
    for value in values {
        let doc: RevocationDocument = serde_json::from_slice(&value)
            .map_err(|_| PostUrbitError::InvalidInput("revocation json"))?;
        out.push(doc);
    }
    Ok(out)
}

fn parse_timestamp(value: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    value
        .parse::<chrono::DateTime<chrono::Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))
}

fn check_revocations(
    verified_key: &[u8],
    signed_at: chrono::DateTime<chrono::Utc>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use std::io::Write;
    use std::time::Duration as StdDuration;
    use x25519_dalek::{PublicKey, StaticSecret};
    use crate::dht::{dht_key_genesis, dht_key_identity, MemoryDht};
    use crate::encoding::base64_encode;
    use crate::identity::{
        derive_iid, encode_idoc_envelope, sign_idoc, Claims, EncryptionKeys, IdentityDocument,
        Keys, Recovery, Signatures, SigningKeys,
    };
    use crate::runtime::{AppMetadata, Author, CapabilitiesConfig, DependenciesConfig, FilesConfig, RuntimeConfig, Manifest};

    fn build_manifest(hash: &str, size: u64) -> Manifest {
        Manifest {
            manifest_version: 1,
            app: AppMetadata {
                id: "com.example.test".to_string(),
                name: "Test".to_string(),
                version: "1.0.0".to_string(),
                description: "Test app".to_string(),
                author: Author {
                    name: "Author".to_string(),
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
                required: vec!["storage:app".to_string()],
                optional: None,
                reasons: None,
            },
            dependencies: DependenciesConfig {
                node_version: None,
                api_version: "1".to_string(),
                apps: None,
            },
            files: FilesConfig {
                hashes: HashMap::from([("main.wasm".to_string(), hash.to_string())]),
                total_size: size,
            },
            secrets: None,
            network: None,
        }
    }

    fn build_package_bytes(manifest: &Manifest, signature: &PackageSignature, main_wasm: &[u8]) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(manifest).unwrap()).unwrap();
        zip.start_file("SIGNATURE", options).unwrap();
        zip.write_all(&serde_json::to_vec(signature).unwrap()).unwrap();
        zip.start_file("main.wasm", options).unwrap();
        zip.write_all(main_wasm).unwrap();
        let cursor = zip.finish().unwrap();
        cursor.into_inner()
    }

    #[test]
    fn parse_postapp_round_trip() {
        let main_wasm = b"\0asm".to_vec();
        let hash = format!("sha256:{}", hex::encode(Sha256::digest(&main_wasm)));
        let manifest = build_manifest(&hash, main_wasm.len() as u64);
        let signing_key = SigningKey::generate(&mut OsRng);
        let signature = crate::runtime::sign_package_signature(
            &manifest,
            "author_iid",
            "2025-01-01T00:00:00Z",
            &signing_key,
        ).unwrap();
        let bytes = build_package_bytes(&manifest, &signature, &main_wasm);

        let parsed = parse_postapp(&bytes).unwrap();
        assert_eq!(parsed.manifest.app.id, "com.example.test");
        assert!(parsed.files.contains_key("main.wasm"));
    }

    #[test]
    fn parse_postapp_rejects_absolute_path() {
        let main_wasm = b"\0asm".to_vec();
        let hash = format!("sha256:{}", hex::encode(Sha256::digest(&main_wasm)));
        let manifest = build_manifest(&hash, main_wasm.len() as u64);
        let signing_key = SigningKey::generate(&mut OsRng);
        let signature = crate::runtime::sign_package_signature(
            &manifest,
            "author_iid",
            "2025-01-01T00:00:00Z",
            &signing_key,
        ).unwrap();

        // Build a zip with an absolute path entry
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        zip.start_file("SIGNATURE", options).unwrap();
        zip.write_all(&serde_json::to_vec(&signature).unwrap()).unwrap();
        zip.start_file("main.wasm", options).unwrap();
        zip.write_all(&main_wasm).unwrap();
        zip.start_file("/etc/passwd", options).unwrap();
        zip.write_all(b"malicious content").unwrap();
        let cursor = zip.finish().unwrap();
        let bytes = cursor.into_inner();

        let result = parse_postapp(&bytes);
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("absolute path"), "Expected error about absolute path, got: {}", err_msg);
    }

    #[test]
    fn parse_postapp_rejects_traversal() {
        let main_wasm = b"\0asm".to_vec();
        let hash = format!("sha256:{}", hex::encode(Sha256::digest(&main_wasm)));
        let manifest = build_manifest(&hash, main_wasm.len() as u64);
        let signing_key = SigningKey::generate(&mut OsRng);
        let signature = crate::runtime::sign_package_signature(
            &manifest,
            "author_iid",
            "2025-01-01T00:00:00Z",
            &signing_key,
        ).unwrap();

        // Build a zip with a path traversal entry
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        zip.start_file("SIGNATURE", options).unwrap();
        zip.write_all(&serde_json::to_vec(&signature).unwrap()).unwrap();
        zip.start_file("main.wasm", options).unwrap();
        zip.write_all(&main_wasm).unwrap();
        zip.start_file("assets/../../evil.txt", options).unwrap();
        zip.write_all(b"malicious content").unwrap();
        let cursor = zip.finish().unwrap();
        let bytes = cursor.into_inner();

        let result = parse_postapp(&bytes);
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("traversal"), "Expected error about traversal, got: {}", err_msg);
    }

    #[test]
    fn extract_package_rejects_traversal() {
        // Create a ParsedPackage directly with a malicious path
        let main_wasm = b"\0asm".to_vec();
        let hash = format!("sha256:{}", hex::encode(Sha256::digest(&main_wasm)));
        let manifest = build_manifest(&hash, main_wasm.len() as u64);
        let signing_key = SigningKey::generate(&mut OsRng);
        let signature = crate::runtime::sign_package_signature(
            &manifest,
            "author_iid",
            "2025-01-01T00:00:00Z",
            &signing_key,
        ).unwrap();

        let mut files = HashMap::new();
        files.insert("main.wasm".to_string(), main_wasm);
        files.insert("../evil.txt".to_string(), b"malicious content".to_vec());

        let package = ParsedPackage {
            manifest,
            signature,
            files,
        };

        let temp_dir = tempfile::tempdir().unwrap();

        let result = extract_package(&package, temp_dir.path());
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("traversal"), "Expected error about traversal, got: {}", err_msg);
        // tempfile::tempdir() auto-cleans on drop
    }

    fn build_sample_identity(signing_key: &SigningKey, enc_key: &StaticSecret, timestamp: &str) -> IdentityDocument {
        let verifying_key = signing_key.verifying_key();
        let iid = derive_iid(&verifying_key);
        let enc_pub = PublicKey::from(enc_key);
        let mut doc = IdentityDocument {
            version: 1,
            iid,
            sequence: "0".to_string(),
            timestamp: timestamp.to_string(),
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

    fn build_repository_manifest(operator_iid: &str, timestamp: &str, signing_key: &SigningKey) -> RepositoryManifest {
        let mut manifest = RepositoryManifest {
            repository: RepositoryInfo {
                name: "Test Repository".to_string(),
                id: "test-repo".to_string(),
                operator_iid: operator_iid.to_string(),
                url: "https://example.com/repo".to_string(),
                description: "A test repository".to_string(),
                policies: serde_json::json!({}),
            },
            apps: vec![],
            signature: RepositorySignature {
                operator_iid: operator_iid.to_string(),
                timestamp: timestamp.to_string(),
                sig: String::new(),
            },
        };

        // Compute signature following the pattern from verify_repository
        let mut value = serde_json::to_value(&manifest).unwrap();
        if let serde_json::Value::Object(ref mut map) = value {
            map.remove("signature");
        }
        let canonical = canonical_json_value(&value).unwrap();
        let hash = Sha256::digest(canonical.as_bytes());
        let hash_hex = hex::encode(hash);
        let payload = format!("postnode-repo-v1:{hash_hex}:{timestamp}");
        let signature = signing_key.sign(payload.as_bytes());
        manifest.signature.sig = base64_encode(signature.to_bytes().as_slice());

        manifest
    }

    async fn store_identity_in_dht(dht: &MemoryDht, identity: &IdentityDocument) {
        let identity_key = dht_key_identity(&identity.iid);
        let genesis_key = dht_key_genesis(&identity.iid);
        let identity_bytes = encode_idoc_envelope(identity).unwrap();
        // Store both identity and genesis documents
        dht.put(&identity_key, identity_bytes.clone(), StdDuration::from_secs(3600)).await.unwrap();
        dht.put(&genesis_key, identity_bytes, StdDuration::from_secs(3600)).await.unwrap();
    }

    #[tokio::test]
    async fn verify_repository_rejects_future_timestamp() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let enc_key = StaticSecret::random_from_rng(OsRng);
        let identity = build_sample_identity(&signing_key, &enc_key, "2025-01-15T00:00:00Z");

        // Create a timestamp 10 minutes in the future
        let future_time = chrono::Utc::now() + chrono::Duration::minutes(10);
        let future_timestamp = future_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let manifest = build_repository_manifest(&identity.iid, &future_timestamp, &signing_key);

        // Set up mock DHT with the identity
        let dht = MemoryDht::new();
        store_identity_in_dht(&dht, &identity).await;

        let result = verify_repository(&dht, &manifest).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("future"), "Expected error about future timestamp, got: {}", err_msg);
    }

    #[tokio::test]
    async fn verify_repository_rejects_pre_genesis_timestamp() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let enc_key = StaticSecret::random_from_rng(OsRng);
        // Identity genesis is at 2025-01-15
        let identity = build_sample_identity(&signing_key, &enc_key, "2025-01-15T00:00:00Z");

        // Signature timestamp is before genesis (2025-01-10)
        let pre_genesis_timestamp = "2025-01-10T00:00:00Z";
        let manifest = build_repository_manifest(&identity.iid, pre_genesis_timestamp, &signing_key);

        // Set up mock DHT with the identity
        let dht = MemoryDht::new();
        store_identity_in_dht(&dht, &identity).await;

        let result = verify_repository(&dht, &manifest).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("genesis"), "Expected error about genesis timestamp, got: {}", err_msg);
    }

    #[tokio::test]
    async fn verify_repository_rejects_operator_iid_mismatch() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let enc_key = StaticSecret::random_from_rng(OsRng);
        let identity = build_sample_identity(&signing_key, &enc_key, "2025-01-15T00:00:00Z");

        // Build a valid manifest
        let mut manifest = build_repository_manifest(&identity.iid, "2025-01-20T00:00:00Z", &signing_key);

        // Tamper: make repository.operator_iid different from signature.operator_iid
        manifest.repository.operator_iid = "different-operator-iid".to_string();

        // Set up mock DHT with the identity
        let dht = MemoryDht::new();
        store_identity_in_dht(&dht, &identity).await;

        let result = verify_repository(&dht, &manifest).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("mismatch"), "Expected error about operator iid mismatch, got: {}", err_msg);
    }
}

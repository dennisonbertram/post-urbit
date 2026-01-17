use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, VerifyingKey};
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
        if name.contains("..") {
            return Err(PostUrbitError::InvalidInput("invalid package path"));
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
    for (path, data) in &package.files {
        if path == "SIGNATURE" {
            continue;
        }
        let full = target_dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|err| PostUrbitError::Io(err.to_string()))?;
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

pub async fn verify_repository(dht: &dyn Dht, manifest: &RepositoryManifest) -> Result<()> {
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
    let mut verified = false;
    for key in candidates {
        if let Ok(verifying) = VerifyingKey::from_bytes(key.as_slice().try_into().map_err(|_| PostUrbitError::InvalidInput("signing key length"))?) {
            if verifying.verify_strict(payload.as_bytes(), &signature).is_ok() {
                verified = true;
                break;
            }
        }
    }
    if !verified {
        return Err(PostUrbitError::Crypto("repository signature"));
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use std::io::Write;
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
}

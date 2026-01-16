use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{PostUrbitError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u8,
    pub app: AppMetadata,
    pub runtime: RuntimeConfig,
    pub capabilities: CapabilitiesConfig,
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
pub struct FilesConfig {
    pub hashes: HashMap<String, String>,
    pub total_size: u64,
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest> {
    serde_json::from_slice(bytes).map_err(|_| PostUrbitError::InvalidInput("manifest json"))
}

pub fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.manifest_version != 1 {
        return Err(PostUrbitError::InvalidInput("manifest version"));
    }
    validate_app_id(&manifest.app.id)?;
    validate_semver(&manifest.app.version)?;
    if !manifest.runtime.entry.ends_with(".wasm") {
        return Err(PostUrbitError::InvalidInput("runtime entry"));
    }
    if manifest.capabilities.required.is_empty() {
        return Err(PostUrbitError::InvalidInput("capabilities required"));
    }
    Ok(())
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

    pub fn require(&self, grants: &[String], method: &str) -> Result<()> {
        let cap = self
            .method_to_cap
            .get(method)
            .ok_or(PostUrbitError::InvalidInput("unknown method"))?;
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
                required: vec!["storage:app".to_string()],
                optional: None,
                reasons: None,
            },
            files: FilesConfig {
                hashes: HashMap::new(),
                total_size: 0,
            },
        }
    }

    #[test]
    fn manifest_validation_ok() {
        let manifest = sample_manifest();
        validate_manifest(&manifest).unwrap();
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
}

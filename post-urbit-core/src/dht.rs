use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::error::{PostUrbitError, Result};

#[async_trait]
pub trait Dht: Send + Sync {
    async fn put(&self, key: &[u8], value: Vec<u8>, ttl: Duration) -> Result<()>;
    async fn get_all(&self, key: &[u8]) -> Result<Vec<Vec<u8>>>;
}

#[derive(Clone)]
pub struct MemoryDht {
    inner: Arc<Mutex<HashMap<Vec<u8>, Vec<StoredValue>>>>,
    now: Arc<dyn Fn() -> SystemTime + Send + Sync>,
}

#[derive(Clone)]
struct StoredValue {
    value: Vec<u8>,
    expires_at: Option<SystemTime>,
}

impl MemoryDht {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            now: Arc::new(SystemTime::now),
        }
    }

    pub fn with_time(now: Arc<dyn Fn() -> SystemTime + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            now,
        }
    }
}

#[async_trait]
impl Dht for MemoryDht {
    async fn put(&self, key: &[u8], value: Vec<u8>, ttl: Duration) -> Result<()> {
        let mut guard = self.inner.lock().await;
        let expires_at = if ttl.as_secs() == 0 && ttl.subsec_nanos() == 0 {
            None
        } else {
            Some((self.now)() + ttl)
        };

        let entry = guard.entry(key.to_vec()).or_default();
        if entry.iter().any(|stored| stored.value == value) {
            return Ok(());
        }
        entry.push(StoredValue { value, expires_at });
        Ok(())
    }

    async fn get_all(&self, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut guard = self.inner.lock().await;
        let now = (self.now)();
        let entry = guard.get_mut(key);
        if entry.is_none() {
            return Ok(Vec::new());
        }
        let values = entry.unwrap();
        values.retain(|stored| match stored.expires_at {
            Some(expires_at) => expires_at > now,
            None => true,
        });
        Ok(values.iter().map(|stored| stored.value.clone()).collect())
    }
}

pub fn dht_key_identity(iid: &str) -> [u8; 32] {
    dht_key_with_prefix(b"post-urbit:identity:", iid)
}

pub fn dht_key_genesis(iid: &str) -> [u8; 32] {
    dht_key_with_prefix(b"post-urbit:genesis:", iid)
}

pub fn dht_key_devices(iid: &str) -> [u8; 32] {
    dht_key_with_prefix(b"post-urbit:devices-for:", iid)
}

fn dht_key_with_prefix(prefix: &[u8], iid: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(iid.as_bytes());
    let digest = hasher.finalize();
    digest
        .as_slice()
        .try_into()
        .expect("sha256 output length")
}

pub fn validate_dht_key(key: &[u8]) -> Result<()> {
    if key.len() != 32 {
        return Err(PostUrbitError::InvalidInput("dht key length"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_dht_put_get_all() {
        let dht = MemoryDht::new();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        dht.put(&key, b"value".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values, vec![b"value".to_vec()]);
    }

    #[tokio::test]
    async fn memory_dht_ttl_expires() {
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let now = Arc::new(std::sync::Mutex::new(base));
        let now_clone = now.clone();
        let now_fn = Arc::new(move || *now_clone.lock().unwrap());
        let dht = MemoryDht::with_time(now_fn);
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        dht.put(&key, b"value".to_vec(), Duration::from_secs(5))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 1);

        *now.lock().unwrap() = base + Duration::from_secs(10);
        let values = dht.get_all(&key).await.unwrap();
        assert!(values.is_empty());
    }

    #[tokio::test]
    async fn memory_dht_dedupes() {
        let dht = MemoryDht::new();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        dht.put(&key, b"value".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();
        dht.put(&key, b"value".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn dht_key_helpers_length() {
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        assert_eq!(key.len(), 32);
    }
}

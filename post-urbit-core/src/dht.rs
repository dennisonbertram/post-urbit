use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read as IoRead, Write as IoWrite};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
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

/// Record stored on disk for FileDht
#[derive(Serialize, Deserialize, Clone, Debug)]
struct DhtRecord {
    /// The stored values (multiple values per key supported)
    values: Vec<DhtValue>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct DhtValue {
    /// The actual value bytes (base64 encoded in JSON)
    #[serde(with = "base64_bytes")]
    value: Vec<u8>,
    /// TTL in seconds (0 means no expiration)
    ttl_secs: u64,
    /// Unix timestamp when the record was created
    created_at: u64,
}

impl DhtValue {
    fn is_expired(&self, now_unix: u64) -> bool {
        if self.ttl_secs == 0 {
            return false;
        }
        now_unix > self.created_at + self.ttl_secs
    }
}

mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = STANDARD.encode(bytes);
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

/// File-based DHT that persists to disk
/// Data is stored in a directory with one file per key
#[derive(Clone)]
pub struct FileDht {
    base_dir: PathBuf,
    cache: Arc<Mutex<HashMap<Vec<u8>, DhtRecord>>>,
    now: Arc<dyn Fn() -> SystemTime + Send + Sync>,
}

impl FileDht {
    /// Create a new FileDht with the given base directory
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        Self::with_time(base_dir, Arc::new(SystemTime::now))
    }

    /// Create a new FileDht with a custom time function (for testing)
    pub fn with_time(base_dir: PathBuf, now: Arc<dyn Fn() -> SystemTime + Send + Sync>) -> Result<Self> {
        // Create the base directory if it doesn't exist
        fs::create_dir_all(&base_dir).map_err(|e| {
            PostUrbitError::Io(format!("Failed to create DHT directory: {}", e))
        })?;

        let mut dht = Self {
            base_dir,
            cache: Arc::new(Mutex::new(HashMap::new())),
            now,
        };

        // Load existing records from disk
        dht.load_from_disk_sync()?;

        Ok(dht)
    }

    /// Get the file path for a given key
    fn key_to_path(&self, key: &[u8]) -> PathBuf {
        let hex_key = hex::encode(key);
        self.base_dir.join(format!("{}.json", hex_key))
    }

    /// Load existing records from disk (synchronous, called during initialization)
    fn load_from_disk_sync(&mut self) -> Result<()> {
        let entries = fs::read_dir(&self.base_dir).map_err(|e| {
            PostUrbitError::Io(format!("Failed to read DHT directory: {}", e))
        })?;

        let now_unix = self.current_unix_time();
        let cache = self.cache.clone();
        let mut cache_guard = futures::executor::block_on(cache.lock());

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // Extract key from filename
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };

            let key = match hex::decode(stem) {
                Ok(k) => k,
                Err(_) => continue,
            };

            // Read and parse the record
            match self.read_record_from_file(&path) {
                Ok(mut record) => {
                    // Filter out expired values
                    record.values.retain(|v| !v.is_expired(now_unix));
                    if !record.values.is_empty() {
                        cache_guard.insert(key, record);
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(())
    }

    /// Read a record from a file with file locking
    fn read_record_from_file(&self, path: &PathBuf) -> Result<DhtRecord> {
        let file = File::open(path).map_err(|e| {
            PostUrbitError::Io(format!("Failed to open DHT file: {}", e))
        })?;

        // Acquire shared lock for reading
        file.lock_shared().map_err(|e| {
            PostUrbitError::Io(format!("Failed to acquire read lock: {}", e))
        })?;

        let mut contents = String::new();
        let mut reader = std::io::BufReader::new(&file);
        reader.read_to_string(&mut contents).map_err(|e| {
            PostUrbitError::Io(format!("Failed to read DHT file: {}", e))
        })?;

        // Lock is released when file is dropped
        serde_json::from_str(&contents).map_err(|e| {
            PostUrbitError::Io(format!("Failed to parse DHT record: {}", e))
        })
    }

    /// Save a record to disk with file locking
    fn save_to_disk(&self, key: &[u8], record: &DhtRecord) -> Result<()> {
        let path = self.key_to_path(key);

        // Create or open the file
        let file = File::create(&path).map_err(|e| {
            PostUrbitError::Io(format!("Failed to create DHT file: {}", e))
        })?;

        // Acquire exclusive lock for writing
        file.lock_exclusive().map_err(|e| {
            PostUrbitError::Io(format!("Failed to acquire write lock: {}", e))
        })?;

        let json = serde_json::to_string_pretty(record).map_err(|e| {
            PostUrbitError::Io(format!("Failed to serialize DHT record: {}", e))
        })?;

        let mut writer = std::io::BufWriter::new(&file);
        writer.write_all(json.as_bytes()).map_err(|e| {
            PostUrbitError::Io(format!("Failed to write DHT file: {}", e))
        })?;

        writer.flush().map_err(|e| {
            PostUrbitError::Io(format!("Failed to flush DHT file: {}", e))
        })?;

        // Lock is released when file is dropped
        Ok(())
    }

    /// Delete a record file from disk
    fn delete_from_disk(&self, key: &[u8]) -> Result<()> {
        let path = self.key_to_path(key);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| {
                PostUrbitError::Io(format!("Failed to delete DHT file: {}", e))
            })?;
        }
        Ok(())
    }

    /// Remove expired records from cache and disk
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let now_unix = self.current_unix_time();
        let mut cache = self.cache.lock().await;
        let mut removed_count = 0;

        let keys: Vec<Vec<u8>> = cache.keys().cloned().collect();

        for key in keys {
            if let Some(record) = cache.get_mut(&key) {
                let original_len = record.values.len();
                record.values.retain(|v| !v.is_expired(now_unix));
                removed_count += original_len - record.values.len();

                if record.values.is_empty() {
                    cache.remove(&key);
                    self.delete_from_disk(&key)?;
                } else if original_len != record.values.len() {
                    self.save_to_disk(&key, record)?;
                }
            }
        }

        Ok(removed_count)
    }

    /// Get current time as Unix timestamp
    fn current_unix_time(&self) -> u64 {
        (self.now)()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[async_trait]
impl Dht for FileDht {
    async fn put(&self, key: &[u8], value: Vec<u8>, ttl: Duration) -> Result<()> {
        let now_unix = self.current_unix_time();
        let ttl_secs = ttl.as_secs();

        let new_value = DhtValue {
            value: value.clone(),
            ttl_secs,
            created_at: now_unix,
        };

        let mut cache = self.cache.lock().await;

        let record = cache.entry(key.to_vec()).or_insert_with(|| DhtRecord {
            values: Vec::new(),
        });

        // Filter out expired values first
        record.values.retain(|v| !v.is_expired(now_unix));

        // Check for duplicates (same value bytes)
        if record.values.iter().any(|v| v.value == value) {
            return Ok(());
        }

        record.values.push(new_value);

        // Save to disk
        self.save_to_disk(key, record)?;

        Ok(())
    }

    async fn get_all(&self, key: &[u8]) -> Result<Vec<Vec<u8>>> {
        let now_unix = self.current_unix_time();
        let mut cache = self.cache.lock().await;

        let record = match cache.get_mut(key) {
            Some(r) => r,
            None => return Ok(Vec::new()),
        };

        // Filter out expired values
        let original_len = record.values.len();
        record.values.retain(|v| !v.is_expired(now_unix));

        // If values were removed, update disk
        if record.values.len() != original_len {
            if record.values.is_empty() {
                let key_owned = key.to_vec();
                cache.remove(&key_owned);
                self.delete_from_disk(key)?;
                return Ok(Vec::new());
            } else {
                self.save_to_disk(key, record)?;
            }
        }

        Ok(record.values.iter().map(|v| v.value.clone()).collect())
    }
}

/// Configuration for creating a DHT instance
pub enum DhtConfig {
    /// In-memory DHT (data lost on restart)
    Memory,
    /// File-based DHT (data persisted to disk)
    File { base_dir: PathBuf },
}

/// Create a DHT instance based on the configuration
pub fn create_dht(config: DhtConfig) -> Result<Box<dyn Dht + Send + Sync>> {
    match config {
        DhtConfig::Memory => Ok(Box::new(MemoryDht::new())),
        DhtConfig::File { base_dir } => Ok(Box::new(FileDht::new(base_dir)?)),
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

pub fn dht_key_device(did: &str) -> [u8; 32] {
    dht_key_with_prefix(b"post-urbit:device:", did)
}

pub fn dht_key_revocation(iid: &str) -> [u8; 32] {
    dht_key_with_prefix(b"post-urbit:revocation:", iid)
}

pub fn dht_key_device_revocation(did: &str) -> [u8; 32] {
    dht_key_with_prefix(b"post-urbit:device-revocation:", did)
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
        let device_key = dht_key_device("42kbzq2tyab939amybd76bm8kfpzgn95");
        assert_eq!(device_key.len(), 32);
    }

    // ================== FileDht Tests ==================

    #[tokio::test]
    async fn file_dht_put_get_all() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dht = FileDht::new(temp_dir.path().to_path_buf()).unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");

        dht.put(&key, b"value".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values, vec![b"value".to_vec()]);
    }

    #[tokio::test]
    async fn file_dht_multiple_values_per_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dht = FileDht::new(temp_dir.path().to_path_buf()).unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");

        dht.put(&key, b"value1".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();
        dht.put(&key, b"value2".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&b"value1".to_vec()));
        assert!(values.contains(&b"value2".to_vec()));
    }

    #[tokio::test]
    async fn file_dht_dedupes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dht = FileDht::new(temp_dir.path().to_path_buf()).unwrap();
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

    #[tokio::test]
    async fn file_dht_ttl_expires() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let now = Arc::new(std::sync::Mutex::new(base));
        let now_clone = now.clone();
        let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
            Arc::new(move || *now_clone.lock().unwrap());

        let dht = FileDht::with_time(temp_dir.path().to_path_buf(), now_fn).unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");

        dht.put(&key, b"value".to_vec(), Duration::from_secs(5))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 1);

        // Advance time past TTL
        *now.lock().unwrap() = base + Duration::from_secs(10);

        let values = dht.get_all(&key).await.unwrap();
        assert!(values.is_empty());
    }

    #[tokio::test]
    async fn file_dht_no_ttl_does_not_expire() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let now = Arc::new(std::sync::Mutex::new(base));
        let now_clone = now.clone();
        let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
            Arc::new(move || *now_clone.lock().unwrap());

        let dht = FileDht::with_time(temp_dir.path().to_path_buf(), now_fn).unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");

        // TTL of 0 means no expiration
        dht.put(&key, b"value".to_vec(), Duration::from_secs(0))
            .await
            .unwrap();

        // Advance time significantly
        *now.lock().unwrap() = base + Duration::from_secs(1000000);

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 1);
    }

    #[tokio::test]
    async fn file_dht_round_trip_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        let key2 = dht_key_device("42kbzq2tyab939amybd76bm8kfpzgn95");

        // Create DHT and write data
        {
            let dht = FileDht::new(temp_dir.path().to_path_buf()).unwrap();
            dht.put(&key, b"value1".to_vec(), Duration::from_secs(3600))
                .await
                .unwrap();
            dht.put(&key, b"value2".to_vec(), Duration::from_secs(3600))
                .await
                .unwrap();
            dht.put(&key2, b"device_data".to_vec(), Duration::from_secs(3600))
                .await
                .unwrap();
        }
        // DHT is dropped here

        // Create new DHT instance and verify data persisted
        {
            let dht = FileDht::new(temp_dir.path().to_path_buf()).unwrap();
            let values = dht.get_all(&key).await.unwrap();
            assert_eq!(values.len(), 2);
            assert!(values.contains(&b"value1".to_vec()));
            assert!(values.contains(&b"value2".to_vec()));

            let values2 = dht.get_all(&key2).await.unwrap();
            assert_eq!(values2, vec![b"device_data".to_vec()]);
        }
    }

    #[tokio::test]
    async fn file_dht_expired_records_not_loaded() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

        // Create DHT at base time and write data
        {
            let now = Arc::new(std::sync::Mutex::new(base));
            let now_clone = now.clone();
            let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
                Arc::new(move || *now_clone.lock().unwrap());

            let dht = FileDht::with_time(temp_dir.path().to_path_buf(), now_fn).unwrap();
            dht.put(&key, b"short_ttl".to_vec(), Duration::from_secs(5))
                .await
                .unwrap();
            dht.put(&key, b"long_ttl".to_vec(), Duration::from_secs(3600))
                .await
                .unwrap();
        }

        // Create new DHT instance at a later time
        {
            let later = base + Duration::from_secs(100);
            let now = Arc::new(std::sync::Mutex::new(later));
            let now_clone = now.clone();
            let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
                Arc::new(move || *now_clone.lock().unwrap());

            let dht = FileDht::with_time(temp_dir.path().to_path_buf(), now_fn).unwrap();
            let values = dht.get_all(&key).await.unwrap();

            // Only the long TTL value should remain
            assert_eq!(values.len(), 1);
            assert_eq!(values[0], b"long_ttl".to_vec());
        }
    }

    #[tokio::test]
    async fn file_dht_cleanup_expired() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let now = Arc::new(std::sync::Mutex::new(base));
        let now_clone = now.clone();
        let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
            Arc::new(move || *now_clone.lock().unwrap());

        let dht = FileDht::with_time(temp_dir.path().to_path_buf(), now_fn).unwrap();

        dht.put(&key, b"expires_soon".to_vec(), Duration::from_secs(5))
            .await
            .unwrap();
        dht.put(&key, b"expires_later".to_vec(), Duration::from_secs(3600))
            .await
            .unwrap();

        // Advance time
        *now.lock().unwrap() = base + Duration::from_secs(100);

        // Cleanup expired records
        let removed = dht.cleanup_expired().await.unwrap();
        assert_eq!(removed, 1);

        // Verify only non-expired value remains
        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], b"expires_later".to_vec());
    }

    #[tokio::test]
    async fn file_dht_cleanup_removes_empty_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let now = Arc::new(std::sync::Mutex::new(base));
        let now_clone = now.clone();
        let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
            Arc::new(move || *now_clone.lock().unwrap());

        let dht = FileDht::with_time(temp_dir.path().to_path_buf(), now_fn).unwrap();

        dht.put(&key, b"expires_soon".to_vec(), Duration::from_secs(5))
            .await
            .unwrap();

        // Verify file exists
        let hex_key = hex::encode(&key);
        let file_path = temp_dir.path().join(format!("{}.json", hex_key));
        assert!(file_path.exists());

        // Advance time past TTL
        *now.lock().unwrap() = base + Duration::from_secs(100);

        // Cleanup should remove the file
        dht.cleanup_expired().await.unwrap();
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn file_dht_get_removes_expired_from_disk() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let now = Arc::new(std::sync::Mutex::new(base));
        let now_clone = now.clone();
        let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
            Arc::new(move || *now_clone.lock().unwrap());

        let dht = FileDht::with_time(temp_dir.path().to_path_buf(), now_fn).unwrap();

        dht.put(&key, b"value".to_vec(), Duration::from_secs(5))
            .await
            .unwrap();

        // Advance time past TTL
        *now.lock().unwrap() = base + Duration::from_secs(100);

        // Get should return empty and remove file
        let values = dht.get_all(&key).await.unwrap();
        assert!(values.is_empty());

        // File should be deleted
        let hex_key = hex::encode(&key);
        let file_path = temp_dir.path().join(format!("{}.json", hex_key));
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn file_dht_concurrent_access() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp_dir = tempfile::tempdir().unwrap();
        let dht = Arc::new(FileDht::new(temp_dir.path().to_path_buf()).unwrap());
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];

        // Spawn multiple concurrent writers
        for i in 0..10 {
            let dht_clone = dht.clone();
            let key_clone = key.clone();
            let counter_clone = counter.clone();

            let handle = tokio::spawn(async move {
                let value = format!("value_{}", i);
                dht_clone
                    .put(&key_clone, value.as_bytes().to_vec(), Duration::from_secs(3600))
                    .await
                    .unwrap();
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });

            handles.push(handle);
        }

        // Wait for all writers
        for handle in handles {
            handle.await.unwrap();
        }

        // All writes should have succeeded
        assert_eq!(counter.load(Ordering::SeqCst), 10);

        // All values should be present
        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 10);
    }

    #[tokio::test]
    async fn file_dht_binary_data() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dht = FileDht::new(temp_dir.path().to_path_buf()).unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");

        // Binary data with null bytes and high bytes
        let binary_data: Vec<u8> = (0..=255).collect();

        dht.put(&key, binary_data.clone(), Duration::from_secs(3600))
            .await
            .unwrap();

        // Drop and reload
        drop(dht);
        let dht = FileDht::new(temp_dir.path().to_path_buf()).unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], binary_data);
    }

    #[tokio::test]
    async fn create_dht_memory() {
        let dht = create_dht(DhtConfig::Memory).unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");

        dht.put(&key, b"value".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values, vec![b"value".to_vec()]);
    }

    #[tokio::test]
    async fn create_dht_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dht = create_dht(DhtConfig::File {
            base_dir: temp_dir.path().to_path_buf(),
        })
        .unwrap();
        let key = dht_key_identity("b1anasr5h0bj3832xqexwy0f0987e1xb");

        dht.put(&key, b"value".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();

        let values = dht.get_all(&key).await.unwrap();
        assert_eq!(values, vec![b"value".to_vec()]);
    }
}

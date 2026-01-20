# Mailbox and DHT Systems Documentation

This document provides comprehensive developer documentation for the Post-Urbit mailbox system and distributed hash table (DHT) implementation.

## Table of Contents

- [Mailbox System](#mailbox-system)
  - [Purpose](#purpose)
  - [Storage Architecture](#storage-architecture)
  - [Bearer Tokens](#bearer-tokens)
  - [HTTP API](#http-api)
  - [Sender Filtering](#sender-filtering)
  - [Group Message Fanout](#group-message-fanout)
- [DHT System](#dht-system)
  - [Purpose](#dht-purpose)
  - [Key Types](#key-types)
  - [Storage Backends](#storage-backends)
  - [TTL and Expiry](#ttl-and-expiry)
  - [Operations](#operations)
- [Code Examples](#code-examples)

---

## Mailbox System

### Purpose

The mailbox system enables **asynchronous message delivery** when a recipient is offline. When a sender wants to send a message to a recipient who is not currently connected to the network, the message is stored in the recipient's mailbox on a mailbox server. When the recipient comes online, they can retrieve their pending messages.

Key design principles:
- Messages are stored as encrypted PUSE (Post-Urbit Secure Envelope) payloads
- Senders must authenticate and obtain bearer tokens to store messages
- Recipients authenticate to retrieve and delete their messages
- Messages have configurable retention periods

### Storage Architecture

Messages are organized by recipient IID (Identity Identifier) in an in-memory store.

**Core Data Structures** (from `src/mailbox_store.rs`, lines 11-23):

```rust
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub message_id: String,      // UUID from the PUSE envelope
    pub stored_at: String,       // ISO8601 timestamp
    pub sender_iid: String,      // Sender's identity
    pub size: u64,               // Envelope size in bytes
    pub envelope: Vec<u8>,       // Raw PUSE envelope
}

#[derive(Debug, Default)]
pub struct MailboxStore {
    // Maps recipient IID -> (message_id -> StoredMessage)
    messages: HashMap<String, BTreeMap<String, StoredMessage>>,
}
```

**Storage Constraints:**
- Maximum envelope size: 1 MB (1,048,576 bytes) - enforced at lines 39-41
- Messages are deduplicated by message_id (idempotent storage) - lines 69-71
- Sender IID in the token must match the sender IID in the PUSE envelope - lines 49-51

### Bearer Tokens

The mailbox system uses **two layers of authentication**:

1. **Identity Token**: Proves the sender's identity (signed by their signing key)
2. **Bearer Token**: Per-recipient HMAC token authorizing message storage

#### Bearer Token Protocol

Bearer tokens are HMAC-SHA256 based tokens that bind a sender to a specific recipient's mailbox. This prevents unauthorized message storage.

**Token Structure** (from `src/mailbox.rs`, lines 347-357):

```rust
pub struct MailboxBearerToken {
    pub sender_iid: String,      // Who is storing messages
    pub recipient_iid: String,   // Whose mailbox is being accessed
    pub expires_at: String,      // RFC3339 expiration timestamp
    pub token: String,           // Base64url-encoded HMAC value
}
```

**Token Generation** (lines 389-442):

The token is generated using:
```
HMAC-SHA256(secret, domain || recipient_iid || sender_iid || expires_at)
```

Where:
- `domain` = `"post-urbit:mailbox-token:v1:"` (line 16)
- `recipient_iid` and `sender_iid` are decoded to raw 20-byte values
- `expires_at` is an ISO8601 timestamp string

**Token Constraints:**
- Validity period: 1-24 hours (enforced at lines 410-415)
- Clock skew tolerance: 5 minutes (line 529)
- Maximum future validity: 24 hours (lines 533-535)

#### Token Generator Usage

```rust
use crate::mailbox::MailboxBearerTokenGenerator;

// Create generator with a 32-byte secret
let secret = [42u8; 32];  // Use cryptographically random bytes in production
let generator = MailboxBearerTokenGenerator::new(secret);

// Generate a token allowing sender to store messages in recipient's mailbox
let (token, expires_at) = generator.generate_token(
    &recipient_iid,  // Mailbox owner
    &sender_iid,     // Who will store messages
    24,              // Validity in hours (1-24)
)?;

// Verify a token
generator.verify_token(
    &token,
    &recipient_iid,
    &sender_iid,
    &expires_at,
)?;
```

### HTTP API

The mailbox HTTP server is implemented in `src/mailbox_http.rs`.

#### Configuration

```rust
pub struct MailboxHttpConfig {
    pub public_url: String,           // Canonical mailbox URL
    pub retention_days: i64,          // How long to keep messages
    pub bearer_token_secret: Option<[u8; 32]>,  // Secret for bearer tokens
}
```

#### Endpoints

##### POST /mailbox/token/{recipient_iid}

Request a bearer token to store messages in a recipient's mailbox.

**Request Headers:**
- `Authorization: Bearer <identity_token>` - Sender's signed identity token

**Request Body:**
```json
{
    "sender_iid": "b1n7cfscgashm32xx7eaxw0y09gy0y2v",
    "validity_hours": 24
}
```

**Response (200 OK):**
```json
{
    "token": "base64url_encoded_hmac",
    "expires_at": "2025-01-16T12:00:00Z",
    "recipient_iid": "a0b1c2d3e4f5g6h7j8k9m0n1p2q3r4s5",
    "sender_iid": "b1n7cfscgashm32xx7eaxw0y09gy0y2v"
}
```

**Error Responses:**
- `403 sender_mismatch`: sender_iid doesn't match authenticated identity
- `501 not_implemented`: Bearer token endpoint disabled

##### POST /messages/{inbox_owner_iid}

Store a message in a recipient's mailbox.

**Request Headers:**
- `Authorization: Bearer <identity_token>` - Sender's identity token
- `X-Mailbox-Bearer-Token: <token>:<expires_at>` - Bearer token for this recipient
- `Content-Type: application/octet-stream`

**Request Body:** Raw PUSE envelope bytes

**Response (201 Created):**
```json
{
    "message_id": "uuid-string",
    "stored_at": "2025-01-15T12:00:00Z",
    "expires_at": "2025-02-14T12:00:00Z"
}
```

**Error Responses:**
- `403 missing_bearer_token`: No bearer token provided
- `403 invalid_bearer_token`: Bearer token invalid or expired
- `403 sender_mismatch`: Envelope sender doesn't match token
- `413 too_large`: Envelope exceeds 1 MB

##### GET /messages

Retrieve messages from your mailbox.

**Request Headers:**
- `Authorization: Bearer <identity_token>` - Recipient's identity token

**Query Parameters:**
- `cursor`: Pagination cursor (optional)
- `limit`: Maximum messages to return (default 100, max 1000)

**Response (200 OK):**
```json
{
    "messages": [
        {
            "message_id": "uuid-string",
            "stored_at": "2025-01-15T12:00:00Z",
            "sender_iid": "b1n7cfscgashm32xx7eaxw0y09gy0y2v",
            "size": 1234,
            "envelope": "base64_encoded_puse_envelope"
        }
    ],
    "next_cursor": "base64_encoded_offset",
    "has_more": true
}
```

##### DELETE /messages

Delete messages from your mailbox.

**Request Headers:**
- `Authorization: Bearer <identity_token>` - Recipient's identity token
- `Content-Type: application/json`

**Request Body:**
```json
{
    "message_ids": ["uuid-1", "uuid-2"]
}
```

**Response (200 OK):**
```json
{
    "deleted": 2
}
```

### Sender Filtering

Recipients can filter messages by sender IID using the `retrieve_by_sender` method (lines 98-126 in `mailbox_store.rs`):

```rust
impl MailboxStore {
    /// Retrieve messages filtered by sender IID
    pub fn retrieve_by_sender(
        &self,
        inbox_owner_iid: &str,
        sender_iid: &str,
    ) -> Result<Vec<StoredMessage>> {
        // Returns only messages from the specified sender
    }

    /// Get all unique sender IIDs in this inbox
    pub fn get_senders(&self, inbox_owner_iid: &str) -> Result<Vec<String>> {
        // Returns sorted list of unique sender IIDs
    }
}
```

This allows recipients to:
1. List all senders who have messages waiting
2. Retrieve messages from specific senders only
3. Implement sender-based filtering/blocking at the application level

### Group Message Fanout

When sending a message to a group where some members are offline, the mailbox system supports **fan-out** to multiple recipients (lines 180-238 in `mailbox_store.rs`):

```rust
impl MailboxStore {
    /// Store a group message to multiple recipients' mailboxes
    pub fn store_group_message(
        &mut self,
        group_id: &str,           // For logging/tracking
        member_iids: &[String],   // Recipients to receive the message
        sender_iid: &str,         // Must match envelope sender
        envelope: &[u8],          // PUSE envelope with group_id as recipient
    ) -> Result<Vec<String>> {
        // Stores the same message in each member's inbox
        // Returns message IDs for each successful store
    }
}
```

**Fanout Behavior:**
- The same PUSE envelope is stored in each member's inbox
- Each member gets the same `message_id`
- Storage is idempotent - duplicate stores are ignored
- Sender verification is performed once for all recipients

---

## DHT System

### Purpose {#dht-purpose}

The Distributed Hash Table (DHT) provides **peer discovery and identity lookup** services. It stores:
- Identity documents (for looking up users by IID)
- Genesis keys (for identity verification)
- Device information (for multi-device support)
- Revocation records (for key compromise handling)

### Key Types

The DHT uses 32-byte SHA256 keys with domain-prefixed hashing to prevent key collisions.

**Key Generation Functions** (from `src/dht.rs`, lines 399-432):

| Function | Prefix | Purpose |
|----------|--------|---------|
| `dht_key_identity(iid)` | `post-urbit:identity:` | Identity document lookup |
| `dht_key_genesis(iid)` | `post-urbit:genesis:` | Genesis key verification |
| `dht_key_devices(iid)` | `post-urbit:devices-for:` | List of devices for an identity |
| `dht_key_device(did)` | `post-urbit:device:` | Individual device lookup |
| `dht_key_revocation(iid)` | `post-urbit:revocation:` | Identity revocation records |
| `dht_key_device_revocation(did)` | `post-urbit:device-revocation:` | Device revocation records |

**Key Generation Example:**

```rust
use crate::dht::{dht_key_identity, dht_key_device};

let iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
let identity_key = dht_key_identity(iid);  // [u8; 32]

let did = "42kbzq2tyab939amybd76bm8kfpzgn95";
let device_key = dht_key_device(did);  // [u8; 32]
```

**Key Validation** (lines 434-439):

```rust
pub fn validate_dht_key(key: &[u8]) -> Result<()> {
    if key.len() != 32 {
        return Err(PostUrbitError::InvalidInput("dht key length"));
    }
    Ok(())
}
```

### Storage Backends

The DHT supports two storage backends:

#### MemoryDht

In-memory storage suitable for testing and ephemeral nodes (lines 22-48):

```rust
pub struct MemoryDht {
    inner: Arc<Mutex<HashMap<Vec<u8>, Vec<StoredValue>>>>,
    now: Arc<dyn Fn() -> SystemTime + Send + Sync>,
}

impl MemoryDht {
    pub fn new() -> Self {
        // Uses SystemTime::now for expiry checking
    }

    pub fn with_time(now: Arc<dyn Fn() -> SystemTime + Send + Sync>) -> Self {
        // Allows injecting custom time function for testing
    }
}
```

**Characteristics:**
- Data is lost on process restart
- Fast access (no I/O)
- Good for tests and short-lived nodes

#### FileDht

File-based persistent storage (lines 100-347):

```rust
pub struct FileDht {
    base_dir: PathBuf,
    cache: Arc<Mutex<HashMap<Vec<u8>, DhtRecord>>>,
    now: Arc<dyn Fn() -> SystemTime + Send + Sync>,
}

impl FileDht {
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        // Creates directory if needed, loads existing records
    }

    pub fn with_time(base_dir: PathBuf, now: Arc<...>) -> Result<Self> {
        // For testing with custom time
    }
}
```

**File Structure:**
- One JSON file per key: `{hex_encoded_key}.json`
- File locking for concurrent access (shared for reads, exclusive for writes)
- Records include TTL and creation timestamp

**Record Format** (lines 51-75):

```rust
#[derive(Serialize, Deserialize)]
struct DhtRecord {
    values: Vec<DhtValue>,  // Multiple values per key supported
}

#[derive(Serialize, Deserialize)]
struct DhtValue {
    value: Vec<u8>,      // Base64-encoded in JSON
    ttl_secs: u64,       // 0 means no expiration
    created_at: u64,     // Unix timestamp
}
```

#### Factory Function

```rust
pub enum DhtConfig {
    Memory,
    File { base_dir: PathBuf },
}

pub fn create_dht(config: DhtConfig) -> Result<Box<dyn Dht + Send + Sync>> {
    match config {
        DhtConfig::Memory => Ok(Box::new(MemoryDht::new())),
        DhtConfig::File { base_dir } => Ok(Box::new(FileDht::new(base_dir)?)),
    }
}
```

### TTL and Expiry

Both DHT backends support Time-To-Live (TTL) for automatic expiration.

**TTL Behavior:**
- TTL of 0 means the value never expires (lines 69-74)
- Expired values are filtered out on read
- `FileDht` removes expired entries from disk during cleanup

**Expiry Check** (line 69-74):

```rust
impl DhtValue {
    fn is_expired(&self, now_unix: u64) -> bool {
        if self.ttl_secs == 0 {
            return false;  // Never expires
        }
        now_unix > self.created_at + self.ttl_secs
    }
}
```

**Cleanup Methods:**

For `FileDht`, explicit cleanup is available (lines 252-275):

```rust
impl FileDht {
    /// Remove expired records from cache and disk
    pub async fn cleanup_expired(&self) -> Result<usize> {
        // Returns number of values removed
    }
}
```

**Refresh Strategy:**
- Re-put a value before it expires to extend its TTL
- The DHT deduplicates identical values, so re-putting updates the TTL without creating duplicates

### Operations

The DHT trait defines two core operations (lines 16-20):

```rust
#[async_trait]
pub trait Dht: Send + Sync {
    /// Store a value with optional TTL
    async fn put(&self, key: &[u8], value: Vec<u8>, ttl: Duration) -> Result<()>;

    /// Retrieve all non-expired values for a key
    async fn get_all(&self, key: &[u8]) -> Result<Vec<Vec<u8>>>;
}
```

**Put Operation:**
- Stores a value under a key with an expiration time
- Deduplicates: if the exact value already exists, no action is taken
- Multiple different values can be stored under the same key

**Get All Operation:**
- Returns all non-expired values for a key
- Filters out expired values automatically
- For `FileDht`, expired values are removed from disk during read

---

## Code Examples

### Complete Mailbox Flow

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{Duration, Utc};

use post_urbit_core::mailbox::{
    create_mailbox_token,
    MailboxBearerTokenGenerator,
};
use post_urbit_core::mailbox_store::MailboxStore;
use post_urbit_core::mailbox_http::{MailboxHttpConfig, MailboxHttpServer};
use post_urbit_core::dht::MemoryDht;

// 1. Setup the mailbox server
let dht = Arc::new(MemoryDht::new());
let store = Arc::new(Mutex::new(MailboxStore::new()));
let config = MailboxHttpConfig {
    public_url: "https://mailbox.example.com/".to_string(),
    retention_days: 30,
    bearer_token_secret: Some([42u8; 32]),  // Use random bytes!
};
let server = Arc::new(MailboxHttpServer::new(config, dht, store.clone()));

// 2. Sender creates identity token for authentication
let expires_at = Utc::now() + Duration::hours(2);
let identity_token = create_mailbox_token(
    &sender_iid,
    "https://mailbox.example.com/",
    expires_at,
    [0u8; 16],  // Random nonce
    &sender_signing_key,
)?;

// 3. Sender requests bearer token for recipient's mailbox
let generator = MailboxBearerTokenGenerator::new([42u8; 32]);
let (bearer_token, bearer_expires) = generator.generate_token(
    &recipient_iid,
    &sender_iid,
    24,  // 24 hours
)?;

// 4. Sender stores message using both tokens
// HTTP: POST /messages/{recipient_iid}
// Headers:
//   Authorization: Bearer {identity_token}
//   X-Mailbox-Bearer-Token: {bearer_token}:{bearer_expires}
// Body: PUSE envelope bytes

// 5. Recipient retrieves messages
// HTTP: GET /messages
// Headers:
//   Authorization: Bearer {recipient_identity_token}

// 6. Recipient deletes processed messages
// HTTP: DELETE /messages
// Headers:
//   Authorization: Bearer {recipient_identity_token}
// Body: { "message_ids": ["uuid-1", "uuid-2"] }
```

### Complete DHT Flow

```rust
use std::path::PathBuf;
use std::time::Duration;

use post_urbit_core::dht::{
    create_dht, DhtConfig, Dht,
    dht_key_identity, dht_key_device,
};

// 1. Create a file-based DHT for persistence
let dht = create_dht(DhtConfig::File {
    base_dir: PathBuf::from("/var/lib/post-urbit/dht"),
})?;

// 2. Store an identity document
let iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
let identity_key = dht_key_identity(iid);
let identity_doc_bytes = serde_json::to_vec(&identity_document)?;

dht.put(
    &identity_key,
    identity_doc_bytes,
    Duration::from_secs(86400),  // 24 hour TTL
).await?;

// 3. Store device information
let did = "42kbzq2tyab939amybd76bm8kfpzgn95";
let device_key = dht_key_device(did);
let device_info_bytes = serde_json::to_vec(&device_info)?;

dht.put(
    &device_key,
    device_info_bytes,
    Duration::from_secs(3600),  // 1 hour TTL
).await?;

// 4. Retrieve identity
let values = dht.get_all(&identity_key).await?;
if let Some(bytes) = values.first() {
    let doc: IdentityDocument = serde_json::from_slice(bytes)?;
    // Use the identity document
}

// 5. For FileDht, periodically clean up expired records
if let Some(file_dht) = dht.as_any().downcast_ref::<FileDht>() {
    let removed = file_dht.cleanup_expired().await?;
    println!("Removed {} expired values", removed);
}
```

### Group Message Fanout Example

```rust
use post_urbit_core::mailbox_store::MailboxStore;

let mut store = MailboxStore::new();

// Group members who are offline
let offline_members = vec![
    "member1_iid".to_string(),
    "member2_iid".to_string(),
    "member3_iid".to_string(),
];

// Fan out the group message to all offline members
let message_ids = store.store_group_message(
    "group_12345",        // Group identifier
    &offline_members,     // Recipients
    &sender_iid,          // Sender (must match envelope)
    &puse_envelope,       // Encrypted group message
)?;

// Each member now has the message in their inbox
for member_iid in &offline_members {
    let messages = store.retrieve(member_iid)?;
    assert!(!messages.is_empty());
}
```

---

## Key Source File References

| File | Key Functions/Structs | Lines |
|------|----------------------|-------|
| `src/mailbox.rs` | `MailboxToken`, `MailboxBearerToken` | 20-27, 347-357 |
| `src/mailbox.rs` | `create_mailbox_token`, `verify_mailbox_token` | 29-91 |
| `src/mailbox.rs` | `MailboxBearerTokenGenerator` | 364-510 |
| `src/mailbox.rs` | `canonicalize_mailbox_url` | 236-306 |
| `src/mailbox_store.rs` | `MailboxStore`, `StoredMessage` | 11-23 |
| `src/mailbox_store.rs` | `store`, `retrieve`, `delete` | 30-165 |
| `src/mailbox_store.rs` | `retrieve_by_sender`, `get_senders` | 98-151 |
| `src/mailbox_store.rs` | `store_group_message` | 180-238 |
| `src/mailbox_http.rs` | `MailboxHttpServer`, `MailboxHttpConfig` | 23-61 |
| `src/mailbox_http.rs` | `handle_token_request` | 107-181 |
| `src/mailbox_http.rs` | `handle_store` | 183-284 |
| `src/mailbox_http.rs` | `handle_retrieve`, `handle_delete` | 286-368 |
| `src/dht.rs` | `Dht` trait | 16-20 |
| `src/dht.rs` | `MemoryDht` | 22-48, 366-397 |
| `src/dht.rs` | `FileDht` | 100-347 |
| `src/dht.rs` | Key generation functions | 399-432 |
| `src/dht.rs` | `create_dht`, `DhtConfig` | 349-363 |

# Sync System and WASM Runtime

This document provides comprehensive developer documentation for the Post-Urbit sync system and WASM runtime, covering CRDT-based synchronization, the WebAssembly sandbox, package format, capabilities, and app lifecycle.

## Table of Contents

1. [Sync System](#sync-system)
   - [CRDT Types](#crdt-types)
   - [Sync Protocol](#sync-protocol)
   - [Conflict Resolution](#conflict-resolution)
   - [Subscriptions](#subscriptions)
2. [WASM Runtime](#wasm-runtime)
   - [Manifest Format](#manifest-format)
   - [Package Format](#package-format)
   - [Memory Model](#memory-model)
   - [Capabilities](#capabilities)
   - [App Lifecycle](#app-lifecycle)
   - [Security Model](#security-model)
   - [Repositories](#repositories)

---

## Sync System

The sync system provides CRDT-based document synchronization across devices and nodes. It uses a Merkle tree structure for efficient diff detection and an operation-based protocol for exchanging changes.

**Source**: `/Users/dennisonbertram/Develop/apps/post-urbit/post-urbit-core/src/sync.rs`

### CRDT Types

#### OR-Set (Observed-Remove Set)

The primary CRDT type supported is the **OR-Set** (Observed-Remove Set), implemented at lines 487-675:

```rust
pub struct ORSet<T: Eq + Hash + Clone> {
    adds: HashMap<T, HashSet<u64>>,
    removes: HashMap<T, HashSet<u64>>,
}
```

**Properties:**
- Elements can be added multiple times with unique tags
- Removals only affect specific add-tags, not all instances
- Concurrent adds and removes are handled correctly
- Merge is commutative, associative, and idempotent

**Operations:**

| Operation | Description |
|-----------|-------------|
| `add(value, tag)` | Add a value with a unique tag |
| `remove(value, tags)` | Remove specific tagged versions |
| `merge(other)` | Merge another OR-Set into this one |
| `values()` | Get all currently present values |

**Example Usage:**

```rust
let mut set_a = ORSet::new();
set_a.add("hello", 1);  // Add with tag 1
set_a.add("hello", 2);  // Add same value with tag 2
set_a.remove(&"hello", &[1]);  // Remove only tag 1

let mut set_b = ORSet::new();
set_b.add("hello", 3);  // Add on another device

set_a.merge(&set_b);  // Tags 2 and 3 remain
assert!(set_a.values().contains("hello"));  // Still present
```

#### Sync Operation Records

Operations are stored as signed records (lines 76-86):

```rust
pub struct SyncOperationRecord {
    pub id: [u8; 32],           // SHA-256 hash of operation
    pub origin: [u8; 20],       // Device/node origin identifier
    pub document_id: [u8; 32],  // Document this operation belongs to
    pub physical_ms: u64,       // Wall clock timestamp (ms since epoch)
    pub logical: u32,           // Lamport logical clock
    pub operation: Vec<u8>,     // CBOR-encoded operation payload
    pub dependencies: Vec<[u8; 32]>,  // Operation IDs this depends on
    pub signature: Vec<u8>,     // Ed25519 signature (64 bytes)
}
```

### Sync Protocol

The sync protocol uses a request-offer-accept-operations-ack flow to efficiently synchronize documents.

#### Message Types

Defined at lines 65-74:

| Type | Code | Description |
|------|------|-------------|
| `Request` | 0x01 | Initiate sync with merkle root |
| `Offer` | 0x02 | List of operation IDs we have |
| `Accept` | 0x03 | List of operation IDs we want |
| `Operations` | 0x04 | Actual operation payloads |
| `Ack` | 0x05 | Acknowledge received operations |
| `Subscribe` | 0x06 | Subscribe to document changes |
| `Unsubscribe` | 0x07 | Unsubscribe from document |
| `Error` | 0x08 | Error response |

#### Protocol Flow

```
    Device A                          Device B
       |                                  |
       |  1. Request (merkle_root)        |
       |--------------------------------->|
       |                                  |
       |  2. Offer (operation_ids)        |
       |<---------------------------------|
       |                                  |
       |  3. Accept (wanted_ids)          |
       |--------------------------------->|
       |                                  |
       |  4. Operations (payloads)        |
       |<---------------------------------|
       |                                  |
       |  5. Ack (operation_ids)          |
       |--------------------------------->|
       |                                  |
```

#### SyncRequest

```rust
pub struct SyncRequest {
    pub document_id: Vec<u8>,   // 32-byte document identifier
    pub merkle_root: Vec<u8>,   // Current merkle root hash
    pub depth: u64,             // Reserved for partial sync
}
```

#### SyncSession

The `SyncSession` (lines 535-603) manages the state machine for a sync exchange:

```rust
let session = SyncSession::new(document_id, store);

// Initiate sync
let request = session.request();

// Handle incoming request
let offer = session.handle_request(&request)?;

// Handle offer, determine what we need
let accept = session.handle_offer(&offer)?;

// Handle accept, send requested operations
let operations = session.handle_accept(&accept)?;

// Handle incoming operations
let ack = session.handle_operations(&operations, &signing_keys)?;
```

### Conflict Resolution

Conflicts are resolved through deterministic ordering of operations.

#### Ordering Algorithm

Operations are sorted by (lines 195-213):
1. `physical_ms` - Wall clock time (ascending)
2. `logical` - Lamport clock (ascending)
3. `origin` - Node identifier (lexicographic)
4. `id` - Operation hash (lexicographic)

```rust
pub fn ordered_operations(records: &[SyncOperationRecord]) -> Vec<SyncOperationRecord> {
    let mut ops = records.to_vec();
    ops.sort_by(|a, b| {
        a.physical_ms.cmp(&b.physical_ms)
            .then(a.logical.cmp(&b.logical))
            .then(a.origin.cmp(&b.origin))
            .then(a.id.cmp(&b.id))
    });
    ops
}
```

#### Merkle Tree Structure

The sync system uses a binary Merkle tree for efficient diff detection (lines 146-193):

- **Leaf prefix**: `post-urbit:merkle-leaf:`
- **Node prefix**: `post-urbit:merkle-node:`
- **Empty prefix**: `post-urbit:merkle-empty:`

```rust
// Compute merkle root from operations
pub fn merkle_root(records: &[SyncOperationRecord]) -> [u8; 32] {
    if records.is_empty() {
        return merkle_empty_hash();
    }
    // Order operations, hash leaves, build tree
    // ...
}
```

#### Operation Signing

All operations must be signed by an authorized key (lines 265-286):

```rust
pub fn sign_sync_operation(
    document_id: &[u8; 32],
    origin: &[u8; 20],
    physical_ms: u64,
    logical: u32,
    operation_bytes: &[u8],
    dependencies: &[[u8; 32]],
    signing_key: &SigningKey,
) -> ([u8; 32], [u8; 64])
```

Signature format: `post-urbit:sync-op:v1:{op_id}{document_id}{timestamp}{operation}{deps}`

### Subscriptions

#### Document Subscriptions

Apps can subscribe to document changes through the WASM runtime:

```rust
// Subscribe message
pub struct SyncSubscribe {
    pub document_id: Vec<u8>,
}

// Unsubscribe message
pub struct SyncUnsubscribe {
    pub document_id: Vec<u8>,
}
```

#### Replication Filtering

The `ReplicationFilter` (lines 493-619) controls which datasets are replicated:

```rust
pub struct ReplicationFilter {
    allowlist: Option<HashSet<String>>,  // If set, only these datasets
    denylist: HashSet<String>,           // Always exclude these
}

impl ReplicationFilter {
    pub fn allows(&self, dataset: &str) -> bool {
        if self.denylist.contains(dataset) {
            return false;
        }
        match &self.allowlist {
            Some(list) => list.contains(dataset),
            None => true,  // No allowlist = allow all
        }
    }
}
```

#### Sync State Machine

The `SyncStateMachine` (lines 499-533) tracks convergence:

```rust
let mut sm = SyncStateMachine::new(local_root);

// Check if we need to sync
if sm.request_diff(&remote_root) {
    // Perform sync...
}

sm.apply_remote_root(new_remote_root);
sm.apply_local_root(new_local_root);

if sm.converged() {
    // Both sides are in sync
}
```

---

## WASM Runtime

The WASM runtime provides a sandboxed execution environment for Post-Urbit apps. It uses wasmtime as the underlying WebAssembly engine with fuel metering for resource control.

**Source**: `/Users/dennisonbertram/Develop/apps/post-urbit/post-urbit-core/src/runtime.rs` (manifest/signing)
**Source**: `/Users/dennisonbertram/Develop/apps/post-urbit/post-urbit-core/src/runtime_wasm.rs` (WASM execution)
**Source**: `/Users/dennisonbertram/Develop/apps/post-urbit/post-urbit-core/src/app_store.rs` (packages/repositories)

### Manifest Format

The manifest.json file describes an app's metadata, requirements, and capabilities.

**Source**: `runtime.rs` lines 13-80

#### Complete Manifest Specification

```json
{
  "manifest_version": 1,
  "app": {
    "id": "com.example.myapp",
    "name": "My App",
    "version": "1.0.0",
    "description": "A sample application",
    "author": {
      "name": "Developer Name",
      "iid": "iid:abc123...",
      "url": "https://example.com"
    },
    "license": "MIT",
    "homepage": "https://github.com/example/myapp",
    "repository": "https://github.com/example/myapp"
  },
  "runtime": {
    "entry": "main.wasm",
    "memory": {
      "initial_pages": 16,
      "maximum_pages": 256
    },
    "fuel": {
      "user_action": 1000000,
      "background_task": 100000,
      "app_start": 500000
    }
  },
  "capabilities": {
    "required": ["storage:app"],
    "optional": ["messaging:send", "contacts:read"],
    "reasons": {
      "storage:app": "Store your data locally",
      "messaging:send": "Send messages to contacts",
      "contacts:read": "Access your contact list"
    }
  },
  "dependencies": {
    "node_version": ">=0.1.0",
    "api_version": "1",
    "apps": {
      "com.example.dependency": ">=1.0.0"
    }
  },
  "files": {
    "hashes": {
      "main.wasm": "sha256:abc123...",
      "ui/index.html": "sha256:def456..."
    },
    "total_size": 102400
  }
}
```

#### Field Reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `manifest_version` | u8 | Yes | Must be `1` |
| `app.id` | string | Yes | Reverse-domain ID (e.g., `com.example.app`), max 64 chars |
| `app.name` | string | Yes | Display name, non-empty |
| `app.version` | string | Yes | Semantic version (X.Y.Z or X.Y.Z-prerelease) |
| `app.description` | string | Yes | App description, non-empty |
| `app.author.name` | string | Yes | Author name, non-empty |
| `app.author.iid` | string | No | Author's Post-Urbit IID |
| `app.author.url` | string | No | Author website |
| `app.license` | string | Yes | License identifier, non-empty |
| `app.homepage` | string | No | App homepage URL |
| `app.repository` | string | No | Source repository URL |
| `runtime.entry` | string | Yes | Entry WASM file, must end in `.wasm` |
| `runtime.memory.initial_pages` | u32 | No | Initial memory (pages = 64KB each) |
| `runtime.memory.maximum_pages` | u32 | No | Maximum memory pages |
| `runtime.fuel.user_action` | u64 | No | Fuel for user-initiated calls |
| `runtime.fuel.background_task` | u64 | No | Fuel for background tasks |
| `runtime.fuel.app_start` | u64 | No | Fuel for initialization |
| `capabilities.required` | [string] | Yes | Capabilities that must be granted |
| `capabilities.optional` | [string] | No | Capabilities user may grant |
| `capabilities.reasons` | {string: string} | No | Human-readable explanations |
| `dependencies.api_version` | string | Yes | Must be `"1"` |
| `dependencies.node_version` | string | No | Semver constraint |
| `dependencies.apps` | {string: string} | No | Required apps with version constraints |
| `files.hashes` | {string: string} | Yes | SHA-256 hashes for all files |
| `files.total_size` | u64 | Yes | Total package size in bytes |

#### App ID Format

App IDs must follow these rules (lines 290-313):
- Maximum 64 characters
- At least two dot-separated segments
- Each segment starts with lowercase letter
- Segments contain only lowercase letters and digits

Valid: `com.example.app`, `org.myorg.tool123`
Invalid: `App`, `com.Example.app`, `com.123app`

#### Version Format

Versions must be semantic (lines 315-328):
- Three numeric segments: `MAJOR.MINOR.PATCH`
- Optional prerelease suffix: `-alpha`, `-beta.1`, etc.

Valid: `1.0.0`, `2.3.4`, `1.0.0-alpha`
Invalid: `1.0`, `v1.0.0`, `1.0.0.0`

### Package Format

A `.postapp` file is a ZIP archive containing the manifest, signature, and app files.

**Source**: `app_store.rs` lines 77-152

#### Structure

```
myapp.postapp (ZIP)
├── manifest.json         # Required: App manifest
├── SIGNATURE             # Required: JSON signature file
├── main.wasm             # Required: Entry WASM binary
├── assets/               # Optional: Static assets
│   └── icon.png
└── ui/                   # Optional: Web UI
    ├── index.html
    ├── app.js
    └── style.css
```

#### Size Limits

Defined at lines 20-24:

| Component | Limit |
|-----------|-------|
| Total package | 100 MB |
| manifest.json | 64 KB |
| main.wasm | 50 MB |
| Single asset | 10 MB |
| UI total | 20 MB |

#### Signature File

The SIGNATURE file contains a JSON `PackageSignature` (lines 82-88):

```json
{
  "author_iid": "iid:abc123...",
  "timestamp": "2025-01-15T12:00:00Z",
  "signature": "base64-encoded-signature",
  "signed_manifest_hash": "sha256:..."
}
```

#### Signing Requirements

Packages must be signed by the author's identity key:

1. Compute SHA-256 of canonical JSON manifest
2. Create payload: `postapp-signature-v1:{manifest_hash_hex}:{timestamp}`
3. Sign with Ed25519 signing key
4. Verify signature matches author's IID

**Source**: `runtime.rs` lines 149-204

```rust
// Sign a package
let signature = sign_package_signature(
    &manifest,
    "iid:author",
    "2025-01-15T12:00:00Z",
    &signing_key,
)?;

// Verify signature
verify_package_signature(&manifest, &signature, &author_identity)?;
```

#### Path Traversal Protection

The package parser validates paths to prevent directory traversal attacks (lines 98-113):

- Rejects absolute paths
- Rejects `..` (parent directory) components
- Only allows `./` and normal path components

### Memory Model

The WASM runtime uses wasmtime with configurable memory limits and fuel metering.

**Source**: `runtime_wasm.rs` lines 15-18, 206-208

#### Memory Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| Initial pages | 16 | Starting memory (1 MB) |
| Maximum pages | 256 | Maximum memory (16 MB) |
| Page size | 64 KB | WebAssembly standard |

Memory is configured via manifest:

```json
{
  "runtime": {
    "memory": {
      "initial_pages": 16,
      "maximum_pages": 256
    }
  }
}
```

#### Fuel Metering

Fuel prevents infinite loops and DoS attacks by limiting computation:

```rust
const WASM_START_FUEL: u64 = 10_000_000;   // For _start initialization
const WASM_HANDLE_FUEL: u64 = 100_000_000; // For handle calls
```

If fuel is exhausted, execution terminates with error:
> `wasm start: fuel exhausted (possible infinite loop)`

Configurable via manifest:

```json
{
  "runtime": {
    "fuel": {
      "user_action": 1000000,
      "background_task": 100000,
      "app_start": 500000
    }
  }
}
```

#### Memory Access

Host functions use safe memory access helpers (lines 1312-1349):

```rust
// Read from WASM memory
fn read_memory(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Result<Vec<u8>>

// Write to WASM memory
fn write_memory(caller: &mut Caller<'_, HostState>, ptr: i32, bytes: &[u8]) -> Result<()>
```

Bounds checking prevents buffer overflows.

### Capabilities

Capabilities are the permission system for Post-Urbit apps. Each host API method requires specific capabilities.

**Source**: `runtime_wasm.rs` lines 348-370

#### Capability Registry

The default registry maps methods to required capabilities:

| Method | Required Capability |
|--------|---------------------|
| `storage.get` | `storage:app` |
| `storage.set` | `storage:app` |
| `storage.delete` | `storage:app` |
| `storage.list` | `storage:app` |
| `messaging.send` | `messaging:send` |
| `messaging.send_group` | `messaging:send` + `messaging:group` |
| `messaging.subscribe` | `messaging:subscribe` |
| `messaging.create_group` | `messaging:group` |
| `contacts.list` | `contacts:read` |
| `contacts.list_app_users` | `contacts:read:limited` |
| `sync.create_document` | `sync:documents` |
| `sync.apply_operation` | `sync:documents` |
| `notifications.show` | `notifications:show` |
| `notifications.set_badge` | `notifications:badge` |
| `notifications.show` (with sound) | `notifications:sound` |
| `system.get_time` | `system:time` |
| `system.get_random` | `system:random` |
| `system.get_deterministic_random` | *(no capability)* |
| `system.get_identity` | `system:identity:read` |
| `system.get_app_info` | *(no capability)* |
| `app.invoke` | `app:invoke:any` or `app:invoke:{target_app}` |

#### Capability Enforcement

Capabilities are checked before each host call (lines 446-452):

```rust
if let Some(capability) = registry.capability_for(method) {
    if !capability.is_empty() && !state.capabilities.iter().any(|c| c == capability) {
        return ResultEnvelope::error("PERMISSION_DENIED", "Capability denied");
    }
}
```

#### App-to-App Invocation

Cross-app calls require specific capabilities (lines 1206-1209):

```rust
fn has_app_invoke_capability(capabilities: &[String], target_app: &str) -> bool {
    let target = format!("app:invoke:{target_app}");
    capabilities.iter().any(|cap| cap == "app:invoke:any" || cap == &target)
}
```

Call depth is limited to 8 levels to prevent infinite recursion.

### App Lifecycle

#### Installation

```rust
// RuntimeManager installation flow
pub fn install_with_metadata(
    &mut self,
    app_id: &str,
    wasm: Vec<u8>,
    version: &str,
    capabilities: Vec<String>,
) -> Result<()>
```

Steps (lines 223-256):
1. Validate WASM is non-empty
2. Compile WASM module with wasmtime
3. Validate required exports exist
4. Store app metadata
5. Add to installed apps set

#### Required Exports

WASM modules must export these functions (lines 1351-1418):

| Export | Signature | Description |
|--------|-----------|-------------|
| `memory` | Memory | Linear memory for data exchange |
| `_start` | `() -> ()` | Initialization function |
| `handle` | `(i32, i32) -> i64` | Handle incoming calls |
| `get_error` | `() -> i64` | Get last error |
| `alloc` | `(i32) -> i32` | Allocate memory |
| `dealloc` | `(i32, i32) -> ()` | Free memory |

#### Starting an App

```rust
pub fn start(&mut self, app_id: &str) -> Result<()>
```

Steps (lines 269-317):
1. Look up installed app
2. Create wasmtime linker with host imports
3. Create store with host state
4. Instantiate module
5. Set fuel limit for `_start`
6. Call `_start` function
7. Refuel for subsequent calls
8. Mark app as running

#### Host Imports

Apps communicate with the host via imported functions (lines 372-389):

| Import | Signature | Description |
|--------|-----------|-------------|
| `host.call` | `(i32, i32, i32, i32) -> i32` | Make a host API call |
| `host.get_result` | `(i32, i32, i32) -> i32` | Get call result |
| `host.get_result_len` | `(i32) -> i32` | Get result length |
| `host.poll` | `(i32) -> i32` | Poll for completed calls |
| `host.log` | `(i32, i32, i32) -> ()` | Log a message |

#### Stopping an App

```rust
pub fn stop(&mut self, app_id: &str) -> Result<()>
```

Steps (lines 319-327):
1. Mark app as not running
2. Drop instance (frees memory)

#### Uninstallation

```rust
pub fn uninstall(&mut self, app_id: &str) -> Result<()>
```

Steps (lines 329-337):
1. Remove from apps map
2. Remove from installed apps set
3. (Storage cleanup is handled separately)

### Security Model

#### Sandbox Isolation

1. **No filesystem access**: Apps cannot read/write files directly
2. **No network access**: Apps cannot make network calls directly
3. **No system calls**: Only host-provided APIs are available
4. **Memory isolation**: Each app has separate linear memory
5. **Fuel metering**: Prevents infinite loops and DoS

#### Capability-Based Security

- Apps declare required and optional capabilities
- Users grant capabilities at install time
- Each API call checks for required capability
- Denied calls return `PERMISSION_DENIED` error

#### Signed Packages

All packages must be cryptographically signed:
1. Author signs manifest hash with Ed25519 key
2. Signature includes timestamp
3. Verification checks signature against author's identity
4. Revoked keys/identities are rejected

**Source**: `runtime.rs` lines 166-204

#### Revocation Support

Packages can be invalidated after signing (lines 257-282):

- **Identity revocation**: All packages by that author are invalid
- **Key revocation**: Packages signed with that key are invalid
- Revocation is time-based: only affects packages signed after effective date

#### Storage Isolation

Each app's storage is namespaced by app ID:

```rust
// Storage is keyed by app_id
let value = map.get(&state.app_id).and_then(|ns| ns.get(&key));
```

Apps cannot access other apps' storage.

#### Rate Limiting

- Pending host calls limited to 16 per app
- Storage keys limited to 256 characters
- Storage values limited to 1 MB
- Message content limited to 1 MB
- Notification title limited to 100 characters
- Notification body limited to 500 characters

### Repositories

App repositories allow distribution and discovery of Post-Urbit apps.

**Source**: `app_store.rs` lines 33-75, 222-282

#### Repository Manifest

```json
{
  "repository": {
    "name": "Example Repository",
    "id": "com.example.repo",
    "operator_iid": "iid:operator...",
    "url": "https://apps.example.com",
    "description": "A collection of Post-Urbit apps",
    "policies": {}
  },
  "apps": [
    {
      "id": "com.example.notes",
      "name": "Notes",
      "author_iid": "iid:author...",
      "latest_version": "1.2.0",
      "download_url": "https://apps.example.com/notes-1.2.0.postapp",
      "listing": {},
      "versions": [
        {
          "version": "1.2.0",
          "download_url": "https://apps.example.com/notes-1.2.0.postapp",
          "size": 512000,
          "released_at": "2025-01-15T00:00:00Z",
          "changelog": "Added sync support"
        }
      ]
    }
  ],
  "signature": {
    "operator_iid": "iid:operator...",
    "timestamp": "2025-01-15T12:00:00Z",
    "sig": "base64-signature"
  }
}
```

#### Repository Signing

Repository manifests are signed by the operator (lines 236-282):

1. Compute canonical JSON (excluding signature field)
2. Hash with SHA-256
3. Create payload: `postnode-repo-v1:{hash_hex}:{timestamp}`
4. Sign with operator's Ed25519 key

Verification includes:
- Signature validation against operator identity
- Timestamp not in future (5 min skew allowed)
- Check for identity/key revocations

#### Fetching from Repository

```rust
// Fetch repository manifest
let manifest = fetch_repository("https://apps.example.com/repo.json").await?;

// Verify signature
let verified_key = verify_repository(&dht, &manifest).await?;

// Download and verify package
let package = parse_postapp(&bytes)?;
verify_package_with_dht(&dht, &package).await?;
```

#### Installing from Repository

Full flow (lines 154-165, 213-220):

1. Fetch repository manifest
2. Verify repository signature
3. Find app in repository
4. Download `.postapp` file
5. Parse package
6. Verify package signature
7. Extract to apps directory
8. Load into runtime

---

## Host API Reference

### Storage API

```
storage.get     { key: string } -> { value: bytes | null, version: u64 }
storage.set     { key: string, value: bytes, expected_version?: u64 } -> { version: u64 }
storage.delete  { key: string } -> { deleted: bool }
storage.list    { prefix?: string, cursor?: string, limit?: u64 } -> { keys: [string], cursor: string | null, has_more: bool }
```

### Messaging API

```
messaging.send          { recipient: string, message_type: string, content: bytes } -> { message_id: string, sent_at: string }
messaging.send_group    { group_id: string, message_type: string, content: bytes } -> { message_id: string, sent_at: string }
messaging.subscribe     { filter?: { message_types?: [string], senders?: [string], groups?: [string] } } -> { subscription_id: string }
messaging.create_group  { name: string, members?: [string] } -> { group_id: string, created_at: string }
```

### Contacts API

```
contacts.list           { cursor?: string, limit?: u64 } -> { contacts: [...], cursor: string | null, has_more: bool }
contacts.list_app_users {} -> { contacts: [...] }
```

### Sync API

```
sync.create_document   { document_type: string, access: { owner: string, readers: [string], writers: [string] } } -> { document_id: string, created_at: string }
sync.apply_operation   { document_id: string, operation: bytes } -> { operation_id: string, applied_at: string }
```

### Notifications API

```
notifications.show      { title: string, body: string, id?: string, icon?: string, sound?: bool } -> { notification_id: string }
notifications.set_badge { count: u64 } -> {}
```

### System API

```
system.get_time                   {} -> { timestamp: string, monotonic_ns: u64 }
system.get_random                 { length: u64 } -> { bytes: bytes }
system.get_deterministic_random   { length: u64, seed?: bytes } -> { bytes: bytes }
system.get_identity               {} -> { iid: string }
system.get_app_info               {} -> { app_id: string, version: string, installed_at: string, storage_used: u64, capabilities_granted: [string] }
```

### App-to-App API

```
app.invoke { target_app: string, method: string, args: bytes } -> { result: bytes }
```

---

## Error Codes

| Code | Description |
|------|-------------|
| `PERMISSION_DENIED` | Missing required capability |
| `NOT_IMPLEMENTED` | Method not supported |
| `NOT_AVAILABLE` | Resource temporarily unavailable |
| `INVALID_REQUEST` | Malformed request |
| `KEY_TOO_LONG` | Storage key exceeds 256 chars |
| `VALUE_TOO_LARGE` | Storage value exceeds 1 MB |
| `VERSION_MISMATCH` | Optimistic locking conflict |
| `MESSAGE_TOO_LARGE` | Message exceeds 1 MB |
| `INVALID_MESSAGE_TYPE` | Empty or invalid message type |
| `NAME_TOO_LONG` | Group name exceeds 100 chars |
| `TITLE_TOO_LONG` | Notification title exceeds 100 chars |
| `BODY_TOO_LONG` | Notification body exceeds 500 chars |
| `DOCUMENT_NOT_FOUND` | Sync document doesn't exist |
| `ACCESS_DENIED` | No write access to document |
| `APP_NOT_FOUND` | Target app not found |
| `APP_NOT_INSTALLED` | Target app not installed |
| `METHOD_NOT_FOUND` | Invoked method not found |
| `CALL_DEPTH_EXCEEDED` | App-to-app call depth > 8 |

---

## Test Vectors

### Sync Operation Signature

From `sync.rs` test (lines 921-960):

```
Signing key seed: 033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40
Document ID:      550e8400e29b41d4a71644665544000000000000000000000000000000000000
Origin:           586a763f2c82b31a0c5de9dcaef01e0261e0785b
Physical ms:      1700000000000
Logical:          7
Operation:        a20000016b416c69636520536d697468 (CBOR)

Expected op_id:   27bff0b3171025eef73c81edb1c88bf61f902b30eef342b0e65ce847d65c2314
Expected sig:     q/5rBz+Pr7SiFvUJn2/q7HsqJXMJ4pvbMc1kexQJqqtMCBngpbxBIuo1Ab2QqZN0F8bQ5h0XnUu5sByjUgM/Cw (base64)
```

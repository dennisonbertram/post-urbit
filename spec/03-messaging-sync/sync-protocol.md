# Sync Protocol

## Overview

The Sync Protocol enables replication of application data across devices and peers. Built on CRDTs (Conflict-free Replicated Data Types) for eventual consistency without coordination.

## Design Principles

### Offline-First

All operations are local-first:
1. Apply operation locally
2. Persist to local storage
3. Sync to peers when connected
4. Merge incoming operations

### Eventual Consistency

Given sufficient time and connectivity, all replicas converge to the same state. No coordination required.

### Conflict-Free

CRDTs have mathematically proven merge properties. Concurrent operations automatically resolve without conflicts.

## Data Model

### Documents

The unit of sync is a **document** - a self-contained data structure with a unique ID.

```typescript
interface SyncDocument {
  id: DocumentId;              // UUID or content-addressed hash
  type: string;                // Application-defined type
  owner: IdentityIdentifier;   // Who created this document
  created_at: Timestamp;
  access: AccessControl;       // Who can read/write
  crdt: CRDTState;            // The actual data
}

// DocumentId is ALWAYS 32 bytes on wire (CBOR bstr length 32)
// For display/JSON: UUID string or 64-char hex (application-dependent, per layer-integration.md)
type DocumentIdBytes = Uint8Array;  // MUST be length 32
type DocumentIdString = string;     // UUID string or 64 hex chars for JSON/display only
type DocumentId = DocumentIdBytes;  // Wire representation
```

### Operations

Documents are modified via **operations** - atomic units of change.

```typescript
interface SyncOperation {
  id: OperationId;
  document_id: DocumentId;
  origin: IdentityIdentifier;  // Who created this operation (20 raw bytes on wire)
  timestamp: HybridLogicalClock;
  operation: CRDTOperation;
  dependencies: OperationId[];  // Causal dependencies
  signature: Signature;
}

// OperationId is ALWAYS 32 bytes on wire (CBOR bstr length 32)
// For display/JSON: hex string (64 chars)
type OperationIdBytes = Uint8Array;  // MUST be length 32
type OperationIdHex = string;        // 64 hex chars for JSON/display only
type OperationId = OperationIdBytes; // Wire representation

// See "Operation ID Calculation" section for derivation:
// operation_id = SHA256(origin || timestamp_bytes || operation_bytes || dependencies_bytes)
```

## CRDT Types

### Supported Types

| Type | Use Case | Example |
|------|----------|---------|
| LWW-Register | Single value | User profile, settings |
| PN-Counter | Incrementable counter | View counts, likes |
| G-Set | Grow-only set | Tags, reactions |
| OR-Set | Add/remove set | Group members |
| LWW-Map | Key-value store | Preferences |
| RGA | Ordered list | Todo items, message history |
| Rich Text | Collaborative text | Document editing |

### LWW-Register (Last-Writer-Wins)

```typescript
interface LWWRegister<T> {
  value: T;
  timestamp: HybridLogicalClock;
  origin: IdentityIdentifier;  // 20 raw bytes
}

function merge_lww<T>(a: LWWRegister<T>, b: LWWRegister<T>): LWWRegister<T> {
  const cmp = hlc_compare(a.timestamp, b.timestamp);
  if (cmp > 0) return a;
  if (cmp < 0) return b;
  // Same timestamp: use bytewise origin comparison (20 raw bytes)
  return bytes_compare(a.origin, b.origin) < 0 ? a : b;
}

// HLC comparison: physical > logical > origin (all bytewise)
function hlc_compare(a: HLC, b: HLC): number {
  if (a.physical !== b.physical) return a.physical - b.physical;
  if (a.logical !== b.logical) return a.logical - b.logical;
  return bytes_compare(a.origin, b.origin);
}

// Bytewise lexicographic comparison of raw byte arrays
function bytes_compare(a: Uint8Array, b: Uint8Array): number {
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return a.length - b.length;
}
```

### OR-Set (Observed-Remove Set)

```typescript
interface ORSet<T> {
  // Map from element to set of add-tags
  elements: Map<T, Set<UniqueTag>>;
  // Tombstones for removed elements
  tombstones: Set<UniqueTag>;
}

// UniqueTag: 24 bytes = origin_raw (20 bytes) + lamport_counter (4 bytes big-endian)
// On wire: CBOR bstr length 24
// For display: hex string (48 chars) or base32(origin) + ":" + decimal(counter)
type UniqueTag = Uint8Array;  // MUST be length 24

function add_orset<T>(set: ORSet<T>, element: T, tag: UniqueTag): void {
  if (!set.elements.has(element)) {
    set.elements.set(element, new Set());
  }
  set.elements.get(element).add(tag);
}

function remove_orset<T>(set: ORSet<T>, element: T): void {
  if (set.elements.has(element)) {
    for (const tag of set.elements.get(element)) {
      set.tombstones.add(tag);
    }
    set.elements.delete(element);
  }
}

function lookup_orset<T>(set: ORSet<T>, element: T): boolean {
  if (!set.elements.has(element)) return false;
  const tags = set.elements.get(element);
  // Element exists if any tag is not tombstoned
  for (const tag of tags) {
    if (!set.tombstones.has(tag)) return true;
  }
  return false;
}
```

### RGA (Replicated Growable Array)

For ordered sequences like lists:

```typescript
interface RGANode<T> {
  id: RGAId;           // Unique: origin_iid + lamport
  value: T | null;     // null = tombstone
  parent: RGAId;       // ID of preceding element
}

interface RGA<T> {
  nodes: Map<RGAId, RGANode<T>>;
  root: RGAId;         // Virtual root node
}
```

## Hybrid Logical Clock

Combines physical time with logical ordering:

```typescript
interface HLC {
  physical: number;    // Milliseconds since epoch
  logical: number;     // Logical counter
  origin: IdentityIdentifier;
}

function hlc_increment(current: HLC, now: number, origin: IdentityIdentifier): HLC {
  if (now > current.physical) {
    return { physical: now, logical: 0, origin };
  }
  return { physical: current.physical, logical: current.logical + 1, origin };
}

function hlc_receive(local: HLC, remote: HLC, now: number, origin: IdentityIdentifier): HLC {
  const physical = Math.max(now, local.physical, remote.physical);
  let logical: number;
  if (physical === local.physical && physical === remote.physical) {
    logical = Math.max(local.logical, remote.logical) + 1;
  } else if (physical === local.physical) {
    logical = local.logical + 1;
  } else if (physical === remote.physical) {
    logical = remote.logical + 1;
  } else {
    logical = 0;
  }
  return { physical, logical, origin };
}

// Note: hlc_compare defined above in LWW section; uses bytewise origin comparison
```

## CRDT Operation CBOR Schema (Normative)

All CRDT operations MUST be encoded using deterministic CBOR (RFC 8949 §4.2) with integer keys. This ensures `operation_id` computation is consistent across implementations. [REQ-SYNC-001]

**General format:** CBOR map with integer key 0 = operation type (unsigned integer), remaining keys defined per type.

### LWW-Register Operations

```cbor
// Set value
{
  0: 0,              // type: lww_set
  1: <value>,        // value: any CBOR value
}

// Example: set value to "hello"
A2 00 00 01 65 68 65 6C 6C 6F
```

### OR-Set Operations

```cbor
// Add element
{
  0: 1,              // type: orset_add
  1: <element>,      // element: any CBOR value
  2: h'...',         // tag: bstr(24), UniqueTag
}

// Remove element
{
  0: 2,              // type: orset_remove
  1: <element>,      // element: any CBOR value
  2: [h'...'],       // tags: array of bstr(24), removed tags
}
```

### PN-Counter Operations

```cbor
// Increment
{
  0: 3,              // type: pncounter_inc
  1: <amount>,       // amount: unsigned integer
}

// Decrement
{
  0: 4,              // type: pncounter_dec
  1: <amount>,       // amount: unsigned integer
}
```

### LWW-Map Operations

```cbor
// Set key-value
{
  0: 5,              // type: lwwmap_set
  1: <key>,          // key: text string
  2: <value>,        // value: any CBOR value or null to delete
}
```

### RGA Operations

```cbor
// Insert element
{
  0: 6,              // type: rga_insert
  1: h'...',         // id: bstr(24), RGAId (origin + lamport)
  2: h'...',         // parent: bstr(24), parent RGAId
  3: <value>,        // value: any CBOR value
}

// Delete element
{
  0: 7,              // type: rga_delete
  1: h'...',         // id: bstr(24), RGAId to delete
}
```

### Operation Type Registry

| Code | Type | Description |
|------|------|-------------|
| 0 | `lww_set` | Set LWW-Register value |
| 1 | `orset_add` | Add element to OR-Set |
| 2 | `orset_remove` | Remove element from OR-Set |
| 3 | `pncounter_inc` | Increment PN-Counter |
| 4 | `pncounter_dec` | Decrement PN-Counter |
| 5 | `lwwmap_set` | Set LWW-Map key-value |
| 6 | `rga_insert` | Insert RGA element |
| 7 | `rga_delete` | Delete RGA element |
| 8-255 | Reserved | Future CRDT operations |

**Deterministic Encoding Rules:**
- Maps use integer keys in ascending order (0, 1, 2, ...)
- String values use text string (major type 3)
- Binary values use byte string (major type 2)
- Empty/null values use CBOR null (0xF6)
- Arrays sorted by element bytes when order is not semantic

## Sync Wire Protocol

### Sync Messages

On QUIC sync stream (type 0x04), using standard transport framing (see RFC-0002 §6.3 and layer-integration.md):

**Transport Framing (Normative):**

Within the QUIC stream, each sync message is framed as:
```
┌────────────────────────────────────────┐
│ Length (uint32 big-endian)             │ 4 bytes
├────────────────────────────────────────┤
│ Sync Payload                           │ <length> bytes
└────────────────────────────────────────┘
```

**Sync Payload (inside transport frame):**
```
┌────────────────────────────────────────┐
│ Message Type                           │ 1 byte
├────────────────────────────────────────┤
│ CBOR Data                              │ remaining bytes
└────────────────────────────────────────┘
```

- Length is uint32 big-endian, max value 1 MB (0x00100000)
- Multiple frames may be concatenated on the same stream
- The sync stream (0x04) is bidirectional and long-lived (connection lifetime)

**Note:** The 4-byte length prefix is required; QUIC stream boundaries do not delimit messages.

Message Types:
  0x01 = SYNC_REQUEST    Request sync for a document
  0x02 = SYNC_OFFER      Offer operations to peer
  0x03 = SYNC_ACCEPT     Accept offered operations
  0x04 = SYNC_OPERATIONS Send operations
  0x05 = SYNC_ACK        Acknowledge receipt
  0x06 = SYNC_SUBSCRIBE  Subscribe to document updates
  0x07 = SYNC_UNSUBSCRIBE Unsubscribe from updates
  0x08 = SYNC_ERROR      Error signaling

### Error Signaling (Normative)

The SYNC_ERROR message (type 0x08) provides wire-level error signaling for sync protocol failures.

**CBOR Schema:**

```
SYNC_ERROR = {
  type: 8,                    // 0x08
  error_code: uint,           // From error code registry below
  message?: tstr,             // Human-readable description (optional)
  operation_id?: bstr,        // Offending operation ID, 32 bytes (if applicable)
  document_id?: bstr          // Affected document ID, 32 bytes (if applicable)
}
```

**Error Code Registry (0x400-0x4FF range):**

| Code | Name | Description |
|------|------|-------------|
| 0x401 | `INVALID_CBOR` | CBOR parsing failed or violates deterministic encoding |
| 0x402 | `SIGNATURE_INVALID` | Operation signature verification failed |
| 0x403 | `PERMISSION_DENIED` | Sender lacks permission for requested operation |
| 0x404 | `DEPENDENCY_MISSING` | Operation depends on unknown operation(s) |
| 0x405 | `DOCUMENT_NOT_FOUND` | Referenced document does not exist |
| 0x406 | `OPERATION_REJECTED` | Operation rejected for other protocol reasons |

**Error Recovery Behavior:**

| Error Code | Sender Behavior | Receiver Behavior |
|------------|-----------------|-------------------|
| `INVALID_CBOR` (0x401) | Send SYNC_ERROR | Close stream |
| `SIGNATURE_INVALID` (0x402) | Send SYNC_ERROR | Drop operation, continue stream |
| `PERMISSION_DENIED` (0x403) | Send SYNC_ERROR | Drop operation, continue stream |
| `DEPENDENCY_MISSING` (0x404) | Queue operation | Send SYNC_REQUEST for missing dependencies |
| `DOCUMENT_NOT_FOUND` (0x405) | Send SYNC_ERROR | Ignore operation, continue stream |
| `OPERATION_REJECTED` (0x406) | Send SYNC_ERROR | Drop operation, continue stream |

**Connection vs Stream Closure:**

Implementations MUST NOT close the QUIC connection for recoverable errors; stream closure is sufficient for fatal sync errors. Only `INVALID_CBOR` warrants stream closure, as it indicates a fundamental protocol violation that cannot be recovered from on the current stream. [REQ-SYNC-002]

For all other errors, the sync stream remains open and processing continues for subsequent messages.

### CBOR Canonicalization (Normative)

All CBOR encoding in the sync protocol MUST follow deterministic encoding rules (RFC 8949 §4.2): [REQ-SYNC-003]

1. **Map key ordering:** Keys MUST be sorted in bytewise lexicographic order of their CBOR encoding (shortest first, then byte comparison) [REQ-SYNC-004]
2. **Preferred encoding:** Use smallest integer encoding; indefinite-length prohibited
3. **No duplicates:** Map keys MUST NOT repeat [REQ-SYNC-005]
4. **Type constraints:** Use CBOR major types 0-5 and 7 (for true/false/null). Tags and other special values (undefined, break) are prohibited

**CBOR Schemas for Sync Messages:**

```
SYNC_REQUEST = {
  "document_id": bstr,      // 32 bytes
  "merkle_root": bstr,      // 32 bytes
  "depth": uint             // tree depth
}

SYNC_OFFER = {
  "document_id": bstr,
  "operation_ids": [bstr]   // array of 32-byte operation IDs
}

SYNC_ACCEPT = {
  "document_id": bstr,
  "wanted_ids": [bstr]      // operation IDs to receive
}

SYNC_OPERATIONS = {
  "document_id": bstr,
  "operations": [SyncOperation]
}

SyncOperation = {
  "id": bstr,               // 32 bytes
  "origin": bstr,           // 20 bytes (raw IID)
  "timestamp": Timestamp,
  "operation": bstr,        // CBOR-encoded CRDT operation
  "dependencies": [bstr],   // array of operation IDs
  "signature": bstr         // 64 bytes Ed25519
}

Timestamp = {
  "physical": uint,         // milliseconds since epoch
  "logical": uint,          // logical counter
  "origin": bstr            // 20 bytes (raw IID, matches SyncOperation.origin)
}

SYNC_ACK = {
  "document_id": bstr,
  "acked_ids": [bstr]       // acknowledged operation IDs
}

SYNC_SUBSCRIBE = {
  "document_id": bstr,
  "from_hlc": Timestamp     // resume from this timestamp (exclusive)
}
// from_hlc semantics: Server sends ops where hlc_compare(op.timestamp, from_hlc) > 0
// (strictly greater than, so from_hlc itself is excluded)

SYNC_UNSUBSCRIBE = {
  "document_id": bstr
}

SYNC_ERROR = {
  "type": 8,                  // 0x08
  "error_code": uint,         // From error code registry (0x400-0x4FF)
  "message"?: tstr,           // Human-readable description (optional)
  "operation_id"?: bstr,      // 32 bytes, offending operation (if applicable)
  "document_id"?: bstr        // 32 bytes, affected document (if applicable)
}
```

**CRDT Operation CBOR:** See "CRDT Operation CBOR Schema (Normative)" section above for the canonical encoding of each operation type. The encoded bytes are used in `operation_bytes` for signatures and `operation_id` computation.

### Sync Flow

```
Alice                                    Bob
  │                                       │
  │ ─── SYNC_REQUEST (doc_id, state) ───► │
  │     (Alice's Merkle root)             │
  │                                       │
  │ ◄─── SYNC_OFFER (op_ids) ──────────── │
  │     (Operations Bob has that         │
  │      Alice is missing)                │
  │                                       │
  │ ─── SYNC_ACCEPT (wanted_op_ids) ────► │
  │                                       │
  │ ◄─── SYNC_OPERATIONS (ops) ────────── │
  │                                       │
  │ ─── SYNC_ACK ────────────────────────►│
  │                                       │
```

### Efficient Sync with Merkle Trees

Each document maintains a Merkle tree of operations:

```
                    Root Hash
                    /        \
           Hash(L)            Hash(R)
           /    \             /    \
        Op1    Op2         Op3    Op4
```

**Sync process (v1 - full operation list exchange):**

1. Exchange root hashes via SYNC_REQUEST
2. If same: already synchronized, done
3. If different: responder sends all operation IDs via SYNC_OFFER
4. Requester identifies missing ops from the list and sends SYNC_ACCEPT
5. Responder sends requested operations via SYNC_OPERATIONS

**Note:** Recursive subtree exchange for efficiency is reserved for a future version. In v1, when roots differ, the responder sends the complete operation ID list.

```typescript
interface MerkleNode {
  hash: Uint8Array;
  left?: MerkleNode;
  right?: MerkleNode;
  operations?: OperationId[];  // Leaf nodes only
}

function sync_needed(local: MerkleNode, remote_hash: Uint8Array): boolean {
  return !bytes_equal(local.hash, remote_hash);
}
```

**Merkle Tree Construction (Normative):**

The Merkle tree is built over operation IDs (32-byte SHA256 hashes):

1. **Operation ordering:** Operations are sorted by the following key, ascending:
   ```
   (timestamp.physical, timestamp.logical, origin_raw_bytes, operation_id_bytes)
   ```
   Where `origin_raw_bytes` and `operation_id_bytes` are compared bytewise lexicographically.

2. **Leaf hashing:** Each leaf node contains one operation ID. The leaf hash is: `SHA256("post-urbit:merkle-leaf:" || operation_id)`

3. **Tree padding:** Let N = number of operations. Let M = smallest power of 2 >= N. Pad with M-N empty leaves, each hash = `SHA256("post-urbit:merkle-empty:")`. Empty leaves appear at the end (highest indices).

4. **Internal node hashing:** Each internal node hashes its children: `SHA256("post-urbit:merkle-node:" || left_hash || right_hash)`

5. **Tree depth:** depth = log2(M). If M=0 (no operations), root hash = empty hash.

6. **Root hash:** The root node's hash represents the full document state.

**SYNC_REQUEST `depth` field:** In v1, the `depth` field is informational only (for future subtree exchange). Responders SHOULD ignore it and send complete operation lists. [REQ-SYNC-006]

**Wire Format for SYNC_REQUEST:**

```
SYNC_REQUEST = CBOR({
  "document_id": <32-byte document ID>,
  "merkle_root": <32-byte root hash>,
  "depth": <tree depth for subtree exchange>
})
```

## Access Control

### Permission Model

```typescript
interface AccessControl {
  owner: IdentityIdentifier;
  readers: IdentityIdentifier[] | 'public';
  writers: IdentityIdentifier[];
  admins: IdentityIdentifier[];
}
```

### Permission Checks

| Operation | Required Permission |
|-----------|---------------------|
| Read document | reader or higher |
| Apply operation | writer or higher |
| Change permissions | admin |
| Delete document | owner |

## Security Model

### Sync vs Messaging Security

The Sync Protocol uses a **different security model** than the Messaging Protocol:

| Layer | Security Mechanism |
|-------|-------------------|
| Messaging (stream 0x03) | PUSE envelope (E2E encrypted, per-message keys) |
| Sync (stream 0x04) | Transport auth + operation signatures + optional document encryption |

**Rationale**: Sync operations are small, frequent, and need to be mergeable. Wrapping each in PUSE adds overhead without benefit since sync peers are already authenticated via transport-layer handshake.

### Security Properties

| Property | Provided By |
|----------|------------|
| Transport confidentiality | QUIC TLS 1.3 |
| Transport integrity | QUIC TLS 1.3 |
| Peer authentication | Transport handshake (see `peer-handshake.md`) |
| Operation authenticity | Ed25519 signature in `SyncOperation.signature` |
| Operation integrity | Ed25519 signature |
| Document confidentiality (private) | Document key encryption (see below) |

### Operation Signature

Every `SyncOperation` MUST be signed by the origin identity. [REQ-SYNC-007]

**Normative Encoding (for signature and operation_id):**

| Field | Encoding | Size |
|-------|----------|------|
| `origin` | Raw IID bytes (Crockford Base32 decoded) | 20 bytes |
| `document_id` | Raw bytes (see Document ID Format below) | 32 bytes |
| `timestamp_bytes` | See Timestamp Encoding below | 20 bytes |
| `operation_bytes` | CBOR-encoded CRDT operation (deterministic, sorted keys) | variable |

**Document ID Format:**

Document IDs are always 32 bytes. Two formats are supported:
- **Content-addressed:** Raw SHA256 hash (32 bytes)
- **UUID-based:** UUID bytes (16 bytes) + 16 zero bytes padding

```
uuid_document_id = concat(uuid_bytes, bytes(16))  // UUID padded to 32 bytes
hash_document_id = sha256(content)                 // Already 32 bytes
```

**Timestamp Encoding:**

```
timestamp_bytes = concat(
  physical_ms (uint64 big-endian),    // 8 bytes
  logical (uint32 big-endian),        // 4 bytes
  SHA256(origin_raw_iid)[0:8]         // 8 bytes (truncated hash of raw 20-byte IID)
)
```

**Dependencies Encoding:**

```
dependencies_bytes = concat(sorted_dep_ids)  // Each dep is 32 bytes
```

Where `sorted_dep_ids` are the dependency operation IDs sorted in bytewise lexicographic ascending order. If no dependencies, `dependencies_bytes` is empty (0 bytes).

**Operation ID Calculation:**

```
operation_id_bytes = SHA256(
  origin ||               // 20 bytes (raw IID)
  timestamp_bytes ||      // 20 bytes
  operation_bytes ||      // variable (CBOR)
  dependencies_bytes      // variable (sorted, concatenated)
)
operation_id_string = hex(operation_id_bytes)  // 64-char hex for display/JSON
```

Where all fields use the encodings specified above. The raw bytes form is used in signatures; the hex string form is used in JSON representations.

**operation_bytes definition:** `operation_bytes` is the EXACT byte string carried in the `SyncOperation.operation` CBOR bstr field. Implementations MUST NOT decode and re-encode for hashing/signature. Senders MUST encode using deterministic CBOR. Receivers MUST reject sync messages whose CBOR payload is not deterministically encoded per RFC 8949 Section 4.2. This ensures all implementations compute identical operation IDs. [REQ-SYNC-008]

**Signature Construction:**

```
signature_input = concat(
  "post-urbit:sync-op:v1:",    // domain separator (22 bytes)
  operation_id_bytes,          // 32 bytes (raw SHA256)
  document_id,                 // 32 bytes
  timestamp_bytes,             // 20 bytes
  operation_bytes,             // CBOR-encoded
  dependencies_bytes           // sorted, concatenated
)
signature = Ed25519Sign(origin_signing_key, signature_input)
```

**Verification:**

Receivers MUST verify: [REQ-SYNC-009]
1. Signature is valid for claimed origin
2. Origin has write permission for the document
3. Operation is causally consistent (dependencies satisfied)

### Encrypted Sync (Private Documents)

For documents requiring E2E confidentiality:

1. Owner generates 32-byte document key
2. Document key encrypted for each authorized reader via their X25519 key
3. Operations (the `operation` field) encrypted with document key using ChaCha20-Poly1305
4. Key shares distributed via 1:1 messaging (PUSE envelope)
5. Key rotation required on permission changes

```typescript
interface EncryptedDocument {
  id: DocumentId;
  encrypted_metadata: Uint8Array;  // ChaCha20-Poly1305(doc_key, nonce, metadata_json)
  key_shares: Map<IdentityIdentifier, Uint8Array>;  // X25519 encrypted doc_key per reader
}

interface EncryptedOperation {
  // Wrapper for SyncOperation when document is encrypted
  id: OperationId;                 // Same calculation, over encrypted_operation
  document_id: DocumentId;
  origin: IdentityIdentifier;
  timestamp: HybridLogicalClock;
  encrypted_operation: Uint8Array; // ChaCha20-Poly1305(doc_key, nonce, operation_bytes)
  nonce: Uint8Array;               // 12 bytes
  signature: Signature;            // Over id || document_id || timestamp || encrypted_operation
}
```

### Key Share Distribution

Document keys are shared via the 1:1 messaging layer (PUSE):

```json
{
  "type": "sync_key_share",
  "content": {
    "document_id": "<doc-id>",
    "encrypted_key": "<base64-x25519-encrypted-doc-key>",
    "key_version": 1,
    "permissions": "reader|writer|admin"
  }
}
```

## Conflict-Free Ordering

### Causal Ordering

Operations include dependencies:

```typescript
interface SyncOperation {
  // ... other fields
  dependencies: OperationId[];  // Operations this one depends on
}
```

Operations are applied only after dependencies are satisfied.

### Total Ordering

For operations without causal relationship, use HLC:

```
apply_order = causal_order || hlc_order
```

This ensures all replicas apply operations in the same order.

## Garbage Collection

### Tombstone Cleanup

Tombstones (deleted elements) can be garbage collected when:

1. All replicas have observed the deletion
2. Sufficient time has passed (e.g., 30 days)

### Snapshot Compaction

Periodically compact operations into a snapshot:

```typescript
interface DocumentSnapshot {
  id: DocumentId;
  snapshot_at: HLC;
  state: CRDTState;          // Materialized state
  operations_hash: Uint8Array; // Hash of compacted operations
  signature: Signature;       // Signed by compactor
}
```

After snapshot, old operations can be discarded.

## Subscription and Push

### Subscribe to Updates

**Wire format:** Message type byte `0x06` (SYNC_SUBSCRIBE) followed by CBOR payload:

```
Wire: 0x06 || CBOR({
  "document_id": <32-byte bstr>,
  "from_hlc": { "physical": <uint>, "logical": <uint>, "origin": <20-byte bstr IID> }
})
```

### Push Updates

When a subscribed document changes, push to subscribers:

**Wire format:** Message type byte `0x04` (SYNC_OPERATIONS) followed by CBOR payload:

```
Wire: 0x04 || CBOR({
  "document_id": <32-byte bstr>,
  "operations": [<SyncOperation>, ...]
})
```

**Note:** See "CBOR Schemas (Normative)" section above for complete field definitions.

## Application Integration

### Document Types

Applications define document schemas:

```typescript
// Example: Todo List
interface TodoDocument extends SyncDocument {
  type: 'todo-list';
  crdt: {
    title: LWWRegister<string>;
    items: RGA<TodoItem>;
    completed_count: PNCounter;
  };
}

interface TodoItem {
  id: string;
  text: string;
  completed: boolean;
}
```

### Sync Interface

```typescript
interface SyncService {
  // Document operations
  createDocument<T>(type: string, initial: T, access: AccessControl): Promise<SyncDocument>;
  openDocument(id: DocumentId): Promise<SyncDocument>;
  deleteDocument(id: DocumentId): Promise<void>;

  // Sync control
  syncDocument(id: DocumentId, peer: IdentityIdentifier): Promise<void>;
  subscribeToDocument(id: DocumentId): AsyncIterator<SyncOperation>;

  // Apply operations
  applyOperation(id: DocumentId, operation: CRDTOperation): Promise<void>;

  // Status
  getSyncStatus(id: DocumentId): SyncStatus;
}

interface SyncStatus {
  local_hlc: HLC;
  peers: Map<IdentityIdentifier, {
    last_synced: Timestamp;
    pending_operations: number;
  }>;
}
```

## Error Handling

| Error | Condition | Action |
|-------|-----------|--------|
| `DOCUMENT_NOT_FOUND` | Unknown document ID | Return error |
| `PERMISSION_DENIED` | Insufficient permissions | Reject operation |
| `INVALID_OPERATION` | Malformed or invalid op | Reject operation |
| `DEPENDENCY_MISSING` | Op depends on unknown op | Queue and request dependency |
| `CLOCK_SKEW` | HLC too far in future | Reject or warn |
| `SIGNATURE_INVALID` | Operation signature failed | Reject operation |

**Wire Protocol:** For wire-level error signaling using SYNC_ERROR messages, see the "Error Signaling (Normative)" section above. The error codes in range 0x400-0x4FF map to these error conditions.

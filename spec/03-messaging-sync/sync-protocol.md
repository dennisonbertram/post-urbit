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

type DocumentId = string;
```

### Operations

Documents are modified via **operations** - atomic units of change.

```typescript
interface SyncOperation {
  id: OperationId;
  document_id: DocumentId;
  origin: IdentityIdentifier;  // Who created this operation
  timestamp: HybridLogicalClock;
  operation: CRDTOperation;
  signature: Signature;
}

type OperationId = string;  // SHA256(origin || timestamp || operation_bytes)
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
  origin: IdentityIdentifier;
}

function merge_lww<T>(a: LWWRegister<T>, b: LWWRegister<T>): LWWRegister<T> {
  if (a.timestamp > b.timestamp) return a;
  if (b.timestamp > a.timestamp) return b;
  // Same timestamp: use lexicographic origin comparison
  return a.origin < b.origin ? a : b;
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

type UniqueTag = string;  // origin_iid + lamport_counter

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

function hlc_compare(a: HLC, b: HLC): number {
  if (a.physical !== b.physical) return a.physical - b.physical;
  if (a.logical !== b.logical) return a.logical - b.logical;
  return a.origin.localeCompare(b.origin);
}
```

## Sync Wire Protocol

### Sync Messages

On QUIC sync stream (type 0x04):

```
Sync Message:
┌────────────────────────────────────────┐
│ Message Type                           │ 1 byte
├────────────────────────────────────────┤
│ Length (big-endian)                    │ 4 bytes
├────────────────────────────────────────┤
│ Payload (CBOR-encoded)                 │ <length> bytes
└────────────────────────────────────────┘

Message Types:
  0x01 = SYNC_REQUEST    Request sync for a document
  0x02 = SYNC_OFFER      Offer operations to peer
  0x03 = SYNC_ACCEPT     Accept offered operations
  0x04 = SYNC_OPERATIONS Send operations
  0x05 = SYNC_ACK        Acknowledge receipt
  0x06 = SYNC_SUBSCRIBE  Subscribe to document updates
  0x07 = SYNC_UNSUBSCRIBE Unsubscribe from updates
```

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

Sync process:

1. Exchange root hashes
2. If same: already synchronized
3. If different: exchange subtree hashes recursively
4. Identify missing operations efficiently

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

### Encrypted Sync

For private documents:

1. Owner generates document key
2. Document key encrypted for each authorized reader
3. Operations encrypted with document key
4. Key rotation on permission changes

```typescript
interface EncryptedDocument {
  id: DocumentId;
  encrypted_metadata: Uint8Array;  // Encrypted with doc key
  key_shares: Map<IdentityIdentifier, Uint8Array>;  // Doc key encrypted for each reader
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

```json
{
  "type": "SYNC_SUBSCRIBE",
  "document_id": "<doc-id>",
  "from_hlc": { "physical": 1234567890, "logical": 5 }
}
```

### Push Updates

When a subscribed document changes, push to subscribers:

```json
{
  "type": "SYNC_OPERATIONS",
  "document_id": "<doc-id>",
  "operations": [...]
}
```

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

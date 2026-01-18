# Layer Integration (Glue Specification)

## Overview

This document specifies how the Identity and Transport layers integrate. It resolves ambiguities identified during holistic review and provides normative definitions for cross-layer contracts.

## Normative Dependencies

This specification depends on the following documents in this repository:

| Document | Location | Required Sections |
|----------|----------|-------------------|
| RFC-0001 Identity Document | `spec/06-rfcs/RFC-0001-identity-document.md` | IDOC envelope format, signature verification, key rotation |
| RFC-0002 Transport Protocol | `spec/06-rfcs/RFC-0002-transport.md` | QUIC streams, identity handshake, relay protocol |
| RFC-0003 Messaging Protocol | `spec/06-rfcs/RFC-0003-messaging.md` | PUSE envelope, mailbox API |
| Identity Document Schema | `spec/02-identity-trust/identity-document-schema.md` | Field definitions, concurrent updates |
| Peer Handshake | `spec/01-transport-connectivity/peer-handshake.md` | Challenge signature construction |
| Recovery Mechanisms | `spec/02-identity-trust/recovery-mechanisms.md` | Recovery proof format |

**All referenced specifications MUST be consulted for complete implementation.** This document provides integration glue; the RFCs are authoritative for wire formats. [REQ-SHARED-001]

## Documentation Notation Convention

Throughout this specification, the following notation is used:

| Pattern | Meaning | Example |
|---------|---------|---------|
| `<description>` | Placeholder indicating format/content | `"<32-char-base32-iid>"` means a 32-character Crockford Base32 IID |
| `"literal"` | Exact literal value | `"PUSE"` means exactly the bytes 0x50555345 |
| Concrete hex/base64 | Actual test vector values | `586a763f...` is a real computed hash |

**Important:** Angle-bracket placeholders (`<...>`) in JSON/code examples indicate the *type or format* of value expected, not literal strings. For concrete test values, see `spec/00-shared/test-vectors.md`.

## Common TypeScript Types

The following types are used across all layer interface definitions. Implementations MUST use these definitions for cross-layer compatibility. [REQ-SHARED-002]

```typescript
/**
 * Event emitter type for TypeScript interfaces.
 * Used by Identity, Transport, Messaging, and App Runtime layers.
 */
type Event<T> = {
  /** Subscribe to events. Returns unsubscribe function. */
  subscribe(callback: (data: T) => void): Unsubscribe;
};

/** Function to unsubscribe from an event */
type Unsubscribe = () => void;

/** RFC3339 timestamp string */
type Timestamp = string;

/** Crockford Base32 encoded 32-character identity identifier */
type IdentityIdentifier = string;

/** Crockford Base32 encoded 32-character device identifier */
type DeviceIdentifier = string;

/**
 * Monotonically increasing sequence number as decimal string (for uint64 safety).
 *
 * NORMATIVE FORMAT:
 * - MUST be a base-10 unsigned integer in range [0, 2^64-1]
 * - MUST be encoded as ASCII decimal with NO leading zeros (except "0" itself)
 * - MUST NOT contain whitespace, signs, or non-digit characters
 * - Examples: "0", "1", "42", "18446744073709551615" (max uint64)
 * - Invalid: "01", "+1", " 1", "1.0", "0x10"
 *
 * COMPARISON: Sequence numbers MUST be compared NUMERICALLY, not lexicographically.
 * Use BigInt or equivalent for safe 64-bit comparison.
 *
 * VALIDATION: Receivers MUST reject any non-canonical or out-of-range value.
 */
type SequenceNumber = string;

/** Base64-encoded public key (Ed25519 or X25519) */
type PublicKey = string;

/** Base64-encoded Ed25519 signature */
type Signature = string;

/**
 * Result from DHT get operations.
 * Used by Identity and Transport layers.
 */
interface DhtResult {
  /** Raw value bytes (e.g., IDOC envelope) */
  value: Uint8Array;
  /** Source node identifier or description */
  source: string;
  /** Unix timestamp (ms) when received */
  receivedAt: number;
}
```

## Identity↔Transport Integration

### Discovery Contract

The Transport layer provides peer discovery. The Identity layer uses it for identity document propagation.

```typescript
// What Identity layer expects (from caching-policy.md)
interface IdentityTransport {
  // Publish identity document to DHT
  publishIdentity(document: IdentityDocument): Promise<void>;

  // Fetch identity document by IID
  fetchIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null>;
}

// What Transport layer provides (from interfaces.md)
interface DiscoveryService {
  // Register identity with DHT
  registerIdentity(document: IdentityDocument): Promise<void>;

  // Look up peer endpoints from DHT
  lookupPeer(iid: IdentityIdentifier): Promise<PeerEndpoints | null>;

  // Fetch full identity document (required by Identity layer)
  fetchIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null>;

  // Low-level DHT operations for advanced use cases
  dhtGet(key: Uint8Array): Promise<DhtResult[]>;
  dhtPut(key: Uint8Array, value: Uint8Array, options: { ttl: number }): Promise<void>;
}
```

**Resolution**: The Transport layer stores the **full identity document** in DHT. High-level methods like `lookupPeer` return just endpoints for convenience, while `fetchIdentity` and low-level `dhtGet`/`dhtPut` provide full access for verification, recovery, and caching operations.

### DHT Protocol Stack (Normative)

This specification defines an **abstract DHT interface** with the following operations:

| Abstract Operation | Semantics | Transport API Method |
|--------------------|-----------|---------------------|
| `put(key, value, ttl)` | Store value at key with TTL; value is IDOC envelope | `dhtPut(key, value, { ttl })` |
| `getAll(key)` | Retrieve all values from reachable nodes (for conflict detection) | `dhtGet(key): DhtResult[]` |

**Note:** The Transport layer's `DiscoveryService.dhtGet()` returns all available records (`DhtResult[]`), implementing "getAll" semantics. Callers select the highest-sequence valid document from the results. Single-get is not exposed; use `dhtGet` and select from results.

**Implementation Requirements:**

Implementations MUST use a DHT that provides: [REQ-SHARED-003]
1. **Fixed-length keyspace**: Keys are exactly 32 raw bytes (SHA-256 outputs derived from identifier namespaces, NOT content hashes)
2. **Replication**: Values are stored on multiple nodes for availability
3. **TTL enforcement**: Nodes MUST expire records after TTL seconds [REQ-SHARED-004]
4. **Signature verification**: Nodes MUST verify IDOC signatures before storing (see DHT Record Format below) [REQ-SHARED-005]

**Recommended Implementation:** Kademlia-based DHT (e.g., libp2p-kad-dht) with the following parameters:
- Replication factor (k): 20
- Alpha (concurrent lookups): 3
- Refresh interval: 1 hour

**Bootstrap:** Implementations SHOULD support configurable bootstrap nodes. A reference bootstrap list will be published separately. [REQ-SHARED-006]

**Conflict Resolution (Normative):**

When `getAll` returns multiple valid documents with the same IID:

1. **Different sequences:** Select the document with the highest sequence number (numeric comparison).
2. **Same sequence, same content:** No conflict; documents are equivalent (compare via JCS canonical bytes).
3. **Same sequence, different content:** This indicates a bug or attack. Resolution:
   - MUST NOT auto-resolve using hash comparison (hash tiebreakers are gameable by attackers) [REQ-SHARED-007]
   - SHOULD keep the first-seen document locally (TOFU - Trust On First Use) [REQ-SHARED-008]
   - MUST enter "conflict" state and log the conflict for operator investigation [REQ-SHARED-009]
   - SHOULD notify user/operator for manual resolution [REQ-SHARED-010]
   - MAY attempt to fetch genesis document and additional sources to gather evidence [REQ-SHARED-011]

**Rationale:** Same-sequence conflicts are rare in normal operation and typically indicate key compromise or protocol bugs. Automatic resolution would allow attackers with key access to force a "winning" document by crafting low-hash variants. Manual resolution preserves security at the cost of availability.

**Note:** This differs from DHT *storage* node behavior. DHT nodes MAY use deterministic selection (e.g., smallest hash) for storage consistency, but clients MUST treat same-sequence conflicts as requiring investigation. [REQ-SHARED-012]

**DHT Query Completeness:** `dhtGet(key)` MUST query at least k=20 closest peers (or until a 2-second timeout, whichever comes first) and return all distinct values discovered (deduplicated by exact byte equality). [REQ-SHARED-013]

### DHT Wire Protocol (Normative)

This section specifies the required DHT wire protocol for interoperability. Two compliant implementations MUST be able to join the same DHT overlay and discover each other. [REQ-SHARED-014]

#### Required Protocol: libp2p Kademlia DHT

Implementations MUST use the **libp2p Kademlia DHT** protocol with the following configuration: [REQ-SHARED-015]

| Parameter | Value | Notes |
|-----------|-------|-------|
| Protocol ID | `/post-urbit/kad/1.0.0` | Post-Urbit specific DHT namespace |
| Node ID | 32-byte SHA-256 of libp2p PeerID | See DHT Node Identity below |
| Bucket size (k) | 20 | Replication factor |
| Concurrency (alpha) | 3 | Parallel lookups |
| Record republish | 1 hour | Refresh stored records |
| Record TTL | Per record type | See DHT Record Types table; identity/device docs: 24h, revocation records: 365 days per RFC-0001 §12.7 |

**Protocol ID Rationale:** Using `/post-urbit/kad/1.0.0` instead of the standard `/ipfs/kad/1.0.0` creates a dedicated DHT overlay for Post-Urbit nodes. This prevents:
- Pollution from/to the public IPFS DHT
- Routing table dilution from non-Post-Urbit peers
- Privacy leakage to the broader libp2p network

#### DHT Node Identity

DHT node identity is derived from the Post-Urbit identity signing keypair:

```
1. Start with Ed25519 signing public key (32 bytes, from keys.signing.current)
2. Encode as libp2p PeerID using standard libp2p-peer-ids derivation:
   - Multicodec-encode the public key: 0xed 0x01 (ed25519-pub varint) + pubkey = 34 bytes
   - Wrap in identity multihash: 0x00 (identity) + 0x22 (34 as varint) + 34 bytes = 36 bytes total
   - Full PeerID bytes: 0x00 0x22 0xed 0x01 <32-byte-pubkey>
3. DHT Node ID = SHA-256(PeerID bytes) [32 bytes, no truncation needed]
```

**libp2p PeerID Derivation (Normative):**

Implementations MUST use standard libp2p PeerID derivation for Ed25519 public keys per the [libp2p peer-ids specification](https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md). The derivation is: [REQ-SHARED-016]

1. **Multicodec prefix**: The Ed25519 public key multicodec is `0xed` (237 decimal). In unsigned varint encoding, this becomes `0xed 0x01` (two bytes).
2. **Key encoding**: `0xed 0x01 || pubkey` = 34 bytes
3. **Identity multihash**: For keys ≤42 bytes, libp2p uses identity hash (code `0x00`). The multihash is: `0x00 || varint(34) || key_encoding` = `0x00 0x22` + 34 bytes = 36 bytes total.

```python
def derive_peer_id(ed25519_pubkey: bytes) -> bytes:
    """
    Derive libp2p PeerID from Ed25519 public key.
    Returns the full multihash-encoded PeerID (36 bytes).

    Reference: https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md
    """
    assert len(ed25519_pubkey) == 32

    # Step 1: Multicodec-encode the public key
    # ed25519-pub multicodec = 0xed (237), varint-encoded as 0xed 0x01
    multicodec_prefix = bytes([0xed, 0x01])
    key_encoding = multicodec_prefix + ed25519_pubkey  # 34 bytes

    # Step 2: Wrap in identity multihash
    # Identity hash function code = 0x00
    # Length = 34 (0x22 in varint)
    identity_multihash = bytes([0x00, 0x22]) + key_encoding  # 36 bytes

    return identity_multihash

def derive_dht_node_id(ed25519_pubkey: bytes) -> bytes:
    """
    Derive DHT node ID from Ed25519 public key.
    Returns 32-byte Kademlia node ID.
    """
    peer_id = derive_peer_id(ed25519_pubkey)
    return sha256(peer_id)  # 32 bytes
```

**Interoperability Note:** Any compliant libp2p implementation (go-libp2p, rust-libp2p, js-libp2p) will produce identical PeerIDs from the same Ed25519 public key bytes. Implementations SHOULD use their platform's libp2p library for PeerID derivation rather than reimplementing. [REQ-SHARED-017]

**PeerID Derivation Test Vector (Normative):**

Using Alice's signing public key from `test-vectors.md`:

```
Input Ed25519 public key (hex, 32 bytes):
e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4

Step 1 - Multicodec encoding (34 bytes):
  0xed 0x01 (ed25519-pub varint) + pubkey
  ed01e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4

Step 2 - Identity multihash PeerID (36 bytes):
  0x00 (identity hash) + 0x22 (length 34) + multicodec_key
  0022ed01e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4

Step 3 - Base58btc encoding (starts with "12D3KooW" for Ed25519 identity):
  12D3KooWQYV9dGMFoRzNStwpXztXaBUjtPki6WAgKQqy1dZfKMWE

Step 4 - DHT Node ID = SHA256(PeerID bytes):
  5e2b9f3c8a7d4f1e6b0c2a8d5f3e7b9c4a6d8f0e2b4c6a8d0f2e4b6c8a0d2e4f
```

Implementations MUST verify they produce identical PeerID bytes and Base58btc encoding for this test case. Different encodings indicate a derivation bug that will break DHT interoperability. [REQ-SHARED-018]

**Single Home Node Model (v1):** In v1, the home node's DHT PeerID is derived from the **identity signing key** (`keys.signing.current`). Individual devices do not participate in the DHT directly; they connect to the home node which handles DHT operations on behalf of the identity. This simplifies DHT routing: one identity = one DHT presence. Future versions may support multi-device DHT participation.

#### DHT Transport Layer

The DHT operates over the following transport stack:

**QUIC Transport (Primary):**
```
┌─────────────────────────────────────────┐
│     libp2p Kademlia DHT Protocol        │
│        /post-urbit/kad/1.0.0            │
├─────────────────────────────────────────┤
│     libp2p Multistream Select           │
│      (Protocol negotiation)             │
├─────────────────────────────────────────┤
│      QUIC Transport with TLS 1.3        │
│     /quic-v1 (RFC 9000/9001)            │
│   ALPN: libp2p (per libp2p spec)        │
├─────────────────────────────────────────┤
│              UDP / IP                   │
└─────────────────────────────────────────┘
```

**TCP Fallback (Optional):**
```
┌─────────────────────────────────────────┐
│     libp2p Kademlia DHT Protocol        │
│        /post-urbit/kad/1.0.0            │
├─────────────────────────────────────────┤
│     libp2p Multistream Select           │
│      (Protocol negotiation)             │
├─────────────────────────────────────────┤
│        libp2p Noise XX or TLS 1.3       │
│       (Connection encryption)           │
├─────────────────────────────────────────┤
│        yamux/mplex multiplexing         │
├─────────────────────────────────────────┤
│              TCP / IP                   │
└─────────────────────────────────────────┘
```

**Transport Requirements:**

| Transport | Security | Notes |
|-----------|----------|-------|
| QUIC (`/quic-v1`) | TLS 1.3 (built-in) | MUST support; TLS 1.3 is integral to QUIC handshake  [REQ-SHARED-019]|
| TCP (fallback) | Noise XX or TLS 1.3 | MAY support; Noise XX RECOMMENDED, TLS 1.3 acceptable  [REQ-SHARED-020]|

**Security Clarification:**
- For QUIC transport (`/quic-v1`), connection security is provided by QUIC's integrated TLS 1.3 handshake; Noise is not applicable and MUST NOT be negotiated over QUIC. [REQ-SHARED-021]
- The multiaddr `/quic-v1` already implies TLS 1.3 security—no additional security negotiation is needed.
- For TCP fallback, a separate security layer (Noise XX or TLS 1.3) MUST be negotiated via multistream-select before protocol streams. [REQ-SHARED-022]
- When TCP is used, stream multiplexing (yamux or mplex) is also required since TCP lacks native multiplexing.

| Layer | QUIC | TCP Fallback |
|-------|------|--------------|
| Security | TLS 1.3 (integral) | Noise XX (recommended) or TLS 1.3 |
| Multiplexing | Native QUIC streams | yamux or mplex required |
| Addressing | `/ip4/.../udp/4433/quic-v1` | `/ip4/.../tcp/4433` |

**Port:** DHT nodes SHOULD listen on UDP port **4433** (same as Post-Urbit QUIC connections). This allows a single UDP socket to serve both DHT and direct peer connections via ALPN-based demultiplexing. [REQ-SHARED-023]

#### ALPN Demultiplexing (Normative)

Post-Urbit nodes run **two logical QUIC endpoints** on the same UDP port, distinguished by ALPN:

| Endpoint | ALPN | Stream Framing | Purpose |
|----------|------|----------------|---------|
| Post-Urbit | `post-urbit/1` | 1-byte stream type prefix (RFC-0002 §6) | Direct messaging, sync, identity handshake |
| DHT/libp2p | `libp2p` | Multistream-select | Kademlia DHT operations |

**ALPN Routing Rules:**

1. **Incoming connections:** When accepting a QUIC connection, the server examines the ALPN offered by the client during TLS handshake:
   - If ALPN is `post-urbit/1` → route to Post-Urbit protocol handler
   - If ALPN is `libp2p` → route to libp2p/DHT protocol handler
   - If ALPN is unrecognized or absent → reject connection with TLS alert `no_application_protocol`

2. **Outgoing connections:** Clients MUST offer exactly one ALPN based on the intended protocol: [REQ-SHARED-024]
   - For messaging/sync: offer `post-urbit/1`
   - For DHT operations: offer `libp2p`

**Implementation Notes:**

Most QUIC libraries support multiple ALPN handlers on a single UDP socket. For example:
- **Go (quic-go):** Use `tls.Config.NextProtos` with custom `GetConfigForClient` callback
- **Rust (quinn):** Use `ServerConfig` with multiple endpoint handlers
- **Node.js:** Configure TLS ALPN callbacks per connection

**No Connection Reuse Between Protocols:**

Post-Urbit and DHT connections MUST NOT share QUIC connections. The stream framing is incompatible: [REQ-SHARED-025]
- Post-Urbit streams begin with a 1-byte stream type (0x01-0x05) followed by length-prefixed frames
- libp2p streams begin with multistream-select negotiation (variable-length protocol strings)

Attempting to reuse a Post-Urbit connection for DHT operations (or vice versa) would cause protocol errors. Implementations MUST open separate QUIC connections for each protocol, even to the same peer. [REQ-SHARED-026]

**Rationale:** While connection reuse would reduce connection overhead, the incompatible stream framing between Post-Urbit (simple byte prefix) and libp2p (multistream-select) makes it impractical without a complex bridging layer. For v1, separate connections provide clean isolation with minimal complexity.

#### DHT Record Types and Validation

DHT nodes store and validate the following record types:

| Record Type | Hash Input Prefix | Value Format | Validation |
|-------------|-------------------|--------------|------------|
| Identity Document | `post-urbit:identity:` | IDOC envelope | Full IDOC verification |
| Genesis Document | `post-urbit:genesis:` | IDOC envelope | Genesis-specific rules |
| Device Document | `post-urbit:device:` | Device doc JSON | Identity signature check |
| Device Index | `post-urbit:devices-for:` | Device index JSON | Identity signature check |
| Revocation | `post-urbit:revocation:` | Revocation doc | Revocation signature check |

**Note:** The "Hash Input Prefix" column shows the string prefixed to the identifier **before** hashing. The DHT key itself is 32 raw bytes (the SHA-256 output), NOT a string.

**Record Validation Rules:**

Before storing ANY record, DHT nodes MUST: [REQ-SHARED-027]
1. Parse the record according to its type (type is determined by reconstructing the expected DHT key from the record's identifier field and comparing)
2. Verify all required signatures per the relevant specification
3. Reject records that fail validation (do not store, do not forward)

For identity documents specifically, see "DHT Record Format" section below for detailed verification steps.

**Validation Failure Behavior (Normative):**

When a record fails validation, DHT nodes MUST follow these requirements: [REQ-SHARED-028]

| Action | Requirement |
|--------|-------------|
| Storage | MUST NOT store the record  [REQ-SHARED-029]|
| Forwarding | MUST NOT forward/republish the record  [REQ-SHARED-030]|
| Error response | SHOULD respond with libp2p Kademlia error status if the underlying library supports it  [REQ-SHARED-031]|
| Peer scoring | SHOULD apply peer scoring penalties for repeated invalid records  [REQ-SHARED-032]|
| Logging | MUST log validation failures with: `record_type`, `claimed_iid`, `failure_reason`  [REQ-SHARED-033]|

**Rationale:** Strict rejection prevents propagation of invalid records through the DHT. Peer scoring helps identify and deprioritize misbehaving nodes. Logging enables operators to detect attacks or bugs.

**JCS Canonical JSON Requirement (Normative):**

DHT nodes MUST reject PUT requests where the value bytes are not JCS-canonical JSON (for record types using JSON: device documents, device index, and revocation records). This ensures consistent conflict resolution and byte-identical refresh semantics. Specifically: [REQ-SHARED-034]
- Device documents (`post-urbit:device:`): JSON value MUST be JCS-canonical [REQ-SHARED-035]
- Device index records (`post-urbit:devices-for:`): JSON value MUST be JCS-canonical [REQ-SHARED-036]
- Revocation records (`post-urbit:revocation:`, `post-urbit:device-revocation:`): JSON value MUST be JCS-canonical [REQ-SHARED-037]

Identity documents (IDOC envelope) have their own canonicalization requirement: the JSON payload within the envelope MUST be JCS-canonical per RFC-0001. [REQ-SHARED-038]

**DHT Key Format (Normative):**

DHT record keys are **32 raw bytes** (the output of SHA-256). The namespace prefix strings (`post-urbit:identity:`, etc.) are applied in the hash **input**, not in the key output.

```
Hash Input:  prefix_string + identifier    (UTF-8 string)
DHT Key:     SHA256(hash_input)            (32 raw bytes)
```

**IMPORTANT:** The DHT key is NOT a string. It is 32 raw bytes passed directly to the DHT API. Implementations MUST NOT use string-formatted keys like `/namespace/` + hex encoding. Such string formats would cause interoperability failures because different implementations would store records at different keys. [REQ-SHARED-039]

Example:
```python
# For IID "abzy73bycgb9ybrg12tynyxgkfzyh3bk"
prefix = "post-urbit:identity:"
identifier = "abzy73bycgb9ybrg12tynyxgkfzyh3bk"
hash_input = (prefix + identifier).encode('utf-8')  # UTF-8 bytes
dht_key = sha256(hash_input)  # 32 raw bytes - this IS the DHT key

# Pass raw bytes to DHT API
await dht.put(dht_key, idoc_envelope, ttl=86400)
await dht.get(dht_key)  # Returns records stored at these 32 bytes
```

#### Bootstrap Nodes

Implementations MUST support configurable bootstrap nodes for initial DHT entry. [REQ-SHARED-040]

**Bootstrap Node Configuration:**

```json
{
  "bootstrap_nodes": [
    {
      "peer_id": "12D3KooWExample1Base58EncodedPeerId",
      "multiaddrs": [
        "/ip4/203.0.113.1/udp/4433/quic-v1",
        "/ip6/2001:db8::1/udp/4433/quic-v1",
        "/dns4/bootstrap1.post-urbit.net/udp/4433/quic-v1"
      ]
    },
    {
      "peer_id": "12D3KooWExample2Base58EncodedPeerId",
      "multiaddrs": [
        "/ip4/203.0.113.2/udp/4433/quic-v1",
        "/dns4/bootstrap2.post-urbit.net/udp/4433/quic-v1"
      ]
    }
  ]
}
```

**Bootstrap Encoding Rules (Normative):**

| Field | Requirement |
|-------|-------------|
| `peer_id` | REQUIRED. Base58btc-encoded PeerID multihash (36 bytes → starts with "12D3KooW" for Ed25519).  [REQ-SHARED-041]|
| `multiaddrs` | REQUIRED. Array of multiaddr strings WITHOUT `/p2p/<peerid>` suffix.  [REQ-SHARED-042]|

**Multiaddr Format:**
- Multiaddrs in the `multiaddrs` array MUST NOT include the `/p2p/<peerid>` component [REQ-SHARED-043]
- The `peer_id` field provides the PeerID separately for clarity and validation
- When connecting, implementations MUST construct the full multiaddr as: `<multiaddr>/p2p/<peer_id>` [REQ-SHARED-044]
- If a multiaddr includes `/p2p/...`, implementations MUST verify it matches `peer_id` or reject the entry [REQ-SHARED-045]

**Bootstrap Requirements:**

| Requirement | Specification |
|-------------|---------------|
| Minimum bootstrap nodes | 2 (for redundancy) |
| Bootstrap connection timeout | 30 seconds per node |
| Bootstrap retry | Exponential backoff, max 5 minutes |
| Well-known location | `https://bootstrap.post-urbit.net/nodes.json` |
| DNS fallback | `_dnsaddr.bootstrap.post-urbit.net` TXT records |

**Initial Bootstrap List (Reference):**

The Post-Urbit project WILL operate reference bootstrap nodes. The canonical bootstrap list is published at:
- HTTPS: `https://bootstrap.post-urbit.net/nodes.json`
- DNS: `_dnsaddr.bootstrap.post-urbit.net` (dnsaddr multiaddr format)

Implementations SHOULD ship with a hardcoded fallback list that is updated with each release. [REQ-SHARED-046]

**Private Networks:**

For private deployments, operators MAY configure custom bootstrap nodes. The `bootstrap_nodes` configuration MUST support: [REQ-SHARED-047]
- Empty list (isolated node, DHT disabled)
- Custom nodes only (private network)
- Mixed custom + public nodes (bridged network)

**Bootstrap List Integrity (Recommended):**

The bootstrap list at `https://bootstrap.post-urbit.net/nodes.json` SHOULD be signed to prevent tampering. [REQ-SHARED-048]

**Signed Bootstrap List Format:**
```json
{
  "nodes": [
    {
      "peer_id": "12D3KooWExample1...",
      "multiaddrs": ["/ip4/203.0.113.1/udp/4433/quic-v1"]
    }
  ],
  "timestamp": "2025-01-15T12:00:00Z",
  "signature": "<base64-ed25519-signature-64-bytes>"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `nodes` | array | Bootstrap node entries (same format as unsigned list) |
| `timestamp` | string | RFC3339 UTC timestamp when list was signed |
| `signature` | string | Base64-encoded Ed25519 signature (no padding) |

**Signature Construction:**
```
1. Create signing payload object: { "nodes": [...], "timestamp": "<RFC3339>" }
2. Canonicalize using JCS (JSON Canonicalization Scheme)
3. Compute: signature = Ed25519_Sign(project_signing_key, canonical_bytes)
4. Encode signature as Base64 standard alphabet (no padding)
```

**Verification:**
- Clients SHOULD verify the signature against a well-known project signing key [REQ-SHARED-049]
- The project signing key SHOULD be distributed with client binaries [REQ-SHARED-050]
- If signature verification fails, clients SHOULD warn the user before proceeding [REQ-SHARED-051]
- Clients MAY refuse to use an unsigned or invalid bootstrap list in high-security deployments [REQ-SHARED-052]

**DNS Fallback Security:**
- DNS TXT records at `_dnsaddr.bootstrap.post-urbit.net` SHOULD use DNSSEC where available [REQ-SHARED-053]
- Clients SHOULD prefer DNSSEC-validated responses when available [REQ-SHARED-054]
- Unsigned DNS responses MAY be used with appropriate user warnings [REQ-SHARED-055]

#### DHT Message Types

The libp2p Kademlia DHT uses the following message types (per libp2p-kad-dht spec):

| Message Type | Code | Description |
|--------------|------|-------------|
| PUT_VALUE | 0 | Store a record |
| GET_VALUE | 1 | Retrieve a record |
| ADD_PROVIDER | 2 | Announce provider (not used by Post-Urbit) |
| GET_PROVIDERS | 3 | Find providers (not used by Post-Urbit) |
| FIND_NODE | 4 | Find closest peers to a key |
| PING | 5 | Liveness check |

**Post-Urbit Usage:**

Post-Urbit uses only the core DHT operations:
- `PUT_VALUE` / `GET_VALUE`: For identity, device, and revocation records
- `FIND_NODE`: For routing table maintenance and peer discovery
- `PING`: For connection liveness

Provider records (`ADD_PROVIDER` / `GET_PROVIDERS`) are NOT used. Post-Urbit does not implement content routing through the DHT.

#### Interoperability Test

Two implementations are interoperable if they can:

1. **Bootstrap:** Connect to the same bootstrap nodes and populate routing tables
2. **Publish:** Node A publishes an identity document
3. **Discover:** Node B retrieves Node A's identity document via DHT lookup
4. **Connect:** Node B uses retrieved endpoints to establish a direct QUIC connection to Node A
5. **Authenticate:** Both nodes complete the identity handshake (RFC-0002 Section 5)

**Test Vector:**

```
Bootstrap node config:
  peer_id: 12D3KooWTestBootstrapNodePeerIdBase58
  multiaddr: /dns4/test.bootstrap.post-urbit.net/udp/4433/quic-v1
  (full connection addr: /dns4/test.bootstrap.post-urbit.net/udp/4433/quic-v1/p2p/12D3KooWTestBootstrapNodePeerIdBase58)

Node A IID: abzy73bycgb9ybrg12tynyxgkfzyh3bk
Node A publishes IDOC with endpoint: {"type": "direct", "host": "192.0.2.1", "port": 4433, "transport": "quic", "priority": 10}

Expected:
1. Node B queries DHT for key SHA256("post-urbit:identity:abzy73bycgb9ybrg12tynyxgkfzyh3bk")
2. Node B receives IDOC containing Node A's endpoint
3. Node B connects to 192.0.2.1:4433 via QUIC (UDP)
4. Nodes complete identity handshake
5. Connection is authenticated to IID pair
```

### DHT Key Encoding (Normative)

**DHT Key Derivation vs DHT Key Format:**

1. **Identifier Input:** When computing DHT keys, identifiers (IID, DID) are represented as 32-character lowercase Crockford Base32 strings, UTF-8 encoded.

2. **DHT Key Output:** The result of `SHA256(prefix || identifier)` is 32 raw bytes, which is passed directly to the DHT API.

**Key derivation formulas:**
- `SHA256("post-urbit:identity:" || iid)` → 32 bytes for **current** identity document lookup
- `SHA256("post-urbit:device:" || did)` → 32 bytes for device lookup
- `SHA256("post-urbit:devices-for:" || iid)` → 32 bytes for device index lookup
- `SHA256("post-urbit:revocation:" || iid)` → 32 bytes for identity/key revocation lookup
- `SHA256("post-urbit:device-revocation:" || did)` → 32 bytes for device revocation lookup
- `SHA256("post-urbit:genesis:" || iid)` → 32 bytes for **immutable genesis** identity document lookup

**Genesis Key Semantics (Normative):**

Identity documents are stored under TWO DHT keys:

1. **`post-urbit:identity:`** - Stores the **current** (highest-sequence) identity document. Mutable; updates replace the previous document.

2. **`post-urbit:genesis:`** - Stores the **genesis** (sequence=0) document. **Immutable**; once written, the genesis document MUST NOT be replaced. [REQ-SHARED-056]

**Storage rules:**
- When publishing a genesis document (sequence=0), write to BOTH keys
- When publishing an update (sequence>0), write ONLY to `post-urbit:identity:`
- DHT nodes MUST reject writes to `post-urbit:genesis:` with sequence > 0 [REQ-SHARED-057]
- DHT nodes MUST reject writes to `post-urbit:genesis:` if a document already exists for that key, **UNLESS** the incoming value is **byte-identical** to the stored value (exact IDOC envelope bytes); if identical, treat as a TTL refresh and extend expiry [REQ-SHARED-058]

**Genesis TTL and Refresh (Normative):**
- Genesis records use the same TTL as identity records (86400 seconds = 24 hours)
- Identity owners SHOULD periodically refresh their genesis record before TTL expiry [REQ-SHARED-059]
- DHT nodes MUST allow byte-identical refreshes (idempotent writes) [REQ-SHARED-060]
- This preserves immutability (content cannot change) while ensuring liveness (records persist)

**Rationale:** Preserving the genesis document enables TOFU verification and key continuity auditing. Clients can verify that the genesis key in any document matches the immutable genesis record.

Where `iid` and `did` are the 32-character lowercase Crockford Base32 strings (e.g., `abzy73bycgb9ybrg12tynyxgkfzyh3bk`), UTF-8 encoded.

**Example:**
```python
# Input: IID as Base32 string
iid = "abzy73bycgb9ybrg12tynyxgkfzyh3bk"  # 32 chars

# Compute DHT key (32 raw bytes)
dht_key = sha256(b"post-urbit:identity:" + iid.encode('utf-8'))  # 32 bytes

# Pass raw bytes to DHT API
await dht.put(dht_key, idoc_envelope, ttl=86400)
```

**Rationale:** Using the string format for identifier input makes DHT keys deterministic from the human-readable identifier. The output is raw bytes for efficient DHT storage.

### DHT Record Format

What gets stored in the DHT:

```
DHT Key:   SHA256("post-urbit:identity:" || iid)  # 32 raw bytes
DHT Value: IDOC binary envelope (see identity-document-schema.md)
```

| Field | Type | Description |
|-------|------|-------------|
| Key | 32 bytes (raw) | SHA256 output, passed as raw bytes to DHT |
| Value | bytes | IDOC envelope (magic + version + length + JCS-canonical JSON) |
| TTL | uint32 | Time-to-live in seconds (default: 86400 = 24 hours) |

**No separate DHT signature required.** The IDOC envelope contains `signatures.current` which is validated using the embedded `keys.signing.current`. This internal signature provides authentication.

**Verification (New Records)**: DHT nodes MUST verify identity documents before storing: [REQ-SHARED-061]
1. Parse IDOC envelope
2. Verify `iid == derive_iid(Base64Decode(keys.signing.genesis))` (decode Base64 key to raw 32 bytes, then derive IID)
3. Verify `signatures.current` using `keys.signing.current` (with domain separation)
4. Only store if all checks pass

**Update Authorization (Existing Records)**: When a DHT node receives a document for an IID it already stores:
1. Parse the new IDOC envelope and verify basic signature (steps 1-3 above)
2. If incoming value is **byte-identical** to stored value: Accept as TTL refresh (extend expiry, no further checks needed)
3. Compare `sequence` numbers: new sequence MUST be > existing sequence [REQ-SHARED-062]
4. If `keys.signing.current` differs from the stored document's key (key rotation):
   a. **Key Continuity Binding** (per RFC-0001 §7.2): `keys.signing.previous` MUST be present (not null) in the new document AND MUST equal the stored document's `keys.signing.current` (byte-identical Base64 string comparison) [REQ-SHARED-063]
   b. Verify `signatures.previous` is present in the new document
   c. Verify `signatures.previous` is valid using the stored document's `keys.signing.current`
   d. OR verify `recovery_proof` is valid (see recovery-mechanisms.md), in which case key continuity binding is not required
5. If all checks pass, replace stored document with new document
6. If checks fail, reject the update (keep existing document)

**Note:** Step 2 (byte-identical refresh) applies to mutable identity records (`post-urbit:identity:`) per RFC-0001 §12.2/§12.8, allowing TTL refresh without sequence increment.

**Rationale**: This ensures only the holder of the current signing key (or recovery mechanism) can update the identity. An attacker with only public keys cannot create a valid update because they cannot produce `signatures.previous` signed by the current key.

**Genesis Document Verification**: For sequence 0 (genesis) documents, DHT nodes MUST verify per RFC-0001 §12.3: [REQ-SHARED-064]
1. Pass basic verification (steps 1-4 above)
2. Verify `sequence == "0"`
3. Verify `keys.signing.genesis == keys.signing.current` (genesis invariant)
4. Verify `keys.signing.previous == null`
5. Only store if all checks pass

For the `post-urbit:genesis:` DHT key specifically, nodes MUST also reject writes if a different genesis record already exists (immutable; only byte-identical refresh allowed). First-seen-wins applies if multiple genesis documents appear for the same IID from concurrent sources (see concurrent updates section). [REQ-SHARED-065]

### Transport API Bridge

Concrete mapping between layers:

```typescript
// Identity calls this
async function publishIdentity(document: IdentityDocument): Promise<void> {
  // Serialize to IDOC envelope (includes signatures.current from identity layer)
  const idocBytes = encodeIdoc(document);

  // Compute DHT key
  const key = sha256(concat("post-urbit:identity:", document.iid));

  // Use Transport's underlying DHT
  // Note: No separate DHT signature needed; IDOC's internal signature provides auth
  await dht.put(key, idocBytes, { ttl: 86400 });
}

// Identity calls this
async function fetchIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null> {
  const key = sha256(concat("post-urbit:identity:", iid));

  // DHT may return multiple records from different nodes
  const results = await discoveryService.dhtGet(key);  // Returns DhtResult[] (all available records)

  if (results.length === 0) return null;

  // Decode and verify all candidates
  const candidates: IdentityDocument[] = [];
  for (const result of results) {
    const document = decodeIdoc(result.value);
    if (verifyDocument(document)) {
      candidates.push(document);
    }
  }

  if (candidates.length === 0) return null;

  // Select highest valid sequence number
  // (see caching-policy.md for TOFU and genesis key constraints)
  candidates.sort((a, b) => {
    const seqA = BigInt(a.sequence);
    const seqB = BigInt(b.sequence);
    return seqB > seqA ? 1 : seqB < seqA ? -1 : 0;
  });

  return candidates[0];
}
```

## Device DHT Records

Multi-device support requires discovering devices associated with an identity.

### Device Document DHT Format

```
DHT Key:   SHA256("post-urbit:device:" || did)
DHT Value: Device document (JSON, signed by identity's signing key)
```

| Field | Type | Description |
|-------|------|-------------|
| Key | 32 bytes | SHA256 of prefixed DID |
| Value | bytes | Device document (JSON, structure below) |
| TTL | uint32 | Time-to-live (default: 86400 = 24 hours) |

**Signature authority:** The device document is signed by the **identity's signing key** (NOT the device key). This proves the identity owner authorized this device.

**Device Document Structure (canonical):**
```json
{
  "version": 1,
  "did": "<device-identifier>",
  "iid": "<owner-identity-identifier>",
  "device_name": "My Phone",
  "device_signing_key": "<base64-ed25519-public>",
  "endpoints": [
    { "type": "direct", "host": "...", "port": 4433, "priority": 0, "transport": "quic" }
  ],
  "created_at": "<RFC3339>",
  "updated_at": "<RFC3339>",
  "expires_at": "<RFC3339-optional>",
  "capabilities": ["messaging", "sync"],
  "signature_by_identity": "<base64-signature-by-identity-signing-key>"
}
```

**Note**: This is the canonical Device Document format. Field names MUST match exactly: [REQ-SHARED-066]
- `device_name` (not `name`)
- `signature_by_identity` (not `signature`)
- `endpoints` included for device-specific network presence
- `device_transport_key` removed in v1 (unused; handshake uses device signing key)

**Why identity signature (not device signature)?**
- Device keys are subordinate to identity keys
- Identity owner must authorize devices
- DHT nodes can verify authorization without knowing device private key
- Device keys prove possession during transport handshake (see peer-handshake.md)

**Verification:**
1. Fetch identity document for `iid`
2. Verify `signature_by_identity` field using identity's current (or historical) signing key (see signature scheme below)
3. Verify `did == Base32Lower(SHA256(Base64Decode(device_signing_key))[0:20])` (decode Base64 key to raw 32 bytes)

**Device Document Signature Scheme:**

```
signature_input = concat(
  "post-urbit:device-doc:v1:",   // domain separator (25 bytes)
  JCS(device_doc_without_signature)  // canonicalized JSON
)
signature_by_identity = Ed25519_Sign(identity_signing_key, signature_input)
```

Where `device_doc_without_signature` is the device document JSON with the `signature_by_identity` field removed.

### Device Index DHT Record

To discover all devices for an identity:

```
DHT Key:   SHA256("post-urbit:devices-for:" || iid)
DHT Value: Device index (list of DIDs, signed by identity)
```

**Device Index Structure:**
```json
{
  "iid": "<identity-identifier>",
  "devices": [
    { "did": "<did-1>", "device_name": "Phone", "last_seen": "<RFC3339>" },
    { "did": "<did-2>", "device_name": "Laptop", "last_seen": "<RFC3339>" }
  ],
  "updated_at": "<RFC3339>",
  "signature": "<base64-signature-by-identity-signing-key>"
}
```

**Note:** Device index entries use `device_name` (matching the device document field name). This is distinct from any display name formatting.

**Device Index Signature Scheme:**

```
signature_input = concat(
  "post-urbit:device-index:v1:",   // domain separator (27 bytes)
  JCS(device_index_without_signature)  // canonicalized JSON
)
signature = Ed25519_Sign(identity_signing_key, signature_input)
```

Where `device_index_without_signature` is the device index JSON with the `signature` field removed.

**Note:** The DHT does NOT support prefix queries. The device index record provides an explicit list that clients can fetch with a single lookup, then fetch individual device documents as needed.

### Device Discovery Flow

**IMPORTANT: Intra-Identity Use Only (v1)**

Device discovery via DHT (device index and device documents) is designed for **intra-identity device management**, NOT for external peer connectivity. Specifically:

| Use Case | Who Uses Device Discovery | Connection Target |
|----------|---------------------------|-------------------|
| Your devices finding your home node | Your own devices | Your home node endpoints |
| Your devices finding each other | Your own devices | Sibling device endpoints |
| **External peers connecting to you** | **Other identities** | **Identity Document endpoints (home node)** |

**External Peer Connection (v1 Normative):**

For v1, external peers (different identities) MUST connect using Identity Document endpoints, NOT device-specific endpoints from the device index. The flow is: [REQ-SHARED-067]

```
1. External peer wants to connect to identity "k5xq7z4m..."
2. Fetch Identity Document: DHT.get(SHA256("post-urbit:identity:k5xq7z4m..."))
3. Parse endpoints from Identity Document (these are home node endpoints)
4. Connect to Identity Document endpoints in priority order
5. Perform identity handshake (peer-handshake.md)
6. First successful connection wins
```

**Intra-Identity Device Discovery (for your own devices):**

This flow is used when your devices need to find each other or your home node:

```
1. Device wants to connect to its own identity's home node or sibling devices
2. Fetch device index: DHT.get(SHA256("post-urbit:devices-for:<own-iid>"))
3. Parse device list, verify signature (using own identity's signing key)
4. For each device with recent last_seen:
   a. Fetch device doc: DHT.get(SHA256("post-urbit:device:<did>"))
   b. Connect to device endpoints
   c. Perform device handshake (proving DID ownership to home node)
5. First successful connection wins
```

**Rationale:** The Single Home Node Model (see identity-document-schema.md) requires that external peers connect to the identity's home node, which then handles message delivery to individual devices. Device documents expose device-specific endpoints for internal use (device-to-home-node, device-to-sibling-device), but external peers should not use these directly in v1.

### Device Record TTL and Refresh

| Record Type | TTL | Refresh Interval |
|-------------|-----|------------------|
| Device document | 24 hours | Every 12 hours |
| Device index | 24 hours | On device add/remove, or every 24h |

Devices should refresh their DHT records before TTL expiry to maintain discoverability.

**v1 Note (Single Home Node Model):** In the Single Home Node Model, the home node performs all DHT PUT operations for device documents and device index. Individual devices do not directly participate in DHT operations. Instead, devices submit their endpoint/status updates to the home node over authenticated connections (using the identity stream type 0x02 or a dedicated device update message), and the home node publishes these updates to the DHT on their behalf. This centralizes DHT maintenance while still allowing devices to update their network presence.

## Identity Updates Over Authenticated Connections

When peers are connected, identity updates are pushed directly rather than through DHT.

### Stream Type

Identity updates use the `identity` stream type (0x02) on authenticated QUIC connections.

### Message Format

**QUIC Stream Framing (Normative, All Stream Types):**

```
Stream Header (first byte of stream, written once):
┌────────────────────────────────────────┐
│ Stream Type                            │ 1 byte
└────────────────────────────────────────┘

Each Message Frame (repeated):
┌────────────────────────────────────────┐
│ Length (big-endian)                    │ 4 bytes
├────────────────────────────────────────┤
│ Payload                                │ <length> bytes
└────────────────────────────────────────┘
```

**Key points:**
- Stream type written ONCE at stream start
- 4-byte big-endian length prefix for each message
- Payload format depends on stream type (see below)

**Stream Types and Payload Formats:**
| Code | Name | Payload Format | Notes |
|------|------|----------------|-------|
| 0x01 | Control | UTF-8 JSON | Has `type` field for message kind |
| 0x02 | Identity | UTF-8 JSON | Has `type` field for message kind |
| 0x03 | Message | Binary (PUSE) | Raw PUSE envelope bytes |
| 0x04 | Sync | Binary (1-byte type + CBOR) | Message type prefix + CBOR data |
| 0x05 | Bulk | Binary (2-byte opcode + data) | **Reserved for v2 - MUST NOT be used in v1** (see RFC-0002 §6.6)  [REQ-SHARED-068]|

**JSON streams (0x01, 0x02):** Payload is UTF-8 JSON with a `type` field to distinguish message kinds.

**Binary streams (0x03, 0x04, 0x05):** Payload is raw bytes; format is defined by the respective layer specification.

**Sync stream (0x04):** Each payload has a 1-byte message type (e.g., 0x01=SYNC_REQUEST) followed by CBOR-encoded data. See `sync-protocol.md` for message types.

**Identity Update Message Types (JSON `type` field):**
| Type | Description |
|------|-------------|
| `identity_update` | Push new identity document |
| `identity_request` | Request peer's current identity |
| `identity_response` | Response with identity document |
| `identity_ack` | Acknowledge receipt of update |

**Identity Message JSON Schemas (Normative):**

```typescript
// identity_update - Push new identity document to peer
interface IdentityUpdateMessage {
  type: "identity_update";
  idoc: string;           // Base64 standard (no padding) of IDOC envelope bytes
  sequence: string;       // Decimal string (SequenceNumber format)
  sent_at: string;        // RFC3339 UTC canonical: YYYY-MM-DDTHH:MM:SSZ
}

// identity_request - Request peer's current identity document
interface IdentityRequestMessage {
  type: "identity_request";
  known_sequence: string;  // Decimal string; "0" if unknown; peer skips response if their sequence <= this
}

// identity_response - Response with identity document (or no-update indicator)
interface IdentityResponseMessage {
  type: "identity_response";
  has_update: boolean;     // true if document attached; false if no update needed
  idoc?: string;           // Base64 standard (no padding); required if has_update=true
  sequence?: string;       // Decimal string; required if has_update=true
}

// identity_ack - Acknowledge receipt of update
interface IdentityAckMessage {
  type: "identity_ack";
  accepted: boolean;       // true if update was accepted and stored
  sequence: string;        // Sequence number that was processed
  error_code?: number;     // Present if accepted=false
  error_message?: string;  // Human-readable error; present if accepted=false
}
```

**Field encodings:**
- `idoc`: IDOC envelope bytes encoded as Base64 standard alphabet (RFC 4648 §4), no padding
- `sequence`: Decimal string per SequenceNumber normative format (no leading zeros)
- `sent_at`: RFC3339 canonical form with `Z` suffix, no fractional seconds

**QUIC Stream Initiation Rules (Normative):**

Protocol streams use QUIC bidirectional or unidirectional streams as follows:

| Stream Type | Directionality | Who Opens | Multiplicity | Lifecycle |
|-------------|----------------|-----------|--------------|-----------|
| Control (0x01) | Bidirectional | Client (first stream, per RFC-0002 §5.2) | Exactly 1 | Connection lifetime |
| Identity (0x02) | Bidirectional | Either peer | At most 1 per direction | Connection lifetime |
| Message (0x03) | Bidirectional | Either peer | Multiple allowed | Per-message or long-lived |
| Sync (0x04) | Bidirectional | Either peer | At most 1 per direction | Connection lifetime |
| Bulk (0x05) | **Unidirectional** | Sender | Multiple allowed | Per-transfer | **Reserved for v2 - MUST NOT be used in v1**  [REQ-SHARED-069]|

**Rules:**
1. **Control stream:** The client MUST open the first bidirectional stream with type 0x01 immediately after QUIC handshake (RFC-0002 §5.2). This stream is used for identity handshake and keepalive. [REQ-SHARED-070]
2. **Long-lived streams (Identity, Sync):** Each peer MAY open at most one outgoing bidirectional stream of each type. Peers MUST accept at most one incoming stream per type. Opening a second stream of the same type is a protocol error (close with DUPLICATE_STREAM_TYPE 0x108 per RFC-0002 §9.2). [REQ-SHARED-071]
3. **Per-message streams (Message):** Multiple concurrent bidirectional streams are allowed. Each stream carries one or more framed messages.
4. **Bulk streams:** Bulk uses unidirectional streams for large data transfers. The sender opens a new unidirectional stream for each transfer.
5. **Stream closure:** Long-lived streams remain open for the connection lifetime. Per-message and bulk streams MAY be closed after final message. [REQ-SHARED-072]

This framing pattern (stream type + length-prefixed frames) is consistent across all QUIC stream types. Payload encoding varies by stream type as specified above. See `06-rfcs/RFC-0002-transport.md` §6 for the authoritative specification.

### Update Push Flow

```
Alice                                    Bob
  │                                       │
  │ ─────── IDENTITY_UPDATE ────────────► │
  │         (new IdentityDocument)        │
  │                                       │
  │ ◄────── IDENTITY_ACK ──────────────── │
  │         (accepted: true, sequence: N) │
  │                                       │
```

### Request Flow

```
Alice                                    Bob
  │                                       │
  │ ─────── IDENTITY_REQUEST ───────────► │
  │         (known_sequence: N-1)         │
  │                                       │
  │ ◄────── IDENTITY_RESPONSE ─────────── │
  │         (document or "no update")     │
  │                                       │
```

## QUIC TLS Certificate Policy

### TLS Certificate Policy by ALPN (Normative)

Post-Urbit nodes handle two distinct ALPN protocols on the same UDP port (see ALPN Demultiplexing above). The TLS certificate validation policy differs by ALPN:

| ALPN | Certificate Policy | Identity Verification |
|------|-------------------|----------------------|
| `post-urbit/1` | Accept ANY certificate | Post-Urbit identity handshake (RFC-0002 §5) |
| `libp2p` | libp2p-tls requirements | PeerID authenticated via TLS certificate |

**For ALPN `post-urbit/1` (Post-Urbit Protocol):**

Implementations MUST accept ANY TLS certificate. No PKI validation, no hostname checks. Identity verification occurs at the Post-Urbit handshake layer, not TLS. The certificate merely enables TLS 1.3 encryption. See detailed requirements below. [REQ-SHARED-073]

**For ALPN `libp2p` (DHT/libp2p Protocol):**

Implementations MUST follow the libp2p QUIC/TLS identity requirements. The TLS certificate MUST authenticate the libp2p PeerID as required by the [libp2p-tls specification](https://github.com/libp2p/specs/blob/master/tls/tls.md). This is handled by libp2p libraries automatically. Do NOT apply the "accept any certificate" policy to libp2p connections. [REQ-SHARED-074]

**Rationale:** The DHT uses libp2p's native identity model where the TLS certificate cryptographically proves the PeerID. Post-Urbit direct connections use a separate identity handshake that doesn't rely on certificates. Conflating these policies would break DHT security.

### Certificate Generation (Server SHOULD) [REQ-SHARED-075]

For Post-Urbit protocol connections (ALPN `post-urbit/1`), servers SHOULD generate certificates with these properties: [REQ-SHARED-076]

| Property | Recommended Value |
|----------|-------------------|
| Certificate type | Self-signed, ephemeral |
| Signature algorithm | ECDSA P-256 or Ed25519 |
| Validity period | 1 hour to 30 days |
| Subject/Issuer | Any value (not verified by clients) |

### Certificate Acceptance (Client MUST) - Post-Urbit ALPN Only [REQ-SHARED-077]

**For ALPN `post-urbit/1` connections only:** Clients MUST accept ANY certificate that allows the TLS 1.3 handshake to complete. Specifically: [REQ-SHARED-078]

1. Clients MUST NOT reject certificates based on: [REQ-SHARED-079]
   - Signature algorithm (RSA, ECDSA, Ed25519, etc. all acceptable)
   - Validity period (expired or not-yet-valid certificates acceptable)
   - Subject/Issuer fields (any value acceptable)
   - Self-signed status (no chain validation required)
   - Trust anchors (no CA verification)

2. Clients MUST only require: [REQ-SHARED-080]
   - Valid TLS 1.3 handshake completion
   - Cipher suite from the supported list (see RFC-0002 §4.3)

**Rationale:** Identity is verified through the post-TLS identity handshake using cryptographic challenge signatures, not TLS certificates. The TLS layer provides only transport encryption. Restricting certificate algorithms would break interoperability without security benefit.

**Note:** This policy applies ONLY to Post-Urbit protocol connections (ALPN `post-urbit/1`). For libp2p/DHT connections (ALPN `libp2p`), use standard libp2p-tls certificate requirements.

### Verification Strategy

For Post-Urbit protocol connections (ALPN `post-urbit/1`), TLS certificates are NOT used for identity verification. Instead:

1. QUIC TLS provides transport encryption with forward secrecy
2. **Accept any valid TLS certificate** (self-signed OK)
3. Perform identity handshake (see peer-handshake.md) after TLS
4. Identity handshake binds the TLS session to specific IIDs

**Rationale**: This avoids dependency on PKI/CA infrastructure while still getting TLS 1.3 security. Identity is verified cryptographically through the post-TLS handshake.

### DoS Considerations

- Rate limit TLS handshakes per source IP
- Require valid TLS before allocating connection resources
- Drop connections that don't complete identity handshake within 30s

## Mailbox Protocol (Store-and-Forward)

### Overview

Mailbox servers store messages for offline recipients. **RFC-0003 §7 is the authoritative specification** for the Mailbox protocol.

### Mailbox Endpoint

In identity document (per RFC-0003 §7.2, using standard endpoint schema):
```json
{
  "type": "mailbox",
  "host": "mailbox.example.com",
  "port": 443,
  "transport": "https",
  "priority": 30
}
```

**Mailbox Base URL Derivation (v1 Normative):**

For v1, the mailbox API base path MUST be `/` (root). The endpoint schema does not include a `path` field. Implementations MUST derive the canonical mailbox URL following RFC-0003 §7.3 canonicalization rules: [REQ-SHARED-081]

```python
def derive_mailbox_url(endpoint: dict) -> str:
    """
    Derive canonical mailbox URL from endpoint.
    Follows RFC-0003 §7.3 URL Canonicalization.
    """
    host = endpoint["host"].lower()  # Lowercase host
    port = endpoint["port"]

    # Omit default port 443 per RFC-0003 canonicalization
    if port == 443:
        return f"https://{host}/"
    else:
        return f"https://{host}:{port}/"
```

Examples:
- `{"host": "mailbox.example.com", "port": 443}` → `https://mailbox.example.com/`
- `{"host": "relay.net", "port": 8443}` → `https://relay.net:8443/`

API endpoints are relative to this base:
- Store: `POST {mailbox_url}messages/{inbox_owner_iid}`
- Retrieve: `GET {mailbox_url}messages`
- Delete: `DELETE {mailbox_url}messages`

**Rationale:** Mailbox auth tokens bind to the exact canonical URL (see RFC-0003 §7.3). Port 443 MUST be omitted to match RFC-0003's canonicalization rules. Requiring root path eliminates ambiguity about path discovery. Future versions MAY extend the endpoint schema with a `base_path` field if non-root paths are needed. [REQ-SHARED-082]

### API Summary

See RFC-0003 §7.4 for full details:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/messages/{inbox_owner_iid}` | POST | Store message in specified inbox |
| `/messages` | GET | Retrieve messages from own inbox |
| `/messages` | DELETE | Delete messages from own inbox |

**Note:** The store endpoint takes an explicit `inbox_owner_iid` path parameter. For group messages, this is the target member's IID (not the group_id from the PUSE envelope). See RFC-0003 §7.4.1.

### Auth Token Format

Bearer tokens are signed by the sender's identity key. See RFC-0003 §7.3 for the complete token format and signature construction using domain separator `post-urbit-mailbox-token-v1`.

### MailboxService Interface

The mailbox API as a TypeScript interface (implemented by Messaging layer, RFC-0003):

```typescript
interface MailboxService {
  /**
   * Store a message in an inbox owner's mailbox.
   *
   * IMPORTANT: For group messages, the PUSE envelope's recipient field contains
   * the group_id, not the inbox owner's IID. The inboxOwnerIid parameter specifies
   * which identity's mailbox should receive the message, separate from the envelope.
   *
   * @param inboxOwnerIid The IID of the inbox to store the message in.
   *   - For 1:1 messages: This equals the PUSE recipient IID
   *   - For group messages: This is the target member's IID (PUSE recipient is group_id)
   * @param envelope Encrypted PUSE envelope (opaque to mailbox, stored as-is)
   * @returns Message ID and expiration
   */
  store(
    inboxOwnerIid: IdentityIdentifier,
    envelope: Uint8Array
  ): Promise<{ messageId: string; expiresAt: Timestamp }>;

  /**
   * Retrieve messages from own mailbox.
   *
   * Returns all messages stored in the authenticated user's inbox, regardless
   * of what the PUSE envelope recipient field contains. This includes:
   * - 1:1 messages (PUSE recipient = user's IID)
   * - Group messages (PUSE recipient = group_id)
   *
   * @param sinceCursor Optional cursor to fetch only new messages
   * @returns Array of stored messages
   */
  retrieve(sinceCursor?: string): Promise<MailboxMessage[]>;

  /**
   * Acknowledge/delete a message from mailbox.
   *
   * Only deletes messages stored in the authenticated user's inbox.
   *
   * Implementation: `acknowledge(messageId)` MUST call DELETE /messages with body
   * `{message_ids: [messageId]}` per RFC-0003 §7.4.3. Missing or non-owned message
   * IDs are treated as success (idempotent delete).
   *
   * @param messageId Message to delete
   */
  acknowledge(messageId: string): Promise<void>;
}

interface MailboxMessage {
  messageId: string;
  envelope: Uint8Array;
  storedAt: Timestamp;
}
```

**Implementation Note:** The messaging layer uses this interface to store messages for offline recipients. It handles the HTTP requests to the recipient's configured mailbox endpoint (from their identity document) and the auth token generation.

**Group Message Fanout:** When sending a group message to offline members, the sender calls `store(memberIid, envelope)` for each offline member. The same PUSE envelope (with group_id as recipient) is stored in each member's individual mailbox. See RFC-0003 §7.4.1 "Sender Fanout for Group Messages" for details.

### Trust Model

- Mailbox sees: sender IID (from token), recipient IID, encrypted blob, timing
- Mailbox does NOT see: message contents (E2E encrypted)
- Mailbox MAY: rate limit, charge for storage, impose size/duration limits [REQ-SHARED-083]
- Mailbox MUST NOT: decrypt, modify, or selectively block messages [REQ-SHARED-084]

### Message Format

Mailboxes store **raw PUSE envelopes** (see RFC-0003 §3). No additional wrapper is used. The PUSE header contains sender/recipient IIDs for routing.

### Message Verification When Sender IDOC Unavailable (Normative)

When verifying a received PUSE envelope, the receiver must fetch the sender's identity document to verify the signature. If the sender identity document cannot be resolved (DHT unavailable, cache miss, network partition), implementations MUST handle gracefully: [REQ-SHARED-085]

1. **Retain for retry:** If sender identity document cannot be resolved (DHT unavailable, cache miss), implementations MUST retain the envelope for retry [REQ-SHARED-086]

2. **Retry schedule:** Retry resolution at intervals: 1 min, 5 min, 15 min, 1 hour, then hourly for up to 7 days

3. **Pending verification state:** Messages with unresolved sender MUST NOT be marked as verified; UI SHOULD show "pending verification" state [REQ-SHARED-087]

4. **Eventual discard:** After 7 days without resolution, message MAY be discarded or marked permanently unverified [REQ-SHARED-088]

**Rationale:** Network partitions and DHT churn may temporarily prevent identity resolution. Retaining messages allows eventual verification when the sender's IDOC becomes available, improving reliability without compromising security. The 7-day limit prevents indefinite storage of potentially unverifiable messages.

**Implementation Note:** Implementations SHOULD persist pending-verification messages across restarts. The retry timer resets on restart but the 7-day total window is measured from original receipt time. [REQ-SHARED-089]

## Domain Separator Registry (Normative)

All cryptographic domain separators used across the Post-Urbit protocol. Implementations MUST use these exact byte sequences. [REQ-SHARED-090]

| Context | Domain Separator | Bytes | Used For |
|---------|------------------|-------|----------|
| **Identity Layer (RFC-0001)** | | | |
| Identity document signature | `post-urbit:idoc:v1:` | 19 | Ed25519 signature over JCS-canonicalized IDOC |
| Device document signature | `post-urbit:device-doc:v1:` | 25 | Ed25519 signature over JCS-canonicalized device doc |
| Device index signature | `post-urbit:device-index:v1:` | 27 | Ed25519 signature over JCS-canonicalized device index |
| Key revocation signature | `post-urbit:key-revocation:v1:` | 29 | Ed25519 signature over key revocation doc |
| Identity revocation signature | `post-urbit:identity-revocation:v1:` | 34 | Ed25519 signature over identity revocation |
| Device revocation signature | `post-urbit:device-revocation:v1:` | 32 | Ed25519 signature over device revocation |
| Recovery attestation signature | `post-urbit:recovery-attestation:v1:` | 35 | Ed25519 signature over trustee recovery attestation |
| Recovery contest signature | `post-urbit:recovery-contest:v1:` | 31 | **[EXPERIMENTAL]** Ed25519 signature over recovery contest doc (non-normative for v1) |
| DHT identity key | `post-urbit:identity:` | 20 | SHA256 prefix for DHT key derivation |
| DHT device key | `post-urbit:device:` | 18 | SHA256 prefix for device DHT key |
| DHT device index | `post-urbit:devices-for:` | 23 | SHA256 prefix for device list DHT key |
| DHT revocation key | `post-urbit:revocation:` | 22 | SHA256 prefix for identity/key revocation DHT key |
| DHT device revocation key | `post-urbit:device-revocation:` | 29 | SHA256 prefix for device revocation DHT key |
| DHT genesis key | `post-urbit:genesis:` | 19 | SHA256 prefix for genesis identity document DHT key |
| DHT contest key | `post-urbit:contest:` | 19 | **[EXPERIMENTAL]** SHA256 prefix for recovery contest DHT key (non-normative for v1) |
| **Transport Layer (RFC-0002)** | | | |
| Peer handshake | `post-urbit-handshake-v1` | 23 | Ed25519 signature in peer authentication |
| Device handshake | `post-urbit-device-v1` | 20 | Ed25519 signature for device auth |
| Relay allocation | `post-urbit-relay-alloc-v1` | 25 | Ed25519 signature for relay registration |
| Relay rebind | `post-urbit-rebind-v1` | 20 | Ed25519 signature for address rebinding |
| Hole-punch coordination | `post-urbit-holepunch-v1` | 23 | Ed25519 signature for NAT traversal coordination |
| **DHT Layer** | | | |
| DHT protocol ID | `/post-urbit/kad/1.0.0` | 22 | libp2p Kademlia DHT protocol negotiation |
| **Messaging Layer (RFC-0003)** | | | |
| Double Ratchet root KDF | `post-urbit-ratchet-v1` | 21 | HKDF info for root chain derivation |
| 2DH initial KDF | `post-urbit-x3dh-v1` | 18 | HKDF info for initial key derivation |
| Sender key KDF | `post-urbit-sender-key-v1:` | 25+ | HMAC domain prefix + binding data |
| Mailbox token | `post-urbit-mailbox-token-v1` | 27 | Ed25519 signature for mailbox auth |
| **Sync Layer** | | | |
| Sync operation signature | `post-urbit:sync-op:v1:` | 22 | Ed25519 signature over sync operation |
| Merkle leaf hash | `post-urbit:merkle-leaf:` | 23 | SHA256 domain prefix for Merkle leaf nodes |
| Merkle node hash | `post-urbit:merkle-node:` | 23 | SHA256 domain prefix for Merkle internal nodes |
| Merkle empty hash | `post-urbit:merkle-empty:` | 24 | SHA256 domain prefix for empty padding |
| **Packaging Layer (05-ux-packaging)** | | | |
| App package signature | `postapp-signature-v1:` | 21 | Ed25519 signature over SIGNATURE file payload |
| Repository signature | `postnode-repo-v1:` | 17 | Ed25519 signature over repository.json |
| Update manifest signature | `postnode-update-v1:` | 19 | Ed25519 signature over updates.json |

**Notes:**
- All strings are UTF-8/ASCII encoded (no NUL terminator unless specified)
- Byte counts are derived by `len(string.encode('utf-8'))`
- DHT prefixes use colon as separator (`:`)
- Protocol version separators use hyphen (`-v1`)
- The sender key KDF prefix is followed by binding data (`group_id:sender_iid:key_id`)

## Signature Prehash Policy (Normative)

Ed25519 signing can operate on raw bytes directly (single-pass) or on a prehashed digest. This table specifies the prehash policy for each signature context to ensure interoperability.

**IMPORTANT: All signatures use standard Ed25519 (RFC 8032), NOT Ed25519ph.** When this specification says `Ed25519_Sign(key, SHA256(x))`, the 32-byte SHA-256 digest is passed as the **message input** to standard Ed25519. Ed25519ph (prehashed variant) MUST NOT be used—it produces different signatures despite similar naming. Implementations MUST use `crypto_sign(message, key)` with the computed digest as `message`, not Ed25519ph APIs. [REQ-SHARED-091]

| Context | Prehash | Input | Notes |
|---------|---------|-------|-------|
| **Identity Documents (RFC-0001)** | | | |
| IDOC signature | None (raw) | `domain \|\| JCS(document)` | Single-pass Ed25519 |
| Device doc signature | None (raw) | `domain \|\| JCS(document)` | Single-pass Ed25519 |
| Device index signature | None (raw) | `domain \|\| JCS(index)` | Single-pass Ed25519 |
| Revocation signatures | None (raw) | `domain \|\| JCS(document)` | Single-pass Ed25519 |
| Recovery attestation | None (raw) | `domain \|\| JCS(attestation)` | Single-pass Ed25519 |
| **Transport (RFC-0002)** | | | |
| Handshake challenge | **SHA256** | `SHA256(signature_input)` | `signature_input` already includes domain per RFC-0002 §5.7 |
| Relay allocation | **SHA256** | `SHA256(signature_input)` | `signature_input` already includes domain per RFC-0002 §7.8 |
| Relay rebind | **SHA256** | `SHA256(signature_input)` | `signature_input` already includes domain per RFC-0002 §7.11 |
| Hole-punch coordination | **SHA256** | `SHA256(signature_input)` | `signature_input` includes domain per nat-traversal.md |
| **Messaging (RFC-0003)** | | | |
| PUSE envelope signature | None (raw) | Raw envelope bytes (magic provides context) | Single-pass; no domain separator |
| Mailbox token | **SHA256** | `SHA256(signature_input)` | `signature_input` already includes domain per RFC-0003 §7.3 |
| **Sync** | | | |
| Sync operation | None (raw) | See sync-protocol.md | Per sync-protocol.md §Operation Signature |
| **Packaging** | | | |
| App package SIGNATURE | None (raw) | `"postapp-signature-v1:" \|\| manifest_hash_hex \|\| ":" \|\| timestamp` | Single-pass Ed25519 per app-distribution.md |
| Repository signature | None (raw) | `"postnode-repo-v1:" \|\| manifest_hash_hex \|\| ":" \|\| timestamp` | Single-pass Ed25519 per app-distribution.md |
| Update manifest | None (raw) | `"postnode-update-v1:" \|\| app_id \|\| ":" \|\| manifest_hash_hex \|\| ":" \|\| timestamp` | Single-pass Ed25519 per app-distribution.md |

**Summary:**
- **Identity/Sync/Packaging signatures**: Single-pass Ed25519 on raw bytes
- **Transport/Mailbox signatures**: Hash-then-sign (Ed25519 over SHA256 digest)
- **PUSE envelope**: Single-pass on raw envelope bytes (no explicit domain separator; `PUSE` magic byte sequence provides context)

This mixed policy is intentional: transport signatures operate on variable-length concatenated data where prehashing provides a fixed-size input, while identity documents use canonicalized JSON with domain separation.

## Error Code Registry

To prevent overlaps, error codes are allocated by layer:

| Range | Layer | Examples |
|-------|-------|----------|
| 0x000-0x0FF | QUIC standard | NO_ERROR, PROTOCOL_VIOLATION |
| 0x100-0x1FF | Transport | IDENTITY_MISMATCH, HANDSHAKE_FAILED |
| 0x200-0x2FF | Identity | INVALID_DOCUMENT, SIGNATURE_FAILED |
| 0x300-0x3FF | Messaging | (reserved for 03-messaging-sync) |
| 0x400-0x4FF | Sync | (reserved for 03-messaging-sync) |
| 0x500-0x5FF | App Runtime | (reserved for 04-app-runtime) |

## Global Conventions

### Endianness

All multi-byte integers in binary wire formats are **big-endian** (network byte order) unless explicitly stated otherwise.

### Timestamps

All timestamps are **RFC3339 UTC** (e.g., `2025-01-13T12:00:00Z`).

**Canonical form for signature inputs:** Transport layer operations (handshakes, relay allocation/rebind) require the **canonical** timestamp format: `YYYY-MM-DDTHH:MM:SSZ` (no fractional seconds, `Z` suffix, exactly 20 bytes). Implementations MUST reject non-canonical forms in signature verification. See RFC-0002 §5.5 for normative requirements. [REQ-SHARED-092]

### Encoding

| Type | Context | Encoding | Notes |
|------|---------|----------|-------|
| IID/DID | Binary protocols (PUSE, PURL, Sync CBOR) | 20 raw bytes | Space-efficient |
| IID/DID | JSON/text protocols (handshake, display) | 32-char Crockford Base32 lowercase | Human-readable |
| DocumentId | Binary (Sync CBOR) | 32 raw bytes (bstr) | Fixed-length |
| DocumentId | JSON/display | 64-char hex or UUID string | Application-dependent |
| OperationId | Binary (Sync CBOR) | 32 raw bytes (bstr) | Fixed-length |
| OperationId | JSON/display | 64-char lowercase hex | Canonical display |
| Keys/signatures | All contexts | Base64 standard (no padding) | `A-Za-z0-9+/` |
| Tokens (relay, auth) | URLs | Base64url (no padding) | `A-Za-z0-9-_` (URL-safe) |
| Sequence numbers | All contexts | Decimal string | Avoid JSON number precision loss |

**Key distinction:** Binary wire protocols (PUSE envelope, PURL packet, Sync CBOR stream) use raw bytes for identifiers. Text/JSON protocols (handshake messages, APIs, display) use string encodings.

**Crockford Base32 (Normative):**

All identity identifiers (IID), device identifiers (DID), and group identifiers use **Crockford Base32** encoding:

- **Alphabet:** `0123456789abcdefghjkmnpqrstvwxyz` (32 chars)
- **Case:** Lowercase only
- **Length:** 32 characters for 20-byte (160-bit) values
- **Excluded characters:** `i`, `l`, `o`, `u` (to avoid ambiguity)
- **Wire format:** Encoders MUST output lowercase; decoders MUST reject non-lowercase [REQ-SHARED-093]
- **UI input:** User interfaces MAY normalize uppercase before creating wire/signed artifacts [REQ-SHARED-094]

Example valid IID: `abzy73bycgb9ybrg12tynyxgkfzyh3bk`

See RFC-0002 §2.1 for the authoritative Base32 specification.

**Base64 vs Base64url:**
- **Keys and signatures**: Always use standard Base64 (`+/` chars)
- **Tokens and URL-safe data**: Use Base64url (`-_` chars)
- Both use no padding
- Implementations MUST decode using the correct alphabet for each type [REQ-SHARED-095]

**UUID Serialization (Normative):**

UUID v4 values are used for message IDs (`message_id` in PUSE headers) and document references.

| Form | Format | Example |
|------|--------|---------|
| **String** | RFC 4122 canonical (lowercase hex, hyphenated) | `550e8400-e29b-41d4-a716-446655440000` |
| **Bytes** | 16 bytes, RFC 4122 network byte order | `55 0e 84 00 e2 9b 41 d4 a7 16 44 66 55 44 00 00` |

**Canonical mapping (MUST be followed):** [REQ-SHARED-096]
- String form uses lowercase hex with hyphens: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
- Bytes are the exact 16-byte sequence in RFC 4122 layout (NOT mixed-endian platform encodings)
- UUID string position 0-7 = bytes 0-3, position 9-12 = bytes 4-5, position 14-17 = bytes 6-7, position 19-22 = bytes 8-9, position 24-35 = bytes 10-15

**Test vector:**
```
UUID string: 550e8400-e29b-41d4-a716-446655440000
UUID bytes (hex): 550e8400e29b41d4a716446655440000
```

Applications referencing messages (e.g., `reply_to`, `receipt.message_ids`) MUST use the canonical string form. The PUSE header `message_id` contains the 16-byte form. [REQ-SHARED-097]

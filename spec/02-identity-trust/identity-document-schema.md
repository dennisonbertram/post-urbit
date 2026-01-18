# Identity Document Schema

## Overview

The Identity Document is the canonical representation of a user's identity. It contains public keys, service endpoints, and update rules. It is self-certifying: the document is signed by the keys it contains.

## Identity Identifier (IID)

The **Identity Identifier (IID)** is the stable, long-term identifier for an identity.

```
IID = Base32Lower(SHA-256(genesis_signing_public_key_raw_bytes)[0:20])
```

### Encoding Specification

| Aspect | Specification |
|--------|---------------|
| **Base32 alphabet** | Crockford Base32: `0123456789abcdefghjkmnpqrstvwxyz` |
| **Padding** | No padding |
| **Input to hash** | Raw 32-byte Ed25519 public key (NOT DER/SPKI) |
| **Hash output** | First 20 bytes (160 bits) of SHA-256 |
| **Length** | 32 characters |
| **Normalization** | Always lowercase; reject non-canonical forms |
| **Valid characters** | `0-9a-hj-km-np-tv-z` (Crockford excludes `i`, `l`, `o`, `u`) |

- **Example**: `b1anasr5h0bj3832xqexwy0f0987e1xb`
- **Invalid examples**: `hello_world` (contains invalid chars `i`, `l`, `o`)
- **Derivation**: Hash of the FIRST (genesis) signing key ever used
- **Immutable**: Never changes, even after key rotation

See RFC-0002 §2.1 and `00-shared/layer-integration.md` for the authoritative Base32 specification.

### IID Derivation Algorithm

```python
# Crockford Base32 alphabet (excludes i, l, o, u)
CROCKFORD_ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz"

def crockford_encode(data: bytes) -> str:
    """Encode bytes to Crockford Base32 lowercase string."""
    result = []
    bits = 0
    buffer = 0
    for byte in data:
        buffer = (buffer << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            result.append(CROCKFORD_ALPHABET[(buffer >> bits) & 0x1F])
    if bits > 0:
        result.append(CROCKFORD_ALPHABET[(buffer << (5 - bits)) & 0x1F])
    return "".join(result)

def derive_iid(genesis_signing_public_key_raw: bytes) -> str:
    assert len(genesis_signing_public_key_raw) == 32  # Raw Ed25519 pubkey
    hash_bytes = sha256(genesis_signing_public_key_raw)
    truncated = hash_bytes[:20]  # First 160 bits
    return crockford_encode(truncated)
```

### Why Genesis Key Hash?

- No external registry needed to establish identity
- Deterministic: anyone can verify the IID from the genesis key
- Rotation-safe: IID persists across key changes
- Collision-resistant: 160-bit security margin
- **Verifiable forever**: Genesis key is preserved in document

## Document Structure

```json
{
  "version": 1,
  "iid": "<base32-lowercase-identity-identifier>",
  "sequence": "<uint64-as-decimal-string>",
  "timestamp": "<RFC3339-timestamp-UTC>",
  "keys": {
    "signing": {
      "genesis": "<base64-raw-ed25519-public-key-32-bytes>",
      "current": "<base64-raw-ed25519-public-key-32-bytes>",
      "previous": "<base64-raw-ed25519-public-key-32-bytes>|null",
      "history": []
    },
    "encryption": {
      "current": "<base64-raw-x25519-public-key-32-bytes>",
      "previous": [
        {
          "key": "<base64-raw-x25519-public-key>",
          "valid_from": "<sequence-when-became-current>",
          "valid_until": "<sequence-when-rotated-out>",
          "expires_at": "<RFC3339-timestamp>"
        }
      ]
    }
  },
  "endpoints": [
    {
      "type": "direct",
      "host": "<hostname-ipv4-or-ipv6>",
      "port": 4433,
      "priority": 10,
      "transport": "quic"
    },
    {
      "type": "relay",
      "host": "relay.example.com",
      "port": 4433,
      "priority": 20,
      "transport": "quic",
      "relay_id": "<relay-identity-id>"
    },
    {
      "type": "mailbox",
      "host": "mailbox.example.com",
      "port": 443,
      "priority": 30,
      "transport": "https"
    }
  ],
  "recovery": {
    "method": "<recovery-method-type>",
    "config": { <method-specific-config> }
  },
  "claims": {
    "name": "<optional-display-name>",
    "avatar": "<optional-content-hash>",
    "bio": "<optional-short-bio>"
  },
  "extensions": { <optional-app-specific-data> },
  "recovery_proof": null,
  "signatures": {
    "current": "<base64-signature-by-current-signing-key>",
    "previous": "<base64-signature-by-previous-signing-key>|null"
  }
}
```

## Cryptographic Encoding Specification

All keys and signatures use these encodings:

| Type | Encoding | Decoded Size | Notes |
|------|----------|--------------|-------|
| Ed25519 public key | Base64 standard (no padding) | 32 bytes | Raw key bytes, NOT DER/SPKI |
| X25519 public key | Base64 standard (no padding) | 32 bytes | Raw key bytes |
| Ed25519 signature | Base64 standard (no padding) | 64 bytes | Raw R\|\|S bytes |

**Base64 Alphabet**: `A-Za-z0-9+/` (standard), no padding characters.

**Note:** Keys and signatures use Base64 (standard alphabet). IIDs and DIDs use Crockford Base32 - see Encoding Specification above.

## Field Specifications

**Wire Encoding Requirement (per RFC-0001 §6.6):** All top-level fields MUST be present in the wire encoding. This ensures byte-identical comparison for DHT TTL refresh and deterministic conflict detection. [REQ-ID-001]

### Top-Level Fields (All Wire-Required)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `version` | uint8 | — | Schema version (must be `1`) |
| `iid` | string | — | Identity Identifier, 32 chars Base32 lowercase |
| `sequence` | string | — | Monotonically increasing uint64 as decimal string |
| `timestamp` | string | — | RFC3339 UTC timestamp |
| `keys` | object | — | Signing and encryption keys |
| `endpoints` | array | `[]` | How to reach this identity's node (max 10) |
| `claims` | object | `{}` | Self-asserted metadata |
| `recovery` | object | — | Recovery configuration |
| `extensions` | object | `{}` | App-specific extensions (max 4KB) |
| `recovery_proof` | object\|null | `null` | Recovery proof when using recovery instead of key continuity |
| `signatures` | object | — | Document signatures |

### Nested Required Fields

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `keys.signing.genesis` | string | Base64, 32 bytes decoded | Genesis Ed25519 public key (NEVER changes) |
| `keys.signing.current` | string | Base64, 32 bytes decoded | Current Ed25519 public key |
| `keys.signing.previous` | string\|null | Base64, 32 bytes or null | Previous signing key (for rotation verification) |
| `keys.signing.history` | array | Max 10 entries | Historical signing keys with validity windows |
| `keys.encryption.current` | string | Base64, 32 bytes decoded | Current X25519 public key |
| `keys.encryption.previous` | array | See EncryptionKeyHistory | Previous encryption keys with validity windows |
| `signatures.current` | string | Base64, 64 bytes decoded | Ed25519 signature over canonical document |
| `signatures.previous` | string\|null | Base64, 64 bytes or null | Signature by previous key (required during rotation) |

### Field Presence Rules

- **Empty arrays** where no values exist: `"endpoints": []`, `"keys.signing.history": []`
- **Empty objects** where no values exist: `"claims": {}`, `"extensions": {}`
- **`null`** for truly absent optional nested fields: `"keys.signing.previous": null`, `"recovery_proof": null`

Verifiers MUST reject documents missing any required top-level field. [REQ-ID-002]

### Signing Key History Entry

Previous signing keys are retained to support signature verification for messages received after key rotation (e.g., mailbox delivery during offline periods):

```json
{
  "key": "<base64-raw-ed25519-public-key>",
  "valid_from": "5",
  "valid_until": "10",
  "expires_at": "2025-03-15T00:00:00Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `key` | string | Base64-encoded Ed25519 public key (32 bytes) |
| `valid_from` | string | Sequence number when this key became current |
| `valid_until` | string | Sequence number when this key was rotated out |
| `expires_at` | timestamp | Metadata for UI warnings and audit trails (see note below) |

**Note on `expires_at`:** The `expires_at` field is metadata for UI warnings and audit trails; it MUST NOT be used as a signature rejection criterion during verification. Verifiers MUST accept valid signatures from historical keys regardless of `expires_at`. UIs MAY display warnings for signatures made with keys past their `expires_at`, but the signature itself remains valid if cryptographically correct. [REQ-ID-003]

**Retention policy**: Keep at most **10 previous signing keys or 2 years**, whichever is less.

**Rationale for extended retention:**
- App package signatures may need verification long after signing (months/years)
- Mailbox-delivered messages may be delayed significantly (days/weeks)
- Verifiers can check `keys.signing.history` if `current`/`previous` don't match
- Historical IDOC versions may also be cached locally (see caching-policy.md)

**Storage impact**: ~500 bytes per historical key entry. 10 entries = ~5KB additional document size, acceptable tradeoff for verification flexibility.

### Encryption Key History Entry

Previous encryption keys are retained with validity windows to support decryption by offline peers:

```json
{
  "key": "<base64-raw-x25519-public-key>",
  "valid_from": "5",
  "valid_until": "10",
  "expires_at": "2025-03-15T00:00:00Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `key` | string | Base64-encoded X25519 public key (32 bytes) |
| `valid_from` | string | Sequence number when this key became current |
| `valid_until` | string | Sequence number when this key was rotated out |
| `expires_at` | timestamp | After this time, senders should not use this key |

**Retention policy**: Keep at most 5 previous keys or 30 days, whichever is less. Senders should prefer `current` key; fall back to `previous` only for replying to old messages.

### Endpoint Object (Normative, shared with Transport layer)

This is the canonical endpoint schema used by both Identity and Transport layers.

```json
{
  "type": "direct|relay|mailbox",
  "host": "<hostname-or-ip>",
  "port": 4433,
  "transport": "quic",
  "priority": 0,
  "relay_id": "<relay-iid-if-type-relay>",
  "observed_at": "<RFC3339-when-last-verified>",
  "metadata": {}
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | Yes | `direct`, `relay`, or `mailbox` |
| `host` | string | Yes | Hostname, IPv4, or `[IPv6]` (brackets for IPv6) |
| `port` | number | Yes | Service port (1-65535). Protocol determined by `transport`. |
| `transport` | string | No | `quic` (default, UDP) or `https` (TCP for mailbox) |
| `priority` | number | Yes | 0-255, lower = higher priority |
| `relay_id` | string | No | IID of relay operator (for relay type) |
| `observed_at` | timestamp | No | When this endpoint was last verified reachable |
| `metadata` | object | No | Type-specific additional data |

**Port interpretation by transport**:
- `quic`: Port is UDP (e.g., 4433 → UDP/4433)
- `https`: Port is TCP (e.g., 443 → TCP/443)

**Type-specific notes**:

| Type | Transport | Usage |
|------|-----------|-------|
| `direct` | quic | Direct QUIC connection to host:port (UDP) |
| `relay` | quic | QUIC via relay server (UDP), relay_id identifies relay |
| `mailbox` | https | Store-and-forward via HTTPS (TCP) |

**Priority**: Lower number = higher priority. Peers try endpoints in priority order.

**Host normalization**:
- Hostnames: lowercase, no trailing dot
- IPv4: standard dotted decimal
- IPv6: lowercase, bracketed `[2001:db8::1]`, no zone ID

### Recovery Configuration

```json
{
  "method": "none|social|device-escrow|threshold|provider",
  "config": { <method-specific> }
}
```

See `recovery-mechanisms.md` for detailed method specifications.

### Claims Object

```json
{
  "name": "<string, max 64 chars, UTF-8>",
  "avatar": "<content-addressed-hash, e.g., sha256:abc123...>",
  "bio": "<string, max 256 chars, UTF-8>"
}
```

**Note**: Claims are self-asserted, not verified. Display with appropriate UI caveats.

### Recovery Proof Object (when using recovery instead of key continuity)

When an identity is recovered without access to the previous signing key, the document includes a `recovery_proof` field:

```json
{
  "recovery_proof": {
    "method": "social|device-escrow|threshold|provider",
    "initiated_at": "<RFC3339-timestamp>",
    "cooldown_expires_at": "<RFC3339-timestamp>",
    "status": "pending|active|contested",
    "proof_data": {
      // Method-specific proof (attestations, signature, etc.)
    }
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `method` | string | Which recovery method was used |
| `initiated_at` | timestamp | When recovery was started |
| `cooldown_expires_at` | timestamp | When recovery becomes final |
| `status` | enum | `pending` (in cooldown), `active` (cooldown passed), `contested` |
| `proof_data` | object | Method-specific proof (see recovery-mechanisms.md) |

**Document with recovery proof**: `signatures.previous` is null; `recovery_proof` substitutes for key-continuity proof.

## Device Identifiers (DID)

An identity may be active on multiple devices simultaneously. Each device is identified by a **Device Identifier (DID)**.

### DID Derivation

```
DID = CrockfordBase32Lower(SHA256(device_signing_public_key_raw)[0:20])
```

Same encoding rules as IID: 32-character Crockford Base32 lowercase string.

### Device Document

Each device publishes a Device Document, signed by the identity:

```json
{
  "version": 1,
  "did": "<base32-device-identifier>",
  "iid": "<base32-identity-identifier>",
  "device_name": "<optional-friendly-name>",
  "device_signing_key": "<base64-raw-ed25519-public-key-32-bytes>",
  "endpoints": [
    { "type": "direct", "host": "192.0.2.1", "port": 4433, "transport": "quic", "priority": 0 }
  ],
  "created_at": "<RFC3339-timestamp>",
  "updated_at": "<RFC3339-timestamp>",
  "expires_at": "<RFC3339-timestamp-optional>",
  "capabilities": ["messaging", "sync", "relay"],
  "signature_by_identity": "<base64-signature>"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `did` | string | Device identifier (derived from device_signing_key) |
| `iid` | string | Parent identity identifier |
| `device_name` | string? | Optional friendly name ("iPhone", "Laptop") |
| `device_signing_key` | string | Ed25519 public key for this device |
| `endpoints` | Endpoint[] | How to reach this device (see layer-integration.md) |
| `created_at` | timestamp | When this device was authorized |
| `updated_at` | timestamp | Last modification time (used for DHT conflict resolution per RFC-0001 §12.5) |
| `expires_at` | timestamp? | Optional expiration (for temporary devices) |
| `capabilities` | string[] | What this device can do |
| `signature_by_identity` | string | Ed25519 signature by identity's signing key (current or historical) |

**Note:** `device_transport_key` (X25519) is not included. Transport-level key exchange uses the device signing key via the identity handshake protocol.

### Device Document Verification

1. Verify `did == Base32Lower(SHA256(Base64Decode(device_signing_key))[0:20])` (decode Base64 key to raw 32 bytes)
2. Look up identity document for `iid`
3. Verify `signature_by_identity` using identity's current or historical signing keys (see RFC-0001 §7.5 for key lookup order: current → previous → history; note that `expires_at` on historical keys is metadata only and MUST NOT be used as a rejection criterion) [REQ-ID-004]
4. Check device document's `expires_at` if present (this is a valid rejection criterion for temporary device authorizations, distinct from signing key `expires_at`)

### Multi-Device Implications (v1: Identity-Level Sessions)

In v1, **messaging sessions are identity-level** (per IID pair), not per-device:

| Component | Behavior |
|-----------|----------|
| Transport connection | Per (peer_iid, peer_did) - devices connect separately |
| Ratchet session | **Per (sender_iid, recipient_iid)** - shared across devices |
| Group sender keys | Distributed to IIDs |
| Connection dedup | Lower IID initiates |
| Device fanout | Recipient's node handles internal device delivery |

**Rationale**: Identity-level sessions are simpler and avoid ratchet state synchronization complexity. Messages are addressed to an identity; the recipient's node(s) handle device fanout internally.

**Multi-Device Ratchet Synchronization (v1 Normative):**

In v1, the following constraint applies to ensure interoperability:

- **Single Home Node Model**: For a given identity, all devices MUST connect to a single "home node" that manages the identity's ratchet state [REQ-ID-005]
- The home node handles:
  - Maintaining the single ratchet session state per (sender_iid, recipient_iid) pair
  - Forwarding outbound messages from any connected device
  - Distributing inbound messages to all connected devices
- Devices do NOT directly communicate with remote peers for encrypted messaging; they proxy through their home node
- This model avoids the need for cross-device ratchet state synchronization at the protocol level

**External Peer Connectivity (v1 Normative):**

External peers (different identities) MUST connect to the **Identity Document endpoints**, NOT to device-specific endpoints. The Identity Document endpoints represent the home node. [REQ-ID-006]

- **Device documents and device indexes are for INTRA-identity use only** in v1
- External peers look up the target identity's Identity Document from DHT and connect to its endpoints
- Device discovery (fetching device index, device documents) is used by devices within the SAME identity to:
  - Find their home node
  - Find sibling devices for internal coordination
- External peers MUST NOT enumerate or connect to device-specific endpoints [REQ-ID-007]

See `00-shared/layer-integration.md` "Device Discovery Flow" for the complete connectivity model.

**Device-specific considerations**:
- Each device has its own transport connection to the home node
- Device handshake proves DID ownership to the home node
- Ratchet state is managed by the home node, not individual devices
- Sender does NOT need to maintain separate ratchets per recipient device

**Note**: Future protocol versions MAY define a distributed ratchet state synchronization mechanism (e.g., via Sync streams) to support truly peer-to-peer multi-device messaging without a home node. [REQ-ID-008]

### Device Registration

Devices are NOT listed in the main Identity Document (to keep it compact). Instead:

1. Device Document is stored in DHT using key: `SHA256("post-urbit:device:" || did_base32)`
2. Identity's device list is discovered via DHT query key: `SHA256("post-urbit:devices-for:" || iid_base32)`
3. Or devices announce themselves to peers directly via 1:1 messaging

See `00-shared/layer-integration.md` "DHT Key Encoding" section for normative key derivation.

### Device Revocation

To revoke a device, publish a signed revocation notice:

```json
{
  "type": "device_revocation",
  "did": "<device-to-revoke>",
  "iid": "<identity-identifier>",
  "revoked_at": "<RFC3339-timestamp>",
  "reason": "lost|stolen|compromised|decommissioned",
  "signature_by_identity": "<base64-signature>"
}
```

Peers MUST check for revocation before accepting connections from a device. [REQ-ID-009]

## Canonical Serialization

For signing, the document MUST be serialized canonically with domain separation: [REQ-ID-010]

1. **JSON Canonicalization Scheme (JCS)** per RFC 8785
2. Remove `signatures` field before signing
3. UTF-8 encode the result
4. **Prepend domain separator**: `b"post-urbit:idoc:v1:" || jcs_bytes`
5. Sign the resulting bytes

```
canonical_bytes = JCS(document_without_signatures)
payload = b"post-urbit:idoc:v1:" + canonical_bytes
signature = Ed25519_Sign(signing_private_key, payload)
```

**Domain separation** prevents cross-context signature replay. All signatures in the Post-Urbit system use a domain separator prefix.

## Signature Verification

### For New Identity (sequence = 0)

1. Verify `iid == Base32Lower(SHA256(Base64Decode(keys.signing.genesis))[0:20])` (decode Base64 key to raw 32 bytes)
2. Verify `keys.signing.genesis == keys.signing.current` (genesis doc must use genesis key)
3. Verify `signatures.current` over canonical document using `keys.signing.current`

### For Updated Identity (sequence > 0)

1. Verify `iid == Base32Lower(SHA256(Base64Decode(keys.signing.genesis))[0:20])` (genesis key must match IID)
2. Verify `sequence > previous_known_sequence`
3. Verify timestamp (see Timestamp Validation Rules below)
4. Verify `signatures.current` over canonical document using `keys.signing.current`
5. Authorization check (one of the following must pass):
   - **Key continuity**: If `keys.signing.current` differs from previous known:
     - Verify `signatures.previous` over canonical document using previous known signing key
     - This proves the rotation was authorized by the previous key holder
   - **Recovery authorization**: If `signatures.previous` is null and `recovery_proof` exists:
     - Verify recovery proof according to the method (see recovery-mechanisms.md)
     - Compute recovery finality from timestamps: if `now < recovery_proof.cooldown_expires_at`, treat document as provisional (pending cooldown). The `recovery_proof.status` field is informational only and MUST NOT be used for verification decisions. [REQ-ID-011]

### Timestamp Validation Rules

The `timestamp` field prevents future-dated documents but does NOT enforce a maximum age (which would break caching and offline operation):

| Rule | Constraint | Rationale |
|------|------------|-----------|
| **Future limit** | MUST NOT be more than 24 hours ahead of verifier's clock | Prevents pre-dating attacks  [REQ-ID-012]|
| **Monotonicity** | MUST be ≥ previous document's timestamp (if known) | Prevents backdating  [REQ-ID-013]|
| **No max age** | MAY be arbitrarily old | Enables caching, offline, and archival  [REQ-ID-014]|
| **Clock skew** | Allow reasonable tolerance (~5 minutes) for sync errors | Network/device variance |

**Implementation guidance**:
- Reject documents with `timestamp > now + 24h`
- Accept documents with old timestamps if sequence number is valid
- UI MAY warn on "stale" documents (e.g., >30 days old), but MUST NOT auto-reject [REQ-ID-015]
- Sequence number is the primary replay protection, not timestamp

### Handling Missed Sequence Numbers (Gaps)

If a peer receives a document with `sequence = N+K` when they last saw `sequence = N` (gap of K > 1):

1. **If signing key unchanged**: Accept if current signature valid
2. **If signing key changed**:
   - Check if `keys.signing.previous` in new doc matches the key at sequence N
   - If yes and `signatures.previous` valid with that key: accept
   - If no: must fetch intermediate documents to verify chain, or reject

**Recommendation**: Nodes should proactively sync identity updates to avoid gaps.

### Conflict Resolution (Same Sequence Number)

Same-sequence conflicts indicate a serious problem: either a bug (multi-writer without coordination) or an active attack.

**Resolution Strategy**: Trust-on-first-use (TOFU) with manual resolution.

```
function handle_conflict(local_doc, incoming_doc):
    assert local_doc.sequence == incoming_doc.sequence
    assert local_doc.iid == incoming_doc.iid

    if canonical_bytes(local_doc) == canonical_bytes(incoming_doc):
        # Same document, no conflict
        return local_doc

    # Real conflict - DO NOT auto-resolve with hash comparison
    # (hash tiebreaker is gameable by attackers who control keys)

    # Option 1: Prefer document you saw first (TOFU)
    if local_doc.first_seen_at < incoming_doc.received_at:
        log_conflict(local_doc, incoming_doc, "keeping first-seen")
        return local_doc

    # Option 2: If both arrived simultaneously, require manual resolution
    mark_conflict(local_doc.iid, [local_doc, incoming_doc])
    notify_user("Identity conflict detected for {iid}, manual resolution required")
    return null  # Suspend operations until resolved
```

**Manual Resolution Options**:
1. Contact identity owner out-of-band to verify which document is legitimate
2. If key compromise suspected, wait for recovery-based update with higher sequence
3. If one document has valid previous-key signature and other doesn't, prefer it

**Why Not Hash Tiebreaker?**

A hash-based tiebreaker allows an attacker with key access to intentionally craft a conflicting document that "wins" by manipulating claims/extensions/endpoints to achieve a lower hash. This turns same-sequence conflict into a stable takeover mechanism.

**Prevention**:
- Single-writer discipline: only one device/process updates identity at a time
- Coordination: before updating, fetch current sequence and verify no concurrent updates
- Recovery: if conflict occurs, use recovery mechanism to create authoritative high-sequence update

## State Machine

```
┌─────────────┐
│   GENESIS   │ ← sequence = 0, IID derived from signing key
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   ACTIVE    │ ← Normal state, can receive messages
└──────┬──────┘
       │ update (sequence + 1)
       ▼
┌─────────────┐
│   ACTIVE    │ ← New sequence, possibly new keys/endpoints
└──────┬──────┘
       │ revoke
       ▼
┌─────────────┐
│   REVOKED   │ ← Terminal state, identity compromised
└─────────────┘
```

## Wire Format

**For DHT storage and binary transports**, Identity Documents are encoded as the IDOC binary envelope:

**For JSON control-plane messages** (e.g., RFC-0002 identity handshake), Identity Documents are transmitted as JSON objects within the message; signature verification operates on `JCS(document_without_signatures)` derived from the parsed object. See RFC-0002 §5.6 for handshake message format.

**IDOC Binary Envelope:**

```
┌────────────────────────────────────────┐
│ Magic: 0x49 0x44 0x4F 0x43 ("IDOC")    │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Length: uint32 (big-endian)            │ 4 bytes
├────────────────────────────────────────┤
│ JCS-Canonical JSON (UTF-8)             │ <length> bytes
└────────────────────────────────────────┘
```

**JSON Canonicalization (Normative):**
- The JSON body MUST be serialized using JSON Canonicalization Scheme (JCS) per RFC 8785 [REQ-ID-016]
- Implementations MUST reject IDOC envelopes where the JSON is not JCS-canonical [REQ-ID-017]
- This ensures signature verification produces consistent results across implementations
- See RFC-0001 §6.2 for detailed canonicalization rules

**Maximum document size**: 16 KB (16384 bytes)

## Example Documents

### Genesis Document (New Identity)

Note: All keys are raw 32-byte Ed25519/X25519 keys encoded as Base64 (NOT DER/SPKI).
Raw Ed25519 pubkey is 32 bytes → 43 Base64 chars (without padding).

```json
{
  "version": 1,
  "iid": "k5xq7z4m2n3p5r6s7t2v3v4w5x2y3z7a",
  "sequence": "0",
  "timestamp": "2025-01-13T12:00:00Z",
  "keys": {
    "signing": {
      "genesis": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
      "current": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
      "previous": null,
      "history": []
    },
    "encryption": {
      "current": "R4tK2mN8pQ6sL1wF3vX5yZ7aB9cD0eG2hJ4kM6nP8r",
      "previous": []
    }
  },
  "endpoints": [
    {
      "type": "direct",
      "host": "192.168.1.100",
      "port": 4433,
      "priority": 10,
      "transport": "quic"
    }
  ],
  "recovery": {
    "method": "none",
    "config": {}
  },
  "claims": {
    "name": "Alice"
  },
  "extensions": {},
  "recovery_proof": null,
  "signatures": {
    "current": "kL3mN4pQ5rS6tU7vW8xY9zA0bC1dE2fG3hI4jK5lM6nO7pQ8rS9tU0vW1xY2zA3bC4dE5fG6hI7jK8lM9nO0pQr",
    "previous": null
  }
}
```

**Genesis document invariants**:
- `keys.signing.genesis == keys.signing.current` (genesis doc uses genesis key)
- `iid == Base32Lower(SHA256(Base64Decode(keys.signing.genesis))[0:20])` (decode Base64 key to raw 32 bytes)
- `sequence == 0`
- `signatures.previous == null`

### After Key Rotation (sequence = 1)

```json
{
  "version": 1,
  "iid": "k5xq7z4m2n3p5r6s7t2v3v4w5x2y3z7a",
  "sequence": "1",
  "timestamp": "2025-02-15T08:30:00Z",
  "keys": {
    "signing": {
      "genesis": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
      "current": "xN2wP4qR6sT8uV0wX2yZ4aB6cD8eF0gH2iJ4kL6mN8p",
      "previous": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
      "history": []
    },
    "encryption": {
      "current": "aB3cD5eF7gH9iJ1kL3mN5oP7qR9sT1uV3wX5yZ7aB9c",
      "previous": [
        {
          "key": "R4tK2mN8pQ6sL1wF3vX5yZ7aB9cD0eG2hJ4kM6nP8r",
          "valid_from": "0",
          "valid_until": "1",
          "expires_at": "2025-03-15T08:30:00Z"
        }
      ]
    }
  },
  "endpoints": [
    {
      "type": "direct",
      "host": "192.168.1.100",
      "port": 4433,
      "priority": 10,
      "transport": "quic"
    }
  ],
  "recovery": {
    "method": "none",
    "config": {}
  },
  "claims": {
    "name": "Alice"
  },
  "extensions": {},
  "recovery_proof": null,
  "signatures": {
    "current": "newKeySignature0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz01234567",
    "previous": "oldKeySignature0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz01234567"
  }
}
```

**Key rotation invariants**:
- `keys.signing.genesis` NEVER changes (same as sequence 0)
- `keys.signing.previous` contains the key from prior document
- `signatures.previous` proves authorization by the old key holder

## Error Conditions

| Error | Condition | Recovery |
|-------|-----------|----------|
| `INVALID_VERSION` | `version` not recognized | Reject document |
| `INVALID_IID` | IID doesn't match genesis key hash | Reject document |
| `SEQUENCE_REGRESSION` | `sequence` <= known sequence | Reject update |
| `INVALID_SIGNATURE` | Signature verification fails | Reject document |
| `MISSING_PREVIOUS_SIG` | Key rotated but no previous signature | Reject update |
| `DOCUMENT_TOO_LARGE` | Exceeds 16KB | Reject document |
| `MALFORMED_JSON` | JSON parse error | Reject document |

## Security Considerations

1. **Key Compromise**: If signing key is compromised, attacker can issue updates. Mitigation: frequent rotation, recovery mechanisms.
2. **Replay Attack**: Old documents could be replayed. Mitigation: sequence numbers must increase.
3. **IID Collision**: 160-bit hash provides ~2^80 collision resistance. Acceptable for this use case.
4. **Timing Attacks**: Signature verification should be constant-time.
5. **Metadata Leakage**: Claims and endpoints are public. Users should be warned.

## Test Vectors

**Authoritative test vectors are defined in `spec/00-shared/test-vectors.md`.**

That document provides:
- Seed-based deterministic key derivation for reproducibility
- Genesis document creation and signature verification
- Key rotation with dual signatures
- IID derivation from raw Ed25519 public keys (NOT DER/SPKI encoded)

Implementers MUST validate against those vectors before claiming conformance. [REQ-ID-018]

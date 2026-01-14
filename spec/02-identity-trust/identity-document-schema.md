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
| **Base32 alphabet** | RFC4648 `A-Z2-7`, converted to lowercase |
| **Padding** | No padding |
| **Input to hash** | Raw 32-byte Ed25519 public key (NOT DER/SPKI) |
| **Hash output** | First 20 bytes (160 bits) of SHA-256 |
| **Length** | 32 characters |
| **Normalization** | Always lowercase; reject non-canonical forms |
| **Valid characters** | `a-z` and `2-7` only (RFC4648 Base32 lowercase) |

- **Example**: `k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a`
- **Invalid examples**: `abc01890xyz` (contains 0, 1, 8, 9 which are not in Base32 alphabet)
- **Derivation**: Hash of the FIRST (genesis) signing key ever used
- **Immutable**: Never changes, even after key rotation

### IID Derivation Algorithm

```python
def derive_iid(genesis_signing_public_key_raw: bytes) -> str:
    assert len(genesis_signing_public_key_raw) == 32  # Raw Ed25519 pubkey
    hash_bytes = sha256(genesis_signing_public_key_raw)
    truncated = hash_bytes[:20]  # First 160 bits
    base32_upper = base64.b32encode(truncated).decode('ascii').rstrip('=')
    return base32_upper.lower()
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
      "previous": "<base64-raw-ed25519-public-key-32-bytes>|null"
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
| Ed25519 public key | Base64 (RFC4648, standard alphabet, no padding) | 32 bytes | Raw key bytes, NOT DER/SPKI |
| X25519 public key | Base64 (RFC4648, standard alphabet, no padding) | 32 bytes | Raw key bytes |
| Ed25519 signature | Base64 (RFC4648, standard alphabet, no padding) | 64 bytes | Raw R\|\|S bytes |

**Base64 Alphabet**: `A-Za-z0-9+/` (standard), no padding characters.

## Field Specifications

### Required Fields

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `version` | uint8 | Must be `1` | Schema version for forward compatibility |
| `iid` | string | 32 chars, Base32 lowercase | Identity Identifier, immutable |
| `sequence` | string (uint64) | Monotonically increasing decimal string, max "18446744073709551614" | Prevents replay, must increment on every update |
| `timestamp` | string | RFC3339, UTC, see validation rules | When this version was created |
| `keys.signing.genesis` | string | Base64, 32 bytes decoded | Genesis Ed25519 public key (NEVER changes) |
| `keys.signing.current` | string | Base64, 32 bytes decoded | Current Ed25519 public key |
| `keys.encryption.current` | string | Base64, 32 bytes decoded | Current X25519 public key |
| `signatures.current` | string | Base64, 64 bytes decoded | Ed25519 signature over canonical document |

### Optional Fields

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `keys.signing.previous` | string\|null | Base64, 32 bytes | Previous signing key (for rotation verification) |
| `keys.encryption.previous` | array | See EncryptionKeyHistory | Previous encryption keys with validity windows (for offline peers) |
| `endpoints` | array | Max 10 entries | How to reach this identity's node |
| `recovery` | object | See recovery spec | Recovery configuration |
| `claims` | object | See claims spec | Optional public metadata |
| `extensions` | object | Max 4KB total | App-specific extensions |
| `signatures.previous` | string\|null | Base64, 64 bytes | Signature by previous key (required during rotation) |

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
| `port` | number | Yes | UDP port number (1-65535) |
| `transport` | string | No | `quic` (default) or `https` for mailbox |
| `priority` | number | Yes | 0-255, lower = higher priority |
| `relay_id` | string | No | IID of relay operator (for relay type) |
| `observed_at` | timestamp | No | When this endpoint was last verified reachable |
| `metadata` | object | No | Type-specific additional data |

**Type-specific notes**:

| Type | Usage |
|------|-------|
| `direct` | Direct QUIC connection to host:port |
| `relay` | QUIC via relay server (relay_id identifies relay) |
| `mailbox` | Store-and-forward via HTTPS (host:port is mailbox server) |

**Priority**: Lower number = higher priority. Peers try endpoints in priority order.

**Host normalization**:
- Hostnames: lowercase, no trailing dot
- IPv4: standard dotted decimal
- IPv6: lowercase, bracketed `[2001:db8::1]`, no zone ID

### Recovery Configuration

```json
{
  "method": "social|device-escrow|threshold|none",
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
DID = Base32Lower(SHA256(device_signing_public_key_raw)[0:20])
```

Same encoding rules as IID: 32-character Base32 lowercase string.

### Device Document

Each device publishes a Device Document, signed by the identity:

```json
{
  "version": 1,
  "did": "<base32-device-identifier>",
  "iid": "<base32-identity-identifier>",
  "device_name": "<optional-friendly-name>",
  "device_signing_key": "<base64-raw-ed25519-public-key-32-bytes>",
  "device_transport_key": "<base64-raw-x25519-public-key-32-bytes>",
  "created_at": "<RFC3339-timestamp>",
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
| `device_transport_key` | string | X25519 public key for transport-level encryption |
| `created_at` | timestamp | When this device was authorized |
| `expires_at` | timestamp? | Optional expiration (for temporary devices) |
| `capabilities` | string[] | What this device can do |
| `signature_by_identity` | string | Ed25519 signature by identity's current signing key |

### Device Document Verification

1. Verify `did == Base32Lower(SHA256(device_signing_key)[0:20])`
2. Look up identity document for `iid`
3. Verify `signature_by_identity` using identity's current signing key
4. Check `expires_at` if present

### Multi-Device Implications

| Component | Without DID | With DID |
|-----------|-------------|----------|
| Transport connection | Per (peer_iid) | Per (peer_iid, peer_did) |
| Ratchet session | Per peer IID | Per (peer_iid, peer_did) |
| Group sender keys | Distributed to IIDs | Distributed to (iid, did) pairs |
| Connection dedup | Lower IID initiates | Lower (iid, did) initiates |

### Device Registration

Devices are NOT listed in the main Identity Document (to keep it compact). Instead:

1. Device Document is stored in DHT: key = `"device:" + did`
2. Identity's device list is discovered via DHT query: prefix = `"devices-for:" + iid`
3. Or devices announce themselves to peers directly via 1:1 messaging

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

Peers MUST check for revocation before accepting connections from a device.

## Canonical Serialization

For signing, the document MUST be serialized canonically:

1. **JSON Canonicalization Scheme (JCS)** per RFC 8785
2. Remove `signatures` field before signing
3. UTF-8 encode the result
4. Sign the bytes

```
canonical_bytes = JCS(document_without_signatures)
signature = Ed25519_Sign(signing_private_key, canonical_bytes)
```

## Signature Verification

### For New Identity (sequence = 0)

1. Verify `iid == Base32Lower(SHA256(keys.signing.genesis)[0:20])`
2. Verify `keys.signing.genesis == keys.signing.current` (genesis doc must use genesis key)
3. Verify `signatures.current` over canonical document using `keys.signing.current`

### For Updated Identity (sequence > 0)

1. Verify `iid == Base32Lower(SHA256(keys.signing.genesis)[0:20])` (genesis key must match IID)
2. Verify `sequence > previous_known_sequence`
3. Verify timestamp (see Timestamp Validation Rules below)
4. Verify `signatures.current` over canonical document using `keys.signing.current`
5. Authorization check (one of the following must pass):
   - **Key continuity**: If `keys.signing.current` differs from previous known:
     - Verify `signatures.previous` over canonical document using previous known signing key
     - This proves the rotation was authorized by the previous key holder
   - **Recovery authorization**: If `signatures.previous` is null and `recovery_proof` exists:
     - Verify recovery proof according to the method (see recovery-mechanisms.md)
     - If `recovery_proof.status == "pending"`, treat document as provisional until cooldown expires

### Timestamp Validation Rules

The `timestamp` field prevents future-dated documents but does NOT enforce a maximum age (which would break caching and offline operation):

| Rule | Constraint | Rationale |
|------|------------|-----------|
| **Future limit** | MUST NOT be more than 24 hours ahead of verifier's clock | Prevents pre-dating attacks |
| **Monotonicity** | MUST be ≥ previous document's timestamp (if known) | Prevents backdating |
| **No max age** | MAY be arbitrarily old | Enables caching, offline, and archival |
| **Clock skew** | Allow reasonable tolerance (~5 minutes) for sync errors | Network/device variance |

**Implementation guidance**:
- Reject documents with `timestamp > now + 24h`
- Accept documents with old timestamps if sequence number is valid
- UI MAY warn on "stale" documents (e.g., >30 days old), but MUST NOT auto-reject
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

For network transmission, Identity Documents are encoded as:

```
┌────────────────────────────────────────┐
│ Magic: 0x49 0x44 0x4F 0x43 ("IDOC")    │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Length: uint32 (big-endian)            │ 4 bytes
├────────────────────────────────────────┤
│ Canonical JSON (UTF-8)                 │ <length> bytes
└────────────────────────────────────────┘
```

**Maximum document size**: 16 KB (16384 bytes)

## Example Documents

### Genesis Document (New Identity)

Note: All keys are raw 32-byte Ed25519/X25519 keys encoded as Base64 (NOT DER/SPKI).
Raw Ed25519 pubkey is 32 bytes → 43 Base64 chars (without padding).

```json
{
  "version": 1,
  "iid": "k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a",
  "sequence": "0",
  "timestamp": "2025-01-13T12:00:00Z",
  "keys": {
    "signing": {
      "genesis": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
      "current": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
      "previous": null
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
  "signatures": {
    "current": "kL3mN4pQ5rS6tU7vW8xY9zA0bC1dE2fG3hI4jK5lM6nO7pQ8rS9tU0vW1xY2zA3bC4dE5fG6hI7jK8lM9nO0pQr",
    "previous": null
  }
}
```

**Genesis document invariants**:
- `keys.signing.genesis == keys.signing.current` (genesis doc uses genesis key)
- `iid == Base32Lower(SHA256(keys.signing.genesis)[0:20])`
- `sequence == 0`
- `signatures.previous == null`

### After Key Rotation (sequence = 1)

```json
{
  "version": 1,
  "iid": "k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a",
  "sequence": "1",
  "timestamp": "2025-02-15T08:30:00Z",
  "keys": {
    "signing": {
      "genesis": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
      "current": "xN2wP4qR6sT8uV0wX2yZ4aB6cD8eF0gH2iJ4kL6mN8p",
      "previous": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M"
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

### Test Vector 1: Genesis Document

```
Signing Private Key (hex): e8f3...2a1b
Signing Public Key (base64): MCowBQYDK2VwAyEAb7YH...
Expected IID: k5xq7z8m9n2p3r4s5t6u7v8w9x0y1z2a
Expected Signature (base64): MEUCIQD7...
```

### Test Vector 2: Key Rotation

```
Old Signing Key (base64): MCowBQYDK2VwAyEA<OLD>...
New Signing Key (base64): MCowBQYDK2VwAyEA<NEW>...
Sequence: 1
Expected Current Signature: <sig-by-new>
Expected Previous Signature: <sig-by-old>
```

(Full test vectors to be generated during implementation)

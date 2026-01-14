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

- **Example**: `k5xq7z8m9n2p3r4s5t6u7v8w9x0y1z2a`
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
  "sequence": <uint64>,
  "timestamp": "<RFC3339-timestamp-UTC>",
  "keys": {
    "signing": {
      "genesis": "<base64-raw-ed25519-public-key-32-bytes>",
      "current": "<base64-raw-ed25519-public-key-32-bytes>",
      "previous": "<base64-raw-ed25519-public-key-32-bytes>|null"
    },
    "encryption": {
      "current": "<base64-raw-x25519-public-key-32-bytes>",
      "previous": "<base64-raw-x25519-public-key-32-bytes>|null"
    }
  },
  "endpoints": [
    {
      "type": "direct",
      "address": "<host:port>",
      "priority": <uint8>
    },
    {
      "type": "relay",
      "address": "<relay-url>",
      "priority": <uint8>
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
| `sequence` | uint64 | Monotonically increasing, max 2^64-2 | Prevents replay, must increment on every update |
| `timestamp` | string | RFC3339, UTC, within ±24h of now | When this version was created |
| `keys.signing.genesis` | string | Base64, 32 bytes decoded | Genesis Ed25519 public key (NEVER changes) |
| `keys.signing.current` | string | Base64, 32 bytes decoded | Current Ed25519 public key |
| `keys.encryption.current` | string | Base64, 32 bytes decoded | Current X25519 public key |
| `signatures.current` | string | Base64, 64 bytes decoded | Ed25519 signature over canonical document |

### Optional Fields

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `keys.signing.previous` | string\|null | Base64, 32 bytes | Previous signing key (for rotation verification) |
| `keys.encryption.previous` | string\|null | Base64, 32 bytes | Previous encryption key (for decrypting old messages) |
| `endpoints` | array | Max 10 entries | How to reach this identity's node |
| `recovery` | object | See recovery spec | Recovery configuration |
| `claims` | object | See claims spec | Optional public metadata |
| `extensions` | object | Max 4KB total | App-specific extensions |
| `signatures.previous` | string\|null | Base64, 64 bytes | Signature by previous key (required during rotation) |

### Endpoint Object

```json
{
  "type": "direct|relay|mailbox",
  "address": "<protocol-specific-address>",
  "priority": 0-255,
  "metadata": { <optional-type-specific-data> }
}
```

| Type | Address Format | Description |
|------|----------------|-------------|
| `direct` | `host:port` or `[ipv6]:port` | Direct QUIC connection |
| `relay` | `https://relay.example.com/path` | Relay service URL |
| `mailbox` | `https://mailbox.example.com/iid` | Store-and-forward service |

**Priority**: Lower number = higher priority. Peers try endpoints in priority order.

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
3. Verify `timestamp` is within ±24 hours of current time (reject stale/future docs)
4. Verify `signatures.current` over canonical document using `keys.signing.current`
5. Authorization check (one of the following must pass):
   - **Key continuity**: If `keys.signing.current` differs from previous known:
     - Verify `signatures.previous` over canonical document using previous known signing key
     - This proves the rotation was authorized by the previous key holder
   - **Recovery authorization**: If `signatures.previous` is null and `recovery_proof` exists:
     - Verify recovery proof according to the method (see recovery-mechanisms.md)
     - If `recovery_proof.status == "pending"`, treat document as provisional until cooldown expires

### Handling Missed Sequence Numbers (Gaps)

If a peer receives a document with `sequence = N+K` when they last saw `sequence = N` (gap of K > 1):

1. **If signing key unchanged**: Accept if current signature valid
2. **If signing key changed**:
   - Check if `keys.signing.previous` in new doc matches the key at sequence N
   - If yes and `signatures.previous` valid with that key: accept
   - If no: must fetch intermediate documents to verify chain, or reject

**Recommendation**: Nodes should proactively sync identity updates to avoid gaps.

### Conflict Resolution (Same Sequence Number)

If two valid documents exist with the same `sequence`:

1. Both must have valid signatures
2. **Deterministic tiebreaker**: Compare SHA256 hash of canonical documents (without signatures)
3. Accept document with lexicographically lower hash
4. Log conflict for investigation (may indicate compromise)

```python
def resolve_conflict(doc_a, doc_b):
    assert doc_a.sequence == doc_b.sequence
    hash_a = sha256(canonical_without_signatures(doc_a))
    hash_b = sha256(canonical_without_signatures(doc_b))
    return doc_a if hash_a < hash_b else doc_b
```

**Note**: Conflicts should be rare. Frequent conflicts suggest either compromise or misconfiguration.

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

```json
{
  "version": 1,
  "iid": "k5xq7z8m9n2p3r4s5t6u7v8w9x0y1z2a",
  "sequence": 0,
  "timestamp": "2025-01-13T12:00:00Z",
  "keys": {
    "signing": {
      "current": "MCowBQYDK2VwAyEA...",
      "previous": null
    },
    "encryption": {
      "current": "MCowBQYDK2VuAyEA...",
      "previous": null
    }
  },
  "endpoints": [
    {
      "type": "direct",
      "address": "192.168.1.100:4433",
      "priority": 0
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
    "current": "MEUCIQD...",
    "previous": null
  }
}
```

### After Key Rotation (sequence = 1)

```json
{
  "version": 1,
  "iid": "k5xq7z8m9n2p3r4s5t6u7v8w9x0y1z2a",
  "sequence": 1,
  "timestamp": "2025-02-15T08:30:00Z",
  "keys": {
    "signing": {
      "current": "MCowBQYDK2VwAyEA<NEW>...",
      "previous": "MCowBQYDK2VwAyEA<OLD>..."
    },
    "encryption": {
      "current": "MCowBQYDK2VuAyEA<NEW>...",
      "previous": "MCowBQYDK2VuAyEA<OLD>..."
    }
  },
  "endpoints": [...],
  "recovery": {...},
  "claims": {...},
  "extensions": {},
  "signatures": {
    "current": "<signature-by-NEW-key>",
    "previous": "<signature-by-OLD-key>"
  }
}
```

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

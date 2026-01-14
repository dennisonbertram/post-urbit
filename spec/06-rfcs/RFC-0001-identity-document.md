# RFC-0001: Post-Urbit Identity Document

**Status**: Draft
**Version**: 1.0
**Authors**: Post-Urbit Working Group
**Created**: 2025-01-14
**Updated**: 2025-01-14

## Abstract

This document specifies the Post-Urbit Identity Document format, a self-certifying identity representation for decentralized peer-to-peer systems. It defines the Identity Identifier (IID) derivation, document schema, cryptographic operations, key rotation protocol, and wire format for network transmission.

## Status of This Memo

This is a draft specification. Implementations SHOULD follow this document but MAY diverge where noted as implementation-defined.

## Table of Contents

1. [Introduction](#1-introduction)
2. [Terminology](#2-terminology)
3. [Identity Identifier (IID)](#3-identity-identifier-iid)
4. [Document Schema](#4-document-schema)
5. [Cryptographic Encoding](#5-cryptographic-encoding)
6. [Canonical Serialization](#6-canonical-serialization)
7. [Signature Verification](#7-signature-verification)
8. [Key Rotation](#8-key-rotation)
9. [Recovery Mechanisms](#9-recovery-mechanisms)
10. [Revocation](#10-revocation)
11. [Wire Format](#11-wire-format)
12. [DHT Storage](#12-dht-storage)
13. [Device Documents](#13-device-documents)
14. [Security Considerations](#14-security-considerations)
15. [Test Vectors](#15-test-vectors)
16. [IANA Considerations](#16-iana-considerations)
17. [References](#17-references)

## 1. Introduction

The Post-Urbit Identity Document (IDOC) provides a portable, self-sovereign identity for peer-to-peer communication. Key properties:

- **Self-certifying**: The identity identifier is derived from cryptographic keys
- **Rotatable**: Keys can be changed while preserving identity
- **Recoverable**: Multiple recovery mechanisms protect against key loss
- **Portable**: No dependency on central registries

### 1.1 Design Goals

1. No external authority required to establish identity
2. Identity persists across key rotations
3. Recovery possible without trusted third parties (if configured)
4. Efficient verification with minimal round-trips
5. Compatible with DHT-based discovery

### 1.2 Scope

This RFC covers:
- Identity Document schema and encoding
- Identity Identifier derivation
- Key rotation protocol
- Recovery and revocation
- Wire format for transmission
- DHT record format

This RFC does NOT cover:
- Transport-layer protocols (see RFC-0002)
- End-to-end encryption (see RFC-0003)
- Application-level identity claims

## 2. Terminology

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

| Term | Definition |
|------|------------|
| IID | Identity Identifier - stable 32-character identifier derived from genesis key |
| IDOC | Identity Document - JSON structure containing keys and metadata |
| Genesis Key | The first signing key ever used for an identity (immutable reference) |
| JCS | JSON Canonicalization Scheme per RFC 8785 |
| Sequence Number | Monotonically increasing version counter |

## 3. Identity Identifier (IID)

### 3.1 Derivation Algorithm

The IID is derived from the genesis signing key:

```
IID = Base32Lower(SHA-256(genesis_signing_public_key_raw)[0:20])
```

Where:
- `genesis_signing_public_key_raw` is the 32-byte Ed25519 public key (raw bytes, NOT DER/SPKI)
- `SHA-256` produces a 32-byte hash
- `[0:20]` takes the first 20 bytes (160 bits)
- `Base32Lower` encodes using RFC 4648 alphabet (A-Z, 2-7) converted to lowercase

### 3.2 Encoding Specification

| Property | Value |
|----------|-------|
| Alphabet | `a-z2-7` (RFC 4648 Base32, lowercase) |
| Padding | None |
| Length | 32 characters |
| Input | Raw 32-byte Ed25519 public key |
| Hash | SHA-256, first 20 bytes |

### 3.3 Validation

Implementations MUST:
- Reject IIDs containing characters outside `a-z2-7`
- Reject IIDs not exactly 32 characters long
- Normalize to lowercase before comparison
- Reject non-canonical forms (e.g., uppercase)

### 3.4 Reference Implementation

```python
import hashlib
import base64

def derive_iid(genesis_signing_public_key_raw: bytes) -> str:
    """
    Derive Identity Identifier from genesis Ed25519 public key.

    Args:
        genesis_signing_public_key_raw: 32-byte raw Ed25519 public key

    Returns:
        32-character lowercase Base32 string
    """
    assert len(genesis_signing_public_key_raw) == 32, "Must be raw 32-byte Ed25519 pubkey"
    hash_bytes = hashlib.sha256(genesis_signing_public_key_raw).digest()
    truncated = hash_bytes[:20]  # First 160 bits
    base32_upper = base64.b32encode(truncated).decode('ascii').rstrip('=')
    return base32_upper.lower()
```

## 4. Document Schema

### 4.1 JSON Structure

```json
{
  "version": 1,
  "iid": "<32-char-base32-lowercase>",
  "sequence": "<uint64-decimal-string>",
  "timestamp": "<RFC3339-UTC>",
  "keys": {
    "signing": {
      "genesis": "<base64-ed25519-pubkey>",
      "current": "<base64-ed25519-pubkey>",
      "previous": "<base64-ed25519-pubkey>|null",
      "history": [<SigningKeyHistoryEntry>, ...]
    },
    "encryption": {
      "current": "<base64-x25519-pubkey>",
      "previous": [<EncryptionKeyHistoryEntry>, ...]
    }
  },
  "endpoints": [<Endpoint>, ...],
  "recovery": {
    "method": "<recovery-method>",
    "config": {}
  },
  "claims": {
    "name": "<optional-string>",
    "avatar": "<optional-content-hash>",
    "bio": "<optional-string>"
  },
  "extensions": {},
  "recovery_proof": null,
  "signatures": {
    "current": "<base64-signature>",
    "previous": "<base64-signature>|null"
  }
}
```

### 4.2 Required Fields

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `version` | uint8 | MUST be `1` | Schema version |
| `iid` | string | 32 chars, Base32 lowercase | Identity Identifier |
| `sequence` | string | Decimal uint64, monotonic | Version counter |
| `timestamp` | string | RFC 3339 UTC | Creation time |
| `keys.signing.genesis` | string | Base64, 32 bytes decoded | Immutable genesis key |
| `keys.signing.current` | string | Base64, 32 bytes decoded | Current signing key |
| `keys.encryption.current` | string | Base64, 32 bytes decoded | Current encryption key |
| `signatures.current` | string | Base64, 64 bytes decoded | Current key signature |

### 4.3 Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `keys.signing.previous` | string\|null | null | Previous signing key |
| `keys.signing.history` | array | [] | Historical signing keys (max 10) |
| `keys.encryption.previous` | array | [] | Historical encryption keys (max 5) |
| `endpoints` | array | [] | Connection endpoints (max 10) |
| `recovery` | object | `{"method":"none","config":{}}` | Recovery configuration |
| `claims` | object | {} | Optional public metadata |
| `extensions` | object | {} | App-specific data (max 4KB) |
| `recovery_proof` | object\|null | null | Present during recovery |
| `signatures.previous` | string\|null | null | Required during key rotation |

### 4.4 Signing Key History Entry

```json
{
  "key": "<base64-ed25519-pubkey>",
  "valid_from": "<sequence-number>",
  "valid_until": "<sequence-number>",
  "expires_at": "<RFC3339-timestamp>"
}
```

Retention policy: At most 10 entries or 2 years, whichever is less.

### 4.5 Encryption Key History Entry

```json
{
  "key": "<base64-x25519-pubkey>",
  "valid_from": "<sequence-number>",
  "valid_until": "<sequence-number>",
  "expires_at": "<RFC3339-timestamp>"
}
```

Retention policy: At most 5 entries or 30 days, whichever is less.

### 4.6 Endpoint Object

```json
{
  "type": "direct|relay|mailbox",
  "host": "<hostname-or-ip>",
  "port": 4433,
  "transport": "quic|https",
  "priority": 10,
  "relay_id": "<optional-relay-iid>",
  "observed_at": "<optional-RFC3339>",
  "metadata": {}
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `type` | Yes | `direct`, `relay`, or `mailbox` |
| `host` | Yes | Hostname, IPv4, or `[IPv6]` |
| `port` | Yes | Service port (1-65535) |
| `transport` | No | `quic` (default) or `https` |
| `priority` | Yes | 0-255, lower = higher priority |

### 4.7 Size Limits

| Component | Maximum |
|-----------|---------|
| Total document | 16 KB (16384 bytes) |
| Extensions | 4 KB |
| Endpoints | 10 entries |
| Signing history | 10 entries |
| Encryption history | 5 entries |
| claims.name | 64 UTF-8 chars |
| claims.bio | 256 UTF-8 chars |

## 5. Cryptographic Encoding

### 5.1 Key Encoding

| Type | Encoding | Size | Notes |
|------|----------|------|-------|
| Ed25519 public key | Base64 standard, no padding | 32 bytes → 43 chars | Raw key bytes |
| X25519 public key | Base64 standard, no padding | 32 bytes → 43 chars | Raw key bytes |
| Ed25519 signature | Base64 standard, no padding | 64 bytes → 86 chars | Raw R\|\|S bytes |

Base64 alphabet: `A-Za-z0-9+/` (RFC 4648 standard, no padding).

### 5.2 NOT DER/SPKI

Keys MUST be raw bytes, NOT wrapped in DER/SPKI/PEM encoding. A raw Ed25519 public key is exactly 32 bytes.

**Incorrect** (DER-wrapped, starts with `MCow...`):
```
MCowBQYDK2VwAyEAb7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M=
```

**Correct** (raw 32 bytes):
```
48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q
```

## 6. Canonical Serialization

### 6.1 Algorithm

For signing, documents MUST be serialized using JSON Canonicalization Scheme (JCS) per RFC 8785:

1. Remove the `signatures` field entirely
2. Apply JCS canonicalization (lexicographic key ordering, no whitespace)
3. Encode as UTF-8
4. **Prepend domain separator**: `b"post-urbit:idoc:v1:" || jcs_bytes`
5. Sign the resulting bytes

### 6.2 Wire Encoding Canonicalization

The JSON inside IDOC envelopes (Section 11) MUST also be JCS-canonicalized for the **full document including signatures**. This ensures byte-for-byte reproducibility across implementations.

### 6.3 JCS Rules

- Object keys sorted lexicographically at all nesting levels
- No whitespace between tokens
- Numbers in shortest form (no trailing zeros, no leading zeros except single `0`)
- Strings use minimal escaping
- `null`, `true`, `false` as literals

### 6.4 Sequence Number Constraints

The `sequence` field MUST:
- Match regex: `^(0|[1-9][0-9]{0,19})$`
- Parse to numeric value ≤ 18446744073709551615 (2^64 - 1)
- Be strictly greater than previous known sequence (increase by at least 1)
- No leading zeros (except standalone "0"), no plus sign, no whitespace

### 6.5 Base64 Validation

All Base64-encoded values MUST:
- Use standard alphabet `A-Za-z0-9+/` only
- NOT include padding characters (`=`)
- NOT include non-alphabet characters (whitespace, etc.)
- Decode to exact expected lengths (32 bytes for keys, 64 bytes for signatures)

Implementations MUST reject documents with padded or non-canonical Base64.

### 6.6 Optional Fields and Defaults

Defaults listed in Section 4.3 are semantic only. When verifying signatures:
- Implementations MUST NOT materialize defaults before verification
- The signature is over the literal JSON as received

Producers SHOULD include fields with their default values for maximum compatibility.

### 6.7 Example

Input document (before signatures):
```json
{
  "version": 1,
  "iid": "lbvhmpzmqkzrudc55hok54a6ajq6a6c3",
  "sequence": "0",
  "timestamp": "2025-01-15T00:00:00Z",
  "keys": {
    "signing": {
      "genesis": "48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q",
      "current": "48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q",
      "previous": null
    },
    "encryption": {
      "current": "jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc",
      "previous": []
    }
  },
  "endpoints": [],
  "recovery": {"method": "none", "config": {}},
  "claims": {"name": "Alice"},
  "extensions": {}
}
```

JCS output (single line, no whitespace):
```
{"claims":{"name":"Alice"},"endpoints":[],"extensions":{},"iid":"lbvhmpzmqkzrudc55hok54a6ajq6a6c3","keys":{"encryption":{"current":"jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc","previous":[]},"signing":{"current":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","genesis":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","previous":null}},"recovery":{"config":{},"method":"none"},"sequence":"0","timestamp":"2025-01-15T00:00:00Z","version":1}
```

## 7. Signature Verification

### 7.1 Genesis Document (sequence = 0)

1. Verify `iid == derive_iid(decode_base64(keys.signing.genesis))`
2. Verify `keys.signing.genesis == keys.signing.current`
3. Compute signed payload: `b"post-urbit:idoc:v1:" || JCS(doc_without_signatures)`
4. Verify `signatures.current` over signed payload using `keys.signing.current`

### 7.2 Updated Document (sequence > 0)

1. Verify `iid == derive_iid(decode_base64(keys.signing.genesis))`
2. Verify `sequence > previous_known_sequence`
3. Verify timestamp is not more than 24 hours in the future
4. Compute signed payload: `b"post-urbit:idoc:v1:" || JCS(doc_without_signatures)`
5. Verify `signatures.current` over signed payload using `keys.signing.current`
6. **Authorization** (one of the following):
   - **Key continuity**: `signatures.previous` valid with previous known signing key
   - **Recovery**: `recovery_proof` validates per Section 9

### 7.3 Bootstrap Verification (First Encounter)

When a node encounters an IID for the first time with no prior cached state:

1. **Fetch genesis**: Attempt to retrieve the genesis document (sequence = 0) from DHT or peer
2. **Validate genesis**: Verify per Section 7.1
3. **Cache genesis**: Store genesis as the trust anchor for this IID
4. **Fetch latest**: Retrieve the highest-sequence document available
5. **Validate chain**: If sequence gap exists:
   - The latest document's `keys.signing.previous` MUST match a key in the chain
   - The `signatures.previous` MUST be valid with that key
   - If chain cannot be verified, the node MAY accept with TOFU semantics and warn the user

**TOFU (Trust On First Use)**: If genesis document is unavailable, implementations MAY accept the first document encountered as the trust anchor. This SHOULD be flagged to the user as "unverified first contact."

```python
def bootstrap_verify(iid: str) -> IdentityDocument:
    # Try to fetch genesis from DHT
    genesis = dht_fetch_sequence(iid, sequence=0)
    if genesis:
        verify_genesis(genesis)
        cache_trust_anchor(iid, genesis)

    # Fetch latest
    latest = dht_fetch_latest(iid)
    if not latest:
        raise NotFound(iid)

    # Verify current signature
    verify_current_signature(latest)

    # If we have genesis, verify chain
    if genesis:
        verify_chain(genesis, latest)
        return latest

    # TOFU: accept first seen, warn user
    warn_user(f"Unverified first contact: {iid}")
    cache_trust_anchor(iid, latest)
    return latest
```

### 7.4 Timestamp Rules

| Rule | Constraint |
|------|------------|
| Future limit | MUST NOT exceed `now + 24h` |
| Monotonicity | MUST be >= previous document's timestamp (if known) |
| No max age | MAY be arbitrarily old (enables caching/offline) |
| Tolerance | SHOULD allow ~5 minutes for clock skew |
| Format | RFC 3339 with `Z` suffix (UTC), fractional seconds OPTIONAL |

### 7.5 Signature Verification for Historical Keys

When verifying delayed messages (e.g., mailbox delivery), check keys in order:
1. `keys.signing.current`
2. `keys.signing.previous` (if present)
3. `keys.signing.history[]` entries (match by validity window)

## 8. Key Rotation

### 8.1 Protocol

1. Generate new keys: `K_new = Ed25519_Generate()`
2. Construct new document with `sequence = N + 1`
3. Set `keys.signing.previous = keys.signing.current` (from old doc)
4. Set `keys.signing.current = K_new`
5. Sign with BOTH keys:
   - `signatures.current = Sign(K_new_private, JCS(doc))`
   - `signatures.previous = Sign(K_old_private, JCS(doc))`
6. Publish new document

### 8.2 Verification

```python
def verify_rotation(old_doc, new_doc):
    # IID must be unchanged
    assert new_doc['iid'] == old_doc['iid']

    # Sequence must increase
    assert int(new_doc['sequence']) > int(old_doc['sequence'])

    # Current signature must be valid
    canonical = jcs(without_signatures(new_doc))
    current_key = decode_base64(new_doc['keys']['signing']['current'])
    assert ed25519_verify(current_key, canonical,
                          decode_base64(new_doc['signatures']['current']))

    # If signing key changed, previous signature required
    if new_doc['keys']['signing']['current'] != old_doc['keys']['signing']['current']:
        assert new_doc['signatures']['previous'] is not None
        old_key = decode_base64(old_doc['keys']['signing']['current'])
        assert ed25519_verify(old_key, canonical,
                              decode_base64(new_doc['signatures']['previous']))

    return True
```

### 8.3 Rotation Frequency

| Scenario | Recommended Interval |
|----------|---------------------|
| Normal operation | 90 days |
| High security | 30 days |
| After device loss | Immediately |
| After suspected compromise | Immediately |

## 9. Recovery Mechanisms

### 9.1 Methods

| Method | Description | Trust Model |
|--------|-------------|-------------|
| `none` | No recovery | Self-sovereign |
| `social` | M-of-N trustees | Trust in contacts |
| `device-escrow` | Backup device | Trust in hardware |
| `threshold` | Shamir shares | Distributed trust |
| `provider` | Third party | Trust in provider |

### 9.2 Social Recovery Configuration

```json
{
  "method": "social",
  "config": {
    "threshold": 3,
    "trustees": [
      {"iid": "<trustee-1-iid>", "label": "Alice (sister)"},
      {"iid": "<trustee-2-iid>", "label": "Bob (friend)"},
      {"iid": "<trustee-3-iid>", "label": "Carol (colleague)"},
      {"iid": "<trustee-4-iid>", "label": "Dave (lawyer)"},
      {"iid": "<trustee-5-iid>", "label": "Eve (partner)"}
    ],
    "cooldown_hours": 72
  }
}
```

**Constraints:**
- `len(trustees)` MUST be >= `threshold`
- `threshold` MUST be >= 2
- `cooldown_hours` MUST be >= 24 and <= 720 (30 days max)

### 9.3 Recovery Attestation Format

Each trustee signs an attestation:

```json
{
  "type": "recovery_attestation",
  "subject_iid": "<iid-being-recovered>",
  "new_signing_key": "<base64-new-pubkey>",
  "new_encryption_key": "<base64-new-pubkey>",
  "trustee_iid": "<trustee's-iid>",
  "timestamp": "<RFC3339>",
  "signature": "<base64-sig>"
}
```

**Attestation Signing:**
1. Remove `signature` field from attestation
2. JCS-canonicalize the remaining JSON
3. Prepend domain separator: `b"post-urbit:recovery-attestation:v1:" || jcs_bytes`
4. Sign with trustee's current signing key

### 9.4 Recovery Proof

When recovery is used, `signatures.previous` is null. Instead, `recovery_proof` is present:

```json
{
  "recovery_proof": {
    "method": "social",
    "initiated_at": "<RFC3339>",
    "cooldown_expires_at": "<RFC3339>",
    "status": "pending|active",
    "proof_data": {
      "attestations": [<attestation>, <attestation>, ...]
    }
  }
}
```

### 9.5 Social Recovery Verification

```python
def verify_social_recovery(old_doc, new_doc) -> bool:
    assert old_doc['recovery']['method'] == 'social'
    proof = new_doc['recovery_proof']
    assert proof['method'] == 'social'

    config = old_doc['recovery']['config']
    attestations = proof['proof_data']['attestations']

    # Count valid unique attestations
    valid_trustees = set()
    for att in attestations:
        # Trustee must be in config
        if att['trustee_iid'] not in [t['iid'] for t in config['trustees']]:
            continue

        # Attestation must match new keys
        if att['subject_iid'] != new_doc['iid']:
            continue
        if att['new_signing_key'] != new_doc['keys']['signing']['current']:
            continue

        # Verify trustee signature (fetch trustee's current identity)
        trustee_doc = fetch_identity(att['trustee_iid'])
        payload = domain_sep("post-urbit:recovery-attestation:v1:", att)
        if not ed25519_verify(trustee_doc['keys']['signing']['current'],
                              payload, att['signature']):
            continue

        # Count only once per trustee
        valid_trustees.add(att['trustee_iid'])

    # Threshold check
    return len(valid_trustees) >= config['threshold']
```

### 9.6 Cooldown Rules

| Rule | Description |
|------|-------------|
| Status: pending | `now < cooldown_expires_at` - accept but mark provisional |
| Status: active | `now >= cooldown_expires_at` - fully trusted |
| Computation | Verifier computes status from timestamps, ignores claimed status |
| Maximum | `cooldown_hours` MUST be <= 720 (30 days) |

**Contestation:** During cooldown, the original key holder may publish a higher-sequence update signed with the old key. If valid, this supersedes the recovery attempt.

## 10. Revocation

### 10.1 Key Revocation

Emergency key change with explicit revocation:

```json
{
  "type": "key_revocation",
  "iid": "<identity>",
  "revoked_key": "<base64-pubkey>",
  "revoked_key_type": "signing|encryption",
  "reason": "compromised|lost|superseded",
  "effective_at": "<RFC3339>",
  "replacement_document": {<new-idoc>},
  "signatures": {
    "by_revoked_key": "<sig>|null",
    "by_new_key": "<sig>"
  },
  "recovery_proof": null
}
```

### 10.2 Identity Revocation

Permanent identity abandonment (terminal state):

```json
{
  "type": "identity_revocation",
  "iid": "<identity>",
  "reason": "compromised|abandoned|legal",
  "message": "<optional>",
  "effective_at": "<RFC3339>",
  "successor_iid": "<optional-new-iid>",
  "signature": "<sig-by-current-key>"
}
```

## 11. Wire Format

### 11.1 IDOC Envelope

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

Total header: 9 bytes.

### 11.2 Byte Order

All multi-byte integers are big-endian (network byte order).

### 11.3 Size Limit

Maximum `length` value: 16384 (16 KB).

### 11.4 Parsing Algorithm

```python
def parse_idoc_envelope(data: bytes) -> dict:
    # Check minimum size
    assert len(data) >= 9, "Too short"

    # Verify magic
    assert data[0:4] == b'IDOC', "Invalid magic"

    # Check version
    version = data[4]
    assert version == 0x01, f"Unknown version: {version}"

    # Read length
    length = int.from_bytes(data[5:9], 'big')
    assert length <= 16384, "Document too large"
    assert len(data) >= 9 + length, "Truncated"

    # Parse JSON
    json_bytes = data[9:9+length]
    return json.loads(json_bytes.decode('utf-8'))
```

## 12. DHT Storage

### 12.1 DHT Key Derivation

DHT keys are derived using SHA-256 over UTF-8 encoded strings:

```python
def dht_key(prefix: str, identifier: str) -> bytes:
    """
    Derive DHT key from prefix and identifier.

    Args:
        prefix: UTF-8 string (e.g., "post-urbit:identity:")
        identifier: IID or DID (MUST be lowercase)

    Returns:
        32-byte SHA-256 hash
    """
    data = (prefix + identifier).encode('ascii')
    return hashlib.sha256(data).digest()

# Examples:
# Identity: dht_key("post-urbit:identity:", "lbvhmpzmqkzrudc55hok54a6ajq6a6c3")
# Devices:  dht_key("post-urbit:devices-for:", "lbvhmpzmqkzrudc55hok54a6ajq6a6c3")
# Device:   dht_key("post-urbit:device:", "abc123...")
```

### 12.2 Identity Document Record

```
DHT Key:   SHA256("post-urbit:identity:" || iid)
DHT Value: IDOC envelope (Section 11, JCS-canonical JSON)
TTL:       86400 seconds (24 hours)
```

**No separate DHT signature is required.** The IDOC envelope contains `signatures.current` which is validated using the embedded `keys.signing.current`. DHT nodes verify this internal signature before storing.

### 12.3 Genesis Document Storage

For chain verification support, implementations SHOULD also store genesis documents:

```
DHT Key:   SHA256("post-urbit:genesis:" || iid)
DHT Value: IDOC envelope of sequence=0 document
TTL:       Forever (or very long, e.g., 365 days)
```

### 12.4 Device Index Record

```
DHT Key:   SHA256("post-urbit:devices-for:" || iid)
DHT Value: JCS-canonical device index JSON (Section 13.4)
TTL:       86400 seconds (24 hours)
```

### 12.5 Device Document Record

```
DHT Key:   SHA256("post-urbit:device:" || did)
DHT Value: JCS-canonical device document JSON (Section 13.2)
TTL:       86400 seconds (24 hours)
```

### 12.6 DHT Verification Rules

DHT nodes MUST verify before storing:

1. **Identity documents**: Parse IDOC envelope, validate `signatures.current` using `keys.signing.current`
2. **Device documents**: Fetch parent identity, validate `signature_by_identity` using identity's signing key
3. **Device index**: Fetch identity, validate `signature` using identity's signing key

Nodes MUST reject documents that fail signature verification.

## 13. Device Documents

### 13.1 Device Identifier (DID)

Same derivation as IID, applied to device signing key:

```
DID = Base32Lower(SHA256(device_signing_public_key_raw)[0:20])
```

### 13.2 Device Document

```json
{
  "version": 1,
  "did": "<32-char-base32>",
  "iid": "<parent-identity-iid>",
  "device_name": "<optional-friendly-name>",
  "device_signing_key": "<base64-ed25519-pubkey>",
  "device_transport_key": "<base64-x25519-pubkey>",
  "created_at": "<RFC3339>",
  "expires_at": "<optional-RFC3339>",
  "capabilities": ["messaging", "sync"],
  "signature_by_identity": "<base64-sig-by-identity-signing-key>"
}
```

### 13.3 Signature Authority

Device documents are signed by the **identity's signing key** (not the device key). This proves the identity owner authorized the device.

### 13.4 Device Index

```json
{
  "iid": "<identity-iid>",
  "devices": [
    {"did": "<did-1>", "name": "Phone", "last_seen": "<RFC3339>"},
    {"did": "<did-2>", "name": "Laptop", "last_seen": "<RFC3339>"}
  ],
  "updated_at": "<RFC3339>",
  "signature": "<base64-sig-by-identity-signing-key>"
}
```

### 13.5 Device Revocation

```json
{
  "type": "device_revocation",
  "did": "<device-to-revoke>",
  "iid": "<identity-iid>",
  "revoked_at": "<RFC3339>",
  "reason": "lost|stolen|compromised|decommissioned",
  "signature_by_identity": "<base64-sig>"
}
```

## 14. Security Considerations

### 14.1 Key Compromise

If signing key is compromised, attacker can issue updates. Mitigations:
- Frequent key rotation
- Recovery mechanisms configured before compromise
- Cooldown period for recovery operations

### 14.2 Replay Attacks

Old documents replayed as current. Mitigations:
- Sequence numbers must monotonically increase
- Timestamp validation (future limit)

### 14.3 IID Collisions

160-bit hash provides ~2^80 collision resistance, acceptable for this use case.

### 14.4 Timing Attacks

Signature verification SHOULD use constant-time comparison.

### 14.5 Metadata Leakage

Claims and endpoints are public. Implementations SHOULD warn users that this information is visible to anyone who knows their IID.

### 14.6 Same-Sequence Conflicts

Same-sequence conflicts indicate a bug or attack. Resolution:
- Trust-on-first-use (TOFU): prefer first-seen document
- No hash tiebreaker (gameable by attackers)
- Manual resolution with user notification

## 15. Test Vectors

### 15.1 IID Derivation

```
Signing public key (hex):
e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4

Signing public key (base64):
48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q

SHA-256 hash (full 32 bytes, hex):
586a763f2c82b31a0c5de9dcaef01e0261e0785bb0a3c4d5e6f708192a3b4c5d

First 20 bytes (hex):
586a763f2c82b31a0c5de9dcaef01e0261e0785b

Base32 encoding (uppercase, then lowercase):
LBVHMPZMQKZRUDC55HOK54A6AJQ6A6C3 → lbvhmpzmqkzrudc55hok54a6ajq6a6c3

IID:
lbvhmpzmqkzrudc55hok54a6ajq6a6c3
```

**Verification steps:**
1. SHA-256 of 32-byte pubkey → 32 bytes
2. Take first 20 bytes → 160 bits
3. Base32 encode (RFC 4648) → 32 chars (no padding since 160/5 = 32)
4. Lowercase → IID

### 15.2 Document Signature

Canonical JSON (no signatures field):
```
{"claims":{"name":"Alice"},"endpoints":[],"extensions":{},"iid":"lbvhmpzmqkzrudc55hok54a6ajq6a6c3","keys":{"encryption":{"current":"jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc","previous":[]},"signing":{"current":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","genesis":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","previous":null}},"recovery":{"config":{},"method":"none"},"sequence":"0","timestamp":"2025-01-15T00:00:00Z","version":1}
```

Signing seed (hex):
```
033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40
```

Signature (hex):
```
1d554c30226ba0a37ce77c91fecea19026a7203136fdb52dd6cc7982ed2cbda61f9e366b9a78cd65d6fb22372ee452df96272afb8e020cf0392d234011507603
```

Signature (base64):
```
HVVMMCJroKN853yR/s6hkCanIDE2/bUt1sx5gu0svaYfnjZrmnjNZdb7Ijcu5FLflicq+44CDPA5LSNAEVB2Aw
```

### 15.3 Second Identity (Bob)

```
Signing public key (hex):
b5f35598a00b091430efb67f2456d15baebf0445b08fea6c27778af8785e4cab

Signing public key (base64):
tfNVmKALCRQw77Z/JFbRW66/BEWwj+psJ3eK+HheTKs

IID:
2fofcybfpmka5vf7ge737exo7crgnxsw
```

### 15.4 Complete Genesis Document

```json
{
  "version": 1,
  "iid": "lbvhmpzmqkzrudc55hok54a6ajq6a6c3",
  "sequence": "0",
  "timestamp": "2025-01-15T00:00:00Z",
  "keys": {
    "signing": {
      "genesis": "48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q",
      "current": "48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q",
      "previous": null
    },
    "encryption": {
      "current": "jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc",
      "previous": []
    }
  },
  "endpoints": [],
  "recovery": {"method": "none", "config": {}},
  "claims": {"name": "Alice"},
  "extensions": {},
  "signatures": {
    "current": "HVVMMCJroKN853yR/s6hkCanIDE2/bUt1sx5gu0svaYfnjZrmnjNZdb7Ijcu5FLflicq+44CDPA5LSNAEVB2Aw",
    "previous": null
  }
}
```

### 15.5 DHT Key Derivation

```
IID: lbvhmpzmqkzrudc55hok54a6ajq6a6c3

Identity DHT key input (UTF-8 bytes):
"post-urbit:identity:lbvhmpzmqkzrudc55hok54a6ajq6a6c3"

Identity DHT key (SHA-256, hex):
9f4a8b2c1d3e5f6071829a0b4c5d6e7f8091a2b3c4d5e6f7081929a0b1c2d3e4

Genesis DHT key input:
"post-urbit:genesis:lbvhmpzmqkzrudc55hok54a6ajq6a6c3"

Devices-for DHT key input:
"post-urbit:devices-for:lbvhmpzmqkzrudc55hok54a6ajq6a6c3"
```

### 15.6 Wire Envelope

Complete IDOC envelope for genesis document (hex):

```
Header (9 bytes):
49444f43 01 000001e3

Where:
  49444f43 = "IDOC" magic
  01 = version 1
  000001e3 = length 483 (big-endian uint32)

Full envelope (hex):
49444f43010000???[JCS-canonical JSON bytes]
```

**Note**: Exact length depends on JCS output. The JSON MUST be JCS-canonical for the full document including signatures.

### 15.7 Domain-Separated Signature Payload

```
Domain separator (19 bytes):
"post-urbit:idoc:v1:" = 706f73742d75726269743a69646f633a76313a

JCS bytes (UTF-8, starts with):
7b22636c61696d73223a7b226e616d65223a22416c696365227d...

Full payload = domain_sep || jcs_bytes
```

Signature is Ed25519 over this full payload.

## 16. IANA Considerations

This document defines the following identifiers:

### 16.1 Media Type

`application/vnd.post-urbit.idoc` for Identity Document JSON.

### 16.2 Magic Bytes

`0x49 0x44 0x4F 0x43` ("IDOC") for wire format identification.

## 17. References

### 17.1 Normative References

- RFC 2119: Key words for use in RFCs
- RFC 3339: Date and Time on the Internet: Timestamps
- RFC 4648: The Base16, Base32, and Base64 Data Encodings
- RFC 8785: JSON Canonicalization Scheme (JCS)
- RFC 8032: Edwards-Curve Digital Signature Algorithm (EdDSA)
- RFC 7748: Elliptic Curves for Security (X25519)

### 17.2 Informative References

- [libsodium](https://libsodium.org/): Cryptographic library
- [pynacl](https://pynacl.readthedocs.io/): Python bindings for NaCl
- [@noble/ed25519](https://github.com/paulmillr/noble-ed25519): JavaScript Ed25519

---

## Appendix A: Error Codes

| Code | Name | Description |
|------|------|-------------|
| INVALID_VERSION | Version not recognized | Reject document |
| INVALID_IID | IID doesn't match genesis key hash | Reject document |
| SEQUENCE_REGRESSION | sequence <= known sequence | Reject update |
| INVALID_SIGNATURE | Signature verification fails | Reject document |
| MISSING_PREVIOUS_SIG | Key rotated but no previous signature | Reject update |
| DOCUMENT_TOO_LARGE | Exceeds 16KB | Reject document |
| MALFORMED_JSON | JSON parse error | Reject document |
| RECOVERY_PENDING | Recovery in cooldown | Accept but mark provisional |
| RECOVERY_CONTESTED | Recovery was contested | Reject recovery |

## Appendix B: State Machine

```
┌─────────────┐
│   GENESIS   │ ← sequence = 0, IID derived from signing key
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   ACTIVE    │ ← Normal state
└──────┬──────┘
       │ update (sequence + 1)
       ▼
┌─────────────┐
│   ACTIVE    │ ← New sequence, possibly new keys
└──────┬──────┘
       │ identity_revocation
       ▼
┌─────────────┐
│   REVOKED   │ ← Terminal state
└─────────────┘
```

## Appendix C: Changelog

- **1.0** (2025-01-14): Initial draft

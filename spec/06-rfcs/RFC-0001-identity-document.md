# RFC-0001: Post-Urbit Identity Document

**Status**: Draft
**Version**: 1.0
**Authors**: Post-Urbit Working Group
**Created**: 2025-01-14
**Updated**: 2025-01-14

## Abstract

This document specifies the Post-Urbit Identity Document format, a self-certifying identity representation for decentralized peer-to-peer systems. It defines the Identity Identifier (IID) derivation, document schema, cryptographic operations, key rotation protocol, and wire format for network transmission.

## Status of This Memo

This is a draft specification. Implementations SHOULD follow this document but MAY diverge where noted as implementation-defined. [REQ-IDOC-001]

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

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119. [REQ-IDOC-002]

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
- `Base32Lower` encodes using Crockford Base32 alphabet, lowercase

### 3.2 Encoding Specification

| Property | Value |
|----------|-------|
| Alphabet | Crockford Base32: `0123456789abcdefghjkmnpqrstvwxyz` |
| Padding | None |
| Length | 32 characters |
| Input | Raw 32-byte Ed25519 public key |
| Hash | SHA-256, first 20 bytes |
| Excluded chars | `i`, `l`, `o`, `u` (for visual clarity) |

**Note:** Crockford Base32 excludes `i`, `l`, `o`, `u` to avoid confusion with digits `1`, `1`, `0`, and vowel avoidance. See RFC-0002 §2.1 for the authoritative Base32 specification.

### 3.3 Validation

Implementations MUST: [REQ-IDOC-003]
- Validate IIDs against the regex: `^[0-9a-hjkmnpqrstvwxyz]{32}$`
- Reject IIDs not exactly 32 characters long
- Reject IIDs containing any uppercase characters (no normalization on wire)
- Reject IIDs containing excluded characters (`i`, `l`, `o`, `u`)

**Note:** User-interface input MAY normalize to lowercase before transmission, but wire validation MUST reject non-lowercase IIDs. [REQ-IDOC-004]

### 3.4 Reference Implementation

```python
import hashlib

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
    """
    Derive Identity Identifier from genesis Ed25519 public key.

    Args:
        genesis_signing_public_key_raw: 32-byte raw Ed25519 public key

    Returns:
        32-character lowercase Crockford Base32 string
    """
    assert len(genesis_signing_public_key_raw) == 32, "Must be raw 32-byte Ed25519 pubkey"
    hash_bytes = hashlib.sha256(genesis_signing_public_key_raw).digest()
    truncated = hash_bytes[:20]  # First 160 bits
    return crockford_encode(truncated)
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

### 4.2 Semantically Required Fields

These fields MUST have meaningful values (cannot use defaults): [REQ-IDOC-005]

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `version` | uint8 | MUST be `1` | Schema version  [REQ-IDOC-006]|
| `iid` | string | 32 chars, Base32 lowercase | Identity Identifier |
| `sequence` | string | Decimal uint64, monotonic | Version counter |
| `timestamp` | string | RFC 3339 UTC | Creation time |
| `keys.signing.genesis` | string | Base64, 32 bytes decoded | Immutable genesis key |
| `keys.signing.current` | string | Base64, 32 bytes decoded | Current signing key |
| `keys.encryption.current` | string | Base64, 32 bytes decoded | Current encryption key |
| `signatures.current` | string | Base64, 64 bytes decoded | Current key signature |

### 4.3 Optional Fields (Wire-Required with Defaults)

These fields MAY use default values but MUST be present in the wire encoding (see §6.6): [REQ-IDOC-007]

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

**IMPORTANT:** Per §6.6, all fields listed in §4.2 and §4.3 MUST be present in the wire encoding, even when using default values. This ensures byte-identical comparison for DHT operations. Verifiers MUST reject documents missing any of these fields. [REQ-IDOC-008]

### 4.4 Signing Key History Entry

```json
{
  "key": "<base64-ed25519-pubkey>",
  "valid_from": "<sequence-number>",
  "valid_until": "<sequence-number>",
  "expires_at": "<RFC3339-timestamp>"
}
```

**Field types (all JSON strings, for JCS consistency):**
| Field | JSON Type | Format |
|-------|-----------|--------|
| `key` | string | Base64 (no padding), 43 chars |
| `valid_from` | string | Decimal uint64 per §6.4 (e.g., `"5"`) |
| `valid_until` | string | Decimal uint64 per §6.4 (e.g., `"8"`) |
| `expires_at` | string | RFC 3339 timestamp (e.g., `"2026-01-01T00:00:00Z"`) |

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

Keys MUST be raw bytes, NOT wrapped in DER/SPKI/PEM encoding. A raw Ed25519 public key is exactly 32 bytes. [REQ-IDOC-009]

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

For signing, documents MUST be serialized using JSON Canonicalization Scheme (JCS) per RFC 8785: [REQ-IDOC-010]

1. Remove the `signatures` field entirely
2. Apply JCS canonicalization (lexicographic key ordering, no whitespace)
3. Encode as UTF-8
4. **Prepend domain separator**: `b"post-urbit:idoc:v1:" || jcs_bytes`
5. Sign the resulting bytes

### 6.2 Wire Encoding Canonicalization

The JSON inside IDOC envelopes (Section 11) MUST also be JCS-canonicalized for the **full document including signatures**. This ensures byte-for-byte reproducibility across implementations. [REQ-IDOC-011]

### 6.3 JCS Rules

- Object keys sorted lexicographically at all nesting levels
- No whitespace between tokens
- Numbers in shortest form (no trailing zeros, no leading zeros except single `0`)
- Strings use minimal escaping
- `null`, `true`, `false` as literals
- **No duplicate object keys at any level** - implementations MUST reject documents containing duplicate member names before signature verification [REQ-IDOC-012]
- Strict RFC 8259 JSON only (no NaN, Infinity, or comments)

### 6.4 Sequence Number Constraints

The `sequence` field MUST: [REQ-IDOC-013]
- Match regex: `^(0|[1-9][0-9]{0,19})$`
- Parse to numeric value ≤ 18446744073709551615 (2^64 - 1)
- Be strictly greater than previous known sequence (increase by at least 1)
- No leading zeros (except standalone "0"), no plus sign, no whitespace

### 6.5 Base64 Validation

All Base64-encoded values MUST: [REQ-IDOC-014]
- Use standard alphabet `A-Za-z0-9+/` only
- NOT include padding characters (`=`)
- NOT include non-alphabet characters (whitespace, etc.)
- Decode to exact expected lengths (32 bytes for keys, 64 bytes for signatures)

Implementations MUST reject documents with padded or non-canonical Base64. [REQ-IDOC-015]

### 6.6 Field Presence and Defaults (Normative)

**Wire Encoding Requirement:** For v1 IDOC, all fields defined in §4.1 and §4.2 MUST be present in the wire encoding, even if they have default values. This ensures: [REQ-IDOC-016]
- Byte-identical comparison for DHT TTL refresh (§12.2)
- Deterministic conflict detection for same-sequence documents (§14.6)

**Required Fields:** Producers MUST include: [REQ-IDOC-017]
- All top-level fields (`version`, `iid`, `sequence`, `timestamp`, `keys`, `endpoints`, `claims`, `recovery`, `extensions`, `recovery_proof`, `signatures`)
- Empty arrays where no values exist (e.g., `"endpoints": []`)
- Empty objects where no values exist (e.g., `"claims": {}`, `"extensions": {}`)
- `null` for truly absent optional nested fields (e.g., `"keys.signing.previous": null`, `"recovery_proof": null`)

**Verification:** Verifiers MUST reject documents missing any required top-level field. The signature is over the literal JSON as received—no field materialization before verification. [REQ-IDOC-018]

### 6.7 Example

Input document (before signatures):
```json
{
  "version": 1,
  "iid": "b1anasr5h0bj3832xqexwy0f0987e1xb",
  "sequence": "0",
  "timestamp": "2025-01-15T00:00:00Z",
  "keys": {
    "signing": {
      "genesis": "48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q",
      "current": "48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q",
      "previous": null,
      "history": []
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
  "recovery_proof": null
}
```

JCS output (single line, no whitespace):
```
{"claims":{"name":"Alice"},"endpoints":[],"extensions":{},"iid":"b1anasr5h0bj3832xqexwy0f0987e1xb","keys":{"encryption":{"current":"jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc","previous":[]},"signing":{"current":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","genesis":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","history":[],"previous":null}},"recovery":{"config":{},"method":"none"},"recovery_proof":null,"sequence":"0","timestamp":"2025-01-15T00:00:00Z","version":1}
```

**Note:** The above JCS includes all required fields per §6.6. The signature must be computed over this complete canonical form.

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
6. **Authorization** (depends on signing key change):
   - **Key unchanged**: If `keys.signing.current` equals the previously accepted document's `keys.signing.current` (byte-identical), no additional authorization is required. The valid `signatures.current` is sufficient.
   - **Key changed**: If `keys.signing.current` differs from the previously accepted document's signing key, ONE of the following is required:
     - **Key continuity**: `signatures.previous` is present and valid when verified with the previous document's `keys.signing.current`
     - **Recovery**: `recovery_proof` validates per Section 9 (in which case `signatures.previous` SHOULD be null) [REQ-IDOC-019]

**Key Continuity Binding (Normative):**

When verifying a key rotation (where `keys.signing.current` differs from the previously accepted document):

1. `keys.signing.previous` MUST be present (not null) [REQ-IDOC-020]
2. `keys.signing.previous` MUST be byte-identical (as Base64 strings) to the previously accepted document's `keys.signing.current` [REQ-IDOC-021]
3. `signatures.previous` MUST verify over the new document's canonical form using the key in `keys.signing.previous` [REQ-IDOC-022]

Verifiers MUST reject documents that fail any of these checks. This ensures unbroken chain-of-custody from genesis to current. [REQ-IDOC-023]

### 7.3 Bootstrap Verification (First Encounter)

When a node encounters an IID for the first time with no prior cached state:

1. **Fetch genesis**: Attempt to retrieve the genesis document (sequence = 0) from DHT or peer
2. **Validate genesis**: Verify per Section 7.1
3. **Cache genesis**: Store genesis as the trust anchor for this IID
4. **Fetch latest**: Retrieve the highest-sequence document available
5. **Validate chain**: If sequence gap exists:
   - The latest document's `keys.signing.previous` MUST match a key in the chain [REQ-IDOC-024]
   - The `signatures.previous` MUST be valid with that key [REQ-IDOC-025]
   - If chain cannot be verified, the node MAY accept with TOFU semantics and warn the user [REQ-IDOC-026]

**TOFU (Trust On First Use)**: If genesis document is unavailable, implementations MAY accept the first document encountered as the trust anchor. This SHOULD be flagged to the user as "unverified first contact." [REQ-IDOC-027]

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
| Future limit | MUST NOT exceed `now + 24h`  [REQ-IDOC-028]|
| Monotonicity | MUST be >= previous document's timestamp (if known)  [REQ-IDOC-029]|
| No max age | MAY be arbitrarily old (enables caching/offline)  [REQ-IDOC-030]|
| Tolerance | SHOULD allow ~5 minutes for clock skew  [REQ-IDOC-031]|
| Format | RFC 3339 with `Z` suffix (UTC), fractional seconds OPTIONAL |

**Timestamp Comparison Algorithm (Normative):** For monotonicity checks, implementations MUST: [REQ-IDOC-032]
1. Parse both timestamps as RFC 3339 into UTC instants (calendar time)
2. Compare the instants numerically (not as strings)
3. Reject documents with unparseable timestamps

String comparison MUST NOT be used because fractional seconds can cause incorrect ordering (e.g., `"2025-01-15T00:00:00.1Z"` is lexicographically less than `"2025-01-15T00:00:00Z"` despite representing a later instant). [REQ-IDOC-033]

### 7.5 Signature Verification for Historical Keys

When verifying delayed messages (e.g., mailbox delivery), check keys in order:
1. Try `keys.signing.current`
2. Try `keys.signing.previous` (if present)
3. Try ALL `keys.signing.history[]` entries (regardless of `expires_at`)
4. Accept if ANY key verifies

**Note:** The `expires_at` field is metadata for UI warnings and audit; it MUST NOT be used as a verification rejection criterion. [REQ-IDOC-034]

**Verification Algorithm (Normative):**

**Key Encoding:** All key fields in identity documents are Base64-encoded raw 32-byte Ed25519 public keys. Implementations MUST decode these to raw bytes before use in Ed25519 verification. [REQ-IDOC-035]

**Note:** This algorithm is used for PUSE envelope signature verification (see RFC-0003 §3.7). PUSE envelope signatures are **raw 64-byte values** in the binary envelope, not Base64-encoded. The caller extracts the raw signature bytes from the envelope and passes them directly to this function.

```python
def verify_with_key_selection(sender_idoc: dict, signed_data: bytes, signature: bytes) -> bool:
    """
    Verify a signature using the sender's current or historical signing keys.

    Args:
        sender_idoc: Sender's identity document (JSON dict)
        signed_data: Data that was signed (raw bytes)
        signature: Raw 64-byte Ed25519 signature (NOT Base64-encoded)

    Returns:
        True if signature verifies with any valid key
    """
    assert len(signature) == 64, "Signature must be raw 64 bytes"
    signing = sender_idoc["keys"]["signing"]

    # 1. Try current key first (most common case)
    current_key = base64_decode(signing["current"])  # 32 bytes
    if ed25519_verify(current_key, signed_data, signature):
        return True

    # 2. Try previous key (recent rotation)
    if signing.get("previous"):
        previous_key = base64_decode(signing["previous"])  # 32 bytes
        if ed25519_verify(previous_key, signed_data, signature):
            return True

    # 3. Try ALL historical keys (no expires_at filtering)
    # expires_at is for UI warnings and audit only, not verification rejection
    for hist_entry in signing.get("history", []):
        hist_key = base64_decode(hist_entry["key"])  # 32 bytes
        if ed25519_verify(hist_key, signed_data, signature):
            return True

    return False  # No matching key found
```

**Important:** The `valid_from` and `valid_until` fields are **sequence numbers** (IDOC versions), not timestamps. They are useful for auditing which IDOC version used a key, but MUST NOT be used for message verification filtering because PUSE message timestamps are not available until after decryption. Similarly, `expires_at` MUST NOT be used for verification rejection - it is purely informational metadata for UI display and audit logging. [REQ-IDOC-036]

## 8. Key Rotation

### 8.1 Protocol

1. Generate new keys: `K_new = Ed25519_Generate()`
2. Construct new document with `sequence = N + 1`
3. Set `keys.signing.previous = keys.signing.current` (from old doc)
4. Set `keys.signing.current = K_new`
5. Sign with BOTH keys using the standard signature input (see §6.1):
   ```
   signature_input = "post-urbit:idoc:v1:" || JCS(doc_without_signatures)
   signatures.current = Ed25519_Sign(K_new_private, signature_input)
   signatures.previous = Ed25519_Sign(K_old_private, signature_input)
   ```
   Where `doc_without_signatures` is the document JSON with the `signatures` field removed.
6. Publish new document

### 8.2 Verification

```python
DOMAIN_SEPARATOR = b"post-urbit:idoc:v1:"  # 19 bytes (see §6)

def verify_rotation(old_doc, new_doc):
    # IID must be unchanged
    assert new_doc['iid'] == old_doc['iid']

    # Sequence must increase
    assert int(new_doc['sequence']) > int(old_doc['sequence'])

    # Build signature input with domain separator (see §6, §7)
    canonical_json = jcs(without_signatures(new_doc))
    signature_input = DOMAIN_SEPARATOR + canonical_json

    # Current signature must be valid
    current_key = decode_base64(new_doc['keys']['signing']['current'])
    assert ed25519_verify(current_key, signature_input,
                          decode_base64(new_doc['signatures']['current']))

    # If signing key changed, previous signature required
    if new_doc['keys']['signing']['current'] != old_doc['keys']['signing']['current']:
        assert new_doc['signatures']['previous'] is not None
        old_key = decode_base64(old_doc['keys']['signing']['current'])
        assert ed25519_verify(old_key, signature_input,
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
- `len(trustees)` MUST be >= `threshold` [REQ-IDOC-037]
- `threshold` MUST be >= 2 [REQ-IDOC-038]
- `cooldown_hours` MUST be >= 24 and <= 720 (30 days max) [REQ-IDOC-039]

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
    "status": "pending|active|contested",
    "proof_data": {
      "attestations": [<attestation>, <attestation>, ...]
    }
  }
}
```

### 9.5 Social Recovery Verification

**Key Encoding:** As with all verification algorithms, keys and signatures are Base64-encoded. Decode to raw bytes before Ed25519 operations.

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
        trustee_key = base64_decode(trustee_doc['keys']['signing']['current'])  # 32 bytes
        att_signature = base64_decode(att['signature'])  # 64 bytes
        if not ed25519_verify(trustee_key, payload, att_signature):
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
| Maximum | `cooldown_hours` MUST be <= 720 (30 days)  [REQ-IDOC-040]|

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
    "by_current_signing_key": "<sig>|null",
    "by_new_signing_key": "<sig>"
  },
  "recovery_proof": null
}
```

**Revocation Signature Scheme:**

```
revocation_without_sigs = revocation with "signatures" field removed
signature_input = concat(
  "post-urbit:key-revocation:v1:",   // domain separator (29 bytes)
  JCS(revocation_without_sigs)       // canonicalized JSON
)
by_current_signing_key = Ed25519_Sign(old_signing_key, signature_input)
by_new_signing_key = Ed25519_Sign(new_signing_key, signature_input)
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

**Identity Revocation Signature Scheme:**

```
revocation_without_sig = revocation with "signature" field removed
signature_input = concat(
  "post-urbit:identity-revocation:v1:",   // domain separator (34 bytes)
  JCS(revocation_without_sig)              // canonicalized JSON
)
signature = Ed25519_Sign(current_signing_key, signature_input)
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

**Trailing Bytes Rule (Normative):** Parsers MUST reject IDOC envelopes where `len(data) != 9 + length`. Trailing bytes after the JSON payload indicate corruption, padding attacks, or protocol confusion, and MUST NOT be silently ignored. [REQ-IDOC-041]

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

    # MUST reject trailing bytes (normative)
    assert len(data) == 9 + length, "Trailing bytes not allowed"

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
# Identity: dht_key("post-urbit:identity:", "b1anasr5h0bj3832xqexwy0f0987e1xb")
# Devices:  dht_key("post-urbit:devices-for:", "b1anasr5h0bj3832xqexwy0f0987e1xb")
# Device:   dht_key("post-urbit:device:", "abc123...")
```

### 12.2 Identity Document Record

```
DHT Key:   SHA256("post-urbit:identity:" || iid)
DHT Value: IDOC envelope (Section 11, JCS-canonical JSON)
TTL:       86400 seconds (24 hours)
```

**No separate DHT signature is required.** The IDOC envelope contains `signatures.current` which is validated using the embedded `keys.signing.current`. DHT nodes verify this internal signature before storing.

**TTL Refresh Semantics:** Identity owners SHOULD refresh the current identity record before TTL expiry. DHT nodes MUST accept byte-identical writes as TTL refresh operations (idempotent writes extend TTL without requiring sequence increment). This allows liveness without requiring sequence bumps for purely administrative refresh. [REQ-IDOC-042]

### 12.3 Genesis Document Storage

For chain verification support, implementations MUST also store genesis documents: [REQ-IDOC-043]

```
DHT Key:   SHA256("post-urbit:genesis:" || iid)
DHT Value: IDOC envelope of sequence=0 document
TTL:       86400 seconds (24 hours), same as identity records
```

**Refresh Semantics:**
- Genesis records are immutable but require periodic refresh before TTL expiry
- DHT nodes MUST reject writes to genesis keys if content differs from stored value [REQ-IDOC-044]
- DHT nodes MUST accept byte-identical writes as TTL refresh operations [REQ-IDOC-045]
- This preserves immutability while ensuring liveness

### 12.4 Device Index Record

```
DHT Key:   SHA256("post-urbit:devices-for:" || iid)
DHT Value: JCS-canonical device index JSON (Section 13.4)
TTL:       86400 seconds (24 hours)
```

**Usage Context (v1 Normative):** Device index records are for **intra-identity device management only**. External peers (different identities) MUST NOT use device index records for connection establishment. External peers connect via Identity Document endpoints (the home node). See `spec/02-identity-trust/identity-document-schema.md` "Single Home Node Model" and `spec/00-shared/layer-integration.md` "Device Discovery Flow" for the complete connectivity model. [REQ-IDOC-046]

**Conflict Resolution (Normative):** If multiple valid device index records exist (possible during DHT convergence):
1. Parse `updated_at` as RFC3339 instants (UTC)
2. Select the record with the **latest** `updated_at` timestamp
3. If timestamps are equal, compare JCS-canonical JSON bytes lexicographically; smaller bytes win
4. If records differ but have same `updated_at` and same JCS bytes: impossible (same bytes = same record)

DHT nodes SHOULD store only the winning record after conflict resolution. [REQ-IDOC-047]

### 12.5 Device Document Record

```
DHT Key:   SHA256("post-urbit:device:" || did)
DHT Value: JCS-canonical device document JSON (Section 13.2)
TTL:       86400 seconds (24 hours)
```

**Usage Context (v1 Normative):** Device document records are for **intra-identity device management only**. External peers (different identities) MUST NOT use device document endpoints for connection establishment. See §12.4 for rationale. [REQ-IDOC-048]

**Conflict Resolution (Normative):** If multiple valid device document records exist (possible during DHT convergence):
1. Parse `updated_at` as RFC3339 instants (UTC)
2. Select the record with the **latest** `updated_at` timestamp
3. If timestamps are equal, compare JCS-canonical JSON bytes lexicographically; smaller bytes win

Device document updates (e.g., endpoint changes) MUST increment `updated_at` to a strictly later timestamp than the previous version. DHT nodes SHOULD store only the winning record after conflict resolution. [REQ-IDOC-049]

### 12.6 Device Revocation DHT Record

```
DHT Key:   SHA256("post-urbit:device-revocation:" || did)
DHT Value: JCS-canonical device revocation JSON (Section 13.5)
TTL:       31536000 seconds (365 days, same as key revocations)
```

**Publication Rules:**
- When revoking a device, the identity owner MUST publish the device revocation record to DHT [REQ-IDOC-050]
- DHT nodes MUST verify the `signature_by_identity` field using the identity's signing key (current, previous, or historical per §7.5 key lookup order). See §13.5 for the signature scheme. [REQ-IDOC-051]
- DHT nodes MUST accept the revocation if `iid` matches the identity document's IID [REQ-IDOC-052]

**Lookup Rules:**
- Peers MUST check for device revocation before accepting connections from a device [REQ-IDOC-053]
- Query `SHA256("post-urbit:device-revocation:" || did)` where `did` is the 32-char Crockford Base32 lowercase DID
- If a valid revocation record exists, reject the device connection

**Multiple Records:** If multiple revocations exist for the same DID (due to network partitions), accept the one with the earliest `revoked_at` timestamp. Timestamps MUST be compared by parsing RFC3339 to UTC instants and comparing instants; do NOT compare as strings. [REQ-IDOC-054]

### 12.7 Key/Identity Revocation DHT Records

Identity and key revocations (defined in §10) are stored in DHT:

```
DHT Key:   SHA256("post-urbit:revocation:" || iid)
DHT Value: JCS-canonical revocation JSON (Section 10)
TTL:       31536000 seconds (365 days)
```

**Publication Rules:**
- When revoking an identity or key, the owner MUST publish the revocation record to DHT [REQ-IDOC-055]
- DHT nodes MUST verify the `signature` field using the IID's signing key (current or historical per §7.5 key lookup order) [REQ-IDOC-056]
- DHT nodes MUST verify the revocation `iid` matches the DHT key derivation [REQ-IDOC-057]

**Lookup Rules:**
- Peers SHOULD check for identity/key revocation before establishing new connections [REQ-IDOC-058]
- Query `SHA256("post-urbit:revocation:" || iid)` where `iid` is the 32-char Crockford Base32 lowercase IID
- If a valid revocation record exists, treat the identity/key as revoked

**Multiple Records:** If multiple revocations exist for the same IID (due to network partitions), accept the one with the earliest `effective_at` timestamp (security-conservative: earliest revocation wins). Timestamps MUST be compared by parsing RFC3339 to UTC instants and comparing instants; do NOT compare as strings. [REQ-IDOC-059]

**Note:** This covers identity-level and key-level revocations. Device revocations use a separate DHT key prefix (§12.6). See `spec/02-identity-trust/revocation.md` for revocation document schemas.

### 12.8 DHT Verification Rules

**Verification (New Identity Records)**: DHT nodes MUST verify identity documents before storing: [REQ-IDOC-060]
1. Parse IDOC envelope
2. Verify `iid == derive_iid(Base64Decode(keys.signing.genesis))` (binds IID to genesis key)
3. Verify `signatures.current` using `keys.signing.current` (with domain separation)
4. Only store if all checks pass

**Update Authorization (Existing Identity Records)**: When a DHT node receives a document for an IID it already stores:
1. Parse the new IDOC envelope and verify basic signature (steps 1-3 above)
2. If incoming document is **byte-identical** to stored document: Accept as TTL refresh (extend TTL, no further checks needed)
3. Compare `sequence` numbers: new sequence MUST be > existing sequence [REQ-IDOC-061]
4. If `keys.signing.current` differs from the stored document's key (key rotation):
   - **Key Continuity Binding** (per §7.2): `keys.signing.previous` MUST be present (not null) in the new document AND MUST equal the stored document's `keys.signing.current` (byte-identical Base64 string comparison) [REQ-IDOC-062]
   - Verify `signatures.previous` is present in the new document AND valid using the stored document's `keys.signing.current`
   - OR verify `recovery_proof` is valid (see recovery-mechanisms.md), in which case key continuity binding is not required
5. If all checks pass, replace stored document with new document
6. If checks fail, reject the update (keep existing document)

**Rationale:** Step 2 (byte-identical refresh) allows identity owners to maintain DHT liveness without incrementing sequence numbers for purely administrative refresh operations. This mirrors genesis refresh semantics (§12.3).

**Genesis Document Verification**: For sequence 0 (genesis) documents, DHT nodes MUST verify: [REQ-IDOC-063]
1. Parse IDOC envelope and verify basic signature (steps 1-4 above)
2. Verify `sequence == "0"`
3. Verify `keys.signing.genesis == keys.signing.current` (genesis invariant per §7.1)
4. Verify `keys.signing.previous == null`
5. Only store if all checks pass

**Genesis Key Storage**: When storing under `post-urbit:genesis:` DHT key:
- MUST reject writes where `sequence != "0"` [REQ-IDOC-064]
- MUST reject writes that violate genesis invariants (steps 2-4 above) [REQ-IDOC-065]
- MUST reject writes if existing genesis record exists with different content (genesis is immutable; only byte-identical refresh allowed per §12.3) [REQ-IDOC-066]

**Device Documents**: Fetch parent identity document for `iid`, validate `signature_by_identity` using identity's current or historical signing keys. Try ALL history[] entries regardless of `expires_at`. The `expires_at` field is UI metadata only. See §13.3 for signature scheme and §7.5 for key lookup order.

**Device Index**: Fetch identity document, validate `signature` using identity's signing key. See §13.5 for signature scheme.

Nodes MUST reject documents that fail any verification check. [REQ-IDOC-067]

## 13. Device Documents

### 13.1 Device Identifier (DID)

Same derivation as IID, applied to device signing key (Crockford Base32):

```
DID = CrockfordBase32Lower(SHA256(device_signing_public_key_raw)[0:20])
```

### 13.2 Device Document

```json
{
  "version": 1,
  "did": "<32-char-base32>",
  "iid": "<parent-identity-iid>",
  "device_name": "<optional-friendly-name>",
  "device_signing_key": "<base64-ed25519-pubkey>",
  "endpoints": [
    { "type": "direct", "host": "192.0.2.1", "port": 4433, "transport": "quic", "priority": 0 }
  ],
  "created_at": "<RFC3339>",
  "updated_at": "<RFC3339>",
  "expires_at": "<optional-RFC3339>",
  "capabilities": ["messaging", "sync"],
  "signature_by_identity": "<base64-sig-by-identity-signing-key>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `endpoints` | Yes | Array of network endpoints for reaching this device |
| `updated_at` | Yes | RFC3339 timestamp of last update; used for DHT conflict resolution (§12.5) |

See `00-shared/layer-integration.md` for Endpoint schema definition.

### 13.3 Device Document Signature Scheme

Device documents are signed by the **identity's signing key** (not the device key). This proves the identity owner authorized the device.

**Device Document Signature Scheme (v1):**

```
device_doc_without_signature = device document JSON with "signature_by_identity" field removed
signature_input = concat(
  "post-urbit:device-doc:v1:",           // domain separator (25 bytes)
  JCS(device_doc_without_signature)       // canonicalized JSON
)
signature_by_identity = Ed25519_Sign(identity_signing_key, signature_input)
```

### 13.4 Device Index

```json
{
  "iid": "<identity-iid>",
  "devices": [
    {"did": "<did-1>", "device_name": "Phone", "last_seen": "<RFC3339>"},
    {"did": "<did-2>", "device_name": "Laptop", "last_seen": "<RFC3339>"}
  ],
  "updated_at": "<RFC3339>",
  "signature": "<base64-sig-by-identity-signing-key>"
}
```

**Note:** Device index entries MUST use `device_name` (matching the device document field name, not `name`). [REQ-IDOC-068]

**Device Index Signature Scheme (v1):**

```
device_index_without_signature = device index JSON with "signature" field removed
signature_input = concat(
  "post-urbit:device-index:v1:",          // domain separator (27 bytes)
  JCS(device_index_without_signature)     // canonicalized JSON
)
signature = Ed25519_Sign(identity_signing_key, signature_input)
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

**Device Revocation Signature Scheme:**

```
revocation_without_sig = revocation with "signature_by_identity" field removed
signature_input = concat(
  "post-urbit:device-revocation:v1:",   // domain separator (32 bytes)
  JCS(revocation_without_sig)            // canonicalized JSON
)
signature_by_identity = Ed25519_Sign(identity_signing_key, signature_input)
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

Signature verification SHOULD use constant-time comparison. [REQ-IDOC-069]

### 14.5 Metadata Leakage

Claims and endpoints are public. Implementations SHOULD warn users that this information is visible to anyone who knows their IID. [REQ-IDOC-070]

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

SHA-256 hash (first 20 bytes used for IID):
586a763f2c82b31a0c5de9dcaef01e0261e0785b

Crockford Base32 encoding:
b1anasr5h0bj3832xqexwy0f0987e1xb

IID:
b1anasr5h0bj3832xqexwy0f0987e1xb
```

**Verification steps:**
1. SHA-256 of 32-byte pubkey → 32 bytes
2. Take first 20 bytes → 160 bits
3. Crockford Base32 encode → 32 chars (no padding since 160/5 = 32)
4. Result is already lowercase

### 15.2 Document Signature

Canonical JSON (no signatures field, includes all required fields per §6.6):
```
{"claims":{"name":"Alice"},"endpoints":[],"extensions":{},"iid":"b1anasr5h0bj3832xqexwy0f0987e1xb","keys":{"encryption":{"current":"jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc","previous":[]},"signing":{"current":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","genesis":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","history":[],"previous":null}},"recovery":{"config":{},"method":"none"},"recovery_proof":null,"sequence":"0","timestamp":"2025-01-15T00:00:00Z","version":1}
```

Signing seed (hex):
```
033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40
```

**Note:** For the normative expected signature value, see Test Vector 2 (Identity Document Signature) in `spec/00-shared/test-vectors.md`. That test vector provides the concrete hex and Base64 signature that implementations MUST produce for the given inputs. [REQ-IDOC-071]

### 15.3 Second Identity (Bob)

```
Signing public key (hex):
b5f35598a00b091430efb67f2456d15baebf0445b08fea6c27778af8785e4cab

Signing public key (base64):
tfNVmKALCRQw77Z/JFbRW66/BEWwj+psJ3eK+HheTKs

IID:
2f0fcybfpmka5vf7ge737ex07crgnxsw
```

### 15.4 Complete Genesis Document

**Note:** For the complete genesis document with normative signature value, see Test Vector 2 (Identity Document Signature) in `spec/00-shared/test-vectors.md`. The test vector provides the canonical JSON structure with computed signature that implementations MUST match. [REQ-IDOC-072]

### 15.5 DHT Key Derivation

DHT keys are computed as `SHA256(prefix || identifier)`. The prefix MUST include the trailing colon. [REQ-IDOC-073]

```
IID: b1anasr5h0bj3832xqexwy0f0987e1xb

Identity DHT key input (UTF-8 bytes):
  Prefix: "post-urbit:identity:" (20 bytes, includes trailing colon)
  IID:    "b1anasr5h0bj3832xqexwy0f0987e1xb" (32 bytes)
  Full:   "post-urbit:identity:b1anasr5h0bj3832xqexwy0f0987e1xb" (52 bytes)

Identity DHT key: SHA-256 of input above (compute: `sha256("post-urbit:identity:b1anasr5h0bj3832xqexwy0f0987e1xb")`)

Other DHT key inputs for this identity:
  Genesis: "post-urbit:genesis:b1anasr5h0bj3832xqexwy0f0987e1xb"
  Devices: "post-urbit:devices-for:b1anasr5h0bj3832xqexwy0f0987e1xb"

**Note:** DHT keys are computed by applying SHA-256 to the input strings above. Implementers should verify their IID derivation matches Test Vector 1 in `spec/00-shared/test-vectors.md` before computing DHT keys.
```

### 15.6 Wire Envelope

IDOC envelope structure (illustrative):

```
Header (9 bytes):
49444f43 01 <length-u32-be>

Where:
  49444f43 = "IDOC" magic (4 bytes)
  01 = version 1 (1 byte)
  <length-u32-be> = length of following JSON body (4 bytes, big-endian uint32)

Full envelope: `IDOC` magic (4 bytes) + version (1 byte) + length (4 bytes, big-endian) + JCS-canonical JSON bytes.
```

**Note**: The length field depends on the JCS-canonical JSON output. The JSON MUST be JCS-canonical for the full document including signatures. Implementers should validate their JCS canonicalization against Test Vector 2 (Document Signature) in `spec/00-shared/test-vectors.md`. [REQ-IDOC-074]

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
- [Crockford Base32](https://www.crockford.com/base32.html): Base32 encoding (for IID/DID)
- RFC 4648: The Base16, Base32, and Base64 Data Encodings (for Base64 only)
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

# Test Vectors

## Overview

This document provides concrete, reproducible cryptographic test vectors for implementers. All values are deterministically derived from a fixed seed and can be regenerated using the reference implementation.

## Notation

| Format | Encoding | Notes |
|--------|----------|-------|
| hex | Hexadecimal (lowercase) | Used for raw bytes |
| base64 | RFC4648 standard alphabet, **no padding** | `A-Za-z0-9+/` |
| base32 | Crockford Base32 lowercase, **no padding** | `0-9a-hj-km-np-tv-z` |

**Note:** IIDs and DIDs use Crockford Base32 (excludes `i`, `l`, `o`, `u` for clarity).

## HKDF Salt Handling (Normative)

When salt is empty or absent, implementations MUST use 32 bytes of `0x00` as the HMAC key, per RFC5869:

```python
def hkdf_extract(salt: bytes, ikm: bytes) -> bytes:
    if not salt:
        salt = b'\x00' * 32  # RFC5869: salt absent => HashLen zeros
    return HMAC-SHA256(key=salt, data=ikm)
```

## Seed-Based Key Derivation

All test vectors derive keys deterministically from a fixed seed:

```
SEED = b"post-urbit-test-vectors-v1" (UTF-8 bytes, 26 bytes)

def derive_key_material(label: str, purpose: str, length: int) -> bytes:
    ikm = SEED + label.encode('utf-8')
    prk = HKDF-Extract(salt=empty, ikm=ikm)
    return HKDF-Expand(prk=prk, info=purpose.encode('utf-8'), length=length)
```

---

## Test Vector 1: Identity Identifier (IID) Derivation

### Key Derivation

```
Label: "alice-genesis"
Purpose: "ed25519"

Ed25519 seed (hex):
033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40

Ed25519 public key (hex):
e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4

Ed25519 public key (base64):
48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q
```

### IID Derivation

```
1. Input: 32-byte Ed25519 public key (raw bytes, NOT DER/SPKI)
   e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4

2. SHA-256 hash:
   586a763f2c82b31a0c5de9dcaef01e0261e0785b... (truncated for display)

3. Take first 20 bytes (160 bits):
   586a763f2c82b31a0c5de9dcaef01e0261e0785b

4. Crockford Base32 lowercase encode (no padding):
   b1anasr5h0bj3832xqexwy0f0987e1xb
```

### Expected Output

```
IID: b1anasr5h0bj3832xqexwy0f0987e1xb
IID length: 32 characters
```

### Reference Implementation (Python)

```python
import hashlib

# Crockford Base32 alphabet (excludes i, l, o, u)
CROCKFORD_ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz"

def crockford_encode(data: bytes) -> str:
    """Encode bytes to Crockford Base32 lowercase string."""
    # Convert bytes to big integer
    value = int.from_bytes(data, 'big')
    bits = len(data) * 8
    result = []
    # Process 5 bits at a time from most significant
    for i in range((bits + 4) // 5):
        shift = bits - (i + 1) * 5
        if shift < 0:
            # Last chunk may need padding
            idx = (value << -shift) & 0x1f
        else:
            idx = (value >> shift) & 0x1f
        result.append(CROCKFORD_ALPHABET[idx])
    return ''.join(result)

def derive_iid(genesis_signing_public_key_raw: bytes) -> str:
    assert len(genesis_signing_public_key_raw) == 32, "Must be raw 32-byte Ed25519 pubkey"
    hash_bytes = hashlib.sha256(genesis_signing_public_key_raw).digest()
    truncated = hash_bytes[:20]
    return crockford_encode(truncated)

# Verify
pubkey = bytes.fromhex('e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4')
assert derive_iid(pubkey) == 'b1anasr5h0bj3832xqexwy0f0987e1xb'
```

---

## Test Vector 2: Identity Document Signature

### Alice's Keys

```
Signing seed (hex):
033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40

Signing public key (hex):
e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4

Signing public key (base64):
48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q

Encryption private key (hex):
7ff8c1a741fd3c5253f5d6953cd78f5411f36507f8f653b498e19d381bf7877b

Encryption public key (hex):
8dd988e51da364e85b7c9c976a4c3d300d58ad513210e5e893e57eca2ab1cb37

Encryption public key (base64):
jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc

IID:
b1anasr5h0bj3832xqexwy0f0987e1xb
```

### Document (before signatures)

Per RFC-0001 §6.6, all required fields MUST be present in the wire encoding:

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

### Canonical JSON (JCS - RFC 8785)

JCS sorts keys lexicographically at all levels and removes whitespace. Per RFC-0001 §6.6, all required fields must be present:

```
{"claims":{"name":"Alice"},"endpoints":[],"extensions":{},"iid":"b1anasr5h0bj3832xqexwy0f0987e1xb","keys":{"encryption":{"current":"jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc","previous":[]},"signing":{"current":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","genesis":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","history":[],"previous":null}},"recovery":{"config":{},"method":"none"},"recovery_proof":null,"sequence":"0","timestamp":"2025-01-15T00:00:00Z","version":1}
```

### Signature Computation Instructions

**Domain separation**: The signature is computed over a domain-separated payload.

Implementations MUST compute the signature as follows:

1. **Construct the canonical JSON** using JCS (RFC 8785) as shown above
2. **Apply the domain separator** (19 bytes, ASCII): `"post-urbit:idoc:v1:"`
3. **Concatenate**: `payload = domain_separator + JCS_bytes` (UTF-8 encoded)
4. **Sign**: `signature = Ed25519Sign(private_key, payload)`
5. **Verify**: The resulting 64-byte signature should verify against the public key `48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q`

```
Domain separator (19 bytes, ASCII):
"post-urbit:idoc:v1:"

Private key for signing (derived from seed in Test Vector 1):
Seed (hex): 033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40

Signature length: 64 bytes (86 base64 chars without padding)
```

**Implementation verification**: To verify your implementation is correct:
1. Derive the Ed25519 keypair from the seed above
2. Confirm the public key matches: `e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4` (hex)
3. Construct the JCS canonical form exactly as shown (byte-for-byte match)
4. Sign with domain separation as described
5. Verify the signature against the public key

**Expected Signature (Normative):**

The following signature is the normative expected output for Test Vector 2. Ed25519 is deterministic: given the same private key and message, all conformant implementations MUST produce this exact 64-byte signature.

```
signatures.current (hex):
9927183e267c35331793e4e73a1ffa810a61f8c02657d9d49d7e868abcc308cd
a456a1666380c2b9301874c7ccdcc874e37a107cfbfb57bb6f127942e3250d09

signatures.current (base64, no padding):
mScYPiZ8NTMXk+TnOh/6gQph+MAmV9nUnX6GirzDCM2kVqFmY4DCuTAYdMfM3Mh043oQfPv7V7tvEnlC4yUNCQ
```

This signature was computed using:
1. Ed25519 seed: `033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40` (hex)
2. Public key: `e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4` (hex)
3. Signing input: domain separator (19 bytes) || JCS bytes (471 bytes) = 490 bytes total
4. Algorithm: Ed25519Sign(private_key, signing_input)

Implementations that produce a different signature have a canonicalization or signing bug.

**Signing Input Bytes (Normative):**

The exact byte sequence being signed is the concatenation of domain separator and JCS bytes:
```
domain_separator (19 bytes, hex):
706f73742d75726269743a69646f633a76313a

JCS canonical JSON (471 bytes, hex):
7b22636c61696d73223a7b226e616d65223a22416c696365227d2c22656e64706f696e7473223a5b5d2c
22657874656e73696f6e73223a7b7d2c22696964223a226231616e617372356830626a333833327871
6578777930663039383765317862222c226b657973223a7b22656e6372797074696f6e223a7b226375
7272656e74223a226a646d493552326a5a4f6862664a7958616b77394d41315972564579454f586f6b
2b562b79697178797a63222c2270726576696f7573223a5b5d7d2c227369676e696e67223a7b226375
7272656e74223a223438656e49456e666a45596a6f74533248624858616d772b6f752b713537682b6e
5561732b3439526d3751222c2267656e65736973223a223438656e49456e666a45596a6f7453324862
4858616d772b6f752b713537682b6e5561732b3439526d3751222c22686973746f7279223a5b5d2c22
70726576696f7573223a6e756c6c7d7d2c227265636f76657279223a7b22636f6e666967223a7b7d2c
226d6574686f64223a226e6f6e65227d2c227265636f766572795f70726f6f66223a6e756c6c2c2273
657175656e6365223a2230222c2274696d657374616d70223a22323032352d30312d31355430303a30
303a30305a222c2276657273696f6e223a317d

signing_input (490 bytes) = domain_separator || JCS_bytes
```

**Cross-Implementation Verification:**

To verify your implementation is correct:
1. Derive the Ed25519 keypair from seed `033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40` (hex)
2. Verify public key equals `e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4` (hex)
3. Construct the signing_input (490 bytes) exactly as specified above
4. Sign with Ed25519: `signature = Ed25519Sign(private_key, signing_input)`
5. Verify: your signature MUST match the expected signature in the "Expected Signature (Normative)" section above
6. If signatures differ, you have a canonicalization or signing bug

### Signed Document

Per RFC-0001 §6.6, all required fields MUST be present in the wire encoding:

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
  "recovery_proof": null,
  "signatures": {
    "current": "mScYPiZ8NTMXk+TnOh/6gQph+MAmV9nUnX6GirzDCM2kVqFmY4DCuTAYdMfM3Mh043oQfPv7V7tvEnlC4yUNCQ",
    "previous": null
  }
}
```

**Note:** The `signatures.current` value is the normative expected output (see "Expected Signature (Normative)" section above). Ed25519 is deterministic: given the same seed, canonical JSON, and domain separator, all conformant implementations MUST produce the identical 64-byte signature. Implementations that produce a different signature have a bug.

---

## Test Vector 3: Bob's Identity

For multi-party scenarios:

```
Label: "bob-genesis"

Signing seed (hex):
a227446ee9fe9e7a55d2d1247bd83639bf213aa035b4faf3b66da60a208be99c

Signing public key (hex):
b5f35598a00b091430efb67f2456d15baebf0445b08fea6c27778af8785e4cab

Signing public key (base64):
tfNVmKALCRQw77Z/JFbRW66/BEWwj+psJ3eK+HheTKs

Encryption private key (hex):
ea7d6a9217038a4c58f81cfe00b87f1c4feeaa3f182d430936646c4cd11885b2

Encryption public key (hex):
e473a89c43f80e7f3702c9ee7984104879474aa53b72b4e4c8e2b79d0f78a84e

Encryption public key (base64):
5HOonEP4Dn83AsnueYQQSHlHSqU7crTkyOK3nQ94qE4

IID:
2f0fcybfpmka5vf7ge737ex07crgnxsw
```

---

## Test Vector 4: KDF Chain Step (Double Ratchet)

This is the fundamental message key derivation used in the Double Ratchet protocol.

### Input

```
chain_key (hex):
000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
```

### Algorithm

```python
def kdf_chain_step(chain_key: bytes) -> tuple[bytes, bytes]:
    message_key = HMAC-SHA256(key=chain_key, data=b"\x01")
    new_chain_key = HMAC-SHA256(key=chain_key, data=b"\x02")
    return new_chain_key, message_key
```

### Expected Output

```
message_key (hex):
9b4c8120a4823a95f47cde17a244f4507244ee6e3957d1fab9fa29b44d3829b7

new_chain_key (hex):
4304c22c84a53755ab08ead8d97a8d429be5efa480682d7ad1da27f73e1fbe1d
```

---

## Test Vector 5: Root Chain KDF (Double Ratchet)

Used when performing a DH ratchet step.

### Input

```
root_key (hex):
000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f

dh_output (hex):
1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100
```

### Algorithm

```python
def kdf_root(root_key: bytes, dh_output: bytes) -> tuple[bytes, bytes]:
    prk = HKDF-Extract(salt=root_key, ikm=dh_output)
    derived = HKDF-Expand(prk=prk, info=b"post-urbit-ratchet-v1", length=64)
    return derived[0:32], derived[32:64]  # new_root_key, new_chain_key
```

### Expected Output

```
PRK (hex):
9d2a0fddbe2de00ed0a9d9ec544d6be4be7b82ae931ce098c4ddfd326afeb11c

new_root_key (hex):
76b6f7be00a618e3cd626650dc9b3c70f044b12499f2ffb94ca72c7fb08f0fb5

new_chain_key (hex):
96c7dbc35d738c6d1729e2cf160f12ee8cc045540836c8b67c18d843ee710d74
```

---

## Test Vector 6: 2DH Key Agreement

Initial key agreement between Alice and Bob using 2DH (Two Diffie-Hellman operations). Despite the historical "x3dh" string in the domain separator, Post-Urbit v1 uses 2DH, not Signal's X3DH.

### Alice's Keys

```
Identity encryption private (hex):
7ff8c1a741fd3c5253f5d6953cd78f5411f36507f8f653b498e19d381bf7877b

Identity encryption public (hex):
8dd988e51da364e85b7c9c976a4c3d300d58ad513210e5e893e57eca2ab1cb37

Ephemeral private (hex):
3803e7c7f979da62ad5f1aaf9253be156695d8ae845b8cbc2e24afcd9a32d50d

Ephemeral public (hex):
89fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be627904522217

Ephemeral public (base64):
if6HNF0cJO1fwW35CA7vk0WoJM3fN7X+xL5ieQRSIhc
```

### Bob's Keys

```
Identity encryption private (hex):
ea7d6a9217038a4c58f81cfe00b87f1c4feeaa3f182d430936646c4cd11885b2

Identity encryption public (hex):
e473a89c43f80e7f3702c9ee7984104879474aa53b72b4e4c8e2b79d0f78a84e
```

### DH Computations

```
DH1 = X25519(IK_A_private, IK_B_public):
858d12b60f9a452f4f0925b669236bf96492d4dfb68b8ad9a4b0c34249db4f1a

DH2 = X25519(EK_A_private, IK_B_public):
31548fcb50ec70e48a1dda37f3e1ea13cee05b5f55ffaa34e88804ff55d8ac5d
```

### Key Derivation

```
IKM (DH1 || DH2, 64 bytes):
858d12b60f9a452f4f0925b669236bf96492d4dfb68b8ad9a4b0c34249db4f1a31548fcb50ec70e48a1dda37f3e1ea13cee05b5f55ffaa34e88804ff55d8ac5d

Salt (sorted IIDs, 40 bytes):
Alice IID raw: 586a763f2c82b31a0c5de9dcaef01e0261e0785b
Bob IID raw: d15c5160257b140ed4bf313fbf92eef8a266de56

Since Alice < Bob lexicographically:
Salt = Alice || Bob = 586a763f2c82b31a0c5de9dcaef01e0261e0785bd15c5160257b140ed4bf313fbf92eef8a266de56

PRK (hex):
23ba70e51bb5be341a88190a016384da24a99b8383329df12d9dcc09888ed79c

Output root_key (hex):
dc32bc7298c8558b3e347cad9196a2a9f1744185be574ea869e441716eb7420d

Output initial_chain_key (hex):
47920ff7fbbdca074b8abebfc125e456909b36635c9177a8afee8a1e6314d86e
```

---

## Test Vector 7: Peer Handshake Challenge

Server (Bob) proving identity to Client (Alice).

### Input

```
Domain separator (UTF-8):
"post-urbit-handshake-v1" (23 bytes)

Client nonce (hex):
0001020304050607080910111213141516171819202122232425262728293031

Server nonce (hex):
3130292827262524232221201918171615141312111009080706050403020100

TLS binding (hex):
ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100

Client IID raw (hex):
586a763f2c82b31a0c5de9dcaef01e0261e0785b

Server IID raw (hex):
d15c5160257b140ed4bf313fbf92eef8a266de56
```

### Challenge Construction

```
challenge_data = concat(
    "post-urbit-handshake-v1",  // 23 bytes
    client_nonce,               // 32 bytes
    server_nonce,               // 32 bytes
    tls_binding,                // 32 bytes
    client_iid_raw,             // 20 bytes
    server_iid_raw              // 20 bytes
)

Total length: 159 bytes
```

### Expected Output

```
challenge_hash = SHA256(challenge_data):
e3c13cb7654e8ef4a20fb3b0f3d48b42be0dddaffa2371d530e07759ae69e2f8

challenge_signature (by Bob's signing key):
Hex: 2b0e539295f8a88321d7b1d6cb092f3e7e6d9b39773ff94ccf03434694293a0400d23f6733b976a339c9bf425f3c01098838c05f947ae760432800996f032f05

Base64: Kw5TkpX4qIMh17HWywkvPn5tmzl3P/lMzwNDRpQpOgQA0j9nM7l2oznJv0JfPAEJiDjAX5R652BDKACZbwMvBQ
```

---

## Test Vector 8: Device Identifier (DID) Derivation

Same algorithm as IID, applied to device signing key.

### Derivation Method

```
Label: "alice-phone"
Purpose: "ed25519"

1. Derive device signing seed: derive_key_material("alice-phone", "ed25519", 32)
2. Generate Ed25519 keypair from seed
3. Take public key (32 bytes, raw Ed25519, NOT DER/SPKI)
4. Hash: SHA256(device_signing_public_key)
5. Truncate to first 20 bytes (160 bits)
6. Encode as Crockford Base32 lowercase (32 chars)
```

### Algorithm (identical to IID)

```
DID = CrockfordBase32Lower(SHA256(device_signing_public_key)[0:20])
```

**Note:** DID derivation uses the exact same algorithm as IID derivation (Test Vector 1), just applied to the device's signing key instead of the identity's genesis key. Implementers should verify their DID derivation produces the same output as IID derivation when given the same key input.

### Concrete Values

```
Device signing seed (hex):
a5eb457d5b8af39124ddada0d29509014140b56e1da10ea92f50a3de8e82e509

Device signing public key (hex):
ea0757f2720fa3459633c30eb2e0ab737656321c4803d849aa7f614239c28652

Device signing public key (base64):
6gdX8nIPo0WWM8MOsuCrc3ZWMhxIA9hJqn9hQjnChlI

SHA256(public_key) (hex):
20a6bfdc5af29691a554f2da732e889bedf855257edfb1d65fc503ea923d0519

First 20 bytes (hex):
20a6bfdc5af29691a554f2da732e889bedf85525

DID (first 20 bytes, Base32):
42kbzq2tyab939amybd76bm8kfpzgn95
```

---

## Test Vector 9: Sync Operation Signature

### Operation Data

```
Document ID (UUID): 550e8400-e29b-41d4-a716-446655440000
Document ID (RFC 4122 binary, 16 bytes, hex):
550e8400e29b41d4a716446655440000

Document ID (32-byte padded for Sync CBOR, hex):
550e8400e29b41d4a71644665544000000000000000000000000000000000000

Origin (Alice IID):
Base32: b1anasr5h0bj3832xqexwy0f0987e1xb
Raw bytes (hex): 586a763f2c82b31a0c5de9dcaef01e0261e0785b

Timestamp (HLC):
physical_ms: 1700000000000
logical: 7
origin_hash (SHA256(origin_raw)[0:8]): f97b7cc4577dffa2
timestamp_bytes (hex, 20 bytes):
0000018bcfe5680000000007f97b7cc4577dffa2

CRDT Operation: lww_set with value "Alice Smith"
CRDT Operation (CBOR, integer keys per sync-protocol.md):
{0: 0, 1: "Alice Smith"}
CBOR hex: A2 00 00 01 6B 41 6C 69 63 65 20 53 6D 69 74 68

Dependencies: empty list
dependencies_bytes: (empty, length = 0)
```

### Signature Construction

```
operation_id = SHA256(origin || timestamp_bytes || operation_bytes || dependencies_bytes)

signature_input = concat(
  "post-urbit:sync-op:v1:",    // domain separator (22 bytes)
  operation_id_bytes,          // 32 bytes
  document_id,                 // 32 bytes
  timestamp_bytes,             // 20 bytes (8 physical + 4 logical + 8 origin_hash)
  operation_bytes,             // CBOR-encoded operation
  dependencies_bytes           // sorted, concatenated operation_ids
)

signature = Ed25519Sign(origin_signing_key, signature_input)
```

### Computed Values

```
operation_bytes (hex):
a20000016b416c69636520536d697468

operation_id (hex):
27bff0b3171025eef73c81edb1c88bf61f902b30eef342b0e65ce847d65c2314

signature_input (hex):
706f73742d75726269743a73796e632d6f703a76313a27bff0b3171025eef73c81edb1c88bf61f902b30eef342b0e65ce847d65c2314550e8400e29b41d4a716446655440000000000000000000000000000000000000000018bcfe5680000000007f97b7cc4577dffa2a20000016b416c69636520536d697468

signature (Ed25519, Alice signing key from Test Vector 1):
Hex: abfe6b073f8fafb4a216f5099f6feaec7b2a257309e29bdb31cd647b1409aaab4c0819e0a5bc4122ea3501bd90a9937417c6d0e61d179d4bb9b01ca352033f0b
Base64: q/5rBz+Pr7SiFvUJn2/q7HsqJXMJ4pvbMc1kexQJqqtMCBngpbxBIuo1Ab2QqZN0F8bQ5h0XnUu5sByjUgM/Cw
```

---

## Test Vector 10: PUSE Envelope (1:1 Initial Message)

This vector follows RFC-0003 §3 (PUSE wire format) and §5 (2DH initial message). Signature is raw Ed25519 over the envelope bytes before the signature (no prehash).

### Inputs

```
Sender IID (Alice, base32): b1anasr5h0bj3832xqexwy0f0987e1xb
Sender IID (raw hex): 586a763f2c82b31a0c5de9dcaef01e0261e0785b

Recipient IID (Bob, base32): 2f0fcybfpmka5vf7ge737ex07crgnxsw
Recipient IID (raw hex): d15c5160257b140ed4bf313fbf92eef8a266de56

Flags: 0x00 (recipient type 00=1:1, all other bits 0)

Message ID (UUID): 550e8400-e29b-41d4-a716-446655440000
Message ID (16 bytes, hex): 550e8400e29b41d4a716446655440000

Header Extension (Initial, type 0x00):
Extension length: 33 (0x0021)
Extension bytes (hex):
0089fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be627904522217
AAD bytes (hex): same as extension

Nonce (12 bytes, hex): 6560a3c00102030405060708
  timestamp seconds = 1700000000 (0x6560a3c0)
  random = 0102030405060708

Plaintext (UTF-8): "hello"
```

### Message Key Derivation

```
initial_chain_key (from Test Vector 6):
47920ff7fbbdca074b8abebfc125e456909b36635c9177a8afee8a1e6314d86e

kdf_chain_step(initial_chain_key) ->
message_key_0 (hex):
6d7fa890fbfc8f49a691773407d79a5c1745daa14a8a87a990cb58fb1894aeec

new_chain_key_1 (hex):
4e75e0384cbd36e42464b656a3a1f8078f4c72ac8a8eceba75e2eb21689cde91
```

### Encryption Output

```
ciphertext_length = 21 (0x00000015)  // plaintext + 16-byte Poly1305 tag
ciphertext (hex):
900c9a179c3e847fdf3660033e1dc73ad0a11a8db6
```

### Signature

```
signature (Ed25519, Alice signing key from Test Vector 1):
Hex: fdc884da4019717b56265c8172c731a3ea577fad6e77fb736f765a93d1cabfe6c2ca99a96620c3d0b60cf6f3c1ccaddfd1dddf8df197ad4e7f480ee513fec70d
Base64: /ciE2kAZcXtWJlyBcscxo+pXf61ud/tzb3Zak9HKv+bCypmpZiDD0LYM9vPBzK3f0d3fjfGXrU5/SA7lE/7HDQ
```

### Full Envelope

```
envelope (hex):
505553450100586a763f2c82b31a0c5de9dcaef01e0261e0785bd15c5160257b140ed4bf313fbf92eef8a266de56550e8400e29b41d4a71644665544000000210089fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be6279045222176560a3c0010203040506070800000015900c9a179c3e847fdf3660033e1dc73ad0a11a8db6fdc884da4019717b56265c8172c731a3ea577fad6e77fb736f765a93d1cabfe6c2ca99a96620c3d0b60cf6f3c1ccaddfd1dddf8df197ad4e7f480ee513fec70d

envelope (base64):
UFVTRQEAWGp2PyyCsxoMXencrvAeAmHgeFvRXFFgJXsUDtS/MT+/ku74ombeVlUOhADim0HUpxZEZlVEAAAAIQCJ/oc0XRwk7V/BbfkIDu+TRagkzd83tf7EvmJ5BFIiF2Vgo8ABAgMEBQYHCAAAABWQDJoXnD6Ef982YAM+Hcc60KEajbb9yITaQBlxe1YmXIFyxzGj6ld/rW53+3NvdlqT0cq/5sLKmalmIMPQtgz288HMrd/R3d+N8ZetTn9IDuUT/scN
```

---

## Test Vector 11: PUSE Ratchet Message (1:1, Same Session)

This vector follows RFC-0003 §3.4.3 and §4.4. It uses the same session as Test Vector 10, with the ratchet header in the PUSE header extension (AAD).

### Inputs

```
Sender IID (Alice, base32): b1anasr5h0bj3832xqexwy0f0987e1xb
Sender IID (raw hex): 586a763f2c82b31a0c5de9dcaef01e0261e0785b

Recipient IID (Bob, base32): 2f0fcybfpmka5vf7ge737ex07crgnxsw
Recipient IID (raw hex): d15c5160257b140ed4bf313fbf92eef8a266de56

Flags: 0x00 (recipient type 00=1:1, all other bits 0)

Message ID (UUID): 550e8400-e29b-41d4-a716-446655440001
Message ID (16 bytes, hex): 550e8400e29b41d4a716446655440001

Header Extension (Ratchet, type 0x01):
Extension length: 41 (0x0029)
DH public key: 89fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be627904522217
PN: 0 (0x00000000)
N: 1 (0x00000001)
Extension bytes (hex):
0189fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be6279045222170000000000000001
AAD bytes (hex): same as extension

Nonce (12 bytes, hex): 6560a3c11112131415161718
  timestamp seconds = 1700000001 (0x6560a3c1)
  random = 1112131415161718

Plaintext (UTF-8): "hello again"
```

### Message Key Derivation

```
chain_key_1 (from Test Vector 10):
4e75e0384cbd36e42464b656a3a1f8078f4c72ac8a8eceba75e2eb21689cde91

kdf_chain_step(chain_key_1) ->
message_key_1 (hex):
862e4ed4478747555509513b555ce09f35ab4731c03107461c77c8f7cec361d7

new_chain_key_2 (hex):
53c2f15079523be63e6d6b3bc984be16f2ea4931e023b434ed507ffc5b0c2782
```

### Encryption Output

```
ciphertext_length = 27 (0x0000001b)  // plaintext + 16-byte Poly1305 tag
ciphertext (hex):
32c8241cd1dd0baff3719c390843c0b056443cc1c0686b5f3c0126
```

### Signature

```
signature (Ed25519, Alice signing key from Test Vector 1):
Hex: 094b4d9c3ca5e0229d6f40a94b13492ff290bf812fbc203dcae818912457fc4befc0af1e857baab75d0ca434de46205b2f64262d1fed5f5963d33f43cb54c60c
Base64: CUtNnDyl4CKdb0CpSxNJL/KQv4EvvCA9yugYkSRX/EvvwK8ehXuqt10MpDTeRiBbL2QmLR/tX1lj0z9Dy1TGDA
```

### Full Envelope

```
envelope (hex):
505553450100586a763f2c82b31a0c5de9dcaef01e0261e0785bd15c5160257b140ed4bf313fbf92eef8a266de56550e8400e29b41d4a71644665544000100290189fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be62790452221700000000000000016560a3c111121314151617180000001b32c8241cd1dd0baff3719c390843c0b056443cc1c0686b5f3c0126094b4d9c3ca5e0229d6f40a94b13492ff290bf812fbc203dcae818912457fc4befc0af1e857baab75d0ca434de46205b2f64262d1fed5f5963d33f43cb54c60c

envelope (base64):
UFVTRQEAWGp2PyyCsxoMXencrvAeAmHgeFvRXFFgJXsUDtS/MT+/ku74ombeVlUOhADim0HUpxZEZlVEAAEAKQGJ/oc0XRwk7V/BbfkIDu+TRagkzd83tf7EvmJ5BFIiFwAAAAAAAAABZWCjwRESExQVFhcYAAAAGzLIJBzR3Quv83GcOQhDwLBWRDzBwGhrXzwBJglLTZw8peAinW9AqUsTSS/ykL+BL7wgPcroGJEkV/xL78CvHoV7qrddDKQ03kYgWy9kJi0f7V9ZY9M/Q8tUxgw
```

---

## Verification Checklist

Implementers should verify:

**Reproducible Vectors (1-11):**
1. [ ] IID derivation matches Test Vector 1
2. [ ] JCS canonicalization produces exact byte sequence
3. [ ] Signature verification passes for Test Vector 2
4. [ ] KDF chain step matches Test Vector 4
5. [ ] Root chain KDF matches Test Vector 5
6. [ ] X3DH key agreement matches Test Vector 6
7. [ ] Handshake challenge matches Test Vector 7
8. [ ] DID derivation matches Test Vector 8
9. [ ] Sync operation signature matches Test Vector 9
10. [ ] PUSE initial envelope round-trip matches Test Vector 10
11. [ ] PUSE ratchet envelope round-trip matches Test Vector 11

---

## Reference Generator

```python
#!/usr/bin/env python3
"""
Generate post-urbit test vectors.
Requires: pip install pynacl
"""

import hashlib
import hmac
import base64
import json
from nacl.signing import SigningKey
from nacl.public import PrivateKey as X25519PrivateKey
from nacl.encoding import RawEncoder
from nacl.bindings import crypto_scalarmult

SEED = b'post-urbit-test-vectors-v1'

def hkdf_extract(salt, ikm):
    if not salt:
        salt = b'\x00' * 32
    return hmac.new(salt, ikm, hashlib.sha256).digest()

def hkdf_expand(prk, info, length):
    output = b''
    prev = b''
    counter = 1
    while len(output) < length:
        prev = hmac.new(prk, prev + info + bytes([counter]), hashlib.sha256).digest()
        output += prev
        counter += 1
    return output[:length]

def derive_key_material(label, purpose, length):
    ikm = SEED + label.encode('utf-8')
    prk = hkdf_extract(b'', ikm)
    return hkdf_expand(prk, purpose.encode('utf-8'), length)

CROCKFORD_ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz"

def crockford_encode(data):
    """Encode bytes to Crockford Base32 (NOT RFC4648)."""
    value = int.from_bytes(data, 'big')
    bits = len(data) * 8
    result = []
    for i in range((bits + 4) // 5):
        shift = bits - (i + 1) * 5
        if shift < 0:
            idx = (value << -shift) & 0x1f
        else:
            idx = (value >> shift) & 0x1f
        result.append(CROCKFORD_ALPHABET[idx])
    return ''.join(result)

def base64_encode(data):
    return base64.b64encode(data).decode('ascii').rstrip('=')

def derive_iid(pubkey):
    h = hashlib.sha256(pubkey).digest()
    return crockford_encode(h[:20])

# Usage: Run this script to regenerate all test vectors
if __name__ == '__main__':
    # Generate Alice
    alice_seed = derive_key_material('alice-genesis', 'ed25519', 32)
    alice_key = SigningKey(alice_seed)
    alice_pubkey = bytes(alice_key.verify_key)
    print(f"Alice IID: {derive_iid(alice_pubkey)}")
    # ... (full implementation in source)
```

---

## Known Good Crypto Libraries

| Language | Ed25519 | X25519 | ChaCha20-Poly1305 | HKDF |
|----------|---------|--------|-------------------|------|
| Rust | ed25519-dalek | x25519-dalek | chacha20poly1305 | hkdf |
| Python | pynacl | pynacl | pynacl | hmac (stdlib) |
| JavaScript | @noble/ed25519 | @noble/curves | @noble/ciphers | @noble/hashes |
| Go | crypto/ed25519 | crypto/ecdh | crypto/chacha20poly1305 | crypto/hkdf |

# Test Vectors

## Overview

This document provides concrete, reproducible cryptographic test vectors for implementers. All values are deterministically derived from a fixed seed and can be regenerated using the reference implementation.

## Notation

| Format | Encoding | Notes |
|--------|----------|-------|
| hex | Hexadecimal (lowercase) | Used for raw bytes |
| base64 | RFC4648 standard alphabet, **no padding** | `A-Za-z0-9+/` |
| base32 | RFC4648 lowercase, **no padding** | `a-z2-7` |

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

4. Base32 lowercase encode (no padding):
   lbvhmpzmqkzrudc55hok54a6ajq6a6c3
```

### Expected Output

```
IID: lbvhmpzmqkzrudc55hok54a6ajq6a6c3
IID length: 32 characters
```

### Reference Implementation (Python)

```python
import hashlib
import base64

def derive_iid(genesis_signing_public_key_raw: bytes) -> str:
    assert len(genesis_signing_public_key_raw) == 32, "Must be raw 32-byte Ed25519 pubkey"
    hash_bytes = hashlib.sha256(genesis_signing_public_key_raw).digest()
    truncated = hash_bytes[:20]
    base32_upper = base64.b32encode(truncated).decode('ascii').rstrip('=')
    return base32_upper.lower()

# Verify
pubkey = bytes.fromhex('e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46acfb8f519bb4')
assert derive_iid(pubkey) == 'lbvhmpzmqkzrudc55hok54a6ajq6a6c3'
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
lbvhmpzmqkzrudc55hok54a6ajq6a6c3
```

### Document (before signatures)

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

### Canonical JSON (JCS - RFC 8785)

JCS sorts keys lexicographically at all levels and removes whitespace:

```
{"claims":{"name":"Alice"},"endpoints":[],"extensions":{},"iid":"lbvhmpzmqkzrudc55hok54a6ajq6a6c3","keys":{"encryption":{"current":"jdmI5R2jZOhbfJyXakw9MA1YrVEyEOXok+V+yiqxyzc","previous":[]},"signing":{"current":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","genesis":"48enIEnfjEYjotS2HbHXamw+ou+q57h+nUas+49Rm7Q","previous":null}},"recovery":{"config":{},"method":"none"},"sequence":"0","timestamp":"2025-01-15T00:00:00Z","version":1}
```

### Signature

Sign the UTF-8 encoded canonical JSON bytes:

```
Signature (hex):
1d554c30226ba0a37ce77c91fecea19026a7203136fdb52dd6cc7982ed2cbda61f9e366b9a78cd65d6fb22372ee452df96272afb8e020cf0392d234011507603

Signature (base64):
HVVMMCJroKN853yR/s6hkCanIDE2/bUt1sx5gu0svaYfnjZrmnjNZdb7Ijcu5FLflicq+44CDPA5LSNAEVB2Aw

Signature length: 64 bytes (86 base64 chars without padding)
```

### Signed Document

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
2fofcybfpmka5vf7ge737exo7crgnxsw
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

## Test Vector 6: X3DH Key Agreement

Initial key agreement between Alice and Bob.

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

### Input

```
Label: "alice-phone"
Purpose: "ed25519"

Device signing seed (hex):
[derive using derive_key_material("alice-phone", "ed25519", 32)]

Device signing public key (hex):
[computed from seed]
```

### Derivation

```
DID = Base32Lower(SHA256(device_signing_public_key)[0:20])
```

(Same steps as IID derivation in Test Vector 1)

---

## Verification Checklist

Implementers should verify:

1. [ ] IID derivation matches Test Vector 1
2. [ ] JCS canonicalization produces exact byte sequence
3. [ ] Signature verification passes for Test Vector 2
4. [ ] KDF chain step matches Test Vector 4
5. [ ] Root chain KDF matches Test Vector 5
6. [ ] X3DH key agreement matches Test Vector 6
7. [ ] Handshake challenge matches Test Vector 7

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

def base32_encode(data):
    return base64.b32encode(data).decode('ascii').rstrip('=').lower()

def base64_encode(data):
    return base64.b64encode(data).decode('ascii').rstrip('=')

def derive_iid(pubkey):
    h = hashlib.sha256(pubkey).digest()
    return base32_encode(h[:20])

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

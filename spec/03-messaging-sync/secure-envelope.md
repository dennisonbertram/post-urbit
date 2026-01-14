# Secure Envelope

## Overview

The Secure Envelope is the fundamental encrypted message container. All messages in the system are wrapped in this format.

## Cryptographic Primitives

| Purpose | Algorithm | Notes |
|---------|-----------|-------|
| Key agreement | X25519 | ECDH for shared secret |
| Key derivation | HKDF-SHA256 | Derive symmetric keys from shared secret |
| Symmetric encryption | ChaCha20-Poly1305 | AEAD cipher |
| Signing | Ed25519 | Message authentication |
| Hashing | SHA-256 | General purpose |

## Envelope Structure

### Wire Format

The envelope has three sections:
1. **Unauthenticated Header** - Routing info (can be read without keys)
2. **Authenticated Header (AAD)** - Cryptographic parameters (authenticated but unencrypted)
3. **Ciphertext** - Encrypted payload

```
Secure Envelope:
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x53 0x45 ("PUSE")   │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Flags                                  │ 1 byte
├────────────────────────────────────────┤
│ Sender IID (raw)                       │ 20 bytes
├────────────────────────────────────────┤
│ Recipient IID (raw)                    │ 20 bytes (or group ID)
├────────────────────────────────────────┤
│ Message ID (for replay detection)      │ 16 bytes (UUID)
├────────────────────────────────────────┤
│ Header Extension Length                │ 2 bytes (big-endian)
├────────────────────────────────────────┤
│ Header Extension (ratchet/group info)  │ <ext_len> bytes (AAD)
├────────────────────────────────────────┤
│ Nonce                                  │ 12 bytes
├────────────────────────────────────────┤
│ Ciphertext Length                      │ 4 bytes (big-endian)
├────────────────────────────────────────┤
│ Ciphertext (AEAD encrypted payload)    │ <length> bytes
├────────────────────────────────────────┤
│ Signature (over everything above)      │ 64 bytes
└────────────────────────────────────────┘

Minimum size: 4 + 1 + 1 + 20 + 20 + 16 + 2 + 0 + 12 + 4 + 16 + 64 = 160 bytes
Maximum size: 1 MB (1048576 bytes)
```

### Header Extension (AAD)

The header extension carries cryptographic metadata needed before decryption. It is **authenticated** (via AEAD AAD and signature) but **not encrypted**.

For 1:1 ratchet messages:
```
Ratchet Header Extension:
┌────────────────────────────────────────┐
│ Extension Type: 0x01 (ratchet)         │ 1 byte
├────────────────────────────────────────┤
│ DH Public Key (ephemeral)              │ 32 bytes
├────────────────────────────────────────┤
│ Previous Chain Length (PN)             │ 4 bytes (big-endian)
├────────────────────────────────────────┤
│ Chain Index (N)                        │ 4 bytes (big-endian)
└────────────────────────────────────────┘
Total: 41 bytes
```

For group sender-key messages:
```
Group Header Extension:
┌────────────────────────────────────────┐
│ Extension Type: 0x02 (group)           │ 1 byte
├────────────────────────────────────────┤
│ Sender Key ID                          │ 16 bytes
├────────────────────────────────────────┤
│ Sender Key Iteration                   │ 4 bytes (big-endian)
└────────────────────────────────────────┘
Total: 21 bytes
```

For initial key exchange (no ratchet yet):
```
Initial Header Extension:
┌────────────────────────────────────────┐
│ Extension Type: 0x00 (initial)         │ 1 byte
├────────────────────────────────────────┤
│ Ephemeral Public Key                   │ 32 bytes
└────────────────────────────────────────┘
Total: 33 bytes
```

**AEAD Construction**: The header extension bytes are used as Additional Authenticated Data (AAD) for ChaCha20-Poly1305:
```
ciphertext = ChaCha20-Poly1305(
  key = message_key,
  nonce = envelope.nonce,
  aad = envelope.header_extension,  // Authenticated, not encrypted
  plaintext = payload
)
```

### Flags Byte

```
Flags (1 byte):
┌─┬─┬─┬─┬─┬─┬─┬─┐
│7│6│5│4│3│2│1│0│
└─┴─┴─┴─┴─┴─┴─┴─┘
 │ │ │ │ │ │ │ │
 │ │ │ │ │ │ └─┴── Recipient type: 00=1:1, 01=group, 10=broadcast, 11=reserved
 │ │ │ │ │ └───── Requires ACK: 1=yes, 0=no
 │ │ │ │ └─────── Priority: 1=high, 0=normal
 │ │ │ └───────── Forward: 1=can forward to other devices, 0=single device
 │ │ └─────────── Reserved
 │ └───────────── Reserved
 └─────────────── Reserved
```

## Key Derivation (Session Protocol)

The AEAD message key used for PUSE encryption is provided by the session protocol, not by PUSE itself. PUSE is purely a container format.

### Key Source by Header Extension Type

| Extension Type | Key Source | Reference |
|----------------|------------|-----------|
| 0x00 (initial) | X3DH key agreement | See `double-ratchet.md` § Session Initialization |
| 0x01 (ratchet) | Double Ratchet chain | See `double-ratchet.md` § Ratchet Operation |
| 0x02 (group)   | Sender Key chain | See `group-messaging.md` § Sender Key Chain |

### Initial Messages (Type 0x00)

For the **first message** to a new recipient (no established session):
1. Use X3DH key agreement as specified in `double-ratchet.md` § Session Initialization
2. Derive `(root_key, initial_chain_key)` via `kdf_initial()` from `double-ratchet.md`
3. Derive message key via `kdf_chain_step(initial_chain_key)`
4. Include ephemeral public key in Initial Header Extension (type 0x00)

### Ongoing Messages (Type 0x01)

For subsequent messages in an established session:
1. Use Double Ratchet protocol as specified in `double-ratchet.md`
2. Message key derived from sending chain via `kdf_chain_step()`
3. Include ratchet parameters in Ratchet Header Extension (type 0x01)

### Group Messages (Type 0x02)

For group messages:
1. Use sender key chain as specified in `group-messaging.md`
2. Message key derived via `kdf_sender_key()` from `double-ratchet.md`
3. Include sender key ID and iteration in Group Header Extension (type 0x02)

### Decryption Flow

```
1. Parse envelope header (see "Parse Order for Streaming" above)
2. Verify signature using sender's signing key (from identity document)
3. Based on header extension type:
   - 0x00: Perform X3DH, derive message key
   - 0x01: Look up ratchet state, derive message key from receiving chain
   - 0x02: Look up sender key, derive message key from sender key chain
4. Decrypt: plaintext = ChaCha20-Poly1305_Decrypt(message_key, nonce, aad=header_extension, ciphertext)
```

## Nonce Management

### Nonce Generation

- 12 bytes (96 bits)
- First 4 bytes: timestamp (seconds since epoch, big-endian)
- Next 8 bytes: random

```
Nonce:
┌────────────────────────────────────────┐
│ Timestamp (seconds, big-endian)        │ 4 bytes
├────────────────────────────────────────┤
│ Random                                 │ 8 bytes
└────────────────────────────────────────┘
```

### Nonce Uniqueness

- MUST never reuse (key, nonce) pair
- Timestamp prevents accidental reuse across restarts
- Random component prevents reuse within same second
- Probability of collision: 1/(2^64) per second ≈ negligible

## Plaintext Format

The encrypted payload is structured JSON:

```json
{
  "type": "<message-type>",
  "timestamp": "<RFC3339>",
  "sequence": "<uint64-string>",
  "thread_id": "<optional-thread-id>",
  "reply_to": "<optional-message-id>",
  "expires_at": "<optional-RFC3339>",
  "content": {
    // Type-specific content
  }
}
```

**Note**: The `message_id` is in the envelope header (16-byte UUID), not in the plaintext. Applications reference messages by this header ID. The plaintext does NOT include a duplicate `id` field.

### Message Types

| Type | Description |
|------|-------------|
| `text` | Plain text message |
| `rich` | Rich text with formatting |
| `media` | Media attachment reference |
| `reaction` | Emoji reaction |
| `receipt` | Read/delivery receipt |
| `typing` | Typing indicator |
| `key_update` | Ratchet key update |
| `sync_op` | Sync operation (for mailbox fallback delivery; see note) |
| `app` | Application-specific |

**Note on `sync_op`**: Sync operations normally flow over the dedicated sync stream (0x04) which is NOT PUSE-wrapped. However, when sync data must be delivered via mailbox (e.g., recipient offline), it may be encapsulated in PUSE with type `sync_op`. Recipients must validate the sync operation's own signature in addition to the PUSE envelope signature.

### Text Message Content

```json
{
  "type": "text",
  "content": {
    "text": "Hello, world!",
    "mentions": [
      {"iid": "<mentioned-iid>", "offset": 0, "length": 5}
    ]
  }
}
```

### Media Message Content

```json
{
  "type": "media",
  "content": {
    "media_type": "image/jpeg",
    "size": 1234567,
    "width": 1920,
    "height": 1080,
    "hash": "sha256:abcd1234...",
    "key": "<encryption-key-for-media-file>",
    "nonce": "<nonce-for-media-file>",
    "url": "<content-addressed-url>"
  }
}
```

### Receipt Content

```json
{
  "type": "receipt",
  "content": {
    "receipt_type": "delivered|read",
    "message_ids": ["msg-1", "msg-2"]
  }
}
```

## Signature Scheme

### What is Signed

The signature covers all bytes of the envelope BEFORE the signature field (i.e., the entire envelope except the final 64-byte signature):

```
signed_data = envelope[0 : len(envelope) - 64]

Specifically:
  magic (4) || version (1) || flags (1) || sender_iid (20) || recipient_iid (20) ||
  message_id (16) || header_extension_length (2) || header_extension (variable) ||
  nonce (12) || ciphertext_length (4) || ciphertext (variable)
```

**Note**: The `header_extension` (including any embedded ephemeral DH public key or ratchet parameters) is automatically covered by the signature because it precedes the signature field.

### Parse Order for Streaming

Receivers MUST parse in this order:
1. Read fixed prefix: magic (4) + version (1) + flags (1) + sender_iid (20) + recipient_iid (20) + message_id (16) + header_extension_length (2) = 64 bytes
2. Read `header_extension` (length from step 1)
3. Read `nonce` (12 bytes)
4. Read `ciphertext_length` (4 bytes)
5. Read `ciphertext` (length from step 4)
6. Read `signature` (64 bytes)
7. Verify signature over bytes from steps 1-5

### Signature Verification

```python
def verify_envelope(envelope: bytes, sender_doc: IdentityDocument) -> bool:
    # Extract signature (last 64 bytes)
    signature = envelope[-64:]
    signed_data = envelope[:-64]

    # Verify with sender's current or previous signing key
    current_key = sender_doc.keys.signing.current
    if ed25519_verify(current_key, signed_data, signature):
        return True

    # Try previous key (for recent key rotations)
    if sender_doc.keys.signing.previous:
        if ed25519_verify(sender_doc.keys.signing.previous, signed_data, signature):
            return True

    return False
```

## Forward Secrecy

### Session Keys

For ongoing conversations, use session keys with ratcheting (see double-ratchet.md):

1. Initial exchange establishes root key
2. Each message uses derived key
3. Keys ratchet forward after each message
4. Deleting old keys provides forward secrecy

### Key Ratchet Update Message

When updating the ratchet, send a key update message:

```json
{
  "type": "key_update",
  "content": {
    "new_ephemeral_public": "<new-X25519-public-key>",
    "previous_chain_length": 5
  }
}
```

## Error Handling

| Error | Condition | Action |
|-------|-----------|--------|
| `INVALID_MAGIC` | Magic bytes don't match | Reject envelope |
| `UNSUPPORTED_VERSION` | Version not recognized | Reject envelope |
| `INVALID_SIGNATURE` | Signature verification failed | Reject envelope |
| `DECRYPTION_FAILED` | AEAD decryption failed | Reject envelope |
| `INVALID_SENDER` | Sender IID not in envelope matches | Reject envelope |
| `EXPIRED_MESSAGE` | Message `expires_at` has passed | Discard message |
| `REPLAY_DETECTED` | Message ID seen before | Discard message |

## Replay Protection

### Message ID Cache

- Store `{message_id, received_at}` for recent messages
- TTL: 7 days (configurable)
- Reject messages with duplicate IDs

### Sequence Number

- Per-conversation monotonic sequence
- Reject messages with sequence ≤ last seen (within window)
- Window size: 100 (allow some reordering)

## Metadata Protection

### What is Visible to Network

| Data | Visible to |
|------|------------|
| Sender IID | Transport, relay, mailbox |
| Recipient IID | Transport, relay, mailbox |
| Envelope size | Everyone |
| Timing | Everyone |
| Message content | Only sender and recipient(s) |

### Mitigation Options

- Padding to fixed sizes (increases bandwidth)
- Delay injection (increases latency)
- Cover traffic (increases bandwidth)

These are NOT implemented by default but can be added by applications.

## Test Vectors

### Test Vector 1: Basic Envelope

```
Sender signing key (hex): e8f32a1b...
Sender encryption key (hex): a1b2c3d4...
Recipient encryption key (hex): 5e6f7a8b...
Plaintext: {"type":"text","timestamp":"2025-01-13T12:00:00Z","sequence":"1","content":{"text":"Hello"}}
Note: Message ID is in the PUSE header (16-byte UUID), NOT in plaintext

Expected envelope (hex): 50555345...
Expected signature (hex): 7a8b9c0d...
```

(Full test vectors to be generated during implementation)

## Implementation Notes

### Performance Optimization

1. **Batch verification**: Verify multiple signatures in batch using Ed25519 batch verification
2. **Key caching**: Cache derived session keys for ongoing conversations
3. **Parallel encryption**: Encrypt to multiple recipients in parallel for group messages

### Memory Safety

1. Zero sensitive data after use (keys, plaintext)
2. Use constant-time operations for cryptographic comparisons
3. Limit decrypted message size to prevent memory exhaustion

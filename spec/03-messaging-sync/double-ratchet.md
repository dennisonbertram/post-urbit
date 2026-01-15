# Double Ratchet Protocol

## Overview

The Double Ratchet protocol provides forward secrecy and break-in recovery for ongoing conversations. Based on Signal Protocol's design.

## Goals

| Property | Description |
|----------|-------------|
| **Forward secrecy** | Compromising current keys doesn't reveal past messages |
| **Break-in recovery** | Compromising current keys doesn't reveal future messages after ratchet |
| **Message ordering** | Handle out-of-order message delivery |
| **Asynchronous** | Work when recipient is offline |

## Key Concepts

### Root Chain

A KDF chain that advances with each DH ratchet step. Provides key material for new sending/receiving chains.

### Sending Chain

A KDF chain for encrypting outgoing messages. Advances with each message sent.

### Receiving Chain

A KDF chain for decrypting incoming messages. Advances with each message received.

### DH Ratchet

Periodic Diffie-Hellman key exchange that provides break-in recovery.

## Key Derivation Functions

All KDFs use HMAC-SHA256 for simplicity and compatibility with Signal Protocol's well-audited design.

### KDF Chain Step (Message Key Derivation)

```
def kdf_chain_step(chain_key: bytes) -> tuple[bytes, bytes]:
    """
    Advance a chain, returning (new_chain_key, message_key).
    Uses HMAC with single-byte constants for domain separation.
    """
    # Message key derivation (constant 0x01)
    message_key = HMAC-SHA256(key=chain_key, data=b"\x01")

    # Chain key derivation (constant 0x02)
    new_chain_key = HMAC-SHA256(key=chain_key, data=b"\x02")

    return new_chain_key, message_key
```

### Root Chain KDF

```
def kdf_root(root_key: bytes, dh_output: bytes) -> tuple[bytes, bytes]:
    """
    Advance root chain with DH output.
    Returns (new_root_key, new_chain_key).
    Uses HKDF-SHA256 with proper domain separation.
    """
    # HKDF-Extract: derive PRK from salt and IKM
    prk = HKDF-Extract(salt=root_key, ikm=dh_output)

    # HKDF-Expand: derive 64 bytes with domain separation
    derived = HKDF-Expand(
        prk=prk,
        info=b"post-urbit-ratchet-v1",
        length=64
    )

    return derived[0:32], derived[32:64]
```

### Initial Key Derivation (2DH)

**Note:** Despite the "x3dh" string in the domain separator (historical), Post-Urbit v1 performs 2 DH operations (2DH), not Signal's X3DH which has 3+ DH operations with signed prekeys.

```
def kdf_initial(dh1: bytes, dh2: bytes, iid_a: bytes, iid_b: bytes) -> tuple[bytes, bytes]:
    """
    Derive initial root key and sending chain from 2DH outputs.
    Domain separation includes both IIDs for binding.
    """
    # Concatenate DH outputs
    ikm = dh1 + dh2

    # Salt includes sorted IIDs (consistent regardless of who initiates)
    # IID comparison is bytewise lexicographic over raw 20 bytes (RFC-0003 §4.2.3)
    # DO NOT compare Base32 strings - raw byte ordering is required
    if iid_a < iid_b:  # bytewise comparison of raw 20-byte values
        salt = iid_a + iid_b
    else:
        salt = iid_b + iid_a

    prk = HKDF-Extract(salt=salt, ikm=ikm)

    derived = HKDF-Expand(
        prk=prk,
        info=b"post-urbit-x3dh-v1",
        length=64
    )

    return derived[0:32], derived[32:64]  # root_key, initial_chain_key
```

### Group Sender Key KDF

**Input encoding (Normative):**
- `chain_key`: 32 bytes (current chain key)
- `group_id`: 20 bytes raw (Crockford Base32 decoded, NOT the 32-char string)
- `sender_iid`: 20 bytes raw (Crockford Base32 decoded, NOT the 32-char string)
- `key_id`: 16 bytes raw (NOT Base64 encoded string)

```
def kdf_sender_key(chain_key: bytes, group_id: bytes, sender_iid: bytes, key_id: bytes) -> tuple[bytes, bytes]:
    """
    Advance sender key chain for group messaging.
    Domain separation binds to group and sender.

    All byte inputs MUST be raw bytes, NOT encoded strings.
    Total info length: 25 (prefix) + 20 + 1 + 20 + 1 + 16 = 83 bytes (fixed)
    """
    # Construct domain-separated info (fixed-length inputs, : separators are literal 0x3a)
    info = b"post-urbit-sender-key-v1:" + group_id + b":" + sender_iid + b":" + key_id

    message_key = HMAC-SHA256(key=chain_key, data=b"\x01" + info)
    new_chain_key = HMAC-SHA256(key=chain_key, data=b"\x02" + info)

    return new_chain_key, message_key
```

## Session Initialization

### 2DH (Two Diffie-Hellman) Initial Key Exchange

For initial key agreement when recipient may be offline. This is intentionally simpler than Signal's X3DH (which uses 3+ DH operations with signed prekeys). See RFC-0003 §5 for the authoritative specification.

```
Alice wants to message Bob:

1. Alice retrieves Bob's identity document:
   - IK_B: Bob's long-term X25519 key (from keys.encryption.current)

2. Alice generates ephemeral key:
   - EK_A: Alice's ephemeral X25519 key pair

3. Alice computes (2 DH operations):
   DH1 = X25519(IK_A_private, IK_B)      # Alice identity × Bob identity
   DH2 = X25519(EK_A_private, IK_B)      # Alice ephemeral × Bob identity

   # Note: No signed prekey (unlike Signal X3DH)
   # Post-Urbit v1 uses 2DH; identity layer provides key rotation

4. Master secret:
   master_secret = KDF(DH1 || DH2, "post-urbit x3dh")  # Historical name preserved

5. Derive initial keys:
   root_key = master_secret[:32]
   alice_sending_chain = master_secret[32:64]
```

### Initial Message

Alice sends initial message containing:
- Alice's ephemeral public key (EK_A_public)
- Alice's IID (for identity lookup)
- Encrypted message using alice_sending_chain

### Bob Processes Initial Message

```
1. Bob looks up Alice's identity document:
   - IK_A: Alice's X25519 key

2. Bob computes:
   DH1 = X25519(IK_B_private, IK_A)      # Same as Alice
   DH2 = X25519(IK_B_private, EK_A)      # Using ephemeral from message

3. Derive same master_secret and initial keys

4. Decrypt message using alice_sending_chain as receiving_chain
```

## Ratchet Operation

### State

```typescript
interface RatchetState {
  // DH ratchet keys
  dhSendingKey: KeyPair | null;      // Our current DH key pair
  dhReceivingKey: PublicKey | null;  // Their current DH public key

  // Root chain
  rootKey: Uint8Array;

  // Sending chain
  sendingChainKey: Uint8Array | null;
  sendingChainIndex: number;          // N: next message number (0-indexed)
  previousChainLength: number;        // PN: messages sent in previous sending chain

  // Receiving chain (may have multiple for out-of-order messages)
  receivingChains: Map<PublicKey, {
    chainKey: Uint8Array;
    n: number;                        // Next expected message number (0-indexed)
  }>;

  // Skipped message keys (for out-of-order delivery)
  skippedKeys: Map<string, Uint8Array>;  // key: "pubkey:n"
  maxSkip: number;  // Maximum messages to skip (default: 100)
}
```

### Sending a Message

```
def send_message(state: RatchetState, plaintext: bytes) -> tuple[bytes, bytes]:
    # If no sending chain, perform DH ratchet
    if state.sending_chain_key is None:
        state.dh_sending_key = generate_x25519_keypair()
        dh_output = x25519(state.dh_sending_key.private, state.dh_receiving_key)
        state.root_key, state.sending_chain_key = kdf_root(state.root_key, dh_output)
        state.sending_chain_index = 0

    # Capture N (message number) BEFORE incrementing - N is 0-indexed per RFC-0003
    n = state.sending_chain_index

    # Get message key and advance chain
    state.sending_chain_key, message_key = kdf_chain_step(state.sending_chain_key)
    state.sending_chain_index += 1

    # Encrypt
    ciphertext = chacha20_poly1305_encrypt(message_key, nonce, plaintext)

    # Header (N is the pre-increment value, 0-indexed)
    header = encode_header(
        dh_public=state.dh_sending_key.public,
        n=n,
        pn=state.previous_chain_length  # Messages sent in previous sending chain
    )

    return header, ciphertext
```

### Receiving a Message

```
def receive_message(state: RatchetState, header: Header, ciphertext: bytes) -> bytes:
    # Check for skipped message key (N is 0-indexed)
    skip_key = f"{header.dh_public}:{header.n}"
    if skip_key in state.skipped_keys:
        message_key = state.skipped_keys.pop(skip_key)
        return chacha20_poly1305_decrypt(message_key, nonce, ciphertext)

    # If new DH key, perform DH ratchet
    if header.dh_public != state.dh_receiving_key:
        # Store skipped keys from current receiving chain (using PN)
        skip_message_keys(state, header.pn)

        # DH ratchet step
        state.dh_receiving_key = header.dh_public
        dh_output = x25519(state.dh_sending_key.private, state.dh_receiving_key)
        state.root_key, receiving_chain_key = kdf_root(state.root_key, dh_output)

        # Save previous chain length for next outgoing header.pn
        state.previous_chain_length = state.sending_chain_index
        # Clear sending chain (will ratchet on next send)
        state.sending_chain_key = None

        # Store new receiving chain (N starts at 0)
        state.receiving_chains[header.dh_public] = {
            chain_key: receiving_chain_key,
            n: 0  # Next expected message number
        }

    # Get receiving chain
    chain = state.receiving_chains[header.dh_public]

    # Skip ahead if needed (N is 0-indexed)
    while chain.n < header.n:
        if len(state.skipped_keys) > state.max_skip:
            raise TooManySkippedMessages()
        chain.chain_key, skipped_key = kdf_chain_step(chain.chain_key)
        state.skipped_keys[f"{header.dh_public}:{chain.n}"] = skipped_key
        chain.n += 1

    # Get message key and advance expected N
    chain.chain_key, message_key = kdf_chain_step(chain.chain_key)
    chain.n += 1

    return chacha20_poly1305_decrypt(message_key, nonce, ciphertext)
```

## Header Format

The ratchet header is placed in the **PUSE header extension** (type `0x01`), NOT in the encrypted plaintext. This is required because the receiver needs the ratchet parameters to derive the decryption key.

**Counter semantics (normative, per RFC-0003):**
- **N (Message Number)**: 0-indexed within each sending chain. The first message in a chain has N=0.
- **PN (Previous Chain Length)**: The count of messages sent in the PREVIOUS sending chain before this DH ratchet occurred.

**Initial→Ratchet Transition (Normative):**

The transition from initial (0x00) to ratchet (0x01) message types is NOT a DH ratchet step. Both message types use the same initial sending chain:

| Message | PUSE Type | Chain | N |
|---------|-----------|-------|---|
| First message (initial) | 0x00 | Initial chain | 0 |
| Second message (before DH ratchet) | 0x01 | Initial chain | 1 |
| After DH ratchet | 0x01 | NEW chain | 0 |

The DH ratchet only occurs when the sender receives a response containing a new DH public key from the recipient. Until then, all messages continue on the initial chain with incrementing N.

**Summary:** The initial (0x00) message consumes N=0; the first ratchet (0x01) message MUST use N=1 (continuing the same chain). Subsequent DH ratchets reset N to 0 for the NEW chain.

```
Ratchet Header (PUSE Header Extension Type 0x01):
┌────────────────────────────────────────┐
│ DH Public Key                          │ 32 bytes
├────────────────────────────────────────┤
│ PN - Previous Chain Length (big-endian)│ 4 bytes
├────────────────────────────────────────┤
│ N - Message Number (big-endian)        │ 4 bytes
└────────────────────────────────────────┘

Total: 40 bytes
```

**Note:** RFC-0003 is authoritative for ratchet header semantics.

**Wire format in PUSE envelope:**
```
header_extension = type (1 byte: 0x01) || ratchet_header (40 bytes)
Total: 41 bytes
```

**Note:** The PUSE envelope already includes a global `Header Extension Length` field (2 bytes) that specifies the total extension size. Individual extensions do NOT include their own length field. See `secure-envelope.md` for the complete envelope format.

**IMPORTANT:** The ratchet header is included in the PUSE AAD (authenticated data) but is NOT encrypted. This allows the receiver to:
1. Parse the ratchet header from PUSE header extension
2. Derive the correct message key using the DH public key and chain index
3. Decrypt the ciphertext

The plaintext contains only the message content (no ratchet params):

```json
{
  "type": "text",
  "content": {
    "text": "Hello!"
  }
}
```

**NOTE:** Previous versions of this spec showed ratchet params in plaintext JSON. This was incorrect. The ratchet header MUST be in the PUSE header extension for the receiver to derive decryption keys.

## State Persistence

### What to Store

```typescript
interface PersistedRatchetState {
  peerId: IdentityIdentifier;

  // DH keys
  dhSendingKeyPrivate: Uint8Array;
  dhSendingKeyPublic: Uint8Array;
  dhReceivingKey: Uint8Array | null;

  // Chains (encrypted at rest)
  rootKey: Uint8Array;
  sendingChainKey: Uint8Array | null;
  sendingChainIndex: number;
  receivingChains: Array<{
    dhPublic: Uint8Array;
    chainKey: Uint8Array;
    chainIndex: number;
  }>;

  // Skipped keys
  skippedKeys: Array<{
    dhPublic: Uint8Array;
    index: number;
    messageKey: Uint8Array;
    expiresAt: Timestamp;
  }>;
}
```

### Storage Security

- Encrypt ratchet state at rest using device key
- Delete old skipped keys after TTL (7 days)
- Securely wipe keys from memory after use

## Session Management

### Session Reset

When a session becomes corrupted or desync'd:

1. Generate new ephemeral key pair
2. Send session reset message (unencrypted metadata, no content)
3. Wait for acknowledgment
4. Reinitialize from 2DH

### Session Reset Message

```json
{
  "type": "session_reset",
  "content": {
    "reason": "desync|key_rotation|manual",
    "new_ephemeral_public": "<base64>"
  }
}
```

This message is sent using a fresh secure envelope (not the broken ratchet).

## Key Rotation Integration

When a peer rotates their identity encryption key:

1. Receive identity update notification
2. Store new encryption key
3. Continue using current ratchet until break-in recovery naturally occurs
4. New DH ratchets will use the updated key

No explicit action needed - the ratchet is resilient to key changes.

## Security Considerations

### Forward Secrecy Window

Forward secrecy is achieved after:
- One round trip (both parties have sent a message since compromise)
- DH ratchet has occurred

Messages before the ratchet remain vulnerable if keys are compromised.

### Skipped Message Key Limits

- `max_skip = 100`: Limits memory usage
- TTL on skipped keys: 7 days
- If exceeded, session needs reset

### Denial of Service

Attacker with message key could:
- Force excessive key skipping
- Exhaust memory

Mitigation: Limit skipped keys, rate limit incoming messages.

## Test Scenarios

1. **Normal conversation**: Messages flow in both directions
2. **Offline recipient**: Sender sends multiple messages before recipient comes online
3. **Out-of-order**: Messages arrive in different order than sent
4. **Key rotation**: Peer rotates identity key mid-conversation
5. **Session reset**: Recover from corrupted session state
6. **Multiple devices**: Same identity on multiple devices (separate sessions per device)

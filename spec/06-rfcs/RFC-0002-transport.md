# RFC-0002: Post-Urbit Transport Protocol

**Status:** Draft
**Version:** 1.2
**Authors:** Post-Urbit Working Group
**Created:** 2026-01-14
**Supersedes:** None
**Layer:** 01-transport-connectivity

## Abstract

This document specifies the transport protocol for the Post-Urbit network. It defines QUIC-based peer-to-peer communication, identity-authenticated handshakes, and relay services for NAT traversal. The protocol provides secure, multiplexed connections between nodes identified by their Identity Identifiers (IIDs).

**Scope:** This RFC covers QUIC configuration, identity handshake, and relay protocol (PURL). Discovery services (DHT) and mailbox protocol are specified in `00-shared/layer-integration.md`.

## 1. Introduction

### 1.1 Purpose

The transport layer provides reliable, authenticated communication between Post-Urbit nodes. It serves two primary functions specified in this RFC:

1. **Direct connectivity** via QUIC with identity-bound authentication
2. **Relay connectivity** when direct connections fail (NAT, firewalls)

**Out of Scope for This RFC:**
- Discovery services (DHT wire protocol) - see `00-shared/layer-integration.md` "DHT Wire Protocol (Normative)"
- Mailbox protocol (store-and-forward) - see `00-shared/layer-integration.md`

### 1.2 Goals

- **Security**: All connections encrypted with TLS 1.3, authenticated to IID
- **Reliability**: Handle NAT traversal, connection migration, offline peers
- **Performance**: 0-RTT resumption, multiplexed streams, minimal overhead
- **Simplicity**: Single protocol stack, predictable behavior

### 1.3 Requirements Notation

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119].

## 2. Terminology

| Term | Definition |
|------|------------|
| **IID** | Identity Identifier - 20-byte hash derived from genesis signing key |
| **DID** | Device Identifier - 20-byte hash of device signing key |
| **QUIC** | RFC 9000 transport protocol with integrated TLS 1.3 |
| **Relay** | Untrusted intermediary that forwards encrypted packets |
| **PURL** | Post-Urbit Relay Layer - wire format for relayed packets |
| **Allocation** | Relay-assigned routing slot for receiving forwarded traffic |

### 2.1 Base32 Encoding (Normative)

IIDs and DIDs are encoded as **32-character lowercase Crockford Base32** strings.

**Alphabet:** `0123456789abcdefghjkmnpqrstvwxyz` (excludes i, l, o, u)

| Property | Value |
|----------|-------|
| Input | 20 bytes (160 bits) |
| Output | 32 characters |
| Case | Lowercase only |
| Padding | None |

**Bit Ordering Algorithm (Normative):**

1. Treat the 20 input bytes as a 160-bit big-endian integer (byte 0 contains bits 159-152, byte 19 contains bits 7-0)
2. Extract 32 groups of 5 bits each, from MSB to LSB:
   - Group 0: bits 159-155
   - Group 1: bits 154-150
   - ...
   - Group 31: bits 4-0
3. Map each 5-bit value (0-31) to the alphabet character at that index
4. Output as lowercase string

**Decoding** reverses this: map each character to its 5-bit value, concatenate all 160 bits in order, interpret as big-endian 20-byte value.

**Encoding:** Standard 5-bit grouping per Crockford Base32. Encoders MUST output lowercase.

**Decoding:** Decoders MUST reject any character not in the lowercase alphabet. No normalization is performed on wire data.

**UI Input:** User interfaces MAY normalize uppercase to lowercase before creating wire/signed artifacts.

**Example:**
```
20 bytes (hex): 55ff38e37cc2169c2e2412a7c6f2f8517f0f8c34
160 bits (binary, MSB first): 0101010111111111001110001110...
5-bit groups: 01010 10111 11111 10011 10001 110...
Values:       10    23    31    19    17    ...
Alphabet[]:   a     b     z     y     7     ...
Base32 string:  abzy73bycgb9ybrg12tynyxgkfzyh3bk
```

This alphabet was chosen for human readability (avoids ambiguous characters like 0/O, 1/l/I).

## 3. Protocol Overview

### 3.1 Connection Types

| Type | Transport | Authentication | Use Case |
|------|-----------|----------------|----------|
| Direct | QUIC/TLS | Identity handshake | Both peers reachable |
| Relayed | QUIC over PURL | Identity handshake | NAT prevents direct |
| Mailbox | HTTPS | Bearer token | Recipient offline |

### 3.2 Protocol Stack

```
┌─────────────────────────────────────────┐
│         Application Streams             │
│   (Identity, Messaging, Sync, Bulk)     │
├─────────────────────────────────────────┤
│       Identity Handshake Protocol       │
│    (Mutual IID/DID authentication)      │
├─────────────────────────────────────────┤
│              QUIC (RFC 9000)            │
│     (Encryption, Multiplexing)          │
├─────────────────────────────────────────┤
│    PURL Framing (if relayed)            │
│  (Relay routing, allocation tokens)     │
├─────────────────────────────────────────┤
│              UDP / IP                   │
└─────────────────────────────────────────┘
```

## 4. QUIC Configuration

### 4.1 ALPN Protocol String

All Post-Urbit QUIC connections MUST use the ALPN protocol identifier:

```
post-urbit/1
```

Implementations MUST reject connections with mismatched ALPN.

### 4.2 Transport Parameters

| Parameter | Value | Notes |
|-----------|-------|-------|
| max_idle_timeout | 30000 ms | Connection closes after 30s idle |
| initial_max_streams_bidi | 100 | Concurrent bidirectional streams |
| initial_max_streams_uni | 100 | Concurrent unidirectional streams |
| initial_rtt | 100 ms | Conservative initial estimate |
| max_udp_payload_size | 1200 bytes | Safe for most network paths |

### 4.3 TLS Configuration

| Parameter | Requirement |
|-----------|-------------|
| TLS version | 1.3 only (MUST) |
| Cipher suites | MUST support both: TLS_CHACHA20_POLY1305_SHA256 AND TLS_AES_128_GCM_SHA256 |
| Key exchange | X25519 (MUST support) |
| Certificate | Self-signed, ephemeral (identity proven via handshake) |
| SNI | SHOULD be empty or ignored |

**Cipher Suite Requirements (Normative):**
- Implementations MUST support `TLS_CHACHA20_POLY1305_SHA256` (mandatory for software-only performance)
- Implementations MUST support `TLS_AES_128_GCM_SHA256` (mandatory for hardware AES-NI support)
- Implementations SHOULD support `TLS_AES_256_GCM_SHA384` (optional, for environments requiring 256-bit AES)
- Peers MUST offer ALL mandatory suites in ClientHello/ServerHello
- This ensures interoperability between implementations with different optimization profiles (ChaCha-preferred vs AES-preferred)

**Certificate Policy (Normative)**: TLS certificates are NOT used for identity verification. Identity authentication is performed exclusively via the identity handshake (Section 5). Implementations:

- MUST accept ANY certificate presented during TLS handshake
- MUST NOT reject certificates due to: expiration (NotBefore/NotAfter), self-signed status, unknown CA, hostname mismatch, or missing/wrong Extended Key Usage
- MUST NOT perform certificate chain validation
- SHOULD generate fresh ephemeral certificates on each daemon restart

The only requirement is that the certificate enables TLS 1.3 handshake completion. This policy exists because the identity handshake provides all necessary authentication guarantees.

**ALPN Scope (Normative):** This "accept any certificate" policy applies ONLY to connections using ALPN `post-urbit/1`. For DHT/libp2p connections (ALPN `libp2p`), implementations MUST follow standard libp2p-tls certificate requirements where the certificate authenticates the PeerID. See `spec/00-shared/layer-integration.md` "TLS Certificate Policy by ALPN" for the complete policy.

### 4.4 TLS Binding

To bind the identity handshake to the TLS session, implementations MUST derive a `tls_binding` value using the TLS Exporter (RFC 8446 §7.5):

```
tls_binding = TLS-Exporter(
  label:   "post-urbit handshake binding",
  context: empty,
  length:  32
)
```

This value MUST be included in ClientHello and ServerHello messages.

## 5. Identity Handshake Protocol

### 5.1 Overview

After QUIC establishes a TLS-encrypted connection, peers perform an identity handshake to prove they control their claimed IIDs. This binds the transport session to specific identities.

### 5.2 Handshake Stream

The identity handshake MUST occur on the **first client-initiated bidirectional stream**.

**Rules:**
- Client opens this stream immediately after QUIC handshake completes
- Client writes stream type byte `0x01` (Control) as first byte
- Implementations MUST NOT open any other application streams until handshake completes
- QUIC assigns stream IDs automatically; do not hardcode stream ID 0

**Rationale:** This avoids race conditions and ensures handshake completes before any application traffic.

### 5.3 Handshake Flow

```
Client                                          Server
  │                                               │
  │ ─────────── QUIC + TLS 1.3 ──────────────────►│
  │ ◄────────────────────────────────────────────│
  │         (TLS session established)             │
  │                                               │
  │  First bidi stream: Control (0x01)            │
  │                                               │
  │  ┌─────────────────────────────────────────┐  │
  │  │ ClientHello                             │  │
  │  │ - client_iid, client_nonce, timestamp   │  │
  │  │ - tls_binding, client_did (optional)    │  │
  │  └─────────────────────────────────────────┘  │
  │ ─────────────────────────────────────────────►│
  │                                               │
  │  ┌─────────────────────────────────────────┐  │
  │  │ ServerHello                             │  │
  │  │ - server_iid, server_nonce, timestamp   │  │
  │  │ - identity_document, challenge_signature│  │
  │  │ - device_document (if server_did)       │  │
  │  └─────────────────────────────────────────┘  │
  │ ◄─────────────────────────────────────────────│
  │                                               │
  │  ┌─────────────────────────────────────────┐  │
  │  │ ClientAuth                              │  │
  │  │ - identity_document, challenge_signature│  │
  │  │ - device_document (if client_did)       │  │
  │  └─────────────────────────────────────────┘  │
  │ ─────────────────────────────────────────────►│
  │                                               │
  │  ┌─────────────────────────────────────────┐  │
  │  │ HandshakeComplete                       │  │
  │  │ - success: true                         │  │
  │  └─────────────────────────────────────────┘  │
  │ ◄─────────────────────────────────────────────│
  │                                               │
  │     (Connection authenticated to IID pair)    │
```

### 5.4 Stream Framing

The handshake uses the first bidirectional stream opened by the client. The stream type is written once, followed by length-prefixed JSON messages.

```
Stream Layout:
┌────────────────────────────────────────┐
│ Stream Type: 0x01 (Control)            │ 1 byte
├────────────────────────────────────────┤
│ Message 1 Length (big-endian)          │ 4 bytes
├────────────────────────────────────────┤
│ Message 1 (JSON, UTF-8)                │ <length> bytes
├────────────────────────────────────────┤
│ Message 2 Length (big-endian)          │ 4 bytes
├────────────────────────────────────────┤
│ Message 2 (JSON, UTF-8)                │ <length> bytes
├────────────────────────────────────────┤
│              ...                       │
└────────────────────────────────────────┘
```

**Framing Rules:**
- Stream type is written ONCE at stream start (1 byte)
- Each message is preceded by 4-byte big-endian length
- Maximum message size: 65536 bytes (64 KB)
- Message type is in JSON `type` field (NOT a separate byte)

### 5.5 ClientHello Message

```json
{
  "type": "client_hello",
  "version": 1,
  "client_iid": "<32-char-base32-iid>",
  "client_did": "<32-char-base32-did>|null",
  "expected_server_iid": "<32-char-base32-iid>|null",
  "client_nonce": "<32-bytes-base64-standard>",
  "timestamp": "<RFC3339-UTC>",
  "tls_binding": "<32-bytes-base64-standard>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| type | MUST | Literal "client_hello" |
| version | MUST | Protocol version (1) |
| client_iid | MUST | Client's Identity Identifier (Crockford Base32, 32 chars) |
| client_did | MAY | Client's Device Identifier for per-device binding |
| expected_server_iid | MUST/MAY | MUST when connecting to a known IID; MAY omit only for discovery scenarios. If provided, MUST verify match. See IID Binding Requirement in `spec/01-transport-connectivity/peer-handshake.md`. |
| client_nonce | MUST | 32 random bytes (Base64 standard, no padding) |
| timestamp | MUST | RFC3339 UTC, canonical form: `YYYY-MM-DDTHH:MM:SSZ` |
| tls_binding | MUST | TLS exporter value (Base64 standard, no padding) |

**Optional Field Handling (Normative):** Fields marked MAY or SHOULD may be either:
- Omitted from the JSON object entirely, OR
- Present with value `null`

Receivers MUST treat both representations equivalently (absent == null). Senders MAY use either form. This applies to all optional handshake fields in §5.5, §5.6, and §5.9.

**Timestamp Canonicalization:** For signature input, timestamps MUST use the canonical form `YYYY-MM-DDTHH:MM:SSZ` (UTC, no fractional seconds, `Z` suffix). Implementations MUST reject non-canonical forms.

### 5.6 ServerHello Message

```json
{
  "type": "server_hello",
  "version": 1,
  "server_iid": "<32-char-base32-iid>",
  "server_did": "<32-char-base32-did>|null",
  "server_nonce": "<32-bytes-base64-standard>",
  "timestamp": "<RFC3339-UTC>",
  "tls_binding": "<32-bytes-base64-standard>",
  "identity_document": { /* IDOC per RFC-0001 */ },
  "device_document": { /* Device doc if server_did */ },
  "challenge_signature": "<64-bytes-base64-standard>",
  "device_signature": "<64-bytes-base64-standard>|null"
}
```

### 5.7 Challenge Signature Construction

The challenge signature proves the signer controls the claimed identity.

**Ed25519 Signing API (Normative):** All signatures in this RFC use **standard Ed25519** (RFC 8032 PureEdDSA), NOT Ed25519ph or Ed25519ctx. When the RFC shows `Ed25519_Sign(key, SHA256(data))`, the 32-byte SHA256 digest is the message input to the standard Ed25519 signing function. Do NOT use prehash variants (Ed25519ph), which produce different signatures.

**Domain Separator:** `post-urbit-handshake-v1` (23 ASCII bytes, no NUL terminator)

**Server's challenge signature:**

```
DOMAIN = b"post-urbit-handshake-v1"   // exactly 23 bytes

challenge_data = concat(
  DOMAIN,                          // 23 bytes
  decode_base64(client_nonce),     // 32 bytes
  decode_base64(server_nonce),     // 32 bytes
  decode_base64(tls_binding),      // 32 bytes
  decode_base32(client_iid),       // 20 bytes raw
  decode_base32(server_iid)        // 20 bytes raw
)
// Total: 23 + 32 + 32 + 32 + 20 + 20 = 159 bytes

challenge_signature = Ed25519_Sign(
  server_signing_key,
  SHA256(challenge_data)
)
```

**Client's challenge signature (in ClientAuth):**

```
challenge_data = concat(
  DOMAIN,                          // 23 bytes
  decode_base64(server_nonce),     // 32 bytes (swapped)
  decode_base64(client_nonce),     // 32 bytes (swapped)
  decode_base64(tls_binding),      // 32 bytes
  decode_base32(server_iid),       // 20 bytes raw (swapped)
  decode_base32(client_iid)        // 20 bytes raw (swapped)
)
// Total: 159 bytes

challenge_signature = Ed25519_Sign(
  client_signing_key,
  SHA256(challenge_data)
)
```

### 5.8 Device Signature (Optional)

If a Device Identifier (DID) is provided, the party MUST also prove device ownership.

**Domain Separator:** `post-urbit-device-v1` (20 ASCII bytes)

```
DEVICE_DOMAIN = b"post-urbit-device-v1"   // exactly 20 bytes

// SERVER device signature (in ServerAuth):
server_device_challenge = concat(
  DEVICE_DOMAIN,                        // 20 bytes
  decode_base64(client_nonce),          // 32 bytes (from ClientHello)
  decode_base64(server_nonce),          // 32 bytes (from ServerHello)
  decode_base64(tls_binding),           // 32 bytes
  decode_base32(server_iid),            // 20 bytes raw
  decode_base32(server_did)             // 20 bytes raw
)

server_device_signature = Ed25519_Sign(
  server_device_signing_key,
  SHA256(server_device_challenge)
)

// CLIENT device signature (in ClientAuth):
client_device_challenge = concat(
  DEVICE_DOMAIN,                        // 20 bytes
  decode_base64(server_nonce),          // 32 bytes (from ServerHello)
  decode_base64(client_nonce),          // 32 bytes (from ClientHello)
  decode_base64(tls_binding),           // 32 bytes
  decode_base32(client_iid),            // 20 bytes raw
  decode_base32(client_did)             // 20 bytes raw
)

client_device_signature = Ed25519_Sign(
  client_device_signing_key,
  SHA256(client_device_challenge)
)

// Both: 20 + 32 + 32 + 32 + 20 + 20 = 156 bytes
```

**Note:** The nonce ordering matches §5.7 (challenge signatures): server signs with client_nonce first, client signs with server_nonce first.

**Device Verification:**
1. Verify `device_document.signature_by_identity` using identity signing key lookup order per RFC-0001 §7 (current → previous → ALL history[] entries regardless of expires_at). The `expires_at` field is UI metadata only; try ALL history[] entries for signature verification.
2. Verify `device_signature` using `device_document.device_signing_key`
3. Check `device_document.iid` matches claimed IID
4. Check `device_document.did` matches claimed DID
5. If `device_document.expires_at` exists, check not expired

**Note:** Device documents may be signed with a key that was subsequently rotated. Verification MUST accept any identity signing key that was valid at the time of signing (per key history lookup rules).

### 5.9 ClientAuth Message

```json
{
  "type": "client_auth",
  "identity_document": { /* IDOC per RFC-0001 */ },
  "device_document": { /* Device doc if client_did was provided */ },
  "challenge_signature": "<64-bytes-base64-standard>",
  "device_signature": "<64-bytes-base64-standard>|null"
}
```

### 5.10 HandshakeComplete Message

```json
{
  "type": "handshake_complete",
  "success": true,
  "error": null
}
```

On failure:

```json
{
  "type": "handshake_complete",
  "success": false,
  "error": {
    "code": "IDENTITY_MISMATCH",
    "message": "Server IID does not match expected"
  }
}
```

### 5.11 Verification Procedures

**Key Encoding Note:** All key fields in identity/device documents (`keys.signing.genesis`, `keys.signing.current`, `device_signing_key`) are Base64-encoded raw 32-byte Ed25519 public keys. Implementations MUST decode these to raw bytes before use in `derive_iid()` or `Ed25519_Verify()`.

**Server Verifies Client:**
1. Check `client_iid` is well-formed (32 chars, Base32 lowercase)
2. Check `timestamp` is within ±5 minutes of server time
3. Check `tls_binding` matches current TLS session's exporter value
4. If `expected_server_iid` provided, verify it matches server's IID
5. Receive ClientAuth and verify:
   a. Validate client's identity document per RFC-0001
   b. Verify `client_iid == derive_iid(Base64Decode(identity_document.keys.signing.genesis))`
   c. Reconstruct challenge data
   d. Verify `challenge_signature` using `Ed25519_Verify(Base64Decode(identity_document.keys.signing.current), ...)`
6. If `client_did` provided, verify device signature per §5.8

**Client Verifies Server:**
1. Check `server_iid` is well-formed
2. Check `timestamp` is within ±5 minutes
3. Check `tls_binding` matches current TLS session
4. Validate server's identity document per RFC-0001
5. Verify `server_iid == derive_iid(Base64Decode(identity_document.keys.signing.genesis))`
6. If `expected_server_iid` provided, verify it matches
7. Reconstruct challenge data
8. Verify `challenge_signature` using `Ed25519_Verify(Base64Decode(identity_document.keys.signing.current), ...)`
9. If `server_did` provided, verify device signature per §5.8

### 5.12 Error Codes (Handshake)

| Code | Meaning | Recovery |
|------|---------|----------|
| IDENTITY_MISMATCH | IID doesn't match expected | Connect to correct peer |
| SIGNATURE_INVALID | Challenge signature failed | May indicate attack |
| TIMESTAMP_EXPIRED | Timestamp too old or future | Sync clocks, retry |
| DOCUMENT_INVALID | Identity document invalid | Refresh peer's document |
| TLS_BINDING_MISMATCH | TLS session mismatch | Possible MITM, abort |
| VERSION_UNSUPPORTED | Protocol version unknown | Upgrade software |
| NONCE_REUSE | Nonce seen before | Possible replay, abort |

### 5.13 Timeouts

| Phase | Timeout | Action |
|-------|---------|--------|
| Awaiting ClientHello/ServerHello | 10 seconds | Close connection |
| Awaiting ClientAuth/HandshakeComplete | 10 seconds | Close connection |
| Total handshake | 30 seconds | Close connection |

### 5.14 Handshake Failure Behavior (Normative)

When a handshake fails, implementations MUST follow these rules:

| Condition | Behavior |
|-----------|----------|
| Framing error (e.g., length prefix exceeds max, truncated message) | MUST close connection with `HANDSHAKE_FAILED` (0x101) |
| JSON parse error (invalid UTF-8, malformed JSON) | MUST close connection with `HANDSHAKE_FAILED` (0x101) |
| Unknown handshake message type | MUST close connection with `HANDSHAKE_FAILED` (0x101) |
| Signature verification failure | Server MAY send `handshake_complete(success=false)` before closing |
| Document validation failure | Server MAY send `handshake_complete(success=false)` before closing |

**Server Response Timing:**
- Server MAY send `handshake_complete` with `success: false` and an error object before closing the connection
- Server MUST NOT wait indefinitely for client acknowledgment after sending failure response
- Server SHOULD close the connection within 1 second of sending `handshake_complete(success=false)`
- If sending `handshake_complete` fails (write error), server MUST close connection immediately

### 5.15 Anonymous Connections (Out of Scope)

Anonymous connections (where `client_iid` is null) are NOT defined in this RFC.

**Rationale:** Anonymous connections require different transcript construction for challenge signatures. They are primarily useful for:
- Public relay access (see relay specification below)
- DHT queries (separate protocol)
- Discovery servers (separate protocol)

For v1, all peer-to-peer connections MUST be mutually authenticated. Anonymous mode MAY be defined in a future RFC.

## 6. Stream Types

### 6.1 Stream Type Codes

| Code | Name | Direction | Payload | Purpose |
|------|------|-----------|---------|---------|
| 0x00 | Reserved | - | - | Reserved for future use |
| 0x01 | Control | Bidirectional | JSON | Handshake, keepalive, management |
| 0x02 | Identity | Bidirectional | JSON | Identity document exchange |
| 0x03 | Message | Bidirectional | Binary | Application messages (PUSE envelopes) |
| 0x04 | Sync | Bidirectional | Binary | CRDT synchronization (CBOR-encoded) |
| 0x05 | Bulk | Unidirectional | Binary | Large data transfers (**Reserved for v2**, see §6.6) |
| 0x06-0xFF | Reserved | - | - | Future use |

### 6.2 Stream Opening

Each stream starts with a 1-byte type identifier:

```
New Stream:
┌────────────────────────────────────────┐
│ Stream Type                            │ 1 byte
├────────────────────────────────────────┤
│ Message Frame 1                        │ 4 + N bytes
├────────────────────────────────────────┤
│ Message Frame 2                        │ 4 + M bytes
├────────────────────────────────────────┤
│              ...                       │
└────────────────────────────────────────┘
```

### 6.3 Message Frame Format

All stream types use length-prefixed framing:

```
Message Frame:
┌────────────────────────────────────────┐
│ Length (big-endian)                    │ 4 bytes
├────────────────────────────────────────┤
│ Payload                                │ <length> bytes
└────────────────────────────────────────┘
```

**Payload Type by Stream:**

| Stream | Payload Format | Notes |
|--------|----------------|-------|
| Control (0x01) | UTF-8 JSON | Has `type` field in JSON |
| Identity (0x02) | UTF-8 JSON | Has `type` field in JSON |
| Message (0x03) | Binary | PUSE envelope (see RFC-0003) |
| Sync (0x04) | Binary | 1-byte type + CBOR (see sync-protocol.md) |
| Bulk (0x05) | Binary | First 2 bytes = opcode |

**JSON Streams (0x01-0x02):**
- Payload MUST be valid UTF-8 JSON
- JSON object MUST have a `type` field (string) identifying message kind
- Example: `{"type": "identity_update", ...}`
- **Identity stream (0x02) schemas:** See `spec/00-shared/layer-integration.md` "Identity Message JSON Schemas (Normative)" for the authoritative message type definitions (`identity_update`, `identity_request`, `identity_response`, `identity_ack`). These schemas are normative for interoperability.

**Binary Streams (0x03-0x05):**
- 0x03 (Message): Raw PUSE envelope bytes (see RFC-0003 §3)
- 0x04 (Sync): 1-byte message type (0x01-0x07) + CBOR data. **Normative schemas:** See `spec/03-messaging-sync/sync-protocol.md` "CBOR Schemas for Sync Messages" and "CBOR Canonicalization (Normative)" for authoritative wire format definitions.
- 0x05 (Bulk): **Reserved for v2** - see §6.6 "Bulk Stream Wire Protocol"

### 6.4 Stream-Specific Limits

| Stream Type | Max Message Size | Notes |
|-------------|------------------|-------|
| Control | 64 KB | Includes identity documents |
| Identity | 64 KB | Identity and device documents |
| Message | 1 MB | Encrypted PUSE envelopes (matches PUSE max) |
| Sync | 1 MB | CRDT operations |
| Bulk | 16 MB | File transfer chunks |

**Frame Size Clarification (Normative):**

The 1 MB (1,048,576 byte) limit for message stream frames applies to the **frame payload length**, NOT including the 4-byte big-endian length prefix.

```
Frame structure:
┌──────────────────────┬────────────────────────────┐
│ Length (4 bytes)     │ Payload (≤ 1,048,576 bytes)│
└──────────────────────┴────────────────────────────┘
```

- Parsers read the 4-byte length first
- If `length > 1,048,576`, reject before reading payload
- Total frame size on wire is `4 + length` bytes (max 1,048,580)

This aligns with PUSE max envelope size: a max-size PUSE envelope (1,048,576 bytes) fits exactly in a single message stream frame.

### 6.5 Stream Multiplicity Rules

| Stream Type | Multiplicity | Lifetime |
|-------------|--------------|----------|
| Control (0x01) | Exactly 1 per connection | Connection lifetime |
| Identity (0x02) | At most 1 per direction | Connection lifetime |
| Message (0x03) | Multiple allowed | Per-message or long-lived |
| Sync (0x04) | At most 1 per direction | Connection lifetime |
| Bulk (0x05) | Multiple allowed | Per-transfer |

**Rules:**
1. **Control stream:** The client MUST open exactly one Control stream immediately after QUIC handshake (§5.2). Additional Control streams MUST be rejected.
2. **Long-lived streams (Identity, Sync):** Each peer MAY open at most one outgoing bidirectional stream of each type. Peers MUST accept at most one incoming stream per type. Opening a second stream of the same type is a protocol error (close connection with DUPLICATE_STREAM_TYPE 0x108).
3. **Per-message streams (Message):** Multiple concurrent bidirectional Message streams are allowed.
4. **Bulk streams:** Multiple unidirectional Bulk streams are allowed for concurrent transfers.

### 6.6 Bulk Stream Wire Protocol (OUT OF SCOPE FOR V1)

**Status:** Stream type 0x05 (Bulk) is **reserved for future use**. The wire protocol is not fully specified in this version.

**V1 Requirement (Normative):**
- Implementations MUST NOT open streams with type byte 0x05
- Implementations MUST reject incoming streams with type byte 0x05 with error code `STREAM_TYPE_UNKNOWN` (0x102)
- The stream type code 0x05 is reserved; implementations MUST NOT reassign it

**Rationale:** Bulk transfer protocols require careful specification of:
- Transfer initiation and metadata exchange
- Chunk ordering, acknowledgment, and flow control
- Integrity verification (progressive or final)
- Resumption after connection loss
- Concurrent transfer management

These requirements warrant dedicated design effort beyond v1 scope.

**Future Direction (Informative):**

A future protocol version will specify the complete bulk stream wire protocol. The following is a non-normative sketch of the anticipated design:

**Opcode Registry (Reserved):**

| Opcode | Name | Direction | Status |
|--------|------|-----------|--------|
| 0x0001 | TRANSFER_START | Sender → Receiver | Reserved |
| 0x0002 | DATA_CHUNK | Sender → Receiver | Reserved |
| 0x0003 | TRANSFER_END | Sender → Receiver | Reserved |
| 0x0004 | ABORT | Either | Reserved |
| 0x0005 | ACK | Receiver → Sender | Reserved |
| 0x0006-0xFFFF | Reserved | - | Reserved |

**Anticipated Message Formats (Non-Normative):**

```
TRANSFER_START (0x0001):
┌────────────────────────────────────────┐
│ Opcode (0x0001, big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Transfer ID (random)                   │ 16 bytes
├────────────────────────────────────────┤
│ Total Size (big-endian, 0 = unknown)   │ 8 bytes
├────────────────────────────────────────┤
│ Chunk Size (big-endian)                │ 4 bytes
├────────────────────────────────────────┤
│ Content Hash (BLAKE3, if known)        │ 32 bytes
├────────────────────────────────────────┤
│ Metadata Length (big-endian)           │ 2 bytes
├────────────────────────────────────────┤
│ Metadata (JSON, optional)              │ <length> bytes
└────────────────────────────────────────┘

DATA_CHUNK (0x0002):
┌────────────────────────────────────────┐
│ Opcode (0x0002, big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Transfer ID                            │ 16 bytes
├────────────────────────────────────────┤
│ Chunk Index (big-endian, 0-based)      │ 4 bytes
├────────────────────────────────────────┤
│ Flags                                  │ 1 byte
│   bit 0: FINAL (last chunk)            │
│   bits 1-7: reserved                   │
├────────────────────────────────────────┤
│ Data                                   │ remaining bytes
└────────────────────────────────────────┘

TRANSFER_END (0x0003):
┌────────────────────────────────────────┐
│ Opcode (0x0003, big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Transfer ID                            │ 16 bytes
├────────────────────────────────────────┤
│ Final Hash (BLAKE3)                    │ 32 bytes
├────────────────────────────────────────┤
│ Total Chunks Sent (big-endian)         │ 4 bytes
└────────────────────────────────────────┘

ABORT (0x0004):
┌────────────────────────────────────────┐
│ Opcode (0x0004, big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Transfer ID                            │ 16 bytes
├────────────────────────────────────────┤
│ Reason Code                            │ 1 byte
│   0x01: CANCELLED                      │
│   0x02: INTEGRITY_FAILURE              │
│   0x03: RESOURCE_EXHAUSTED             │
│   0x04: TIMEOUT                        │
├────────────────────────────────────────┤
│ Message Length (big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Message (UTF-8)                        │ <length> bytes
└────────────────────────────────────────┘

ACK (0x0005):
┌────────────────────────────────────────┐
│ Opcode (0x0005, big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Transfer ID                            │ 16 bytes
├────────────────────────────────────────┤
│ Acknowledged Chunk Index (big-endian)  │ 4 bytes
├────────────────────────────────────────┤
│ Receiver Window (chunks, big-endian)   │ 2 bytes
└────────────────────────────────────────┘
```

**Anticipated Lifecycle (Non-Normative):**

1. **Initiation:** Sender opens unidirectional bulk stream, sends TRANSFER_START
2. **Data Transfer:** Sender sends DATA_CHUNK messages in order (or out-of-order if receiver supports)
3. **Acknowledgment:** Receiver sends ACK on reverse stream (or relies on QUIC-level ACK)
4. **Completion:** Sender sends TRANSFER_END with final hash; receiver verifies
5. **Abort:** Either party may send ABORT at any time

**Open Design Questions:**
- Should bulk streams be bidirectional (for ACK) or use paired unidirectional streams?
- Progressive hash verification vs. final-only verification
- Integration with application-layer encryption (PUSE envelope for chunks?)
- Resumption across connections (persistent transfer IDs?)

These questions will be resolved in a future RFC dedicated to bulk transfer.

## 7. Relay Protocol (PURL)

### 7.1 Overview

Relays are untrusted intermediaries that forward encrypted QUIC packets when direct connectivity fails. They see only:
- Source IP address
- Destination IID
- Packet timing and sizes
- Encrypted payload (opaque)

### 7.2 Trust Model

| Property | Guarantee |
|----------|-----------|
| Content confidentiality | Relays see encrypted blobs only |
| No authority | Relays cannot forge, modify, or inject |
| Availability | Relays may drop connections or go offline |
| Replaceability | Users can switch relays anytime |

### 7.3 Stable Port Model

Relays use a **stable port model** for compatibility with identity document publishing:

| Aspect | Design |
|--------|--------|
| Relay port | Stable (e.g., 4433) for all clients |
| Routing key | Destination IID in packet header |
| Allocation token | Authenticates sender to relay |
| Identity publishing | Publish `relay.example.com:4433` once |

**Rationale:**
- Identity documents publish relay endpoints
- Identity updates are expensive (signed, sequence-incrementing)
- Allocation lifetimes (~1h) << identity publish intervals (~24h)
- Per-allocation ports would require frequent identity updates

### 7.4 PURL Wire Format

Every UDP datagram on a relayed connection has a PURL header:

```
PURL Packet:
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x52 0x4C ("PURL")   │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Packet Type                            │ 1 byte
├────────────────────────────────────────┤
│ Allocation Token                       │ 16 bytes
├────────────────────────────────────────┤
│ Destination IID (raw bytes)            │ 20 bytes
├────────────────────────────────────────┤
│ Payload Length (big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Payload (opaque bytes; semantics       │ <length> bytes
│   depend on Packet Type)               │
└────────────────────────────────────────┘

Total header: 44 bytes
Max payload capacity: 65535 bytes (u16 field capacity)

**Trailing Bytes Rule (Normative):** Parsers MUST reject PURL datagrams where `datagram_length != 44 + payload_length`. Trailing bytes indicate corruption or protocol confusion.

**Payload Limits (Normative):**

| Path Type | Max Inner Payload | Total Datagram | Notes |
|-----------|-------------------|----------------|-------|
| Direct (no relay) | 1200 bytes | 1200 bytes | Standard QUIC safe payload |
| Relay path | 1200 bytes | 1244 bytes | QUIC Initial compatibility; may exceed IPv6 minimum MTU |

- **Direct connections**: QUIC datagrams use `max_udp_payload_size = 1200`
- **Relay connections**: Inner QUIC payloads MUST NOT exceed **1200 bytes** to maintain compatibility with standard QUIC stacks (particularly the 1200-byte Initial datagram minimum)
- Receivers MUST silently drop PURL packets with Payload Length > 1200

**MTU Considerations:**
The relay path produces 1244-byte UDP datagrams (44-byte PURL header + 1200-byte payload), which exceeds the IPv6 minimum MTU safe payload (1232 bytes = 1280 - 40 - 8).

Operational guidance:
- **Most networks**: Support 1500+ byte packets; 1244-byte datagrams are not fragmented
- **IPv6 minimum MTU paths**: May fragment or drop 1244-byte datagrams. Implementations SHOULD use Path MTU Discovery (PMTUD) and reduce inner payload size if MTU issues are detected
- **Fallback**: If relay path consistently fails, clients SHOULD attempt direct connectivity or use HTTP-based relay (future extension)

**Rationale:** Using 1200-byte inner payload ensures compatibility with commodity QUIC implementations that enforce the 1200-byte Initial minimum. The 44-byte PURL overhead is acceptable for the vast majority of network paths.
```

**Payload Semantics by Packet Type (Normative):**

| Packet Type | Payload Semantics |
|-------------|-------------------|
| DATA (0x01) | Payload MUST be a QUIC UDP payload. Recipients MUST pass it to their QUIC stack. |
| COORDINATE (0x09) | Payload MUST be UTF-8 JSON per `spec/01-transport-connectivity/nat-traversal.md`. Recipients MUST NOT pass it to QUIC stack; handle at coordination layer. |
| PING (0x02), PONG (0x03), REFRESH (0x05), RELEASE (0x06) | Empty payload (0 bytes). |
| ERROR (0x07) | Binary error payload per §7.12. |
| REBIND (0x08) | UTF-8 JSON per §7.11. |

### 7.5 PURL Packet Types (Normative Registry)

| Code | Name | Description |
|------|------|-------------|
| 0x01 | DATA | Forward payload to destination IID |
| 0x02 | PING | Keepalive request |
| 0x03 | PONG | Keepalive response |
| 0x04 | Reserved | (allocation via HTTPS, not UDP) |
| 0x05 | REFRESH | Extend allocation lifetime |
| 0x06 | RELEASE | End allocation |
| 0x07 | ERROR | Relay error response |
| 0x08 | REBIND | Update source IP:port binding |
| 0x09 | COORDINATE | Hole-punch coordination (see nat-traversal.md) |
| 0x0A-0xFF | Reserved | Future use |

**Note:** REBIND is 0x08, ERROR is 0x07, COORDINATE is 0x09. This is the authoritative registry.

### 7.6 Encapsulation Model

**Client ↔ Relay Communication:**
- All UDP datagrams between client and relay are PURL-framed
- Client sends PURL packets to relay's stable port (e.g., 4433)
- Relay receives PURL, processes based on packet type

**Relay Forwarding (DATA packets):**
- Relay extracts destination IID from PURL header
- Relay looks up allocation for that IID
- Relay forwards **the entire PURL packet unchanged** to destination's bound IP:port
- Destination client decapsulates PURL and passes inner payload to QUIC stack

**Receiver Decapsulation (Normative):**
- For relay-forwarded PURL packets including DATA (0x01) and COORDINATE (0x09), the destination peer MUST ignore the allocation token field (the relay validates it)
- Recipients MUST NOT validate the allocation token on received (forwarded) packets
- Only relays validate allocation tokens; recipients ignore them during decapsulation
- For COORDINATE packets, the destination peer validates the coordination message using the message-level signature (per nat-traversal.md §Signature Requirements), not the allocation token
- Recipients MAY sanity-check that `Destination IID == my IID` and drop misrouted packets

**Control Packets (non-DATA):**
- PING/PONG/REFRESH/RELEASE/REBIND/ERROR are relay control plane
- These packets are processed by relay and NOT forwarded to peers
- Destination IID field is zero (20 null bytes) for control packets

**Control Packet Payload Formats:**
| Type | Payload | Notes |
|------|---------|-------|
| PING (0x02) | Empty (0 bytes) | Client→Relay keepalive |
| PONG (0x03) | Empty (0 bytes) | Relay→Client keepalive response |
| REFRESH (0x05) | Empty (0 bytes) | Extends allocation by original lifetime |
| RELEASE (0x06) | Empty (0 bytes) | Client terminates allocation gracefully |
| ERROR (0x07) | See §7.12 | Error code + message |
| REBIND (0x08) | UTF-8 JSON | See §7.11 for signed message format |

```
Sender                Relay                 Receiver
   │                     │                      │
   │ PURL[DATA,dest=B]   │                      │
   ├────────────────────►│                      │
   │                     │ lookup(B) → IP:port  │
   │                     │                      │
   │                     │ PURL[DATA,dest=B]    │
   │                     ├─────────────────────►│
   │                     │                      │ decapsulate
   │                     │                      │ → QUIC packet
```

### 7.7 Field Encoding

**Destination IID:**
- Raw 20 bytes (NOT Base32 encoded on wire)
- Decode Crockford Base32 string to get 20-byte value
- Zero-fill (20 null bytes) for control packets (PING, REFRESH, etc.)

**Allocation Token:**
- 16 random bytes assigned by relay during allocation
- When serialized to string: Base64url, no padding (22 chars)

### 7.8 Allocation Protocol

Allocation requests use HTTPS (not UDP) for reliability:

```
POST /allocate HTTP/1.1
Host: relay.example.com
Content-Type: application/json

{
  "iid": "<32-char-base32-iid>",
  "lifetime": 3600,
  "timestamp": "<RFC3339-UTC-canonical>",
  "nonce": "<16-bytes-base64url-no-padding>",
  "identity_doc_sequence": "42",
  "signature": "<64-bytes-base64-standard>"
}
```

**Note:** `identity_doc_sequence` is a decimal string (not number) to avoid JSON uint64 precision issues.

**HTTPS TLS Policy (Normative):**
Clients MUST perform WebPKI certificate validation and hostname verification for allocation requests. Self-signed certificates MUST be rejected. This policy is distinct from the `post-urbit/1` QUIC "accept any certificate" policy (§4.3), which applies only to peer-to-peer QUIC connections. Relay allocation is an HTTPS API call to a relay operator's infrastructure, and standard web security practices apply.

**Signature Construction:**

**Domain Separator:** `post-urbit-relay-alloc-v1` (25 ASCII bytes)

```
DOMAIN = b"post-urbit-relay-alloc-v1"  // exactly 25 bytes

signature_input = concat(
  DOMAIN,                              // 25 bytes
  encode_utf8(iid),                    // 32 bytes (Base32 string)
  encode_be_u32(lifetime),             // 4 bytes
  encode_utf8(timestamp),              // 20 bytes (canonical)
  decode_base64url(nonce)              // 16 bytes raw
)
// Total: 25 + 32 + 4 + 20 + 16 = 97 bytes

signature = Ed25519_Sign(
  signing_key,
  SHA256(signature_input)
)
```

**Relay Verification:**
1. Parse request JSON
2. Check `timestamp` within ±5 minutes
3. Check `nonce` is exactly 22 characters (Base64url of 16 bytes, no padding) and not seen before (10-minute replay cache)
4. Fetch/cache identity document for `iid` at `identity_doc_sequence` or higher
5. Reconstruct signature input
6. Verify signature against identity document's current signing key
7. If valid, create allocation record with UDP binding **pending** (HTTPS source IP:port is TCP, not UDP)

**UDP Binding Establishment (Two-Step Model):**
The HTTPS allocation returns `(allocation_id, token)` but does NOT establish the UDP binding. The relay learns the client's UDP source address:port from the **first UDP packet** the client sends:
- **Initial bind:** First valid PURL packet from client with valid token in header establishes the UDP binding
- **Rebind:** REBIND packet (type 0x08) with signed JSON payload updates binding after NAT changes
- **Validation:** Subsequent DATA packets must come from the bound UDP address; different source with same token is rejected (potential token theft)

**Valid Packet Types for Initial Binding (Normative):**
A relay MUST establish initial UDP binding upon receiving ANY well-formed PURL packet with a valid token (matching an existing `pending` allocation), regardless of Packet Type. Specifically:
- PING (0x02), PONG (0x03), DATA (0x01): Bind immediately
- REFRESH (0x05), RELEASE (0x06): Bind immediately
- REBIND (0x08): Bind after payload signature validation passes

The Destination IID field is NOT validated for binding purposes (it may be all-zeros for control packets, or any IID for DATA). A packet with correct token but any Destination IID value MUST trigger binding if other validation passes.

**Binding Timestamp (Normative):**
Relays MUST record a `bound_at` timestamp for each allocation equal to the relay's **local monotonic time** (e.g., Unix milliseconds) when the binding is established or updated. This timestamp is used for routing selection (§7.13). The signed `timestamp` inside REBIND JSON is used only for authorization validation (±5 min window), NOT for routing selection.

**Allocation Lifetime Bounds (Normative):**
- Relays MUST reject or clamp `lifetime` values outside the range [60, 86400] seconds (1 minute to 24 hours)
- The allocation response MUST include the effective `granted_lifetime` (which may differ from requested `lifetime` if clamped)
- REFRESH operations extend the allocation by the `granted_lifetime` from the original allocation, NOT the originally requested `lifetime`

### 7.9 Allocation Response

```json
{
  "allocation_id": "<unique-id>",
  "relay_address": "relay.example.com",
  "relay_port": 4433,
  "expires_at": "<RFC3339-UTC>",
  "granted_lifetime": 3600,
  "token": "<22-char-base64url-token>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| allocation_id | MUST | Unique identifier for this allocation |
| relay_address | MUST | Hostname of the relay |
| relay_port | MUST | UDP port for PURL packets |
| expires_at | MUST | RFC3339 UTC timestamp when allocation expires |
| granted_lifetime | MUST | Effective lifetime in seconds (may differ from requested if clamped) |
| token | MUST | 22-character Base64url allocation token |

### 7.10 Allocation Binding

Allocations are bound to the source IP:port **when the first valid UDP PURL packet is received** (per §7.8 two-step model). The HTTPS allocation request does NOT establish UDP binding; binding state is initially `pending` until the first UDP packet arrives.

| Scenario | Behavior |
|----------|----------|
| First valid UDP packet | Establishes binding to source IP:port |
| Same IP:port (after binding) | Token accepted, packet forwarded |
| Different IP, same token | Rejected (potential token theft) |
| NAT rebinding | Client sends REBIND message (§7.11) |

### 7.11 REBIND Message

When a client's IP changes (NAT rebinding, network handoff), it sends a PURL packet with:
- **Type:** 0x08 (REBIND)
- **Destination IID:** 20 zero bytes (control-plane marker)
- **Token:** Same token from allocation
- **Payload:** UTF-8 JSON as follows:

```json
{
  "type": "rebind",
  "allocation_id": "<id>",
  "token": "<22-char-base64url>",
  "timestamp": "<RFC3339-UTC>",
  "signature": "<64-bytes-base64-standard>"
}
```

**Relay MUST verify:**
1. Payload `token` matches header token
2. Signature is valid for the IID that created the allocation
3. Timestamp is within ±5 minutes
4. Update UDP binding to the source address:port of this UDP packet

**Signature Construction:**

**Domain Separator:** `post-urbit-rebind-v1` (20 ASCII bytes)

```
DOMAIN = b"post-urbit-rebind-v1"  // exactly 20 bytes

rebind_input = concat(
  DOMAIN,                          // 20 bytes
  encode_utf8(allocation_id),      // variable
  decode_base64url(token),         // 16 bytes
  encode_utf8(timestamp)           // 20 bytes (canonical)
)

signature = Ed25519_Sign(signing_key, SHA256(rebind_input))
```

### 7.12 Relay Errors

```
ERROR Packet Payload:
┌────────────────────────────────────────┐
│ Error Code                             │ 1 byte
├────────────────────────────────────────┤
│ Retry After (seconds, big-endian)      │ 4 bytes
├────────────────────────────────────────┤
│ Message Length                         │ 2 bytes
├────────────────────────────────────────┤
│ Message (UTF-8)                        │ <length> bytes
└────────────────────────────────────────┘
```

**Error Codes:**

| Code | Name | Description |
|------|------|-------------|
| 0x01 | RATE_LIMITED | Too many requests |
| 0x02 | ALLOCATION_NOT_FOUND | No allocation for this IID |
| 0x03 | ALLOCATION_EXPIRED | Allocation timed out |
| 0x04 | INVALID_DESTINATION | Unknown destination IID |
| 0x05 | RELAY_OVERLOADED | Server capacity exceeded |
| 0x06 | AUTHENTICATION_FAILED | Invalid signature or token |
| 0x07 | BANNED | Client blocked by policy |

### 7.13 Rate Limits

| Limit | Default | Purpose |
|-------|---------|---------|
| Allocations per IID | 5 | Prevent exhaustion |
| Packets per second | 1000 | Prevent flooding |
| Bytes per second | 10 MB | Bandwidth cap |
| Concurrent connections | 100 | Resource protection |
| Allocation lifetime | 3600s | Reclaim unused |

**Allocation Routing Selection (Normative):** When multiple allocations exist for the same IID on a relay, the relay MUST use deterministic selection for inbound packet routing:
1. Route to the allocation with the highest `bound_at` timestamp (per §7.8 "Binding Timestamp")
2. If `bound_at` values are **exactly equal** (same millisecond), route to the allocation with the lexicographically smallest `allocation_id`
3. Allocations in `pending` state (no UDP binding yet, `bound_at` undefined) are NOT considered for routing

**Timestamp Comparison:** The `bound_at` timestamp is relay-local monotonic time in milliseconds. Compare as unsigned 64-bit integers. Do NOT use "within N seconds" approximations—exact comparison only.

**Allocation ID Comparison:** The `allocation_id` is an ASCII string matching `[a-z0-9-]+`. Comparison MUST be bytewise lexicographic over UTF-8/ASCII bytes, case-sensitive. No Unicode normalization is applied.

This ensures consistent routing behavior across relay implementations when multiple devices share an IID.

### 7.14 Data Flow Diagram

```
Alice ──────────────────────────────────────────────────── Bob
  │                                                          │
  │  ┌─────────────────────────────────────────────────┐    │
  │  │              Relay Server (port 4433)            │    │
  │  │                                                  │    │
  │  │   ┌─────────────────┐    ┌─────────────────┐    │    │
  │  │   │ Alice's Alloc   │    │ Bob's Alloc     │    │    │
  ├──┼──►│ Token: abc...   │    │ Token: xyz...   │◄───┼────┤
  │  │   │ Bound: 1.2.3.4  │    │ Bound: 5.6.7.8  │    │    │
  │  │   └────────┬────────┘    └────────┬────────┘    │    │
  │  │            │                      │             │    │
  │  │            │  ┌──────────────┐    │             │    │
  │  │            └─►│ Route by IID │◄───┘             │    │
  │  │               └──────────────┘                  │    │
  │  └─────────────────────────────────────────────────┘    │
  │                                                          │

1. Alice connects to relay:4433 with her token
2. Alice sends PURL DATA with dest=Bob's IID
3. Relay looks up Bob's allocation by IID
4. Relay forwards to Bob's bound IP:port
5. Bob receives; Alice's IP is hidden
```

## 8. Connection Lifecycle

### 8.1 State Machine

```
┌─────────────┐
│    IDLE     │
└──────┬──────┘
       │ connect()
       ▼
┌─────────────┐
│ CONNECTING  │ ← QUIC handshake
└──────┬──────┘
       │ TLS complete
       ▼
┌─────────────┐
│ HANDSHAKING │ ← Identity verification (§5)
└──────┬──────┘
       │ identity verified
       ▼
┌─────────────┐
│  CONNECTED  │ ← Application streams ready
└──────┬──────┘
       │ timeout / error / close
       ▼
┌─────────────┐
│   CLOSED    │
└─────────────┘
```

### 8.2 Connection Events

| Event | Trigger | Action |
|-------|---------|--------|
| connected | Handshake complete | Start application streams |
| stream_opened | Peer opens stream | Handle by stream type |
| data_received | Data on stream | Dispatch to handler |
| connection_lost | Timeout or error | Reconnect or notify app |
| migration | IP address change | Continue seamlessly |

### 8.3 0-RTT Resumption

For previously connected peers:

1. Store session ticket after successful handshake
2. On reconnect, use QUIC 0-RTT with stored ticket
3. Server accepts or rejects based on ticket validity

**Post-Urbit Data in 0-RTT (Normative):**

In Post-Urbit v1, implementations MUST NOT send application-layer data (control stream messages, identity handshake, or any stream data) in QUIC 0-RTT early data. The `tls_binding` required for identity handshake is only available after TLS 1.3 handshake completion, making 0-RTT impossible for authenticated streams.

QUIC 0-RTT MAY be used by the underlying transport for connection resumption (TLS session tickets), but all Post-Urbit protocol bytes MUST be sent in 1-RTT or later.

**0-RTT Security Rationale (Informative):**

The restriction on 0-RTT application data exists because:
- 0-RTT data can be replayed by network attackers
- The `tls_binding` required for Post-Urbit identity authentication is not available until after TLS handshake completion
- Even theoretically "safe" operations (identity_request, ping) cannot be sent because they would lack authentication context

Future protocol versions MAY define specific 0-RTT-safe message types, but v1 prohibits all Post-Urbit protocol bytes in 0-RTT early data.

### 8.4 Abbreviated Handshake (Resumption) [OUT OF SCOPE FOR V1]

**Status:** This section describes a future optimization. For v1, implementations MUST NOT use abbreviated handshake resumption. All connections MUST use the full handshake (§5.3-§5.11).

**Reserved Fields:** The `resume` field in `client_hello` and the `resume_accepted` message type are reserved for future use. v1 implementations:
- MUST NOT include `resume` in `client_hello`
- MUST ignore `resume` if received (proceed with full handshake)
- MUST NOT send `resume_accepted`
- MUST treat `resume_accepted` as an unknown message type (error)

**Future Direction:** A future protocol version will specify the complete abbreviated handshake, including:
- Full `resume_accepted` message schema
- Cryptographic binding requirements
- Fallback behavior when resumption is rejected

### 8.5 Glare Resolution (Duplicate Connection Handling)

When two peers attempt to connect to each other simultaneously, both connections may complete successfully. This is called "glare" and must be resolved deterministically to ensure exactly one connection remains.

**Connection Tuple:**

Connections are identified by the tuple: `(local_iid, local_did, peer_iid, peer_did)`

Where `did` is the Device Identifier (may be `null` if device authentication is not used).

**Glare Detection:**

After the identity handshake completes on both ends:
1. Each peer checks if they already have a CONNECTED or HANDSHAKING connection to the same peer tuple
2. If a duplicate exists (regardless of which peer initiated), glare has occurred

**Resolution Algorithm (Normative):**

When glare is detected, peers MUST resolve as follows:

1. Compare the initiator's `(iid, did)` tuple of each connection lexicographically
2. The connection initiated by the **smaller** `(iid, did)` tuple survives
3. Close the other connection with error code `0x105` (DUPLICATE_CONNECTION)

**Tuple Comparison:**

```python
def tuple_less_than(a: tuple[str, str|None], b: tuple[str, str|None]) -> bool:
    """
    Compare (iid, did) tuples lexicographically.
    - iid: 32-char Crockford Base32 string
    - did: 32-char Crockford Base32 string or None
    - None sorts before any defined value
    """
    if a[0] != b[0]:
        return a[0] < b[0]  # Compare IIDs lexicographically

    # IIDs equal, compare DIDs
    if a[1] is None and b[1] is None:
        return False  # Equal
    if a[1] is None:
        return True   # None < any defined value
    if b[1] is None:
        return False  # Any defined value > None
    return a[1] < b[1]  # Compare DIDs lexicographically
```

**Ordering Domain (Normative):** Glare resolution uses ASCII lexicographic comparison of the 32-character lowercase Crockford Base32 string representations. Implementations MUST NOT decode IIDs/DIDs to raw bytes for glare ordering. This differs from cryptographic salt ordering (e.g., RFC-0003 `kdf_initial` which uses raw-byte comparison for determining initiator/responder roles).

**Example:**

```
Alice (IID: a1b2c3..., DID: d1e2f3...)
Bob   (IID: b4c5d6..., DID: e7f8g9...)

1. Alice connects to Bob (Alice is initiator)
2. Bob connects to Alice (Bob is initiator)
3. Both handshakes complete
4. Alice's tuple: (a1b2c3..., d1e2f3...)
   Bob's tuple:   (b4c5d6..., e7f8g9...)
5. Alice's tuple < Bob's tuple (lexicographically)
6. Result: Keep Alice's connection (Alice initiated)
           Close Bob's connection with DUPLICATE_CONNECTION
```

**Timing:**

- Glare resolution MUST occur immediately after handshake completion
- Implementations SHOULD NOT delay resolution or race to close first
- The deterministic algorithm ensures both peers close the same connection

## 9. Error Handling

### 9.1 QUIC Error Codes

Standard QUIC error codes (0x00-0x10) per RFC 9000.

### 9.2 Application Error Codes (Normative Registry)

| Code | Name | Meaning |
|------|------|---------|
| 0x100 | IDENTITY_MISMATCH | Peer IID doesn't match expected |
| 0x101 | HANDSHAKE_FAILED | Identity handshake failed |
| 0x102 | STREAM_TYPE_UNKNOWN | Unknown stream type byte |
| 0x103 | MESSAGE_TOO_LARGE | Message exceeds stream limit |
| 0x104 | RATE_LIMITED | Too many requests |
| 0x105 | DUPLICATE_CONNECTION | Connection already exists (glare resolution) |
| 0x106 | REVOKED_IDENTITY | Peer's identity is revoked |
| 0x107 | REVOKED_KEY | Peer's signing key is revoked |
| 0x108 | DUPLICATE_STREAM_TYPE | Second stream of single-instance type opened |
| 0x109-0x1FF | Reserved | Transport layer |

**Note:** This is the authoritative registry. Code 0x105 is DUPLICATE_CONNECTION for glare resolution (when both peers simultaneously initiate connections). Code 0x108 DUPLICATE_STREAM_TYPE is used when a peer opens a second stream of a type that allows only one per direction (Identity, Sync). Revocation codes are 0x106-0x107.

## 10. Security Considerations

### 10.1 TLS Certificate Policy

TLS certificates are self-signed and ephemeral. They provide:
- Transport encryption with forward secrecy
- Key exchange for session keys

They do NOT provide:
- Identity authentication (done via handshake)
- Long-term trust (identity documents handle this)

As specified in §4.3, implementations MUST accept ANY certificate and MUST NOT perform validation (expiration, chain, hostname, EKU). The identity handshake provides all authentication guarantees.

**ALPN Scope:** This policy applies ONLY to Post-Urbit protocol connections (ALPN `post-urbit/1`). DHT/libp2p connections (ALPN `libp2p`) use libp2p's standard TLS certificate requirements where the certificate cryptographically proves the PeerID. Implementations MUST NOT apply "accept any certificate" to libp2p connections. See `spec/00-shared/layer-integration.md` "TLS Certificate Policy by ALPN" for normative requirements.

### 10.2 Replay Protection

- **Nonces**: 32-byte random nonces in each handshake
- **TLS binding**: Handshake tied to specific TLS session
- **Timestamps**: ±5 minute validity window
- **Relay nonce cache**: 10-minute TTL for allocation nonces

### 10.3 Man-in-the-Middle

The identity handshake prevents MITM by:
1. TLS provides encrypted channel
2. Challenge signatures prove identity ownership
3. TLS binding prevents message transplanting
4. Identity documents provide key continuity

### 10.4 Relay Metadata Exposure

Relays see:
- Source IP addresses
- Destination IIDs
- Packet timing and sizes
- Traffic patterns

Mitigations:
- Use multiple relays from different operators
- Consider traffic padding (out of scope for v1)
- Self-host relay for maximum privacy

### 10.5 Denial of Service

Protections:
- Rate limiting at all layers
- TLS handshake required before allocation
- Resource limits per connection and per IID
- Quick timeout for incomplete handshakes (30s)

## 11. Test Vectors

All test vectors use deterministic inputs to enable independent verification.

### 11.1 Base32 Encoding (Crockford)

**Encode 20 bytes to IID:**
```
Input (hex):  55ff38e37cc2169c2e2412a7c6f2f8517f0f8c34
Output:       abzy73bycgb9ybrg12tynyxgkfzyh3bk
```

**Verification:** Each 5 bits maps to one character from alphabet `0123456789abcdefghjkmnpqrstvwxyz`.

### 11.2 Challenge Signature

**Input Values:**
```
client_iid = "abzy73bycgb9ybrg12tynyxgkfzyh3bk"
server_iid = "00000000000000000000000000000000"  // 20 zero bytes
client_nonce (32 bytes, hex) = 0000...0000 (32 zeros)
server_nonce (32 bytes, hex) = 0101...0101 (32 ones)
tls_binding (32 bytes, hex)  = 0202...0202 (32 twos)
server_signing_key (seed, hex) = 0303...0303 (32 threes)
```

**Challenge Data Construction (159 bytes):**
```
DOMAIN (23 bytes) = 706f73742d75726269742d68616e647368616b652d7631  // "post-urbit-handshake-v1"
client_nonce (32)  = 0000...0000
server_nonce (32)  = 0101...0101
tls_binding (32)   = 0202...0202
client_iid_raw (20) = 55ff38e37cc2169c2e2412a7c6f2f8517f0f8c34
server_iid_raw (20) = 0000...0000

challenge_data (hex) =
  706f73742d757262 69742d68616e6473 68616b652d7631
  0000000000000000 0000000000000000 0000000000000000 0000000000000000
  0101010101010101 0101010101010101 0101010101010101 0101010101010101
  0202020202020202 0202020202020202 0202020202020202 0202020202020202
  55ff38e37cc2169c 2e2412a7c6f2f851 7f0f8c34
  0000000000000000 0000000000000000 00000000
```

**Expected Results:** Compute `challenge_data_sha256` and `signature` using your cryptographic library. Verify your Ed25519 implementation against the signature test vectors in `spec/00-shared/test-vectors.md` (Test Vector 2).

### 11.3 PURL Packet

**DATA Packet Example:**
```
Input:
  Token (hex): 00112233445566778899aabbccddeeff
  Dest IID (Crockford Base32): abzy73bycgb9ybrg12tynyxgkfzyh3bk
  Payload (4 bytes): 01020304

Packet (hex):
  5055524c         // Magic "PURL" (P=0x50, U=0x55, R=0x52, L=0x4c)
  01               // Version
  01               // Type: DATA
  00112233445566778899aabbccddeeff  // Token (16 bytes)
  55ff38e37cc2169c2e2412a7c6f2f8517f0f8c34  // Dest IID raw (20 bytes)
  0004             // Length (big-endian)
  01020304         // Payload

Full packet (48 bytes hex):
  5055524c 01 01 00112233445566778899aabbccddeeff
  55ff38e37cc2169c2e2412a7c6f2f8517f0f8c34
  0004 01020304
```

**Magic Verification:** `0x50 0x55 0x52 0x4c` = ASCII "PURL"

### 11.4 Allocation Signature

**Input:**
```
iid = "abzy73bycgb9ybrg12tynyxgkfzyh3bk"
lifetime = 3600
timestamp = "2026-01-14T00:00:00Z"  (canonical)
nonce (16 bytes, hex) = 00000000000000000000000000000000
signing_key (seed, hex) = 0000...0000 (32 zeros)
```

**Signature Input Construction (97 bytes):**
```
DOMAIN (25 bytes): "post-urbit-relay-alloc-v1"
  = 706f73742d757262 69742d72656c6179 2d616c6c6f632d76 31
iid_utf8 (32 bytes): "abzy73bycgb9ybrg12tynyxgkfzyh3bk"
  = 61627a7937336279 6367623979627267 6932746e79786766 7a796833626b
lifetime_be (4 bytes): 0x00000E10
timestamp_utf8 (20 bytes): "2026-01-14T00:00:00Z"
  = 323032362d30312d 31345430303a3030 3a30305a
nonce (16 bytes): 0x00...00

Full input (hex):
  706f73742d757262 69742d72656c6179 2d616c6c6f632d76 31
  61627a7937336279 6367623979627267 6932746e79786766 7a796833626b
  00000e10
  323032362d30312d 31345430303a3030 3a30305a
  0000000000000000 0000000000000000
```

**Expected:** Compute the SHA-256 of signature input and Ed25519 signature using your cryptographic library. Verify your Ed25519 implementation against `spec/00-shared/test-vectors.md` (Test Vector 2).

### 11.5 Domain Separator Reference

| Context | Domain Separator | Length |
|---------|------------------|--------|
| Handshake | `post-urbit-handshake-v1` | 23 bytes |
| Device | `post-urbit-device-v1` | 20 bytes |
| Relay Allocate | `post-urbit-relay-alloc-v1` | 25 bytes |
| Relay Rebind | `post-urbit-rebind-v1` | 20 bytes |
| Hole Punch | `post-urbit-holepunch-v1` | 23 bytes |

## 12. Implementation Notes

### 12.1 Recommended QUIC Libraries

| Language | Library | Notes |
|----------|---------|-------|
| Rust | quinn, quiche | quinn has better ergonomics |
| Go | quic-go | Mature, well-maintained |
| C/C++ | quiche, msquic | quiche for portability |

### 12.2 Performance Targets

| Metric | Target |
|--------|--------|
| Connection establishment | < 1 RTT (0-RTT resumption) |
| First message | < 2 RTT (new connection) |
| Concurrent connections | 1000+ per node |
| Memory per connection | < 100 KB |

## 13. References

- [RFC 2119] Key words for use in RFCs
- [RFC 8446] TLS 1.3
- [RFC 9000] QUIC: A UDP-Based Multiplexed Transport
- [RFC-0001] Post-Urbit Identity Document

## 14. Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-14 | Initial draft |
| 1.1 | 2026-01-14 | Fixed BLOCKING issues from GPT-5.2 review: Base32 spec, stream identification, error code registry, PURL packet types, domain separators, encapsulation model, test vectors |
| 1.2 | 2026-01-15 | Added §6.6 Bulk Stream Wire Protocol: marked stream type 0x05 as reserved for v2 with normative requirement that v1 implementations MUST NOT use it; included non-normative future direction with anticipated message formats |

## 15. Appendix: Wire Format Summary

### 15.1 QUIC Stream Framing

```
┌──────────────────────────────────────────────────┐
│                  QUIC Stream                     │
├──────────────────────────────────────────────────┤
│ Stream Type                           │ 1 byte   │
├──────────────────────────────────────────────────┤
│ Frame 1: Length (BE u32)              │ 4 bytes  │
│          Payload (JSON/binary)        │ N bytes  │
├──────────────────────────────────────────────────┤
│ Frame 2: Length (BE u32)              │ 4 bytes  │
│          Payload (JSON/binary)        │ M bytes  │
├──────────────────────────────────────────────────┤
│              ...                                 │
└──────────────────────────────────────────────────┘
```

### 15.2 PURL Framing

```
┌──────────────────────────────────────────────────┐
│                  PURL Packet                     │
├──────────────────────────────────────────────────┤
│ Magic: "PURL"                         │ 4 bytes  │
│ Version: 0x01                         │ 1 byte   │
│ Packet Type                           │ 1 byte   │
│ Allocation Token                      │ 16 bytes │
│ Destination IID (raw)                 │ 20 bytes │
│ Payload Length (BE u16)               │ 2 bytes  │
│ Payload (QUIC packet)                 │ N bytes  │
└──────────────────────────────────────────────────┘
Header: 44 bytes
Max Payload: 65535 bytes
```

### 15.3 Identity Handshake Messages

| Message | Type Field | Direction |
|---------|------------|-----------|
| ClientHello | "client_hello" | Client → Server |
| ServerHello | "server_hello" | Server → Client |
| ClientAuth | "client_auth" | Client → Server |
| HandshakeComplete | "handshake_complete" | Server → Client |
| ResumeAccepted | "resume_accepted" | Server → Client (Reserved, v1 MUST NOT use) |

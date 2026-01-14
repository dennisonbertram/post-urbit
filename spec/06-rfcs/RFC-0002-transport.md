# RFC-0002: Post-Urbit Transport Protocol

**Status:** Draft
**Version:** 1.1
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
- Discovery services (DHT) - see `00-shared/layer-integration.md`
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
| Case | Lowercase only (MUST normalize) |
| Padding | None |

**Encoding:** Standard 5-bit grouping per Crockford Base32.

**Decoding:** Accept lowercase only. Reject any character not in alphabet.

**Example:**
```
20 bytes (hex): 55ff38e37cc2169c2e2412a7c6f2f8517f0f8c34
Base32 string:  abzy73bycgb9ybrgi2tynyxgkfzyh3bk
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
| Cipher suites | TLS_CHACHA20_POLY1305_SHA256, TLS_AES_256_GCM_SHA384 |
| Key exchange | X25519 (MUST support) |
| Certificate | Self-signed, ephemeral (identity proven via handshake) |
| SNI | SHOULD be empty or ignored |

**Certificate Policy**: TLS certificates are NOT used for identity verification. Implementations MUST accept any valid self-signed certificate and rely on the identity handshake (Section 5) for authentication.

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
| expected_server_iid | SHOULD | Expected server IID; MUST verify match if provided |
| client_nonce | MUST | 32 random bytes (Base64 standard, no padding) |
| timestamp | MUST | RFC3339 UTC, canonical form: `YYYY-MM-DDTHH:MM:SSZ` |
| tls_binding | MUST | TLS exporter value (Base64 standard, no padding) |

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

device_challenge_data = concat(
  DEVICE_DOMAIN,                   // 20 bytes
  decode_base64(their_nonce),      // 32 bytes
  decode_base64(my_nonce),         // 32 bytes
  decode_base64(tls_binding),      // 32 bytes
  decode_base32(my_iid),           // 20 bytes raw
  decode_base32(my_did)            // 20 bytes raw
)
// Total: 20 + 32 + 32 + 32 + 20 + 20 = 156 bytes

device_signature = Ed25519_Sign(
  device_signing_key,
  SHA256(device_challenge_data)
)
```

**Device Verification:**
1. Verify `device_document.signature_by_identity` using identity's current signing key
2. Verify `device_signature` using `device_document.device_signing_key`
3. Check `device_document.iid` matches claimed IID
4. Check `device_document.did` matches claimed DID
5. If `device_document.expires_at` exists, check not expired

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

**Server Verifies Client:**
1. Check `client_iid` is well-formed (32 chars, Base32 lowercase)
2. Check `timestamp` is within ±5 minutes of server time
3. Check `tls_binding` matches current TLS session's exporter value
4. If `expected_server_iid` provided, verify it matches server's IID
5. Receive ClientAuth and verify:
   a. Validate client's identity document per RFC-0001
   b. Verify `client_iid == derive_iid(identity_document.keys.signing.genesis)`
   c. Reconstruct challenge data
   d. Verify `challenge_signature` using `identity_document.keys.signing.current`
6. If `client_did` provided, verify device signature per §5.7

**Client Verifies Server:**
1. Check `server_iid` is well-formed
2. Check `timestamp` is within ±5 minutes
3. Check `tls_binding` matches current TLS session
4. Validate server's identity document per RFC-0001
5. Verify `server_iid == derive_iid(identity_document.keys.signing.genesis)`
6. If `expected_server_iid` provided, verify it matches
7. Reconstruct challenge data
8. Verify `challenge_signature` using `identity_document.keys.signing.current`
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

### 5.14 Anonymous Connections (Out of Scope)

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
| 0x03 | Message | Bidirectional | JSON | Application messages (PUSE envelopes) |
| 0x04 | Sync | Bidirectional | JSON | CRDT synchronization |
| 0x05 | Bulk | Unidirectional | Binary | Large data transfers |
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

| Stream | Payload Format | Message Type Field |
|--------|----------------|-------------------|
| Control (0x01) | UTF-8 JSON | `type` field in JSON |
| Identity (0x02) | UTF-8 JSON | `type` field in JSON |
| Message (0x03) | UTF-8 JSON | `type` field in JSON |
| Sync (0x04) | UTF-8 JSON | `type` field in JSON |
| Bulk (0x05) | Binary | First 2 bytes = opcode |

**JSON Streams (0x01-0x04):**
- Payload MUST be valid UTF-8 JSON
- JSON object MUST have a `type` field (string) identifying message kind
- Example: `{"type": "identity_update", ...}`

**Binary Streams (0x05):**
- First 2 bytes of payload are big-endian opcode
- Remaining bytes are opcode-specific (see bulk transfer spec)
- Opcodes: 0x0001=DATA_CHUNK, 0x0002=COMPLETE, 0x0003=ABORT

### 6.4 Stream-Specific Limits

| Stream Type | Max Message Size | Notes |
|-------------|------------------|-------|
| Control | 64 KB | Includes identity documents |
| Identity | 64 KB | Identity and device documents |
| Message | 256 KB | Encrypted PUSE envelopes |
| Sync | 1 MB | CRDT operations |
| Bulk | 16 MB | File transfer chunks |

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
│ Payload (QUIC UDP payload)             │ <length> bytes
└────────────────────────────────────────┘

Total header: 44 bytes
Max payload: 65535 bytes (limited by u16)
```

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
| 0x09-0xFF | Reserved | Future use |

**Note:** REBIND is 0x08, ERROR is 0x07. This is the authoritative registry.

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

**Control Packets (non-DATA):**
- PING/PONG/REFRESH/RELEASE/REBIND/ERROR are relay control plane
- These packets are processed by relay and NOT forwarded to peers
- Destination IID field is zero (20 null bytes) for control packets

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
  "nonce": "<16-bytes-base64url>",
  "identity_doc_sequence": "42",
  "signature": "<64-bytes-base64-standard>"
}
```

**Note:** `identity_doc_sequence` is a decimal string (not number) to avoid JSON uint64 precision issues.

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
3. Check `nonce` not seen before (10-minute replay cache)
4. Fetch/cache identity document for `iid` at `identity_doc_sequence` or higher
5. Reconstruct signature input
6. Verify signature against identity document's current signing key
7. If valid, create allocation bound to client's source IP:port

### 7.9 Allocation Response

```json
{
  "allocation_id": "<unique-id>",
  "relay_address": "relay.example.com",
  "relay_port": 4433,
  "expires_at": "<RFC3339-UTC>",
  "token": "<22-char-base64url-token>"
}
```

### 7.10 Allocation Binding

Allocations are bound to the source IP:port at creation time:

| Scenario | Behavior |
|----------|----------|
| Same IP:port | Token accepted, packet forwarded |
| Different IP, same token | Rejected (potential token theft) |
| NAT rebinding | Client sends REBIND message |

### 7.11 REBIND Message

When a client's IP changes (NAT rebinding, network handoff):

```json
{
  "type": "rebind",
  "allocation_id": "<id>",
  "token": "<22-char-base64url>",
  "timestamp": "<RFC3339-UTC>",
  "signature": "<64-bytes-base64-standard>"
}
```

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

**0-RTT Security:**
- 0-RTT data can be replayed by network attackers
- Safe operations: identity_request, ping, presence
- Unsafe operations: message_send, sync_write, key_rotation

Implementations MUST NOT send replay-sensitive operations in 0-RTT.

### 8.4 Abbreviated Handshake (Resumption)

```json
{
  "type": "client_hello",
  "version": 1,
  "client_iid": "<...>",
  "expected_server_iid": "<...>",
  "client_nonce": "<...>",
  "timestamp": "<...>",
  "tls_binding": "<...>",
  "resume": {
    "last_seen_sequence": "5",
    "session_id": "<previous-session-id>"
  }
}
```

Server responds:
- If sequence unchanged: `{"type": "resume_accepted"}` → proceed to connected
- If sequence changed: Full ServerHello with identity_document

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
| 0x108-0x1FF | Reserved | Transport layer |

**Note:** This is the authoritative registry. Code 0x105 is DUPLICATE_CONNECTION for glare resolution (when both peers simultaneously initiate connections). Revocation codes moved to 0x106-0x107.

## 10. Security Considerations

### 10.1 TLS Certificate Policy

TLS certificates are self-signed and ephemeral. They provide:
- Transport encryption with forward secrecy
- Key exchange for session keys

They do NOT provide:
- Identity authentication (done via handshake)
- Long-term trust (identity documents handle this)

Implementations MUST accept any valid self-signed certificate.

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
Output:       abzy73bycgb9ybrgi2tynyxgkfzyh3bk
```

**Verification:** Each 5 bits maps to one character from alphabet `0123456789abcdefghjkmnpqrstvwxyz`.

### 11.2 Challenge Signature

**Input Values:**
```
client_iid = "abzy73bycgb9ybrgi2tynyxgkfzyh3bk"
server_iid = "00000000000000000000000000000000"  // 20 zero bytes
client_nonce (32 bytes, hex) = 0000...0000 (32 zeros)
server_nonce (32 bytes, hex) = 0101...0101 (32 ones)
tls_binding (32 bytes, hex)  = 0202...0202 (32 twos)
server_signing_key (seed, hex) = 0303...0303 (32 threes)
```

**Challenge Data Construction (159 bytes):**
```
DOMAIN (23 bytes) = 706f73742d757262...763120  // "post-urbit-handshake-v1"
client_nonce (32)  = 0000...0000
server_nonce (32)  = 0101...0101
tls_binding (32)   = 0202...0202
client_iid_raw (20) = 55ff38e37cc2169c2e2412a7c6f2f8517f0f8c34
server_iid_raw (20) = 0000...0000

challenge_data (hex) =
  706f73742d757262 69742d68616e6473 68616b652d763100
  0000000000000000 0000000000000000 0000000000000000 0000000000000000
  0101010101010101 0101010101010101 0101010101010101 0101010101010101
  0202020202020202 0202020202020202 0202020202020202 0202020202020202
  55ff38e37cc2169c 2e2412a7c6f2f851 7f0f8c34
  0000000000000000 0000000000000000 00000000
```

**Expected Results:**
```
challenge_data_sha256 (hex) = [compute with reference implementation]
signature (base64, no pad) = [compute with Ed25519 using seed 0x03*32]
```

**Note:** Implementers should compute expected values using their cryptographic library and verify they match.

### 11.3 PURL Packet

**DATA Packet Example:**
```
Input:
  Token (hex): 00112233445566778899aabbccddeeff
  Dest IID (Crockford Base32): abzy73bycgb9ybrgi2tynyxgkfzyh3bk
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
iid = "abzy73bycgb9ybrgi2tynyxgkfzyh3bk"
lifetime = 3600
timestamp = "2026-01-14T00:00:00Z"  (canonical)
nonce (16 bytes, hex) = 00000000000000000000000000000000
signing_key (seed, hex) = 0000...0000 (32 zeros)
```

**Signature Input Construction (97 bytes):**
```
DOMAIN (25 bytes): "post-urbit-relay-alloc-v1"
  = 706f73742d757262 69742d72656c6179 2d616c6c6f632d76 31
iid_utf8 (32 bytes): "abzy73bycgb9ybrgi2tynyxgkfzyh3bk"
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

**Expected:**
```
signature_input_sha256 (hex) = [compute with reference implementation]
signature (base64, no pad) = [compute with Ed25519 using seed 0x00*32]
```

### 11.5 Domain Separator Reference

| Context | Domain Separator | Length |
|---------|------------------|--------|
| Handshake | `post-urbit-handshake-v1` | 23 bytes |
| Device | `post-urbit-device-v1` | 20 bytes |
| Relay Allocate | `post-urbit-relay-alloc-v1` | 25 bytes |
| Relay Rebind | `post-urbit-rebind-v1` | 20 bytes |

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
| ResumeAccepted | "resume_accepted" | Server → Client |

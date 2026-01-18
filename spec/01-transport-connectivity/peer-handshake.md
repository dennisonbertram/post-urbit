# Peer Handshake Protocol

**Note:** The authoritative specification is RFC-0002 (Transport Protocol). This document provides additional context and implementation guidance.

## Overview

The peer handshake establishes an **identity-authenticated connection** on top of QUIC TLS. After QUIC handshake completes, both peers prove they control their claimed identities and optionally their device identifiers (DIDs).

**Optional Field Handling (Normative):**

Per RFC-0002 §5.5, optional handshake fields MAY be omitted entirely or present with value `null`. Receivers MUST treat both forms equivalently: [REQ-TRANS-037]

```json
// These are semantically identical:
{"device": "abc123", "capabilities": null}
{"device": "abc123"}
```

Implementations:
- Senders MAY omit optional fields or include them as `null` [REQ-TRANS-038]
- Receivers MUST accept both forms without error [REQ-TRANS-039]
- Receivers MUST NOT require a specific representation [REQ-TRANS-040]

This ensures interoperability between implementations that serialize optionals differently.

## Goals

1. **Mutual authentication**: Both peers prove identity ownership
2. **Device binding**: Optionally bind session to specific device (DID)
3. **TLS binding**: Session is bound to specific IIDs (and DIDs if provided)
4. **Key freshness**: Prevent replay of old handshakes
5. **Document exchange**: Peers share current identity and device documents

## Handshake Flow

```
   Client                                          Server
     │                                               │
     │ ─────────── QUIC Handshake (TLS 1.3) ──────► │
     │                                               │
     │ ◄──────────────────────────────────────────── │
     │              (TLS session established)        │
     │                                               │
     │ ══════════ Identity Handshake Stream ══════► │
     │                                               │
     │  ┌─────────────────────────────────────────┐  │
     │  │ ClientHello                             │  │
     │  │ - client_iid                            │  │
     │  │ - client_did (optional)                 │  │
     │  │ - expected_server_iid (optional)        │  │
     │  │ - client_nonce                          │  │
     │  │ - timestamp                             │  │
     │  └─────────────────────────────────────────┘  │
     │ ─────────────────────────────────────────────►│
     │                                               │
     │  ┌─────────────────────────────────────────┐  │
     │  │ ServerHello                             │  │
     │  │ - server_iid                            │  │
     │  │ - server_did (optional)                 │  │
     │  │ - server_nonce                          │  │
     │  │ - timestamp                             │  │
     │  │ - identity_document                     │  │
     │  │ - device_document (if server_did)       │  │
     │  │ - challenge_signature                   │  │
     │  │ - device_signature (if server_did)      │  │
     │  └─────────────────────────────────────────┘  │
     │ ◄─────────────────────────────────────────────│
     │                                               │
     │  ┌─────────────────────────────────────────┐  │
     │  │ ClientAuth                              │  │
     │  │ - identity_document                     │  │
     │  │ - device_document (if client_did)       │  │
     │  │ - challenge_signature                   │  │
     │  │ - device_signature (if client_did)      │  │
     │  └─────────────────────────────────────────┘  │
     │ ─────────────────────────────────────────────►│
     │                                               │
     │  ┌─────────────────────────────────────────┐  │
     │  │ HandshakeComplete                       │  │
     │  │ - success: true                         │  │
     │  └─────────────────────────────────────────┘  │
     │ ◄─────────────────────────────────────────────│
     │                                               │
     │   (Connection authenticated: iid + optional did)
     │                                               │
```

## Message Formats

### ClientHello

```json
{
  "type": "client_hello",
  "version": 1,
  "client_iid": "<32-char-base32-iid>",
  "client_did": "<32-char-base32-did>|null",
  "expected_server_iid": "<32-char-base32-iid>|null",
  "client_nonce": "<32-bytes-base64>",
  "timestamp": "<RFC3339-UTC>",
  "tls_binding": "<TLS-exporter-derived-value-base64>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `type` | Yes | Message type identifier |
| `version` | Yes | Handshake protocol version |
| `client_iid` | Yes | Client's identity identifier (Crockford Base32, 32 chars) |
| `client_did` | No | Client's device identifier (Crockford Base32, 32 chars) |
| `expected_server_iid` | No | Expected server IID (Crockford Base32, 32 chars) |
| `client_nonce` | Yes | 32 random bytes, Base64 standard alphabet (RFC 4648 §4), **no padding** |
| `timestamp` | Yes | Canonical RFC3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`, no fractional seconds); must be within ±5 minutes. Implementations MUST reject non-canonical forms.  [REQ-TRANS-041]|
| `tls_binding` | Yes | 32 bytes from TLS exporter, Base64 standard alphabet, **no padding** |

### ServerHello

```json
{
  "type": "server_hello",
  "version": 1,
  "server_iid": "<32-char-base32-iid>",
  "server_did": "<32-char-base32-did>|null",
  "server_nonce": "<32-bytes-base64>",
  "timestamp": "<RFC3339-UTC>",
  "identity_document": { /* full identity document */ },
  "device_document": { /* device document, if server_did */ },
  "challenge_signature": "<Ed25519-signature-base64>",
  "device_signature": "<Ed25519-signature-base64>|null",
  "tls_binding": "<TLS-exporter-derived-value-base64>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `server_iid` | Yes | Server's identity identifier (Crockford Base32, 32 chars) |
| `server_did` | No | Server's device identifier (Crockford Base32, 32 chars) |
| `server_nonce` | Yes | 32 random bytes, Base64 standard alphabet, **no padding** |
| `timestamp` | Yes | Canonical RFC3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`, no fractional seconds); must be within ±5 minutes. Implementations MUST reject non-canonical forms.  [REQ-TRANS-042]|
| `tls_binding` | Yes | 32 bytes from TLS exporter, Base64 standard alphabet, **no padding** |
| `identity_document` | Yes | Full identity document (JSON object) |
| `device_document` | If `server_did` | Device document proving DID ownership |
| `challenge_signature` | Yes | Ed25519 signature, Base64 standard alphabet, **no padding** (64 bytes → 86 chars) |
| `device_signature` | If `server_did` | Ed25519 signature using device signing key, Base64 standard, **no padding** |

### Challenge Signature

Server signs to prove identity ownership:

```
challenge_data = concat(
  "post-urbit-handshake-v1",         // domain separator (23 bytes)
  decode_base64(client_nonce),       // 32 bytes from ClientHello
  decode_base64(server_nonce),       // 32 bytes from ServerHello
  decode_base64(tls_binding),        // 32 bytes, binds to TLS session
  decode_base32(client_iid),         // 20 bytes raw, who we're authenticating to
  decode_base32(server_iid)          // 20 bytes raw, who we are
)
// Total: 23 + 32 + 32 + 32 + 20 + 20 = 159 bytes

challenge_signature = Ed25519_Sign(server_signing_key, SHA256(challenge_data))
```

**Note:** IIDs MUST be decoded from Base32 to 20 raw bytes. Nonces and tls_binding are decoded from Base64 standard (no padding). [REQ-TRANS-043]

### Device Signature (if DID provided)

If the server provides a `server_did`, it must also prove device ownership:

```
device_challenge_data = concat(
  "post-urbit-device-v1",        // domain separator (20 bytes)
  decode_base64(client_nonce),   // 32 bytes
  decode_base64(server_nonce),   // 32 bytes
  decode_base64(tls_binding),    // 32 bytes
  decode_base32(server_iid),     // 20 bytes raw
  decode_base32(server_did)      // 20 bytes raw
)
// Total: 20 + 32 + 32 + 32 + 20 + 20 = 156 bytes

device_signature = Ed25519_Sign(device_signing_key, SHA256(device_challenge_data))
```

**Device verification:**
1. Verify `device_document.signature_by_identity` using identity signing key (current, previous, or history per RFC-0001 §7 key lookup)
2. Verify `device_signature` using `device_document.device_signing_key`
3. Check `device_document.iid` matches `server_iid`
4. Check `device_document.did` matches `server_did`
5. Check device document is not expired (`expires_at` if present)

**Note:** Device documents may be signed with a key that was rotated since. Accept any signing key valid at signature time.

### ClientAuth

```json
{
  "type": "client_auth",
  "identity_document": { /* full identity document */ },
  "device_document": { /* device document, if client_did */ },
  "challenge_signature": "<Ed25519-signature-base64>",
  "device_signature": "<Ed25519-signature-base64>|null"
}
```

Client signs the same challenge data with swapped roles:

```
challenge_data = concat(
  "post-urbit-handshake-v1",         // domain separator (23 bytes)
  decode_base64(server_nonce),       // 32 bytes (swapped)
  decode_base64(client_nonce),       // 32 bytes (swapped)
  decode_base64(tls_binding),        // 32 bytes
  decode_base32(server_iid),         // 20 bytes raw (swapped)
  decode_base32(client_iid)          // 20 bytes raw (swapped)
)
// Total: 159 bytes

challenge_signature = Ed25519_Sign(client_signing_key, SHA256(challenge_data))
```

If `client_did` was provided in ClientHello, client also includes device proof:

```
device_challenge_data = concat(
  "post-urbit-device-v1",        // domain separator (20 bytes)
  decode_base64(server_nonce),   // 32 bytes
  decode_base64(client_nonce),   // 32 bytes
  decode_base64(tls_binding),    // 32 bytes
  decode_base32(client_iid),     // 20 bytes raw
  decode_base32(client_did)      // 20 bytes raw
)
// Total: 156 bytes

device_signature = Ed25519_Sign(device_signing_key, SHA256(device_challenge_data))
```

### HandshakeComplete

```json
{
  "type": "handshake_complete",
  "success": true,
  "error": null
}
```

Or on failure:

```json
{
  "type": "handshake_complete",
  "success": false,
  "error": {
    "code": "IDENTITY_MISMATCH|SIGNATURE_INVALID|TIMESTAMP_EXPIRED|...",
    "message": "Human-readable error"
  }
}
```

## Wire Format

Handshake occurs on the **first bidirectional stream opened by the client** (NOT a specific QUIC stream ID - QUIC assigns stream IDs automatically).

### Stream Initialization

1. Client opens first bidirectional stream
2. Client writes stream type header (1 byte): `0x01` (Control)
3. All subsequent messages on this stream are length-prefixed JSON

### Message Framing

```
Stream Header (first byte only):
┌────────────────────────────────────────┐
│ Stream Type: 0x01 (Control)            │ 1 byte
└────────────────────────────────────────┘

Each Message:
┌────────────────────────────────────────┐
│ Message Length (big-endian)            │ 4 bytes
├────────────────────────────────────────┤
│ JSON Message (UTF-8)                   │ <length> bytes
└────────────────────────────────────────┘
```

### Framing Rules

- **Stream type** is written ONCE at stream start (1 byte)
- **Each message** is prefixed with 4-byte big-endian length
- Implementations MUST buffer partial reads to reassemble complete frames [REQ-TRANS-044]
- Maximum message size: 64 KB (65536 bytes, includes identity document)

## Verification Steps

### Server Verifies Client

1. Check `client_iid` is well-formed (32 chars, Base32)
2. Check `timestamp` is within ±5 minutes of server time
3. Check `tls_binding` matches current TLS session
4. Wait for `ClientAuth`:
   a. Verify client's identity document (see 02-identity-trust)
   b. Verify `client_iid` matches document's IID
   c. Reconstruct challenge data
   d. Verify `challenge_signature` with document's current signing key
5. If `expected_server_iid` was provided, verify it matches

### Client Verifies Server

1. Check `server_iid` is well-formed
2. Check `timestamp` is within ±5 minutes
3. Check `tls_binding` matches current TLS session
4. Verify server's identity document
5. Verify `server_iid` matches document's IID
6. If client had `expected_server_iid`, verify it matches `server_iid`; MUST abort if mismatch [REQ-TRANS-045]
7. Reconstruct challenge data
8. Verify `challenge_signature` with document's current signing key

**IID Binding Requirement (Normative):**

When the connection is initiated for a **specific known IID** (e.g., via `TransportService.connect(peerIID)` or equivalent API where the caller knows the intended peer):

1. The client MUST include `expected_server_iid` in the ClientHello [REQ-TRANS-046]
2. The client MUST abort the handshake with error `IDENTITY_MISMATCH` if `server_iid` differs from `expected_server_iid` [REQ-TRANS-047]
3. The connection MUST NOT be established if the expected IID check fails [REQ-TRANS-048]

The `expected_server_iid` field MAY be omitted only in discovery scenarios where the client genuinely does not know who it is connecting to (e.g., connecting to a relay endpoint where the relay's IID was not pre-known). Such scenarios require explicit threat modeling and are not typical for peer-to-peer messaging. [REQ-TRANS-049]

**Rationale:** This prevents wrong-peer acceptance attacks where a user could be induced to complete a valid handshake with an unintended identity (e.g., via DNS poisoning, misdirected endpoint hints, or UI confusion).

## TLS Binding

The `tls_binding` field prevents handshake messages from being replayed on different connections.

**Using TLS Exporter (RFC 8446 Section 7.5)**:

```
tls_binding = Base64(TLS-Exporter(
  label: "post-urbit handshake binding",
  context: "",
  length: 32
))
```

This uses QUIC's TLS 1.3 exporter interface to derive a connection-specific value.

### Implementation Notes

Most QUIC libraries expose the TLS exporter:
- **quinn (Rust)**: `connection.export_keying_material()`
- **quic-go**: `connection.ExportKeyingMaterial()`
- **quiche**: `quiche_conn_export_keying_material()`

This ensures:
- Handshake is tied to specific TLS session
- Man-in-the-middle cannot transplant handshake to different connection
- Replay of handshake messages on new connection will fail

## Error Handling

| Error Code | Meaning | Recovery |
|------------|---------|----------|
| `IDENTITY_MISMATCH` | IID doesn't match expected | Connect to correct peer |
| `SIGNATURE_INVALID` | Challenge signature verification failed | May indicate attack |
| `TIMESTAMP_EXPIRED` | Timestamp too old or in future | Sync clocks, retry |
| `DOCUMENT_INVALID` | Identity document verification failed | Refresh peer's document |
| `TLS_BINDING_MISMATCH` | TLS session doesn't match | Possible MITM, abort |
| `VERSION_UNSUPPORTED` | Protocol version not supported | Upgrade client/server |
| `NONCE_REUSE` | Nonce was seen before | Possible replay, abort |

## State Machine

```
┌─────────────┐
│    INIT     │
└──────┬──────┘
       │ QUIC connected
       ▼
┌─────────────┐
│  AWAITING   │ ← Waiting for ClientHello (server) or ServerHello (client)
│   HELLO     │
└──────┬──────┘
       │ Hello received
       ▼
┌─────────────┐
│  AWAITING   │ ← Waiting for ClientAuth (server) or HandshakeComplete (client)
│    AUTH     │
└──────┬──────┘
       │ Auth verified
       ▼
┌─────────────┐
│ AUTHENTICATED│ ← Ready for application streams
└──────┬──────┘
       │ error / timeout / close
       ▼
┌─────────────┐
│   CLOSED    │
└─────────────┘
```

### Timeouts

| Phase | Timeout | Action |
|-------|---------|--------|
| AWAITING_HELLO | 10 seconds | Close connection |
| AWAITING_AUTH | 10 seconds | Close connection |
| Total handshake | 30 seconds | Close connection |

## Anonymous Connections

**Note:** Anonymous connections are **OUT OF SCOPE for v1**. All peer-to-peer connections MUST be mutually authenticated. See RFC-0002 §5.14 for the authoritative specification. [REQ-TRANS-050]

Relay and DHT services use separate protocols (HTTPS for relay allocation, DHT-native authentication) that don't require the identity handshake.

Future versions may define anonymous connection modes for specific use cases.

## Abbreviated Handshake Resumption [OUT OF SCOPE FOR V1]

**Status:** This section describes a future optimization. For v1, implementations MUST NOT use application-level abbreviated handshake resumption. The `resume` field and `resume_accepted` message are reserved for future use. [REQ-TRANS-051]

This refers to Post-Urbit application-level handshake abbreviation, NOT QUIC/TLS session resumption. QUIC TLS resumption MAY be used for transport efficiency, but Post-Urbit protocol bytes MUST NOT be sent in 0-RTT early data. [REQ-TRANS-052]

**V1 Requirements (Normative):**
- Implementations MUST NOT include `resume` in `client_hello` [REQ-TRANS-053]
- Implementations MUST ignore `resume` if received (proceed with full handshake) [REQ-TRANS-054]
- Implementations MUST NOT send `resume_accepted` [REQ-TRANS-055]
- Implementations MUST treat `resume_accepted` as an unknown message type (error) [REQ-TRANS-056]

All v1 connections MUST use the full handshake flow (§Handshake Flow above). [REQ-TRANS-057]

---

**Future Direction (Non-Normative):**

The following describes the anticipated design for a future protocol version:

For performance, previously authenticated connections could resume:

1. **Session ticket**: Store `(peer_iid, session_ticket, timestamp)` after successful handshake
2. **On reconnect**: Use QUIC 0-RTT with stored session ticket
3. **Abbreviated handshake**: Skip identity document exchange if sequence unchanged

### Resumption Check (Reserved)

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
    "last_seen_sequence": "5",     // String (uint64 safe) - last known sequence number
    "session_id": "<...>"          // From previous session
  }
}
```

Server could respond:
- If sequence unchanged: `{"type": "resume_accepted"}` → proceed to authenticated
- If sequence changed: Include full `identity_document` in ServerHello

## Security Considerations

1. **Replay attacks**: Nonces and TLS binding prevent replay
2. **Man-in-the-middle**: TLS provides channel security; identity handshake binds to IID
3. **Clock skew**: ±5 minute window balances security vs. usability
4. **Downgrade attacks**: Version negotiation in clear; consider pinning
5. **Identity document freshness**: May use stale document; refresh if signature fails

## Test Scenarios

1. **Happy path**: Both peers authenticate successfully
2. **Wrong server**: Client expects different IID, rejects
3. **Expired timestamp**: Server rejects stale ClientHello
4. **Invalid signature**: Peer rejects forged challenge response
5. **Missing client identity**: Server rejects unauthenticated client (anonymous rejected)
6. **Session resumption**: Skip document exchange on reconnect
7. **Key rotation during session**: Detect sequence change, re-exchange documents

# Peer Handshake Protocol

## Overview

The peer handshake establishes an **identity-authenticated connection** on top of QUIC TLS. After QUIC handshake completes, both peers prove they control their claimed identities and optionally their device identifiers (DIDs).

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
| `client_iid` | Yes | Client's identity identifier |
| `client_did` | No | Client's device identifier (for per-device sessions) |
| `expected_server_iid` | No | Expected server IID (if connecting to specific peer) |
| `client_nonce` | Yes | 32 random bytes for challenge |
| `timestamp` | Yes | Current time, must be within ±5 minutes |
| `tls_binding` | Yes | Binds handshake to TLS session |

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
| `server_did` | No | Server's device identifier |
| `device_document` | If `server_did` | Device document proving DID ownership |
| `device_signature` | If `server_did` | Signature using device signing key |

### Challenge Signature

Server signs to prove identity ownership:

```
challenge_data = concat(
  "post-urbit-handshake-v1",    // domain separator
  client_nonce,                  // from ClientHello
  server_nonce,                  // from ServerHello
  tls_binding,                   // binds to TLS session
  client_iid,                    // who we're authenticating to
  server_iid                     // who we are
)

challenge_signature = Ed25519_Sign(server_signing_key, SHA256(challenge_data))
```

### Device Signature (if DID provided)

If the server provides a `server_did`, it must also prove device ownership:

```
device_challenge_data = concat(
  "post-urbit-device-handshake-v1",  // domain separator
  client_nonce,
  server_nonce,
  tls_binding,
  server_iid,
  server_did
)

device_signature = Ed25519_Sign(device_signing_key, SHA256(device_challenge_data))
```

**Device verification:**
1. Verify `device_document.signature_by_identity` using identity's current signing key
2. Verify `device_signature` using `device_document.device_signing_key`
3. Check `device_document.iid` matches `server_iid`
4. Check `device_document.did` matches `server_did`
5. Check device document is not expired (`expires_at` if present)

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
  "post-urbit-handshake-v1",
  server_nonce,                  // from ServerHello
  client_nonce,                  // from ClientHello
  tls_binding,
  server_iid,
  client_iid
)

challenge_signature = Ed25519_Sign(client_signing_key, SHA256(challenge_data))
```

If `client_did` was provided in ClientHello, client also includes device proof:

```
device_challenge_data = concat(
  "post-urbit-device-handshake-v1",
  server_nonce,
  client_nonce,
  tls_binding,
  client_iid,
  client_did
)

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
- Implementations MUST buffer partial reads to reassemble complete frames
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
6. If client had `expected_server_iid`, verify it matches
7. Reconstruct challenge data
8. Verify `challenge_signature` with document's current signing key

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

Some scenarios allow anonymous connections (no client authentication):

1. **Public relays**: Client authenticates to relay, relay doesn't authenticate to client beyond TLS
2. **Discovery servers**: Simple request/response, no identity needed
3. **DHT queries**: May not require authentication

For anonymous connections, skip `ClientAuth` and proceed with `HandshakeComplete` after `ServerHello`.

### Anonymous Handshake

```json
// ClientHello with no identity claim
{
  "type": "client_hello",
  "version": 1,
  "client_iid": null,              // Anonymous
  "expected_server_iid": "<...>",
  "client_nonce": "<...>",
  "timestamp": "<...>",
  "tls_binding": "<...>"
}

// ServerHello as normal

// No ClientAuth

// HandshakeComplete
```

## Connection Resumption

For performance, previously authenticated connections can resume:

1. **Session ticket**: Store `(peer_iid, session_ticket, timestamp)` after successful handshake
2. **On reconnect**: Use QUIC 0-RTT with stored session ticket
3. **Abbreviated handshake**: Skip identity document exchange if sequence unchanged

### Resumption Check

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

Server can respond:
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
5. **Anonymous client**: Server accepts anonymous connection
6. **Session resumption**: Skip document exchange on reconnect
7. **Key rotation during session**: Detect sequence change, re-exchange documents

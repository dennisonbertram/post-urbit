# Layer Integration (Glue Specification)

## Overview

This document specifies how the Identity and Transport layers integrate. It resolves ambiguities identified during holistic review and provides normative definitions for cross-layer contracts.

## Identity↔Transport Integration

### Discovery Contract

The Transport layer provides peer discovery. The Identity layer uses it for identity document propagation.

```typescript
// What Identity layer expects (from caching-policy.md)
interface IdentityTransport {
  // Publish identity document to DHT
  publishIdentity(document: IdentityDocument): Promise<void>;

  // Fetch identity document by IID
  fetchIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null>;
}

// What Transport layer provides (from interfaces.md)
interface DiscoveryService {
  // Register identity with DHT
  registerIdentity(document: IdentityDocument): Promise<void>;

  // Look up peer endpoints from DHT
  lookupPeer(iid: IdentityIdentifier): Promise<PeerEndpoints | null>;
}
```

**Resolution**: The Transport layer stores the **full identity document** in DHT, not just endpoints. `lookupPeer` returns endpoints, but internally the DHT stores the complete IDOC.

### DHT Record Format

What gets stored in the DHT:

```
DHT Key:   SHA256("post-urbit:identity:" || iid)
DHT Value: IDOC binary envelope (see identity-document-schema.md)
```

| Field | Type | Description |
|-------|------|-------------|
| Key | 32 bytes | SHA256 of prefixed IID |
| Value | bytes | IDOC envelope (magic + version + length + JSON) |
| TTL | uint32 | Time-to-live in seconds (default: 86400 = 24 hours) |
| Signature | 64 bytes | Ed25519 signature by document's signing key |

**Verification**: DHT nodes MUST verify the identity document signature before storing. This prevents arbitrary data storage and ensures only identity owners can update their records.

### Transport API Bridge

Concrete mapping between layers:

```typescript
// Identity calls this
async function publishIdentity(document: IdentityDocument): Promise<void> {
  // Serialize to IDOC envelope
  const idocBytes = encodeIdoc(document);

  // Compute DHT key
  const key = sha256(concat("post-urbit:identity:", document.iid));

  // Sign the DHT record
  const recordSig = await keyStorage.signWith("signing:current", idocBytes);

  // Use Transport's underlying DHT
  await dht.put(key, idocBytes, { ttl: 86400, signature: recordSig });
}

// Identity calls this
async function fetchIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null> {
  const key = sha256(concat("post-urbit:identity:", iid));

  // DHT may return multiple records from different nodes
  const results = await dht.getAll(key);

  if (results.length === 0) return null;

  // Decode and verify all candidates
  const candidates: IdentityDocument[] = [];
  for (const result of results) {
    const document = decodeIdoc(result.value);
    if (verifyDocument(document)) {
      candidates.push(document);
    }
  }

  if (candidates.length === 0) return null;

  // Select highest valid sequence number
  // (see caching-policy.md for TOFU and genesis key constraints)
  candidates.sort((a, b) => {
    const seqA = BigInt(a.sequence);
    const seqB = BigInt(b.sequence);
    return seqB > seqA ? 1 : seqB < seqA ? -1 : 0;
  });

  return candidates[0];
}
```

## Device DHT Records

Multi-device support requires discovering devices associated with an identity.

### Device Document DHT Format

```
DHT Key:   SHA256("post-urbit:device:" || did)
DHT Value: Device document (JSON, signed by identity's signing key)
```

| Field | Type | Description |
|-------|------|-------------|
| Key | 32 bytes | SHA256 of prefixed DID |
| Value | bytes | Device document (JSON, structure below) |
| TTL | uint32 | Time-to-live (default: 86400 = 24 hours) |

**Signature authority:** The device document is signed by the **identity's signing key** (NOT the device key). This proves the identity owner authorized this device.

**Device Document Structure:**
```json
{
  "did": "<device-identifier>",
  "iid": "<owner-identity-identifier>",
  "name": "My Phone",
  "device_signing_key": "<base64-ed25519-public>",
  "device_transport_key": "<base64-x25519-public>",
  "endpoints": [
    { "type": "direct", "host": "...", "port": 4433, "transport": "quic" }
  ],
  "created_at": "<RFC3339>",
  "last_seen": "<RFC3339>",
  "signature": "<base64-signature-by-identity-signing-key>"
}
```

**Why identity signature (not device signature)?**
- Device keys are subordinate to identity keys
- Identity owner must authorize devices
- DHT nodes can verify authorization without knowing device private key
- Device keys prove possession during transport handshake (see peer-handshake.md)

**Verification:**
1. Fetch identity document for `iid`
2. Verify `signature` field using identity's current (or historical) signing key
3. Verify `did == Base32Lower(SHA256(device_signing_key)[0:20])`

### Device Index DHT Record

To discover all devices for an identity:

```
DHT Key:   SHA256("post-urbit:devices-for:" || iid)
DHT Value: Device index (list of DIDs, signed by identity)
```

**Device Index Structure:**
```json
{
  "iid": "<identity-identifier>",
  "devices": [
    { "did": "<did-1>", "name": "Phone", "last_seen": "<RFC3339>" },
    { "did": "<did-2>", "name": "Laptop", "last_seen": "<RFC3339>" }
  ],
  "updated_at": "<RFC3339>",
  "signature": "<base64-signature-by-identity-signing-key>"
}
```

**Note:** The DHT does NOT support prefix queries. The device index record provides an explicit list that clients can fetch with a single lookup, then fetch individual device documents as needed.

### Device Discovery Flow

```
1. Peer wants to connect to identity "k5xq7z4m..."
2. Fetch device index: DHT.get(SHA256("post-urbit:devices-for:k5xq7z4m..."))
3. Parse device list, verify signature
4. For each device with recent last_seen:
   a. Fetch device doc: DHT.get(SHA256("post-urbit:device:<did>"))
   b. Connect to device endpoints
   c. Perform identity handshake (peer-handshake.md)
5. First successful connection wins
```

### Device Record TTL and Refresh

| Record Type | TTL | Refresh Interval |
|-------------|-----|------------------|
| Device document | 24 hours | Every 12 hours |
| Device index | 24 hours | On device add/remove, or every 24h |

Devices should refresh their DHT records before TTL expiry to maintain discoverability.

## Identity Updates Over Authenticated Connections

When peers are connected, identity updates are pushed directly rather than through DHT.

### Stream Type

Identity updates use the `identity` stream type (0x02) on authenticated QUIC connections.

### Message Format

**Stream type is written ONCE at stream start**, then message frames follow:

```
Identity Update Stream:

Stream Header (first byte of stream, written once):
┌────────────────────────────────────────┐
│ Stream Type: 0x02 (identity)           │ 1 byte
└────────────────────────────────────────┘

Each Message Frame (repeated):
┌────────────────────────────────────────┐
│ Message Type                           │ 1 byte
├────────────────────────────────────────┤
│ Length (big-endian)                    │ 4 bytes
├────────────────────────────────────────┤
│ Payload                                │ <length> bytes
└────────────────────────────────────────┘
```

**Message Types:**
| Code | Name | Description |
|------|------|-------------|
| 0x01 | IDENTITY_UPDATE | Push new identity document |
| 0x02 | IDENTITY_REQUEST | Request peer's current identity |
| 0x03 | IDENTITY_RESPONSE | Response with identity document |
| 0x04 | IDENTITY_ACK | Acknowledge receipt of update |

**Note:** This pattern (stream type once, then length-prefixed frames) is consistent across all QUIC stream types. See `01-transport-connectivity/quic-integration.md` and `01-transport-connectivity/peer-handshake.md` for the normative framing specification.

### Update Push Flow

```
Alice                                    Bob
  │                                       │
  │ ─────── IDENTITY_UPDATE ────────────► │
  │         (new IdentityDocument)        │
  │                                       │
  │ ◄────── IDENTITY_ACK ──────────────── │
  │         (accepted: true, sequence: N) │
  │                                       │
```

### Request Flow

```
Alice                                    Bob
  │                                       │
  │ ─────── IDENTITY_REQUEST ───────────► │
  │         (known_sequence: N-1)         │
  │                                       │
  │ ◄────── IDENTITY_RESPONSE ─────────── │
  │         (document or "no update")     │
  │                                       │
```

## QUIC TLS Certificate Policy

### Certificate Requirements

| Requirement | Value |
|-------------|-------|
| Certificate type | Self-signed, ephemeral |
| Signature algorithm | ECDSA P-256 or Ed25519 |
| Validity period | 1 hour to 30 days |
| Subject/Issuer | Not verified (any value) |

### Verification Strategy

TLS certificates are NOT used for identity verification. Instead:

1. QUIC TLS provides transport encryption with forward secrecy
2. **Accept any valid TLS certificate** (self-signed OK)
3. Perform identity handshake (see peer-handshake.md) after TLS
4. Identity handshake binds the TLS session to specific IIDs

**Rationale**: This avoids dependency on PKI/CA infrastructure while still getting TLS 1.3 security. Identity is verified cryptographically through the post-TLS handshake.

### DoS Considerations

- Rate limit TLS handshakes per source IP
- Require valid TLS before allocating connection resources
- Drop connections that don't complete identity handshake within 30s

## Mailbox Protocol (Store-and-Forward)

### Overview

Mailbox servers store messages for offline recipients. This is a minimal specification to enable offline messaging.

### Mailbox Endpoint

In identity document:
```json
{
  "type": "mailbox",
  "host": "mailbox.example.com",
  "port": 443,
  "transport": "https",
  "priority": 100
}
```

### API

```
POST /store/{recipient_iid}
Content-Type: application/octet-stream
Authorization: Bearer <sender-signed-token>

Body: Encrypted message envelope (opaque to mailbox)

Response: 201 Created, {"id": "<message-id>", "expires_at": "<timestamp>"}
```

```
GET /retrieve/{my_iid}
Authorization: Bearer <recipient-signed-token>

Response: 200 OK, [{"id": "...", "envelope": "...", "stored_at": "..."}]
```

```
DELETE /retrieve/{my_iid}/{message_id}
Authorization: Bearer <recipient-signed-token>

Response: 204 No Content
```

### Auth Token Format

The `Authorization: Bearer` token is a signed request object (Base64-encoded JSON):

```json
{
  "v": 1,
  "action": "store|retrieve|delete",
  "sender_iid": "<sender-identity-id>",
  "recipient_iid": "<recipient-identity-id>",
  "issued_at": "<RFC3339-timestamp>",
  "expires_at": "<RFC3339-timestamp>",
  "nonce": "<16-byte-random-base64>",
  "signature": "<ed25519-signature-base64>"
}
```

**Token fields**:
| Field | Description |
|-------|-------------|
| `v` | Version (must be 1) |
| `action` | One of: `store`, `retrieve`, `delete` |
| `sender_iid` | For `store`: sender's IID. For `retrieve`/`delete`: same as recipient_iid |
| `recipient_iid` | Whose mailbox is being accessed |
| `issued_at` | Token creation timestamp |
| `expires_at` | Token expiration (max 5 minutes from issued_at) |
| `nonce` | Random bytes for replay prevention |
| `signature` | Ed25519 signature over canonical JSON (without signature field) |

**Signature verification**: Sign over `{"v":1,"action":"...","sender_iid":"...","recipient_iid":"...","issued_at":"...","expires_at":"...","nonce":"..."}` (JCS-canonicalized, without signature field).

**Mailbox MUST enforce**:
- Token expiration (`expires_at` < now)
- Nonce replay prevention (cache nonces for token lifetime + margin)
- Signature verification using sender's signing key (from identity document)
- Per-sender rate limits (e.g., 100 stores/minute)

### Trust Model

- Mailbox sees: sender IID (from token), recipient IID, encrypted blob, timing
- Mailbox does NOT see: message contents (E2E encrypted)
- Mailbox MAY: rate limit, charge for storage, impose size/duration limits
- Mailbox MUST NOT: decrypt, modify, or selectively block messages

### Message Envelope

Encrypted message for mailbox storage (full format specified in 03-messaging-sync):

```
Mailbox Envelope (outer layer, minimal):
┌────────────────────────────────────────┐
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Sender IID (raw 20 bytes)              │ 20 bytes
├────────────────────────────────────────┤
│ Recipient IID (raw 20 bytes)           │ 20 bytes
├────────────────────────────────────────┤
│ Encrypted payload length               │ 4 bytes
├────────────────────────────────────────┤
│ Encrypted payload                      │ variable
└────────────────────────────────────────┘
```

## Error Code Registry

To prevent overlaps, error codes are allocated by layer:

| Range | Layer | Examples |
|-------|-------|----------|
| 0x000-0x0FF | QUIC standard | NO_ERROR, PROTOCOL_VIOLATION |
| 0x100-0x1FF | Transport | IDENTITY_MISMATCH, HANDSHAKE_FAILED |
| 0x200-0x2FF | Identity | INVALID_DOCUMENT, SIGNATURE_FAILED |
| 0x300-0x3FF | Messaging | (reserved for 03-messaging-sync) |
| 0x400-0x4FF | Sync | (reserved for 03-messaging-sync) |
| 0x500-0x5FF | App Runtime | (reserved for 04-app-runtime) |

## Global Conventions

### Endianness

All multi-byte integers in binary wire formats are **big-endian** (network byte order) unless explicitly stated otherwise.

### Timestamps

All timestamps are **RFC3339 UTC** (e.g., `2025-01-13T12:00:00Z`).

### Encoding

| Type | Encoding | Notes |
|------|----------|-------|
| IID on wire | 32-char Base32 lowercase | Human-readable |
| IID in packets | 20 raw bytes | Space-efficient |
| Keys/signatures | Base64 standard (no padding) | `A-Za-z0-9+/` |
| Tokens (relay, auth) | Base64url (no padding) | `A-Za-z0-9-_` (URL-safe) |
| Sequence numbers | Decimal string | Avoid JSON number precision loss |

**Base64 vs Base64url:**
- **Keys and signatures**: Always use standard Base64 (`+/` chars)
- **Tokens and URL-safe data**: Use Base64url (`-_` chars)
- Both use no padding
- Implementations MUST decode using the correct alphabet for each type

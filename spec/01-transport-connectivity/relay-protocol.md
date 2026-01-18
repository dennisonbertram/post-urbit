# Relay Protocol

## Overview

Relays provide connectivity when direct connections fail (symmetric NAT, firewalls, mobile networks). They are **untrusted intermediaries** that forward encrypted data.

## Trust Model

| Property | Guarantee |
|----------|-----------|
| **Content confidentiality** | Relays see encrypted blobs only |
| **Metadata visibility** | Relays see: source IP, dest IID, timestamps, sizes |
| **Availability** | Relays can drop connections or go offline |
| **No authority** | Relays cannot modify, inject, or impersonate |

**Key principle**: Relays are replaceable. Users can run their own or choose from multiple providers.

## Relay Selection

Nodes discover relays from:

1. **Hardcoded defaults**: Bootstrap relays in client software
2. **Identity document**: Peer's preferred relays listed in endpoints
3. **DHT announcement**: Relays advertise availability
4. **Manual configuration**: User-specified relays

### Relay Requirements

A relay must:
- Accept authenticated allocations
- Forward packets bidirectionally
- Provide stable addressing for allocation lifetime
- Rate limit to prevent abuse

A relay must NOT:
- Require payment in protocol (out-of-band acceptable)
- Have special identity privileges
- Be able to decrypt content

## Allocation Protocol

Clients allocate a relay address before receiving connections.

### Allocation URL Derivation (Normative)

The relay endpoint in identity documents specifies the UDP/QUIC relay address. The HTTPS allocation API is derived as follows:

```python
def derive_allocation_url(relay_endpoint: dict) -> str:
    """
    Derive HTTPS allocation URL from relay endpoint.

    relay_endpoint: {"host": "relay.example.com", "port": 4433, ...}
    """
    host = relay_endpoint["host"].lower()

    # Allocation uses HTTPS on port 443 (or 8443 for non-standard deployments)
    # If relay port is 4433 (default QUIC), allocation port is 443
    # If relay port is non-standard, allocation port is relay_port - 4000
    # (e.g., relay 8433 → allocation 4433 is invalid; use explicit config)

    # v1 simple rule: allocation always on port 443
    return f"https://{host}/allocate"
```

**v1 Normative Rule:** For v1, the allocation endpoint is always `https://{relay.host}/allocate` on port 443. Relay operators MUST serve the allocation API on port 443 with a valid TLS certificate. The QUIC relay port (typically 4433) is separate from the HTTPS allocation port. [REQ-TRANS-059]

**Future versions** MAY extend the relay endpoint schema to include an explicit `allocation_port` or `allocation_url` field. [REQ-TRANS-060]

### HTTPS TLS Policy for Allocation (Normative)

The HTTPS allocation request MUST use standard WebPKI TLS validation: [REQ-TRANS-061]

1. **Certificate validation:** Clients MUST verify the server certificate against the system trust store (WebPKI roots) [REQ-TRANS-062]
2. **Hostname verification:** Clients MUST verify the certificate is valid for `relay.host` [REQ-TRANS-063]
3. **No self-signed:** Self-signed certificates MUST be rejected for allocation requests [REQ-TRANS-064]

**Rationale:** Unlike Post-Urbit QUIC connections (which use identity-based authentication and can accept any TLS certificate), the allocation request occurs before the client has established identity-based trust with the relay. A MITM on the allocation channel could steal the allocation token and race to bind the allocation to their own IP:port, hijacking inbound traffic. WebPKI validation prevents this attack.

**Note:** This is stricter than the QUIC TLS policy (which accepts any certificate for ALPN `post-urbit/1`). The allocation HTTPS endpoint is a separate trust domain.

### Allocation Request

Allocation uses signed request body (NOT JWT) to prove identity ownership:

```
POST /allocate HTTP/1.1
Host: relay.example.com
Content-Type: application/json

{
  "iid": "<client's identity identifier>",
  "lifetime": 3600,
  "timestamp": "<RFC3339-UTC-canonical>",
  "nonce": "<16-bytes-base64url>",
  "identity_doc_sequence": "42",
  "signature": "<Ed25519-signature-base64>"
}
```

**Note:** `identity_doc_sequence` is a decimal string (not number) to avoid JSON uint64 precision issues. See RFC-0002 §7.8.

**Signature construction**:
```
signature_input = concat(
  "post-urbit-relay-alloc-v1",  // domain separator (25 bytes)
  iid,                              // 32-char Base32
  lifetime (big-endian uint32),     // 4 bytes
  timestamp (UTF-8),                // 20 bytes (canonical YYYY-MM-DDTHH:MM:SSZ)
  nonce (raw bytes)                 // 16 bytes
)
signature = Ed25519_Sign(signing_key, SHA256(signature_input))
```

**Timestamp canonicalization:** Timestamps MUST use canonical RFC3339 UTC format: `YYYY-MM-DDTHH:MM:SSZ` (no fractional seconds, `Z` suffix). Implementations MUST reject non-canonical forms (see RFC-0002 §5.5). [REQ-TRANS-065]

**Relay verification**:
1. Parse request body
2. Check `timestamp` within ±5 minutes of relay clock
3. Check `nonce` not seen before (replay cache with 10-minute TTL)
4. Fetch/cache identity document for `iid` at `identity_doc_sequence` or higher
5. Reconstruct signature input and verify against identity document's current signing key
6. If valid, create allocation record with **UDP binding pending** (HTTPS source is TCP, not UDP; see RFC-0002 §7.8)

### Allocation Response

```json
{
  "allocation_id": "<unique-allocation-id>",
  "relay_address": "relay.example.com",
  "relay_port": 4433,
  "expires_at": "<RFC3339>",
  "token": "<allocation-token-for-renewal>"
}
```

### Stable Relay Port Model

**Important**: Relays use a **stable port model** for discovery compatibility:

| Aspect | Design |
|--------|--------|
| Relay port | Stable (e.g., 4433) - same for all clients |
| Routing key | Destination IID in packet header |
| Allocation token | Authenticates sender to relay |
| Identity publishing | Publish relay endpoint as `relay.example.com:4433` |

**Why stable port?**
- Identity documents publish relay endpoints (e.g., `{ type: 'relay', host: 'relay.example.com', port: 4433 }`)
- Identity publishing is expensive (signed, sequence-incrementing)
- Allocation lifetimes (~1h) are shorter than identity publish intervals (~24h)
- Per-allocation ports would require hourly identity updates

**How routing works:**
1. Sender connects to relay's stable port
2. Sender includes destination IID in packet header
3. Relay looks up active allocation for that IID
4. Relay forwards packet to allocation's bound IP:port

This means a relay can host many identities on one port, with the allocation token + IID determining which client receives forwarded data.

### Allocation Binding and Mobility

**Two-Step UDP Binding (per RFC-0002 §7.8):**
1. HTTPS allocation creates record with UDP binding **pending**
2. First valid PURL packet from client establishes UDP binding (relay learns client's UDP source address:port)
3. Subsequent packets must come from bound address; REBIND updates binding after NAT changes

**IP Binding Validation (after initial bind):**

| Scenario | Behavior |
|----------|----------|
| Same IP:port | Token accepted, packet forwarded |
| Different IP, same token | Rejected (potential token theft) |
| NAT rebinding (new port) | Client must send REBIND message |
| Mobile handoff | Client must send REBIND message |

**REBIND Message**: When a client's IP changes (NAT rebinding, WiFi→cellular), it sends a signed REBIND:

```json
{
  "type": "rebind",
  "allocation_id": "<id>",
  "token": "<allocation-token>",
  "timestamp": "<RFC3339-UTC-canonical>",
  "signature": "<Ed25519-sig-over-rebind-request>"
}
```

The relay verifies the signature, updates the binding, and resumes forwarding.

### Allocation Lifecycle

```
┌─────────────┐
│    NONE     │
└──────┬──────┘
       │ allocate()
       ▼
┌─────────────┐
│  ALLOCATED  │ ← Can receive connections
└──────┬──────┘
       │ timeout / refresh / rebind
       ▼
┌─────────────┐
│  ALLOCATED  │ ← Renewed or rebound
└──────┬──────┘
       │ expire / release
       ▼
┌─────────────┐
│  RELEASED   │
└─────────────┘
```

## Relay Wire Protocol

QUIC connections through relay use a framing layer.

### Relay Header

Every packet through relay has a header:

```
Relay Packet:
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x52 0x4C ("PURL")   │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Packet Type                            │ 1 byte
├────────────────────────────────────────┤
│ Allocation Token                       │ 16 bytes
├────────────────────────────────────────┤
│ Destination IID (raw bytes)            │ 20 bytes; for control packets (ALLOCATE, REFRESH, REBIND, RELEASE, KEEPALIVE, ERROR) this field MUST be 20 zero bytes (0x00 × 20)
├────────────────────────────────────────┤
│ Payload Length (big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Payload (QUIC packet)                  │ <length> bytes
└────────────────────────────────────────┘

Packet Types (see RFC-0002 §7.5 for authoritative registry):
  0x01 = DATA          Forward to destination
  0x02 = PING          Keepalive
  0x03 = PONG          Keepalive response
  0x04 = Reserved      (allocation via HTTPS, not UDP)
  0x05 = REFRESH       Extend allocation
  0x06 = RELEASE       End allocation
  0x07 = ERROR         Relay error response
  0x08 = REBIND        Update source IP:port binding
  0x09 = COORDINATE    Hole-punch coordination (see nat-traversal.md)

IID Encoding:
  - IID on wire is the raw 20-byte hash value (NOT Base32 encoded)
  - Decode Base32 IID string to get 20 bytes for packet
  - Zero-fill (20 null bytes) for relay commands that don't target a peer

Allocation Token Encoding:
  - 16 raw bytes
  - When returned in API as string: Base64url (no padding), yielding 22 chars
```

### Relay Data Flow (Stable Port Model)

```
Alice ──────────────────────────────────────────────────── Bob
  │                                                          │
  │  ┌─────────────────────────────────────────────────┐    │
  │  │              Relay Server (port 4433)            │    │
  │  │                                                  │    │
  │  │   ┌─────────────────┐    ┌─────────────────┐    │    │
  ├──┼──►│ Alice's Alloc   │    │ Bob's Alloc     │◄───┼────┤
  │  │   │ Token: abc123   │    │ Token: xyz789   │    │    │
  │  │   └────────┬────────┘    └────────┬────────┘    │    │
  │  │            │    Forward           │             │    │
  │  │            └──────────────────────┘             │    │
  │  └─────────────────────────────────────────────────┘    │
  │                                                          │

1. Alice connects to relay:4433, authenticates with her token
2. Alice sends packet with dest=Bob's IID in PURL header
3. Relay looks up Bob's allocation by IID
4. Relay forwards to Bob's bound IP:port (Bob's NAT-mapped address)
5. Bob receives with source=relay:4433 (Alice's IP hidden)
```

**Key points:**
- All clients connect to the SAME relay port (4433)
- Routing is by destination IID, not per-allocation ports
- Allocations track the client's bound IP:port (their NAT mapping), not relay-assigned ports

### Encapsulation Model (Normative)

**Per RFC-0002 §7.6:**

1. **DATA packets forwarded unchanged**: The relay forwards the **entire PURL packet** (header + payload) to the destination without modification
2. **Receiver decapsulates**: The receiving node strips the PURL header and passes only the inner QUIC payload to the QUIC stack
3. **Token validation on receive**: Recipients MUST NOT validate the allocation token on forwarded DATA packets (only the relay validates tokens for routing) [REQ-TRANS-066]
4. **Destination IID sanity check**: Receivers SHOULD verify the destination IID matches their own IID. **Note:** Per RFC-0002 §7.4, the PURL destination field is always an IID (20 bytes), not a DID. Device-level routing is NOT supported in v1. [REQ-TRANS-067]
5. **Payload size limit**: Payload length MUST NOT exceed 1200 bytes; relays and receivers MUST silently drop oversized packets [REQ-TRANS-068]

```
Sender flow:
  QUIC packet → wrap in PURL header → send to relay:4433

Relay flow:
  receive PURL → validate token → lookup dest IID → forward entire PURL packet unchanged

Receiver flow:
  receive PURL → verify dest IID → strip PURL header → pass inner payload to QUIC
```

## Relay Authentication

### Client → Relay Authentication

Clients authenticate allocations with their identity:

1. Create allocation request with timestamp
2. Sign with identity signing key
3. Relay verifies signature against IID's identity document
4. Relay caches identity document for allocation lifetime

```typescript
interface AllocationAuth {
  iid: string;
  timestamp: Timestamp;
  nonce: string;            // Prevent replay
  signature: Signature;     // Ed25519(signing_key, iid + timestamp + nonce)
}
```

### Relay → Client Authentication

Relays prove identity via TLS certificate or signed announcement:

```typescript
interface RelayInfo {
  relayId: string;          // Stable identifier
  addresses: string[];      // IP addresses
  ports: number[];          // UDP ports
  publicKey: PublicKey;     // For verifying announcements
  operator: string;         // Human-readable operator name
  terms?: string;           // Link to terms of service
}
```

## Rate Limiting

Relays enforce limits to prevent abuse:

| Limit | Default | Purpose |
|-------|---------|---------|
| Allocations per IID | 5 | Prevent allocation exhaustion |
| Packets per second | 1000 | Prevent flooding |
| Bytes per second | 10 MB | Bandwidth cap |
| Concurrent connections | 100 | Resource protection |
| Allocation lifetime | 3600s | Reclaim unused allocations |

### Rate Limit Response

When limits exceeded:

```
ERROR Packet:
┌────────────────────────────────────────┐
│ ... header ...                         │
├────────────────────────────────────────┤
│ Error Code: 0x01 (RATE_LIMITED)        │ 1 byte
├────────────────────────────────────────┤
│ Retry After (seconds, big-endian)      │ 4 bytes
├────────────────────────────────────────┤
│ Message Length (big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Message (UTF-8)                        │ <length> bytes
└────────────────────────────────────────┘

Error Codes:
  0x01 = RATE_LIMITED
  0x02 = ALLOCATION_NOT_FOUND
  0x03 = ALLOCATION_EXPIRED
  0x04 = INVALID_DESTINATION
  0x05 = RELAY_OVERLOADED
  0x06 = AUTHENTICATION_FAILED
  0x07 = BANNED

ERROR Payload Framing (Normative):
  - ERROR payload total length MUST equal: 1 (error_code) + 4 (details) + 2 (msg_len) + msg_len
  - Recipients MUST reject ERROR packets where lengths are inconsistent
  - Malformed ERROR packets MUST be silently dropped
```

## Multiple Relays

Nodes can use multiple relays for redundancy:

```typescript
interface RelayConfig {
  // Primary relay (used first)
  primary: RelayInfo;

  // Fallback relays (tried in order)
  fallbacks: RelayInfo[];

  // Selection strategy
  strategy: 'failover' | 'round-robin' | 'latency-based';
}
```

### Failover Process

```
1. Allocate on primary relay
2. If primary fails:
   a. Try allocation on first fallback
   b. Update endpoints in identity document
   c. Notify connected peers of new relay
3. Periodically check primary, migrate back if available
```

## Relay-Assisted Hole Punching

Relays can coordinate hole punching using the PURL COORDINATE packet type (0x09).

**Normative Specification:** See `nat-traversal.md` "Coordination Message Transport (Normative)" for the authoritative specification of:
- PURL COORDINATE packet format
- Message schemas (hole_punch_request, hole_punch_offer, hole_punch_accept)
- Signature construction for relay-assisted coordination
- Authentication requirements

**Overview:**
```
1. Alice and Bob both have allocations on same relay
2. Alice sends PURL COORDINATE packet with hole_punch_request to relay
3. Relay forwards COORDINATE packet to Bob (same as DATA forwarding)
4. Bob responds with hole_punch_accept via COORDINATE packet
5. Relay forwards accept back to Alice
6. Both Alice and Bob attempt direct connection via PUHP probes
7. If successful: close relay path, use direct QUIC
8. If failed (5s timeout): continue via relay
```

## Relay Operator Guidelines

For those running relays:

### Infrastructure

- Stable IP address and DNS
- Sufficient bandwidth (100 Mbps+)
- Low latency location relative to users
- DDoS protection recommended

### Privacy

- Minimize logging (no content, minimal metadata)
- Clear data retention policy
- Consider Tor-like onion relay mode (future enhancement)

### Economics

- Relays are NOT required to be free
- Payment is out-of-band (subscription, tokens, etc.)
- Free tier with rate limits is recommended for bootstrapping

## Security Considerations

1. **Relay impersonation**: Verify relay identity via TLS and/or signed announcements
2. **Traffic analysis**: Relays see metadata; consider padding and timing obfuscation
3. **Relay compromise**: Use multiple relays from different operators
4. **Censorship**: Relay blocking is possible; support relay discovery via DHT
5. **Sybil relays**: Malicious relays could collect metadata; prefer known operators

## Test Scenarios

1. **Successful allocation**: Client allocates, receives traffic, releases
2. **Allocation expiry**: Allocation times out, client re-allocates
3. **Rate limiting**: Client exceeds limits, receives error, backs off
4. **Relay failover**: Primary fails, client migrates to fallback
5. **Hole punch via relay**: Relay coordinates direct connection attempt
6. **Invalid destination**: Client sends to unknown IID, receives error

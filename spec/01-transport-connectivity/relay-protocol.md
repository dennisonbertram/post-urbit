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

### Allocation Request

Allocation uses signed request body (NOT JWT) to prove identity ownership:

```
POST /allocate HTTP/1.1
Host: relay.example.com
Content-Type: application/json

{
  "iid": "<client's identity identifier>",
  "lifetime": 3600,
  "timestamp": "<RFC3339-UTC>",
  "nonce": "<16-bytes-base64-random>",
  "identity_doc_sequence": 42,
  "signature": "<Ed25519-signature-base64>"
}
```

**Signature construction**:
```
signature_input = concat(
  "post-urbit-relay-allocate-v1",  // domain separator
  iid,                              // 32-char Base32
  lifetime (big-endian uint32),     // 4 bytes
  timestamp (UTF-8),                // variable
  nonce (raw bytes)                 // 16 bytes
)
signature = Ed25519_Sign(signing_key, SHA256(signature_input))
```

**Relay verification**:
1. Parse request body
2. Check `timestamp` within ±5 minutes of relay clock
3. Check `nonce` not seen before (replay cache with 10-minute TTL)
4. Fetch/cache identity document for `iid` at `identity_doc_sequence` or higher
5. Reconstruct signature input and verify against identity document's current signing key
6. If valid, create allocation bound to source IP:port

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

**IP Binding**: Allocations are bound to the source IP:port at creation time.

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
  "timestamp": "<RFC3339>",
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
│ Destination IID (raw, decoded)         │ 20 bytes (or 0 for relay commands)
├────────────────────────────────────────┤
│ Payload Length (big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Payload (QUIC packet)                  │ <length> bytes
└────────────────────────────────────────┘

Packet Types:
  0x01 = DATA          Forward to destination
  0x02 = PING          Keepalive
  0x03 = PONG          Keepalive response
  0x04 = ALLOCATE      Request allocation (over HTTPS, not UDP)
  0x05 = REFRESH       Extend allocation
  0x06 = RELEASE       End allocation
  0x07 = ERROR         Relay error

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
│ Retry After (seconds)                  │ 4 bytes
├────────────────────────────────────────┤
│ Message (UTF-8)                        │ variable
└────────────────────────────────────────┘

Error Codes:
  0x01 = RATE_LIMITED
  0x02 = ALLOCATION_NOT_FOUND
  0x03 = ALLOCATION_EXPIRED
  0x04 = INVALID_DESTINATION
  0x05 = RELAY_OVERLOADED
  0x06 = AUTHENTICATION_FAILED
  0x07 = BANNED
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

Relays can coordinate hole punching:

```
1. Alice and Bob both have allocations on same relay
2. Alice requests hole punch via relay
3. Relay sends coordination messages to both
4. Alice and Bob attempt direct connection
5. If successful: close relay path, use direct
6. If failed: continue via relay
```

### Hole Punch Coordination Message

```json
{
  "type": "hole_punch_coordinate",
  "transaction_id": "<random-16-bytes>",
  "initiator_iid": "<alice>",
  "target_iid": "<bob>",
  "initiator_candidates": [
    {"address": "1.2.3.4", "port": 5000, "type": "srflx"},
    {"address": "10.0.0.5", "port": 5000, "type": "host"}
  ],
  "timestamp": "<RFC3339>"
}
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

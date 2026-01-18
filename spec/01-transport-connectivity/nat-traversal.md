# NAT Traversal

## Overview

Most nodes will be behind NAT (Network Address Translation), which prevents direct incoming connections. This document specifies how nodes discover their public address and establish direct connections through NAT.

## NAT Types

| Type | Behavior | Traversability |
|------|----------|----------------|
| **Full Cone** | Any external host can send to mapped port | Easy |
| **Restricted Cone** | Only hosts node has contacted can reply | Medium |
| **Port Restricted** | Only same host:port can reply | Medium |
| **Symmetric** | New mapping per destination | Hard (relay needed) |

## Discovery Protocol (STUN-like)

Nodes discover their public IP and port using a simple STUN-like protocol.

### Discovery Server

Discovery servers are simple UDP echo services that tell nodes their observed address.

```
Discovery Request:
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x44 0x53 ("PUDS")   │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Transaction ID                         │ 16 bytes
├────────────────────────────────────────┤
│ (empty payload)                        │
└────────────────────────────────────────┘

Discovery Response:
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x44 0x53 ("PUDS")   │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Transaction ID                         │ 16 bytes
├────────────────────────────────────────┤
│ Address Type (1=IPv4, 2=IPv6)          │ 1 byte
├────────────────────────────────────────┤
│ Observed Address                       │ 4 or 16 bytes (network byte order)
├────────────────────────────────────────┤
│ Observed Port                          │ 2 bytes (uint16 big-endian)
└────────────────────────────────────────┘

All multi-byte integers in PUDS messages are big-endian (network byte order) per layer-integration.md §Global Conventions.
```

### Parsing Rules (Normative)

Implementations MUST enforce strict packet length validation: [REQ-TRANS-001]

| Packet Type | Required Length | Description |
|-------------|-----------------|-------------|
| Request | Exactly 21 bytes | 4 (magic) + 1 (version) + 16 (transaction_id) |
| Response (IPv4) | Exactly 24 bytes | 4 (magic) + 1 (version) + 16 (transaction_id) + 1 (addr_type) + 4 (address) + 2 (port) |
| Response (IPv6) | Exactly 36 bytes | 4 (magic) + 1 (version) + 16 (transaction_id) + 1 (addr_type) + 16 (address) + 2 (port) |

**Normative Requirements:**

1. **Request packets** MUST be exactly 21 bytes (4 magic + 1 version + 16 transaction_id) [REQ-TRANS-002]
2. **Response packets** MUST be exactly 24 bytes (IPv4) or 36 bytes (IPv6) [REQ-TRANS-003]
3. **Packets with incorrect length** MUST be silently dropped [REQ-TRANS-004]
4. **Unknown version values** MUST cause silent drop (no response) [REQ-TRANS-005]
5. **Unknown address type values** in responses MUST be ignored by clients [REQ-TRANS-006]

**Rationale:** Strict parsing prevents amplification attacks where malformed packets could trigger error responses larger than the request. Silent dropping ensures no bandwidth amplification.

### Discovery Process

```
1. Send Discovery Request to multiple discovery servers
2. Collect responses (observed address:port)
3. Compare results:
   - All same: likely Full Cone NAT or direct IP
   - Different ports: likely Symmetric NAT
   - Some failures: firewall or unreachable server
4. Record public address for use in endpoints
5. Determine NAT type for path selection hints
```

### NAT Type Detection

```
function detect_nat_type():
    # Query multiple servers from same local port
    result1 = query_server(server_a, local_port=5000)
    result2 = query_server(server_b, local_port=5000)
    result3 = query_server(server_c, local_port=5000)

    if all addresses same and all ports same:
        # Query server_a again, but from different port
        result4 = query_server(server_a, local_port=5001)
        if result1.port == result4.port:
            return FULL_CONE
        else:
            return PORT_RESTRICTED

    if all addresses same but ports differ:
        return SYMMETRIC

    return UNKNOWN
```

## Hole Punching

For NAT types that allow it, hole punching enables direct connections.

### Coordination Message Transport (Normative)

This section specifies how hole-punch coordination messages are transported between peers. Implementations MUST follow these requirements for interoperability. [REQ-TRANS-007]

#### Transport Channels

Hole-punch coordination can occur via two channels:

| Channel | Use Case | Transport |
|---------|----------|-----------|
| **Direct Peer** | Both peers have existing authenticated connection | QUIC Control stream (0x01) |
| **Relay-Assisted** | No direct connection; relay mediates | PURL COORDINATE packet (0x09) |

#### Direct Peer Coordination

When peers have an existing authenticated QUIC connection (e.g., through a relay or prior direct path), coordination messages are sent on the **Control stream (0x01)**.

**Framing (per RFC-0002 §5.4):**
```
┌────────────────────────────────────────┐
│ Message Length (4 bytes, big-endian)   │
├────────────────────────────────────────┤
│ JSON Message (UTF-8)                   │
└────────────────────────────────────────┘
```

**Authentication Requirement:** Control stream messages require a **completed identity handshake**. Coordination messages MUST NOT be sent until both peers are mutually authenticated (connection state = CONNECTED per RFC-0002 §8.1). [REQ-TRANS-008]

**Message Type Field:** All coordination messages include a `type` field:
- `"hole_punch_request"` - Initiator requests hole punch
- `"hole_punch_offer"` - Forwarded request to target
- `"hole_punch_accept"` - Target accepts and provides endpoints
- `"hole_punch_reject"` - Target declines (optional)

#### Relay-Assisted Coordination

When peers have no direct connection, a relay can coordinate hole punching using PURL packet type `0x09` (COORDINATE).

**PURL COORDINATE Packet:**
```
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x52 0x4C ("PURL")   │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Packet Type: 0x09 (COORDINATE)         │ 1 byte
├────────────────────────────────────────┤
│ Allocation Token                       │ 16 bytes
├────────────────────────────────────────┤
│ Destination IID (raw bytes)            │ 20 bytes
├────────────────────────────────────────┤
│ Payload Length (big-endian)            │ 2 bytes
├────────────────────────────────────────┤
│ Payload (JSON, UTF-8)                  │ <length> bytes
└────────────────────────────────────────┘
```

**Relay Processing:**
1. Relay receives COORDINATE packet from Alice with `dest=Bob's IID`
2. Relay validates Alice's allocation token
3. Relay looks up Bob's allocation by IID
4. Relay forwards the **entire PURL packet** to Bob's bound IP:port (same as DATA forwarding per RFC-0002 §7.6)
5. Bob decapsulates PURL header and processes JSON payload

**Authentication:** COORDINATE packets are authenticated by the allocation token. The sender MUST have a valid allocation. The JSON payload MAY include signed fields for additional verification (see Message Schemas below). [REQ-TRANS-009]

**Max Payload Size:** COORDINATE payloads MUST NOT exceed 1200 bytes (same limit as DATA payloads per RFC-0002 §7.4). [REQ-TRANS-010]

#### Message Schemas (Normative)

All coordination messages MUST include the base fields specified below. The `signature` field requirement depends on the transport channel (see Signature Requirements section): [REQ-TRANS-011]
- **Direct Peer (Control stream):** `signature` field MAY be omitted (field can be absent or null) [REQ-TRANS-012]
- **Relay-Assisted (PURL COORDINATE):** `signature` field MUST be present [REQ-TRANS-013]

**hole_punch_request:**
```json
{
  "type": "hole_punch_request",
  "transaction_id": "<16-bytes-base64url-no-padding>",
  "initiator": "<32-char-base32-iid>",
  "target": "<32-char-base32-iid>",
  "initiator_endpoints": [
    {
      "address": "<IPv4 or IPv6>",
      "port": 12345,
      "type": "srflx|host|mapped"
    }
  ],
  "timestamp": "<RFC3339-UTC-canonical>",
  "signature": "<64-bytes-base64-standard>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| type | MUST | Literal `"hole_punch_request"`  [REQ-TRANS-014]|
| transaction_id | MUST | 16 random bytes, Base64url no padding (22 chars)  [REQ-TRANS-015]|
| initiator | MUST | Initiator's IID (Crockford Base32, 32 chars)  [REQ-TRANS-016]|
| target | MUST | Target's IID (Crockford Base32, 32 chars)  [REQ-TRANS-017]|
| initiator_endpoints | MUST | Array of candidate endpoints (at least 1)  [REQ-TRANS-018]|
| timestamp | MUST | RFC3339 UTC canonical (`YYYY-MM-DDTHH:MM:SSZ`)  [REQ-TRANS-019]|
| signature | Transport-dependent | See Signature Requirements below |

**hole_punch_offer** (relay-generated or forwarded):
```json
{
  "type": "hole_punch_offer",
  "transaction_id": "<same-as-request>",
  "initiator": "<32-char-base32-iid>",
  "initiator_endpoints": [
    {"address": "...", "port": ..., "type": "..."}
  ],
  "timestamp": "<RFC3339-UTC-canonical>",
  "signature": "<64-bytes-base64-standard>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| type | MUST | Literal `"hole_punch_offer"`  [REQ-TRANS-020]|
| transaction_id | MUST | Same as originating request  [REQ-TRANS-021]|
| initiator | MUST | Initiator's IID  [REQ-TRANS-022]|
| initiator_endpoints | MUST | Initiator's candidate endpoints  [REQ-TRANS-023]|
| timestamp | MUST | From original request  [REQ-TRANS-024]|
| signature | Transport-dependent | Original initiator's signature; see Signature Requirements below |

**hole_punch_accept:**
```json
{
  "type": "hole_punch_accept",
  "transaction_id": "<same-as-request>",
  "responder": "<32-char-base32-iid>",
  "responder_endpoints": [
    {"address": "...", "port": ..., "type": "..."}
  ],
  "timestamp": "<RFC3339-UTC-canonical>",
  "signature": "<64-bytes-base64-standard>"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| type | MUST | Literal `"hole_punch_accept"`  [REQ-TRANS-025]|
| transaction_id | MUST | Same as originating request (for correlation)  [REQ-TRANS-026]|
| responder | MUST | Responder's IID  [REQ-TRANS-027]|
| responder_endpoints | MUST | Responder's candidate endpoints (at least 1)  [REQ-TRANS-028]|
| timestamp | MUST | RFC3339 UTC canonical  [REQ-TRANS-029]|
| signature | Transport-dependent | See Signature Requirements below |

**hole_punch_reject** (optional):
```json
{
  "type": "hole_punch_reject",
  "transaction_id": "<same-as-request>",
  "responder": "<32-char-base32-iid>",
  "reason": "unavailable|policy|busy"
}
```

#### Signature Requirements (Normative)

Signature requirements depend on the transport channel used for coordination:

| Transport Channel | Signature Requirement | Verification Requirement |
|-------------------|----------------------|--------------------------|
| **Direct Peer (Control stream)** | MAY be omitted | If present, recipients SHOULD verify  [REQ-TRANS-030]|
| **Relay-Assisted (PURL COORDINATE)** | MUST be present | Recipients MUST verify  [REQ-TRANS-031]|

**Rationale:**
- **Direct Peer:** The QUIC connection is already mutually authenticated via the identity handshake (connection state = CONNECTED per RFC-0002 §8.1). The authenticated channel proves the sender's identity, making signatures redundant but optionally permitted for defense-in-depth.
- **Relay-Assisted:** The relay connection authenticates the allocation token but does NOT prove the originator's identity. Signatures are required to prevent impersonation attacks where a malicious relay or attacker forges coordination messages.

**Implementation Note:** Implementations MUST reject relay-assisted coordination messages (PURL COORDINATE packets) that lack a valid signature. Implementations MAY accept direct peer coordination messages without signatures when the peer is already authenticated. [REQ-TRANS-032]

#### Signature Construction

When signatures are present (required for relay-assisted, optional for direct peer), they prove identity ownership.

**Domain Separator:** `post-urbit-holepunch-v1` (23 ASCII bytes)

**Request Signature:**
```
DOMAIN = b"post-urbit-holepunch-v1"  // 23 bytes

signature_input = concat(
  DOMAIN,                                    // 23 bytes
  decode_base64url(transaction_id),          // 16 bytes
  encode_utf8(initiator),                    // 32 bytes (Base32 IID)
  encode_utf8(target),                       // 32 bytes (Base32 IID)
  encode_utf8(timestamp),                    // 20 bytes (canonical)
  SHA256(JCS(initiator_endpoints))           // 32 bytes (endpoint binding)
)
// Total: 155 bytes

signature = Ed25519_Sign(signing_key, SHA256(signature_input))
```

**Accept Signature:**
```
signature_input = concat(
  DOMAIN,                                    // 23 bytes
  decode_base64url(transaction_id),          // 16 bytes
  encode_utf8(responder),                    // 32 bytes (Base32 IID)
  encode_utf8(timestamp),                    // 20 bytes (canonical)
  SHA256(JCS(responder_endpoints))           // 32 bytes (endpoint binding)
)
// Total: 123 bytes

signature = Ed25519_Sign(signing_key, SHA256(signature_input))
```

**Endpoint Binding (Normative):**

Endpoints are bound into the signature via SHA256 of their JCS-canonical JSON representation. This binding is critical for security:

- Recipients MUST verify the signature covers the received endpoints [REQ-TRANS-033]
- This prevents relay tampering with endpoint lists
- JCS (JSON Canonicalization Scheme, RFC 8785) ensures deterministic serialization

**Verification:** When verification is required or when a signature is present and SHOULD be verified, recipients fetch the signer's identity document and verify using the current signing key. Recipients MUST reconstruct the `signature_input` using the received endpoint arrays and verify that the signature is valid over that input. [REQ-TRANS-034]

#### Response Correlation

All responses (offer, accept, reject) MUST include the same `transaction_id` as the originating request. This enables: [REQ-TRANS-035]
- Matching responses to pending requests
- Detecting duplicate/replayed messages
- Timeout management per transaction

Implementations SHOULD maintain a pending transaction table with 10-second timeout per transaction. [REQ-TRANS-036]

#### Sequence Diagram (Relay-Assisted)

```
Alice                    Relay                     Bob
  │                        │                        │
  │ PURL COORDINATE        │                        │
  │ {hole_punch_request}   │                        │
  ├───────────────────────►│                        │
  │                        │ PURL COORDINATE        │
  │                        │ {hole_punch_offer}     │
  │                        ├───────────────────────►│
  │                        │                        │
  │                        │ PURL COORDINATE        │
  │                        │ {hole_punch_accept}    │
  │                        │◄───────────────────────┤
  │ PURL COORDINATE        │                        │
  │ {hole_punch_accept}    │                        │
  │◄───────────────────────┤                        │
  │                        │                        │
  │ ═══════ Both start sending PUHP probes ═══════ │
  │                        │                        │
  │◄──────────────────────────────────────────────►│
  │           Direct UDP (PUHP probes)              │
  │                        │                        │
  │◄═══════════════════════════════════════════════►│
  │           Direct QUIC connection                │
```

### Hole Punching Protocol

Requires a coordination channel (relay or mutual peer) as specified above.

```
Alice (behind NAT-A) wants to connect to Bob (behind NAT-B):

1. Alice sends connection request to coordinator:
   {
     "type": "hole_punch_request",
     "initiator": "alice_iid",
     "target": "bob_iid",
     "initiator_endpoints": [{...}],
     "transaction_id": "..."
   }

2. Coordinator forwards to Bob:
   {
     "type": "hole_punch_offer",
     "initiator": "alice_iid",
     "initiator_endpoints": [{...}],
     "transaction_id": "..."
   }

3. Bob sends to Alice (via coordinator):
   {
     "type": "hole_punch_accept",
     "responder": "bob_iid",
     "responder_endpoints": [{...}],
     "transaction_id": "..."
   }

4. Both Alice and Bob simultaneously:
   - Send UDP packets to each other's public address:port
   - This "punches holes" in their NATs
   - First QUIC packet that gets through establishes connection

5. If successful: direct QUIC connection
   If timeout (5s): fall back to relay
```

### Hole Punch Timing

```
Timeline (both sides):

T+0ms:    Receive peer's endpoints
T+50ms:   Send first probe packet
T+100ms:  Send second probe packet
T+200ms:  Send third probe packet
T+500ms:  Send fourth probe packet
T+1000ms: Send fifth probe packet
T+5000ms: Timeout, fall back to relay
```

### Probe Packet Format

```
Hole Punch Probe:
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x48 0x50 ("PUHP")   │ 4 bytes
├────────────────────────────────────────┤
│ Transaction ID                         │ 16 bytes (raw bytes)
├────────────────────────────────────────┤
│ Sender IID Prefix                      │ 8 bytes (raw bytes)
├────────────────────────────────────────┤
│ Timestamp (ms since epoch, big-endian) │ 8 bytes
└────────────────────────────────────────┘

Total: 36 bytes (fits in single UDP packet)
```

**Field Encoding (Normative):**

| Field | Wire Format | Derivation |
|-------|-------------|------------|
| Transaction ID | 16 raw bytes | In coordination JSON: `transaction_id` is Base64url (no padding) of these 16 bytes |
| Sender IID Prefix | 8 raw bytes | `decode_base32(sender_iid)[0:8]` — first 8 bytes of the 20-byte raw IID |
| Timestamp | uint64 big-endian | Unix milliseconds since epoch |

**Example:** If `transaction_id` in JSON is `"AAAAAAAAAAAAAAAAAAAAAA"` (Base64url), the 16 wire bytes are `0x00 0x00 ... 0x00`.

When a probe is received, the receiver knows the NAT mapping is open and can begin QUIC handshake.

## Port Mapping (UPnP/NAT-PMP)

For nodes behind home routers, automatic port mapping can enable direct connections.

### Protocol Support

| Protocol | Support Level |
|----------|---------------|
| UPnP IGD | Optional, try first |
| NAT-PMP | Optional, try if UPnP fails |
| PCP | Optional, modern replacement for NAT-PMP |

### Port Mapping Process

```
1. Discover gateway (via UPnP SSDP or NAT-PMP)
2. Request port mapping:
   - External port: same as internal (preferred) or any
   - Internal port: QUIC listening port
   - Protocol: UDP
   - Lifetime: 3600 seconds (1 hour)
3. If successful: add external address to endpoints
4. Refresh mapping before expiry
5. Delete mapping on shutdown
```

### Port Mapping Interface

```typescript
interface PortMapping {
  protocol: 'upnp' | 'nat-pmp' | 'pcp';
  externalAddress: string;
  externalPort: number;
  internalPort: number;
  lifetime: number;       // seconds
  createdAt: Timestamp;
  expiresAt: Timestamp;
}

interface PortMappingService {
  // Discover available protocols
  discoverGateway(): Promise<GatewayInfo | null>;

  // Request a port mapping
  createMapping(internalPort: number, lifetime: number): Promise<PortMapping>;

  // Refresh an existing mapping
  refreshMapping(mapping: PortMapping): Promise<PortMapping>;

  // Delete a mapping
  deleteMapping(mapping: PortMapping): Promise<void>;

  // List active mappings
  listMappings(): Promise<PortMapping[]>;
}
```

## Address Candidates

Nodes collect multiple address candidates for connectivity:

| Priority | Type | Source |
|----------|------|--------|
| 1 | Host | Local interface addresses |
| 2 | Server Reflexive | STUN-like discovery |
| 3 | Port Mapped | UPnP/NAT-PMP |
| 4 | Relay | Relay server allocation |

### Candidate Collection

```typescript
interface AddressCandidate {
  type: 'host' | 'srflx' | 'mapped' | 'relay';
  address: string;        // IP address (for relay: relay server's address)
  port: number;           // Port (for relay: relay server's port)
  priority: number;       // ICE-like priority calculation
  foundation: string;     // For candidate pairing
  relayServer?: RelayServer;    // If type == 'relay'
  allocationToken?: string;     // If type == 'relay', authentication token
}

function collectCandidates(): AddressCandidate[] {
  candidates = [];

  // Host candidates (local interfaces)
  for iface in network_interfaces():
    if iface.is_up and not iface.is_loopback:
      candidates.push({
        type: 'host',
        address: iface.address,
        port: listening_port,
        priority: calculate_priority('host', iface),
        foundation: hash(iface.name)
      });

  // Server reflexive (STUN-like)
  for server in discovery_servers:
    result = discover(server);
    if result:
      candidates.push({
        type: 'srflx',
        address: result.address,
        port: result.port,
        priority: calculate_priority('srflx', server),
        foundation: hash(server)
      });

  // Port mapped (UPnP/NAT-PMP)
  if port_mapping_available():
    mapping = create_port_mapping();
    candidates.push({
      type: 'mapped',
      address: mapping.external_address,
      port: mapping.external_port,
      priority: calculate_priority('mapped'),
      foundation: 'mapped'
    });

  // Relay (always available as fallback)
  // Relay candidates use the relay SERVER's address (where peers connect)
  for relay in configured_relays:
    allocation = allocate_relay(relay);  // Returns RelayAllocation with token
    candidates.push({
      type: 'relay',
      address: relay.address,    // Relay server address (NOT client's bound address)
      port: relay.port,          // Relay server port
      priority: calculate_priority('relay', relay),
      foundation: hash(relay.id),
      relayServer: relay,
      allocationToken: allocation.token  // Token for relay authentication
    });

  return sort_by_priority(candidates);
}
```

### Priority Calculation

ICE-like priority formula:

```
priority = (2^24 * type_preference) + (2^8 * local_preference) + (256 - component_id)

Type preferences:
  host:   126
  srflx:  100
  mapped: 90
  relay:  50

Local preference:
  IPv4:   65535
  IPv6:   65534
```

## Path Selection

Given multiple candidates, select the best path:

```
1. Sort candidates by priority
2. For each candidate pair (local, remote):
   a. If both are relay: use relay path
   b. If one is relay: try direct first, relay fallback
   c. If both are srflx/mapped: try hole punching
   d. If one is host and reachable: direct connection
3. Attempt connection with timeout
4. Fall back to next candidate pair on failure
```

### Connection Timeout Strategy

| Attempt | Timeout | Candidate Type |
|---------|---------|----------------|
| 1 | 2s | Highest priority direct |
| 2 | 2s | Second priority direct |
| 3 | 2s | Third priority direct |
| 4 | 5s | Hole punch attempt |
| 5 | ∞ | Relay (always works) |

## Security Considerations

1. **Amplification attacks**: Discovery servers should rate-limit responses
2. **Port scanning**: Don't respond to probes without valid transaction ID
3. **Relay abuse**: Authenticate relay allocations (see relay-protocol.md)
4. **IP disclosure**: Public IP is revealed to peers and relays

## Test Scenarios

1. **Full cone NAT**: Direct connection succeeds immediately
2. **Symmetric NAT**: Hole punch fails, relay used
3. **Port restricted**: Hole punch succeeds after probes
4. **UPnP available**: Port mapping enables direct connection
5. **No discovery servers**: Relay-only mode
6. **Mobile handoff**: Connection migrates between WiFi and cellular

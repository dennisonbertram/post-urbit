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
│ Observed Address                       │ 4 or 16 bytes
├────────────────────────────────────────┤
│ Observed Port                          │ 2 bytes
└────────────────────────────────────────┘
```

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

### Hole Punching Protocol

Requires a coordination channel (relay or mutual peer).

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
│ Transaction ID (from coordination)     │ 16 bytes
├────────────────────────────────────────┤
│ Sender IID (truncated, first 8 bytes)  │ 8 bytes
├────────────────────────────────────────┤
│ Timestamp (ms since epoch, big-endian) │ 8 bytes
└────────────────────────────────────────┘

Total: 36 bytes (fits in single UDP packet)
```

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
  address: string;        // IP address
  port: number;
  priority: number;       // ICE-like priority calculation
  foundation: string;     // For candidate pairing
  relayServer?: string;   // If type == 'relay'
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
  for relay in configured_relays:
    allocation = allocate_relay(relay);
    candidates.push({
      type: 'relay',
      address: allocation.address,
      port: allocation.port,
      priority: calculate_priority('relay', relay),
      foundation: hash(relay),
      relayServer: relay
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

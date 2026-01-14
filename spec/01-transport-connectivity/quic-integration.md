# QUIC Integration

## Overview

QUIC (RFC 9000) is the foundation transport protocol. It provides:

- **Encryption**: TLS 1.3 integrated into the protocol
- **Multiplexing**: Multiple streams over one connection
- **0-RTT**: Fast reconnection to known peers
- **Connection migration**: Handles IP address changes (mobile, NAT rebinding)

## QUIC Configuration

### Connection Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| **ALPN** | `post-urbit/1` | Protocol identification |
| **Max idle timeout** | 30 seconds | Balance keepalive vs resources |
| **Max streams (bidi)** | 100 | Sufficient for concurrent operations |
| **Max streams (uni)** | 100 | For push notifications |
| **Initial RTT** | 100ms | Conservative default |
| **Max UDP payload** | 1200 bytes | Safe for most paths |

### TLS Configuration

| Parameter | Value |
|-----------|-------|
| **TLS version** | 1.3 only |
| **Cipher suites** | TLS_CHACHA20_POLY1305_SHA256, TLS_AES_256_GCM_SHA384 |
| **Key exchange** | X25519 |
| **Signature** | Ed25519 (for identity binding) |
| **Certificate type** | Self-signed, identity-bound (see peer-handshake.md) |

### ALPN Protocol String

```
post-urbit/1
```

Version negotiation via ALPN allows future protocol upgrades.

## Connection Establishment

### Client Initiating

```
1. Resolve peer IID to endpoints (from identity document or cache)
2. Select endpoint (direct preferred, relay fallback)
3. Create QUIC connection with identity-bound certificate
4. Perform identity handshake (see peer-handshake.md)
5. Connection ready for application streams
```

### Server Listening

```
1. Listen on configured UDP port(s)
2. Accept incoming QUIC connections
3. Verify client's identity proof during handshake
4. Connection ready for application streams
```

## Stream Types

QUIC supports multiple concurrent streams. We define:

| Stream Type | Direction | Purpose |
|-------------|-----------|---------|
| **Control** | Bidirectional | Handshake, keepalive, connection management |
| **Identity** | Bidirectional | Identity document exchange and updates |
| **Message** | Bidirectional | Application messages (chat, notifications) |
| **Sync** | Bidirectional | Data synchronization |
| **Bulk** | Unidirectional | Large data transfers (files, backups) |

### Stream Multiplexing

```
┌─────────────────────────────────────────┐
│              QUIC Connection            │
├─────────┬─────────┬─────────┬───────────┤
│ Stream 0│ Stream 1│ Stream 2│ Stream N  │
│ Control │ Identity│ Message │   ...     │
└─────────┴─────────┴─────────┴───────────┘
```

### Stream Opening

Streams are opened on-demand with a type identifier:

```
Stream Header (first bytes):
┌──────────────────────────────────────┐
│ Stream Type (1 byte)                 │
├──────────────────────────────────────┤
│ Payload...                           │
└──────────────────────────────────────┘

Stream Types:
  0x00 = Reserved
  0x01 = Control
  0x02 = Identity
  0x03 = Message
  0x04 = Sync
  0x05 = Bulk
  0x06-0xFF = Reserved for future use
```

## Connection Lifecycle

### State Machine

```
┌─────────────┐
│    IDLE     │
└──────┬──────┘
       │ connect()
       ▼
┌─────────────┐
│ CONNECTING  │ ← QUIC handshake in progress
└──────┬──────┘
       │ TLS complete
       ▼
┌─────────────┐
│ HANDSHAKING │ ← Identity verification
└──────┬──────┘
       │ identity verified
       ▼
┌─────────────┐
│  CONNECTED  │ ← Ready for application use
└──────┬──────┘
       │ timeout / error / close
       ▼
┌─────────────┐
│   CLOSED    │
└─────────────┘
```

### Connection Events

| Event | Trigger | Action |
|-------|---------|--------|
| `connected` | Handshake complete | Start application streams |
| `stream_opened` | Peer opens stream | Handle based on stream type |
| `data_received` | Data on any stream | Dispatch to handler |
| `connection_lost` | Timeout or error | Reconnect or notify app |
| `migration` | IP address change | Continue seamlessly |

## 0-RTT Resumption

For previously connected peers:

1. **Store session ticket** after successful connection
2. **On reconnect**: Send 0-RTT data with session ticket
3. **Server verifies**: Accept if ticket valid, reject replay

### 0-RTT Security Considerations

- 0-RTT data can be replayed by network attackers
- Only use for idempotent operations (identity fetch, presence)
- Non-idempotent operations (messages) must wait for 1-RTT confirmation

```typescript
interface ZeroRttPolicy {
  // Operations safe for 0-RTT (can be replayed)
  SAFE_FOR_0RTT: ['identity_request', 'ping', 'presence'];

  // Operations requiring 1-RTT (replay-sensitive)
  REQUIRE_1RTT: ['message_send', 'sync_write', 'key_rotation'];
}
```

## Connection Migration

QUIC handles IP address changes gracefully:

1. **Mobile scenarios**: WiFi to cellular handoff
2. **NAT rebinding**: Router assigns new port
3. **VPN transitions**: Tunnel up/down

### Migration Process

```
1. Detect address change (send fails, ICMP unreachable)
2. Probe new path with PATH_CHALLENGE
3. Receive PATH_RESPONSE
4. Switch to new path, continue connection
```

## Congestion Control

Use QUIC's default congestion control (Cubic or BBR):

| Parameter | Value |
|-----------|-------|
| **Initial window** | 10 * max_udp_payload |
| **Min window** | 2 * max_udp_payload |
| **Algorithm** | Cubic (default) or BBR (optional) |

## Error Handling

### QUIC Error Codes

| Code | Name | Meaning |
|------|------|---------|
| `0x00` | NO_ERROR | Clean close |
| `0x01` | INTERNAL_ERROR | Implementation error |
| `0x02` | CONNECTION_REFUSED | Server rejected connection |
| `0x03` | FLOW_CONTROL_ERROR | Flow control violation |
| `0x04` | STREAM_LIMIT_ERROR | Too many streams |
| `0x05` | STREAM_STATE_ERROR | Invalid stream state |
| `0x06` | FINAL_SIZE_ERROR | Size mismatch |
| `0x07` | FRAME_ENCODING_ERROR | Invalid frame |
| `0x08` | TRANSPORT_PARAMETER_ERROR | Invalid parameters |
| `0x09` | CONNECTION_ID_LIMIT_ERROR | Too many connection IDs |
| `0x0A` | PROTOCOL_VIOLATION | Protocol error |
| `0x0B` | INVALID_TOKEN | Bad token |
| `0x0C` | APPLICATION_ERROR | Application-level error |
| `0x0D` | CRYPTO_BUFFER_EXCEEDED | Crypto buffer overflow |
| `0x0E` | KEY_UPDATE_ERROR | Key update failed |
| `0x0F` | AEAD_LIMIT_REACHED | AEAD usage limit |
| `0x10` | NO_VIABLE_PATH | No path available |

### Application Error Codes (0x100+)

| Code | Name | Meaning |
|------|------|---------|
| `0x100` | IDENTITY_MISMATCH | Peer identity doesn't match expected |
| `0x101` | HANDSHAKE_FAILED | Identity handshake failed |
| `0x102` | STREAM_TYPE_UNKNOWN | Unknown stream type |
| `0x103` | MESSAGE_TOO_LARGE | Message exceeds limit |
| `0x104` | RATE_LIMITED | Too many requests |

## Performance Targets

| Metric | Target |
|--------|--------|
| **Connection establishment** | < 1 RTT (0-RTT resumption) |
| **First message** | < 2 RTT (new connection) |
| **Throughput** | Limited by network, not protocol |
| **Concurrent connections** | 1000+ per node |
| **Memory per connection** | < 100 KB |

## Implementation Notes

### Recommended Libraries

| Language | Library |
|----------|---------|
| Rust | `quinn` or `quiche` |
| Go | `quic-go` |
| C/C++ | `quiche`, `msquic`, `ngtcp2` |

### Configuration Example (Rust/quinn)

```rust
let mut transport_config = TransportConfig::default();
transport_config.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
transport_config.initial_rtt(Duration::from_millis(100));

let mut server_config = ServerConfig::with_single_cert(certs, key)?;
server_config.transport = Arc::new(transport_config);
server_config.alpn_protocols = vec![b"post-urbit/1".to_vec()];
```

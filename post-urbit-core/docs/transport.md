# Post-Urbit Transport Layer

This document provides comprehensive developer documentation for the Post-Urbit transport layer, covering QUIC-based peer-to-peer communication, identity-authenticated handshakes, TLS configuration, NAT traversal, and connection management.

## Table of Contents

1. [QUIC Transport](#1-quic-transport)
2. [TLS 1.3 Configuration](#2-tls-13-configuration)
3. [Identity Handshake Protocol](#3-identity-handshake-protocol)
4. [Glare Resolution](#4-glare-resolution)
5. [NAT Traversal](#5-nat-traversal)
6. [Connection Lifecycle](#6-connection-lifecycle)
7. [Code Examples](#7-code-examples)

---

## 1. QUIC Transport

### 1.1 Why QUIC?

Post-Urbit uses QUIC (RFC 9000) as its transport protocol for several key advantages:

| Feature | Benefit |
|---------|---------|
| **Integrated TLS 1.3** | Encryption built into the protocol, not layered |
| **Multiplexed Streams** | Multiple concurrent streams without head-of-line blocking |
| **Connection Migration** | Seamlessly handle IP address changes |
| **0-RTT Resumption** | Fast reconnection to previously connected peers |
| **UDP-based** | Works through NAT, no TCP handshake overhead |

### 1.2 Connection Setup

The transport is implemented in `QuicTransport` (src/transport.rs, lines 18-49):

```rust
pub struct QuicTransport {
    endpoint: Endpoint,
    identity: Arc<IdentityManager>,
}
```

**Initialization Flow:**

1. Generate self-signed TLS certificate (ephemeral, not for identity verification)
2. Configure server and client TLS settings
3. Bind UDP socket to specified port
4. Configure QUIC transport parameters

### 1.3 Transport Parameters

The transport is configured with these parameters (src/transport.rs, lines 96-107):

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `max_idle_timeout` | 30 seconds | Close idle connections |
| `max_concurrent_bidi_streams` | 100 | Bidirectional stream limit |
| `max_concurrent_uni_streams` | 100 | Unidirectional stream limit |
| `initial_rtt` | 100ms | Conservative RTT estimate |
| `max_udp_payload_size` | 1200 bytes | Safe for most network paths |

### 1.4 Stream Types

Post-Urbit defines specific stream types (src/transport.rs, lines 335-347):

| Code | Name | Purpose |
|------|------|---------|
| `0x01` | Control | Handshake, keepalive, management |
| `0x02` | Identity | Identity document exchange |
| `0x03` | Message | Application messages (PUSE envelopes) |
| `0x04` | Sync | CRDT synchronization |
| `0x05` | Bulk | Reserved for v2 (large data transfers) |

Each stream starts with a 1-byte type identifier, followed by length-prefixed frames.

---

## 2. TLS 1.3 Configuration

### 2.1 Certificate Handling

Post-Urbit uses **ephemeral self-signed certificates** for TLS. This is intentional:

- TLS provides transport encryption and key exchange
- Identity authentication happens via the handshake protocol (Section 3)
- Certificates are regenerated on each daemon restart

**Server Configuration** (src/transport.rs, lines 51-69):

```rust
fn configure_server(cert_der: Vec<u8>, priv_key_der: Vec<u8>) -> Result<ServerConfig> {
    let mut tls_config = rustls::ServerConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .with_no_client_auth()
        .with_single_cert(vec![cert], priv_key)
    // ALPN: post-urbit/1
    tls_config.alpn_protocols = vec![b"post-urbit/1".to_vec()];
}
```

**Client Configuration** (src/transport.rs, lines 71-86):

The client uses `NoCertificateVerification` (lines 276-316) to accept any server certificate. This is secure because:

1. TLS still provides encryption
2. Identity is verified via the handshake protocol
3. TLS binding prevents MITM attacks

### 2.2 Cipher Suites

Implementations MUST support:

- `TLS_CHACHA20_POLY1305_SHA256` - Software-optimized
- `TLS_AES_128_GCM_SHA256` - Hardware AES-NI support

### 2.3 TLS Binding (Exporter)

The critical security mechanism binding identity to the TLS session (src/transport.rs, lines 258-274):

```rust
const TLS_EXPORTER_LABEL: &[u8] = b"post-urbit handshake binding";

pub fn extract_tls_binding(connection: &quinn::Connection) -> Result<[u8; 32]> {
    let mut output = [0u8; 32];
    connection
        .export_keying_material(&mut output, TLS_EXPORTER_LABEL, &[])
        .map_err(|_| PostUrbitError::Crypto("TLS exporter failed"))?;
    Ok(output)
}
```

This 32-byte value:

- Is derived from the TLS session keys using RFC 8446 section 7.5
- Is unique to each TLS connection
- Is included in all handshake signatures
- Prevents message transplanting attacks (MITM)

---

## 3. Identity Handshake Protocol

### 3.1 Overview

After QUIC establishes a TLS-encrypted connection, peers perform a mutual identity handshake to prove they control their claimed Identity Identifiers (IIDs). This binds the transport session to specific cryptographic identities.

### 3.2 Handshake Flow

```
    Client                                           Server
      |                                                |
      |  ========= QUIC + TLS 1.3 Handshake ========>  |
      |  <========================================     |
      |           (TLS session established)            |
      |                                                |
      |  [Extract TLS binding: 32 bytes]               |
      |                                                |
      |  +------------------------------------------+  |
      |  | ClientHello                              |  |
      |  | - version: 1                             |  |
      |  | - client_iid (32-char base32)            |  |
      |  | - client_nonce (32 bytes, base64)        |  |
      |  | - timestamp (RFC3339 UTC)                |  |
      |  | - tls_binding (32 bytes, base64)         |  |
      |  | - expected_server_iid (optional)         |  |
      |  +------------------------------------------+  |
      |  ============================================> |
      |                                                |
      |                     [Server validates ClientHello]
      |                     [Extracts TLS binding]
      |                                                |
      |  +------------------------------------------+  |
      |  | ServerHello                              |  |
      |  | - version: 1                             |  |
      |  | - server_iid (32-char base32)            |  |
      |  | - server_nonce (32 bytes, base64)        |  |
      |  | - timestamp (RFC3339 UTC)                |  |
      |  | - tls_binding (32 bytes, base64)         |  |
      |  | - identity_document (full IDOC)          |  |
      |  | - challenge_signature (64 bytes, base64) |  |
      |  +------------------------------------------+  |
      |  <============================================ |
      |                                                |
      |  [Client validates ServerHello]                |
      |  [Verifies challenge_signature]                |
      |  [Verifies identity_document]                  |
      |                                                |
      |  +------------------------------------------+  |
      |  | ClientAuth                               |  |
      |  | - version: 1                             |  |
      |  | - identity_document (full IDOC)          |  |
      |  | - challenge_signature (64 bytes, base64) |  |
      |  | - tls_binding (32 bytes, base64)         |  |
      |  +------------------------------------------+  |
      |  ============================================> |
      |                                                |
      |                     [Server verifies ClientAuth]
      |                     [Verifies challenge_signature]
      |                     [Verifies identity_document]
      |                                                |
      |  +------------------------------------------+  |
      |  | HandshakeComplete                        |  |
      |  | - version: 1                             |  |
      |  | - success: true                          |  |
      |  +------------------------------------------+  |
      |  <============================================ |
      |                                                |
      |    [Connection authenticated to IID pair]      |
```

### 3.3 Message Types

The handshake uses JSON messages defined in src/transport.rs (lines 393-463):

```rust
#[derive(Serialize, Deserialize)]
pub enum HandshakeMessage {
    ClientHello(ClientHello),
    ServerHello(ServerHello),
    ClientAuth(ClientAuth),
    HandshakeComplete(HandshakeComplete),
}
```

### 3.4 Challenge Signature Construction

The challenge signature proves identity ownership. The construction differs for server and client to prevent reflection attacks.

**Domain Separator**: `post-urbit-handshake-v1` (23 bytes)

**Server's Challenge** (signs first):

```
challenge_data = concat(
    "post-urbit-handshake-v1",   // 23 bytes domain
    client_nonce,                // 32 bytes
    server_nonce,                // 32 bytes
    tls_binding,                 // 32 bytes
    client_iid_raw,              // 20 bytes (decoded base32)
    server_iid_raw               // 20 bytes (decoded base32)
)
// Total: 159 bytes

signature = Ed25519_Sign(server_key, SHA256(challenge_data))
```

**Client's Challenge** (reversed nonce/IID order):

```
challenge_data = concat(
    "post-urbit-handshake-v1",   // 23 bytes domain
    server_nonce,                // 32 bytes (swapped)
    client_nonce,                // 32 bytes (swapped)
    tls_binding,                 // 32 bytes
    server_iid_raw,              // 20 bytes (swapped)
    client_iid_raw               // 20 bytes (swapped)
)
// Total: 159 bytes
```

Implementation in src/transport.rs, lines 918-951:

```rust
fn create_challenge_signature(
    signing_key: &ed25519_dalek::SigningKey,
    client_nonce: &[u8],
    server_nonce: &[u8],
    tls_binding: &[u8],
    client_iid: &str,
    server_iid: &str,
    is_server: bool,
) -> Result<String>
```

### 3.5 Handshake State Machine

The handshake follows a strict state machine (src/transport.rs, lines 478-559):

```
                 Client                          Server
                   |                               |
                 START                           START
                   |                               |
    [send ClientHello]                  [recv ClientHello]
                   |                               |
           ClientHelloSent              ClientHelloReceived
                   |                               |
    [recv ServerHello]                  [send ServerHello]
                   |                               |
          ServerHelloReceived            ServerHelloSent
                   |                               |
    [send ClientAuth]                   [recv ClientAuth]
                   |                               |
           ClientAuthSent               ClientAuthReceived
                   |                               |
    [recv HandshakeComplete]          [send HandshakeComplete]
                   |                               |
               COMPLETE                        COMPLETE
```

### 3.6 Timeouts

Defined in src/transport.rs (lines 466-476):

| Phase | Timeout |
|-------|---------|
| Awaiting ClientHello/ServerHello | 10 seconds |
| Awaiting ClientAuth/HandshakeComplete | 10 seconds |
| Total handshake | 30 seconds |

### 3.7 Verification Procedures

**Server Verifies Client:**

1. Check `client_iid` format (32-char lowercase Crockford Base32)
2. Check `timestamp` within +/-5 minutes
3. Verify `tls_binding` matches current session
4. If `expected_server_iid` provided, verify match
5. Validate client's identity document per RFC-0001
6. Verify `client_iid == derive_iid(genesis_key)`
7. Reconstruct challenge data
8. Verify `challenge_signature` using identity document's current signing key

**Client Verifies Server:**

Same steps, reversing roles.

---

## 4. Glare Resolution

### 4.1 The Problem

When two peers attempt to connect simultaneously, both connections may succeed. This "glare" condition must be resolved deterministically to ensure exactly one connection remains.

### 4.2 Detection

After handshake completion, each peer checks for existing connections to the same `(remote_iid, remote_did)` tuple. If a duplicate exists, glare has occurred.

### 4.3 Resolution Algorithm

Defined in src/transport.rs (lines 1439-1501):

```
Resolution Rule:
  Compare initiator (iid, did) tuples lexicographically.
  The connection initiated by the SMALLER tuple survives.
  Close the other connection with error 0x105 (DUPLICATE_CONNECTION).
```

**Tuple Comparison** (lines 1509-1525):

```rust
fn tuple_less_than(
    a: (&str, Option<&str>),  // (iid, did)
    b: (&str, Option<&str>),
) -> bool {
    // Compare IIDs first (lexicographic on base32 strings)
    if a.0 != b.0 {
        return a.0 < b.0;
    }
    // IIDs equal, compare DIDs
    // None sorts before any defined value
    match (a.1, b.1) {
        (None, None) => false,
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(a_did), Some(b_did)) => a_did < b_did,
    }
}
```

### 4.4 Example

```
Alice IID: a1b2c3... (smaller)
Bob IID:   b4c5d6... (larger)

1. Alice connects to Bob (Alice is initiator)
2. Bob connects to Alice (Bob is initiator)
3. Both handshakes complete
4. Glare detected

Resolution:
  - Alice's tuple < Bob's tuple (lexicographically)
  - Alice initiated -> that connection survives
  - Bob's connection closed with DUPLICATE_CONNECTION
```

### 4.5 Connection Tracker

The `ConnectionTracker` struct (lines 1370-1501) manages active connections:

```rust
pub struct ConnectionTracker {
    active: HashMap<(String, Option<String>), ConnectionInfo>,
}

impl ConnectionTracker {
    pub fn register(&mut self, remote_iid: &str, remote_did: Option<&str>, we_initiated: bool) -> bool;
    pub fn resolve_glare(&mut self, local_iid: &str, local_did: Option<&str>, ...) -> bool;
    pub fn remove(&mut self, remote_iid: &str, remote_did: Option<&str>);
}
```

---

## 5. NAT Traversal

### 5.1 STUN Discovery

Post-Urbit uses STUN (RFC 5389) for external address discovery. Implementation in src/nat.rs (lines 79-431).

**Default STUN Servers** (lines 93-97):

- `stun.l.google.com:19302`
- `stun1.l.google.com:19302`
- `stun.cloudflare.com:3478`

### 5.2 NAT Types

The system classifies NAT behavior (src/nat.rs, lines 29-44):

| NAT Type | Description | P2P Capability |
|----------|-------------|----------------|
| `None` | No NAT (public IP) | Full connectivity |
| `FullCone` | Most permissive | Direct connections work |
| `RestrictedCone` | Filter by IP | Requires coordination |
| `PortRestricted` | Filter by IP+port | Requires coordination |
| `Symmetric` | Most restrictive | Needs relay |

### 5.3 STUN Protocol Implementation

**Request Building** (lines 221-238):

```rust
pub fn build_stun_request() -> Vec<u8> {
    // 20-byte STUN Binding Request:
    // - Message Type: 0x0001 (Binding Request)
    // - Message Length: 0x0000 (no attributes)
    // - Magic Cookie: 0x2112A442
    // - Transaction ID: 12 random bytes
}
```

**Response Parsing** (lines 241-293):

```rust
pub fn parse_stun_response(response: &[u8]) -> Result<SocketAddr> {
    // Parse STUN Binding Response
    // Extract XOR-MAPPED-ADDRESS or MAPPED-ADDRESS attribute
    // XOR decode IP and port with magic cookie
}
```

### 5.4 NAT Type Detection

The `detect_nat_type` method (lines 359-411) queries multiple STUN servers:

```
1. Query first STUN server -> get external IP:port A
2. Compare external IP with local IP:
   - Same? No NAT detected
   - Different? Continue...
3. Query second STUN server -> get external IP:port B
4. Compare ports:
   - Same port? Cone NAT (full/restricted)
   - Different port? Symmetric NAT
```

### 5.5 External Address Caching

The `StunNatDiscovery` struct caches results (lines 72-77, 131-148):

- **Cache Duration**: 5 minutes (configurable)
- **Cache Invalidation**: Manual via `clear_cache()`
- **Stale Check**: Automatic on each lookup

### 5.6 Hole Punching Strategies

For NAT traversal beyond STUN:

| NAT Combination | Strategy |
|-----------------|----------|
| Full Cone + Any | Direct connection |
| Restricted + Restricted | Coordinated simultaneous open |
| Symmetric + Non-Symmetric | Relay required |
| Symmetric + Symmetric | Relay required |

When direct connectivity fails, peers use the PURL relay protocol (defined in RFC-0002 Section 7).

---

## 6. Connection Lifecycle

### 6.1 State Machine

```
                 +----------+
                 |   IDLE   |
                 +----+-----+
                      | connect()
                      v
                 +----------+
                 |CONNECTING| <-- QUIC handshake
                 +----+-----+
                      | TLS complete
                      v
                 +----------+
                 |HANDSHAKING| <-- Identity verification
                 +----+-----+
                      | identity verified
                      v
                 +----------+
                 | CONNECTED | <-- Application streams ready
                 +----+-----+
                      | timeout/error/close
                      v
                 +----------+
                 |  CLOSED  |
                 +----------+
```

### 6.2 Opening a Connection

**Client Side** (src/transport.rs, lines 212-250):

```rust
pub async fn connect_to_peer_secure(
    &self,
    address: std::net::SocketAddr,
    expected_server_iid: Option<&str>,
) -> Result<(quinn::Connection, HandshakeResult)> {
    // 1. QUIC connect
    let connection = self.endpoint.connect(address, "localhost")?.await?;

    // 2. Extract TLS binding
    let tls_binding = extract_tls_binding(&connection)?;

    // 3. Open control stream
    let (mut send, mut recv) = connection.open_bi().await?;

    // 4. Execute handshake
    let result = execute_client_handshake(&mut send, &mut recv, ...).await?;

    Ok((connection, result))
}
```

### 6.3 Accepting Connections

**Server Side** (src/transport.rs, lines 109-144):

```rust
pub async fn run(self: Arc<Self>) -> Result<()> {
    while let Some(conn) = self.endpoint.accept().await {
        tokio::spawn(async move {
            if let Ok(connection) = conn.await {
                // Extract TLS binding
                let tls_binding = extract_tls_binding(&connection)?;

                // Accept control stream
                let (mut send, mut recv) = connection.accept_bi().await?;

                // Execute server handshake
                let result = execute_server_handshake(&mut send, &mut recv, ...).await?;

                // Connection authenticated - ready for application streams
            }
        });
    }
}
```

### 6.4 Maintaining Connections

- **Idle Timeout**: 30 seconds of no activity
- **Keep-Alive**: QUIC handles at transport level
- **Stream Limits**: 100 concurrent bidirectional, 100 unidirectional

### 6.5 Connection Migration

QUIC handles IP address changes transparently:

1. NAT rebinding detection
2. Path validation
3. Seamless continuation

### 6.6 Closing Connections

Connections close when:

- Idle timeout expires (30s)
- Either peer calls `close()`
- Fatal error occurs
- Glare resolution (code `0x105`)
- Handshake failure (code `0x101`)

---

## 7. Code Examples

### 7.1 Creating a Transport and Listening

```rust
use post_urbit::transport::QuicTransport;
use post_urbit::identity::IdentityManager;
use std::sync::Arc;

async fn setup_transport() -> Result<Arc<QuicTransport>> {
    // Create identity manager
    let identity = Arc::new(IdentityManager::new("./data").await?);

    // Create transport on port 4433
    let transport = Arc::new(QuicTransport::new(4433, identity).await?);

    // Start listening (spawns accept loop)
    let t = transport.clone();
    tokio::spawn(async move {
        t.run().await.expect("transport error");
    });

    Ok(transport)
}
```

### 7.2 Connecting to a Peer

```rust
use std::net::SocketAddr;

async fn connect_to_peer(
    transport: &QuicTransport,
    peer_address: &str,
    expected_iid: Option<&str>,
) -> Result<()> {
    let addr: SocketAddr = peer_address.parse()?;

    // Connect with identity verification
    let (connection, handshake_result) = transport
        .connect_to_peer_secure(addr, expected_iid)
        .await?;

    println!("Connected to peer: {}", handshake_result.peer_iid);

    // Open application streams
    let (mut send, mut recv) = connection.open_bi().await?;

    // Write stream type (Message = 0x03)
    send.write_all(&[0x03]).await?;

    // Send/receive application data...

    Ok(())
}
```

### 7.3 Handling Incoming Connections

```rust
use quinn::Connection;
use post_urbit::transport::{HandshakeResult, execute_server_handshake, extract_tls_binding};

async fn handle_connection(
    connection: Connection,
    identity: Arc<IdentityManager>,
) -> Result<HandshakeResult> {
    // Extract TLS binding first (before any streams)
    let tls_binding = extract_tls_binding(&connection)?;

    // Accept the control stream
    let (mut send, mut recv) = connection.accept_bi().await?;

    // Perform handshake
    let result = execute_server_handshake(
        &mut send,
        &mut recv,
        &identity,
        tls_binding,
    ).await?;

    println!("Authenticated peer: {}", result.peer_iid);

    // Handle additional streams
    while let Ok((send, recv)) = connection.accept_bi().await {
        tokio::spawn(handle_stream(send, recv));
    }

    Ok(result)
}
```

### 7.4 NAT Discovery

```rust
use post_urbit::nat::{StunNatDiscovery, NATDiscovery, NATType};

async fn discover_nat() -> Result<()> {
    // Create discovery with local port 4433
    let discovery = StunNatDiscovery::new(4433)
        .with_cache_duration(Duration::from_secs(300))
        .with_timeout(Duration::from_secs(5));

    // Discover external address
    if let Some(external) = discovery.discover_external_address().await? {
        println!("External address: {}", external);
    }

    // Detect NAT type
    let nat_type = discovery.detect_nat_type().await?;
    println!("NAT type: {}", nat_type);

    match nat_type {
        NATType::None => println!("No NAT - full connectivity"),
        NATType::FullCone => println!("Full cone - direct connections should work"),
        NATType::Symmetric => println!("Symmetric NAT - may need relay"),
        _ => println!("Restricted NAT - coordination required"),
    }

    Ok(())
}
```

### 7.5 Glare Resolution

```rust
use post_urbit::transport::ConnectionTracker;

fn setup_connection_tracking() {
    let mut tracker = ConnectionTracker::new();

    // Register outgoing connection
    if tracker.register(peer_iid, peer_did.as_deref(), true) {
        // Connection registered successfully
    } else {
        // Glare detected - existing connection to this peer
        let keep = tracker.resolve_glare(
            &my_iid,
            my_did.as_deref(),
            peer_iid,
            peer_did.as_deref(),
        );

        if keep {
            // This connection wins, close existing
        } else {
            // Existing connection wins, close this one
        }
    }
}
```

### 7.6 Frame Reading/Writing

```rust
use post_urbit::transport::{write_frame, read_frame, write_stream_type, read_stream_type};
use post_urbit::transport::{STREAM_MESSAGE, HANDSHAKE_MAX_MESSAGE_SIZE};

// Writing frames (synchronous)
fn write_message(writer: &mut impl Write, payload: &[u8]) -> Result<()> {
    write_stream_type(writer, STREAM_MESSAGE)?;
    write_frame(writer, payload)?;
    Ok(())
}

// Reading frames (synchronous)
fn read_message(reader: &mut impl Read) -> Result<Vec<u8>> {
    let stream_type = read_stream_type(reader)?;
    let payload = read_frame(reader, HANDSHAKE_MAX_MESSAGE_SIZE)?;
    Ok(payload)
}
```

---

## Key Source File References

| File | Lines | Description |
|------|-------|-------------|
| `src/transport.rs` | 18-49 | `QuicTransport` struct and initialization |
| `src/transport.rs` | 51-107 | TLS and QUIC configuration |
| `src/transport.rs` | 146-177 | Server connection handling |
| `src/transport.rs` | 212-250 | Client connection with handshake |
| `src/transport.rs` | 258-274 | TLS binding extraction |
| `src/transport.rs` | 393-463 | Handshake message types |
| `src/transport.rs` | 478-559 | Handshake state machine |
| `src/transport.rs` | 578-646 | Challenge signature verification |
| `src/transport.rs` | 918-951 | Challenge signature creation |
| `src/transport.rs` | 994-1131 | Client handshake execution |
| `src/transport.rs` | 1144-1313 | Server handshake execution |
| `src/transport.rs` | 1356-1525 | Glare resolution |
| `src/nat.rs` | 29-44 | NAT type enum |
| `src/nat.rs` | 59-70 | NATDiscovery trait |
| `src/nat.rs` | 79-116 | StunNatDiscovery struct |
| `src/nat.rs` | 157-218 | STUN query implementation |
| `src/nat.rs` | 221-293 | STUN request/response parsing |
| `src/nat.rs` | 359-411 | NAT type detection |

---

## Error Codes Reference

| Code | Name | Description |
|------|------|-------------|
| `0x100` | IDENTITY_MISMATCH | Peer IID does not match expected |
| `0x101` | HANDSHAKE_FAILED | Identity handshake failed |
| `0x102` | STREAM_TYPE_UNKNOWN | Unknown stream type byte |
| `0x103` | MESSAGE_TOO_LARGE | Message exceeds stream limit |
| `0x104` | RATE_LIMITED | Too many requests |
| `0x105` | DUPLICATE_CONNECTION | Glare resolution |
| `0x106` | REVOKED_IDENTITY | Peer's identity is revoked |
| `0x107` | REVOKED_KEY | Peer's signing key is revoked |
| `0x108` | DUPLICATE_STREAM_TYPE | Second stream of single-instance type |

---

## Security Considerations

1. **TLS Certificates**: Not used for identity - only for transport encryption
2. **TLS Binding**: Cryptographically binds identity to specific TLS session
3. **Challenge Signatures**: Prove identity ownership, prevent replay/reflection
4. **Timestamps**: 5-minute validity window prevents replay attacks
5. **Nonces**: 32-byte random values ensure signature freshness
6. **Glare Resolution**: Deterministic algorithm prevents connection conflicts

For the complete protocol specification, see RFC-0002 at `spec/06-rfcs/RFC-0002-transport.md`.

# Transport Interfaces

## Overview

This document specifies the complete API surface for the Transport & Connectivity layer.

## Core Types

```typescript
// Identity types from 02-identity-trust
type IdentityIdentifier = string;  // 32-char Base32 (IID)
type DeviceIdentifier = string;    // 32-char Base32 (DID)
type PublicKey = string;           // Base64
type Signature = string;           // Base64
type Timestamp = string;           // RFC3339
type SequenceNumber = string;      // Decimal uint64 string

// Transport-specific types
type ConnectionId = string;        // Unique connection identifier
type StreamId = number;            // QUIC stream ID

// Peer identification (identity + optional device)
interface PeerId {
  iid: IdentityIdentifier;         // Required: which identity
  did?: DeviceIdentifier;          // Optional: which device (if multi-device)
}

// For anonymous connections (relays, discovery, DHT)
type MaybePeerId = PeerId | null;
```

## Connection Types

```typescript
// Connection path types
type PathType = 'direct' | 'relay' | 'hole-punched';

interface ConnectionPath {
  type: PathType;
  localAddress: string;
  localPort: number;
  remoteAddress: string;
  remotePort: number;
  relay?: RelayInfo;               // Present if type == 'relay'
  latency?: number;                // Measured RTT in ms
}

interface RelayInfo {
  relayId: string;
  address: string;
  port: number;
  allocationId: string;
  expiresAt: Timestamp;
}

// Connection state (see quic-integration.md for state machine)
type ConnectionState =
  | 'connecting'
  | 'handshaking'
  | 'connected'    // Fully authenticated, ready for application use (CONNECTED in QUIC diagram)
  | 'closed';

interface Connection {
  id: ConnectionId;
  peerId: MaybePeerId;              // null for anonymous connections (relay/discovery)
  state: ConnectionState;
  path: ConnectionPath;
  establishedAt: Timestamp;
  authenticatedAt?: Timestamp;
  peerDocument?: IdentityDocument;  // Populated after handshake
  peerDeviceDocument?: DeviceDocument;  // Populated if device is specified
  streams: StreamInfo[];
}

// Device document type (see 02-identity-trust/identity-document-schema.md § Device Identifiers)
// Wire format uses snake_case (device_signing_key, created_at, etc.)
// TypeScript uses camelCase per language convention
interface DeviceDocument {
  version: number;
  did: DeviceIdentifier;
  iid: IdentityIdentifier;
  deviceName?: string;
  deviceSigningKey: PublicKey;
  endpoints: Endpoint[];  // Required for device discovery
  createdAt: Timestamp;
  updatedAt: Timestamp;   // Required for DHT conflict resolution (RFC-0001 §12.5)
  expiresAt?: Timestamp;
  capabilities: string[];
  signatureByIdentity: Signature;
}

// Endpoint type (normative definition, shared between Identity and Transport layers)
// Wire format uses snake_case; TypeScript uses camelCase
interface Endpoint {
  type: 'direct' | 'relay' | 'mailbox';
  host: string;                       // Hostname, IPv4, or [IPv6]
  port: number;                       // Service port (1-65535). UDP for quic, TCP for https.
  priority: number;                   // REQUIRED: 0-255, lower = higher priority
  transport?: 'quic' | 'https';       // Default: quic. Determines port protocol (UDP/TCP).
  relayId?: IdentityIdentifier;       // For relay endpoints (relay's IID)
  observedAt?: Timestamp;             // When this endpoint was last verified
  metadata?: Record<string, string>;
}

interface StreamInfo {
  id: StreamId;
  type: StreamType;
  kind: 'unidirectional' | 'bidirectional';
  initiator: 'local' | 'remote';
  bytesRead: number;
  bytesWritten: number;
}

// Note: 'bulk' (0x05) is reserved for v2 and MUST NOT be used by v1 implementations (see RFC-0002 §6.6)
type StreamType = 'control' | 'identity' | 'message' | 'sync' | 'bulk';
```

## Transport Service Interface

```typescript
interface TransportService {
  // === Connection Management ===

  /**
   * Connect to a peer by IID.
   * Resolves endpoints from identity document and attempts connection.
   *
   * **v1 Connectivity Model:**
   * - When connecting to an EXTERNAL peer (different identity), implementations
   *   SHOULD resolve endpoints from the peer's Identity Document, NOT from
   *   device-specific documents. The `did` field in PeerId is ignored for
   *   external peer connections in v1.
   * - When connecting to your OWN identity's devices (intra-identity), the
   *   `did` field MAY be used to connect to a specific device endpoint.
   * - External peers connect to the home node (Identity Document endpoints);
   *   the home node handles device fanout internally.
   *
   * See identity-document-schema.md "Single Home Node Model" for rationale.
   *
   * @param peerId Peer's identity (and optionally device) identifier
   * @param options Connection options
   * @returns Authenticated connection
   */
  connect(
    peerId: PeerId | IdentityIdentifier,  // Can pass just IID for convenience
    options?: ConnectOptions
  ): Promise<Connection>;

  /**
   * Disconnect from a peer.
   * @param connectionId Connection to close
   * @param reason Optional close reason
   */
  disconnect(connectionId: ConnectionId, reason?: string): Promise<void>;

  /**
   * Get active connection to a peer (if any).
   * If did is not specified and peer has multiple devices, returns any active connection.
   */
  getConnection(peerId: PeerId | IdentityIdentifier): Connection | null;

  /**
   * List all active connections.
   */
  listConnections(): Connection[];

  // === Listening ===

  /**
   * Start listening for incoming connections.
   * @param options Listening options
   */
  listen(options?: ListenOptions): Promise<void>;

  /**
   * Stop listening for incoming connections.
   */
  stopListening(): Promise<void>;

  /**
   * Check if currently listening.
   */
  isListening(): boolean;

  // === Streaming ===

  /**
   * Open a new stream on an existing connection.
   * @param connectionId Target connection
   * @param type Stream type
   * @returns Stream handle
   */
  openStream(
    connectionId: ConnectionId,
    type: StreamType
  ): Promise<Stream>;

  /**
   * Accept an incoming stream.
   * @param connectionId Source connection
   * @param streamId Stream to accept
   * @returns Stream handle
   */
  acceptStream(connectionId: ConnectionId, streamId: StreamId): Promise<Stream>;

  // === NAT Traversal ===

  /**
   * Discover public address using STUN-like servers.
   * @returns Discovered candidates
   */
  discoverAddresses(): Promise<AddressCandidate[]>;

  /**
   * Attempt port mapping via UPnP/NAT-PMP.
   * @returns Port mapping if successful
   */
  createPortMapping(): Promise<PortMapping | null>;

  /**
   * Get current NAT type assessment.
   */
  getNatType(): Promise<NatType>;

  // === Relay Management ===

  /**
   * Allocate a relay address.
   * @param relay Relay server to use
   * @returns Allocation details
   */
  allocateRelay(relay: RelayServer): Promise<RelayAllocation>;

  /**
   * Release a relay allocation.
   * @param allocationId Allocation to release
   */
  releaseRelay(allocationId: string): Promise<void>;

  /**
   * List active relay allocations.
   */
  listRelayAllocations(): RelayAllocation[];

  // === Events ===

  /** Emitted when a new connection is established */
  onConnectionEstablished: Event<{ connection: Connection }>;

  /** Emitted when a connection is closed */
  onConnectionClosed: Event<{ connectionId: ConnectionId; reason: string }>;

  /** Emitted when a new stream is opened by peer */
  onStreamOpened: Event<{ connectionId: ConnectionId; stream: Stream }>;

  /** Emitted when connection path changes (e.g., direct → relay) */
  onPathChanged: Event<{ connectionId: ConnectionId; oldPath: ConnectionPath; newPath: ConnectionPath }>;

  /** Emitted when NAT type changes */
  onNatTypeChanged: Event<{ oldType: NatType; newType: NatType }>;

  /** Emitted when incoming connection attempt is received (before auth) */
  onIncomingConnection: Event<{
    provisionalId: string;
    remoteAddress: string;
    remotePort: number;
  }>;

  /**
   * Emitted when session ticket is issued (for 0-RTT resumption).
   * Reserved for TLS-layer session resumption only. MUST NOT carry Post-Urbit
   * protocol bytes in 0-RTT early data. Application-level handshake resumption
   * is not supported in v1. See RFC-0002 §8.3-§8.4.
   */
  onSessionTicket: Event<{
    peerId: IdentityIdentifier;
    ticket: Uint8Array;
    issuedAt: Timestamp;
    expiresAt: Timestamp;
  }>;

  /** Emitted when relay allocation is refreshed */
  onRelayAllocationRefreshed: Event<{
    allocationId: string;
    newExpiresAt: Timestamp;
  }>;
}

// Connection deduplication rules (see RFC-0002 §9.2 for error code registry)
const CONNECTION_DEDUP = {
  /**
   * Connections are unique per (local_iid, local_did, peer_iid, peer_did) tuple.
   *
   * When two peers/devices connect to each other simultaneously (glare):
   * - Compare (iid, did) tuples lexicographically
   * - Keep the connection initiated by the smaller tuple
   * - Close the other connection with code DUPLICATE_CONNECTION (0x105)
   *
   * Comparison: (iid1, did1) < (iid2, did2) iff:
   *   iid1 < iid2, or (iid1 == iid2 and did1 < did2)
   * Note: did may be undefined; undefined sorts before any defined value.
   */
  GLARE_RESOLUTION: 'smaller_iid_did_tuple_initiator_wins',
  DUPLICATE_CONNECTION_CODE: 0x105,  // RFC-0002 §9.2: DUPLICATE_CONNECTION
} as const;
```

## Connection Options

```typescript
interface ConnectOptions {
  // Timeout for connection establishment (default: 30s)
  timeout?: number;

  // Preferred path types (try in order)
  preferredPaths?: PathType[];

  // Whether to try hole punching (default: true)
  attemptHolePunch?: boolean;

  // Maximum hole punch attempts (default: 5)
  maxHolePunchAttempts?: number;

  // Relay servers to use as fallback
  relayServers?: RelayServer[];

  // Expected identity document sequence (for resumption)
  expectedSequence?: SequenceNumber;

  // Session ticket for 0-RTT resumption
  // Reserved for TLS-layer session resumption only. MUST NOT carry Post-Urbit
  // protocol bytes in 0-RTT early data. Application-level handshake resumption
  // is not supported in v1. See RFC-0002 §8.3-§8.4.
  sessionTicket?: Uint8Array;
}

interface ListenOptions {
  // UDP port(s) to listen on
  ports?: number[];

  // Network interfaces to bind
  interfaces?: string[];

  // Whether to create port mappings (default: true)
  enablePortMapping?: boolean;

  // Whether to allocate relays (default: true)
  enableRelay?: boolean;

  // Maximum concurrent connections (default: 1000)
  maxConnections?: number;

  // Connection rate limit (per minute, default: 100)
  connectionRateLimit?: number;
}
```

## Stream Interface

```typescript
interface Stream {
  id: StreamId;
  connectionId: ConnectionId;
  type: StreamType;
  direction: 'inbound' | 'outbound' | 'bidirectional';

  /**
   * Read data from stream.
   * @param maxBytes Maximum bytes to read
   * @returns Data read, or null if stream ended
   */
  read(maxBytes?: number): Promise<Uint8Array | null>;

  /**
   * Write data to stream.
   * @param data Data to write
   */
  write(data: Uint8Array): Promise<void>;

  /**
   * Close the stream gracefully.
   */
  close(): Promise<void>;

  /**
   * Abort the stream with error.
   * @param errorCode Application error code
   */
  abort(errorCode: number): Promise<void>;

  /**
   * Check if stream is readable.
   */
  isReadable(): boolean;

  /**
   * Check if stream is writable.
   */
  isWritable(): boolean;

  /** Emitted when data is available */
  onData: Event<{ data: Uint8Array }>;

  /** Emitted when stream is closed */
  onClose: Event<{ graceful: boolean; errorCode?: number }>;
}
```

## Address Discovery Types

```typescript
type NatType =
  | 'open'             // No NAT, direct IP
  | 'full-cone'        // Easy to traverse
  | 'restricted-cone'  // Medium
  | 'port-restricted'  // Medium
  | 'symmetric'        // Hard, relay needed
  | 'unknown';         // Detection failed

interface AddressCandidate {
  type: 'host' | 'srflx' | 'mapped' | 'relay';
  address: string;
  port: number;
  priority: number;
  foundation: string;
  relayServer?: string;
}

interface PortMapping {
  protocol: 'upnp' | 'nat-pmp' | 'pcp';
  externalAddress: string;
  externalPort: number;
  internalPort: number;
  lifetime: number;
  createdAt: Timestamp;
  expiresAt: Timestamp;
}

interface RelayServer {
  id: string;
  address: string;
  port: number;
  publicKey?: PublicKey;
}

interface RelayAllocation {
  allocationId: string;
  relay: RelayServer;
  // NOTE: binding is established on first UDP packet, NOT at HTTPS allocation time (RFC-0002 §7.10)
  bindingState: 'pending' | 'bound';
  // boundAddress/boundPort are only available after first UDP packet establishes binding
  boundAddress?: string;    // Client's observed public IP (set when bindingState == 'bound')
  boundPort?: number;       // Client's observed public port (set when bindingState == 'bound')
  expiresAt: Timestamp;
  token: string;            // Base64url, 16 bytes
}
```

## Discovery Service Interface

```typescript
interface DiscoveryService {
  /**
   * Query discovery servers to find public address.
   * @param servers Discovery servers to query
   * @returns Observed addresses
   */
  discoverPublicAddress(
    servers?: DiscoveryServer[]
  ): Promise<DiscoveryResult[]>;

  /**
   * Detect NAT type by comparing results from multiple servers.
   */
  detectNatType(): Promise<NatType>;

  /**
   * Register identity with DHT for peer discovery.
   * @param document Identity document with endpoints
   */
  registerIdentity(document: IdentityDocument): Promise<void>;

  /**
   * Look up a peer's endpoints from DHT.
   * @param iid Peer's identity identifier
   * @returns Known endpoints
   */
  lookupPeer(iid: IdentityIdentifier): Promise<PeerEndpoints | null>;

  /**
   * Fetch full identity document from DHT.
   * Required by Identity layer for verification, recovery, and caching.
   * @param iid Peer's identity identifier
   * @returns Full IdentityDocument or null if not found
   */
  fetchIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null>;

  /**
   * Low-level DHT get (returns multiple values for conflict resolution).
   * Keys are 32 raw bytes (SHA256 output).
   * @param key DHT key (32 bytes)
   * @returns All values from reachable nodes
   */
  dhtGet(key: Uint8Array): Promise<DhtResult[]>;

  /**
   * Low-level DHT put.
   * @param key DHT key (32 bytes)
   * @param value Value to store (typically IDOC envelope)
   * @param options TTL in seconds
   */
  dhtPut(key: Uint8Array, value: Uint8Array, options: { ttl: number }): Promise<void>;

  /**
   * Subscribe to peer endpoint updates.
   * @param iid Peer to watch
   * @param callback Called when endpoints change
   */
  watchPeer(
    iid: IdentityIdentifier,
    callback: (endpoints: PeerEndpoints) => void
  ): Unsubscribe;
}

// DhtResult is defined in spec/00-shared/layer-integration.md (authoritative)
// Duplicated here for Transport-layer completeness; MUST match shared definition
interface DhtResult {
  value: Uint8Array;           // Raw value bytes (e.g., IDOC envelope)
  source: string;              // Source node identifier
  receivedAt: number;          // Unix timestamp in milliseconds
}

interface DiscoveryServer {
  address: string;
  port: number;
}

interface DiscoveryResult {
  server: DiscoveryServer;
  observedAddress: string;
  observedPort: number;
  latency: number;
}

interface PeerEndpoints {
  iid: IdentityIdentifier;
  endpoints: Endpoint[];
  lastUpdated: Timestamp;
  sequence: SequenceNumber;  // Decimal string, NOT number (uint64 safety)
}

type Unsubscribe = () => void;
```

## Hole Punching Interface

```typescript
interface HolePunchService {
  /**
   * Initiate hole punching to a peer.
   * @param peerId Target peer
   * @param coordinator Coordination channel (relay or mutual peer)
   * @returns Direct connection if successful
   */
  punchThrough(
    peerId: IdentityIdentifier,
    coordinator: ConnectionId
  ): Promise<Connection | null>;

  /**
   * Accept a hole punch request from a peer.
   * @param request Incoming request
   * @returns Direct connection if successful
   */
  acceptPunch(request: HolePunchRequest): Promise<Connection | null>;

  /** Emitted when a hole punch request is received */
  onPunchRequest: Event<{ request: HolePunchRequest }>;
}

interface HolePunchRequest {
  transactionId: string;
  initiatorIid: IdentityIdentifier;
  initiatorCandidates: AddressCandidate[];
  timestamp: Timestamp;
}
```

## Error Types

```typescript
class TransportError extends Error {
  constructor(
    public code: TransportErrorCode,
    message: string,
    public details?: Record<string, unknown>
  ) {
    super(message);
  }
}

type TransportErrorCode =
  // Connection errors
  | 'CONNECTION_FAILED'
  | 'CONNECTION_TIMEOUT'
  | 'CONNECTION_REFUSED'
  | 'CONNECTION_CLOSED'
  | 'PEER_UNREACHABLE'
  // Handshake errors
  | 'HANDSHAKE_FAILED'
  | 'IDENTITY_MISMATCH'
  | 'SIGNATURE_INVALID'
  | 'TIMESTAMP_EXPIRED'
  // Stream errors
  | 'STREAM_LIMIT_EXCEEDED'
  | 'STREAM_CLOSED'
  | 'MESSAGE_TOO_LARGE'
  // Relay errors
  | 'RELAY_UNAVAILABLE'
  | 'ALLOCATION_FAILED'
  | 'ALLOCATION_EXPIRED'
  | 'RATE_LIMITED'
  // NAT errors
  | 'NAT_DETECTION_FAILED'
  | 'HOLE_PUNCH_FAILED'
  | 'PORT_MAPPING_FAILED';
```

## Constants

```typescript
const TRANSPORT_CONSTANTS = {
  // QUIC configuration
  ALPN_PROTOCOL: 'post-urbit/1',
  MAX_IDLE_TIMEOUT_MS: 30000,
  MAX_STREAMS_BIDI: 100,
  MAX_STREAMS_UNI: 100,
  MAX_UDP_PAYLOAD: 1200,

  // Handshake
  HANDSHAKE_TIMEOUT_MS: 30000,
  TIMESTAMP_SKEW_SECONDS: 300,  // ±5 minutes
  MAX_HANDSHAKE_MESSAGE_SIZE: 65536,

  // Connection
  DEFAULT_CONNECT_TIMEOUT_MS: 30000,
  MAX_CONNECTIONS_PER_NODE: 1000,
  CONNECTION_RATE_LIMIT_PER_MINUTE: 100,

  // NAT traversal
  DISCOVERY_TIMEOUT_MS: 5000,
  HOLE_PUNCH_TIMEOUT_MS: 5000,
  HOLE_PUNCH_PROBE_COUNT: 5,
  PORT_MAPPING_LIFETIME_SECONDS: 3600,

  // Relay
  RELAY_ALLOCATION_LIFETIME_SECONDS: 3600,
  MAX_RELAY_ALLOCATIONS: 5,
  // PURL header is 44 bytes (RFC-0002 §7.4)
  PURL_HEADER_SIZE: 44,
  // Relay path: inner QUIC payload uses standard 1200 for QUIC Initial compatibility
  RELAY_MAX_INNER_PAYLOAD: 1200,
  // Total outer UDP datagram: 1244 bytes (may exceed IPv6 min MTU; see RFC-0002 §7.4)
  RELAY_PACKET_MAX_SIZE: 1244,

  // Streams
  // Note: BULK (0x05) is reserved for v2 and MUST NOT be used by v1 implementations (see RFC-0002 §6.6)
  STREAM_TYPES: {
    CONTROL: 0x01,
    IDENTITY: 0x02,
    MESSAGE: 0x03,
    SYNC: 0x04,
    BULK: 0x05,  // Reserved for v2 - MUST NOT be used in v1
  },
} as const;
```

## Integration with Identity Layer

The transport layer uses the identity layer for:

```typescript
// Required from identity layer
interface IdentityIntegration {
  // Get own identity document
  getSelfIdentity(): Promise<IdentityDocument>;

  // Get peer's identity document (from cache or network)
  getIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null>;

  // Sign data for handshake
  sign(data: Uint8Array): Promise<Signature>;

  // Verify peer's signature
  verify(
    iid: IdentityIdentifier,
    data: Uint8Array,
    signature: Signature
  ): Promise<boolean>;

  // Verify identity document
  verifyDocument(doc: IdentityDocument): VerificationResult;
}
```

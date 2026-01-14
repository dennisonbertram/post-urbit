# Identity & Trust Interfaces

## Overview

This document specifies the complete API surface for the Identity & Trust layer. These interfaces are used by other layers (Transport, Messaging, Apps) and by applications.

## Wire Format vs TypeScript Convention

| Context | Convention | Example |
|---------|------------|---------|
| **On-wire JSON** | snake_case | `recovery_proof`, `initiated_at`, `signing_key` |
| **TypeScript interfaces** | camelCase | `recoveryProof`, `initiatedAt`, `signingKey` |

**Normative rule**: The on-wire JSON format uses snake_case (as shown in identity-document-schema.md). TypeScript interfaces use camelCase for developer ergonomics. Implementations MUST:
1. Serialize to snake_case when sending over the wire
2. Deserialize from snake_case when receiving
3. Use JCS (JSON Canonicalization Scheme) on the snake_case wire format for signature verification

**Field mapping** (TypeScript → Wire):
- `recoveryProof` → `recovery_proof`
- `initiatedAt` → `initiated_at`
- `cooldownExpiresAt` → `cooldown_expires_at`
- `proofData` → `proof_data`
- `validFrom` → `valid_from`
- `validUntil` → `valid_until`
- `expiresAt` → `expires_at`

## Core Types

```typescript
// Base32-encoded, 32 characters, derived from genesis signing key
type IdentityIdentifier = string;

// Base64-encoded public keys
type PublicKey = string;
type PrivateKey = string; // Never exposed outside secure storage

// Base64-encoded signatures
type Signature = string;

// RFC3339 timestamp
type Timestamp = string;

// Monotonically increasing version number
// On-wire: decimal string to support uint64 safely (JSON numbers lose precision >2^53)
// In TypeScript: use string or bigint, never number
type SequenceNumber = string;  // Decimal string, e.g., "0", "42", "18446744073709551614"
```

## Identity Document Types

```typescript
interface IdentityDocument {
  version: 1;
  iid: IdentityIdentifier;
  sequence: SequenceNumber;
  timestamp: Timestamp;
  keys: KeySet;
  endpoints: Endpoint[];
  recovery: RecoveryConfig;
  claims: Claims;
  extensions: Record<string, unknown>;
  signatures: SignatureSet;
  // Present only when update was authorized via recovery (not key continuity)
  recoveryProof?: RecoveryProofEmbed;
}

// Recovery proof embedded in identity document
interface RecoveryProofEmbed {
  method: 'social' | 'device-escrow' | 'threshold' | 'provider';
  initiatedAt: Timestamp;
  cooldownExpiresAt: Timestamp;
  status: 'pending' | 'active' | 'contested';
  proofData: SocialRecoveryProof | DeviceEscrowProof | ThresholdProof | ProviderProof;
}

interface SocialRecoveryProof {
  attestations: RecoveryAttestation[];
}

interface DeviceEscrowProof {
  escrowSignature: Signature;
}

interface ThresholdProof {
  reconstructedSignature: Signature;
}

interface ProviderProof {
  providerIid: IdentityIdentifier;
  providerAttestation: Signature;
  verificationMethod: string;
}

interface KeySet {
  signing: {
    genesis: PublicKey;           // IMMUTABLE - IID derived from this, never changes
    current: PublicKey;
    previous: PublicKey | null;
  };
  encryption: {
    current: PublicKey;
    // Support multiple previous keys for offline peers
    previous: EncryptionKeyHistory[];
  };
}

// Previous encryption keys with validity windows
interface EncryptionKeyHistory {
  key: PublicKey;
  validFrom: SequenceNumber;      // Sequence when this key became current
  validUntil: SequenceNumber;     // Sequence when this key was rotated out
  expiresAt: Timestamp;           // After this time, senders should not use this key
}

// Normative definition in identity-document-schema.md
interface Endpoint {
  type: 'direct' | 'relay' | 'mailbox';
  host: string;                       // Hostname, IPv4, or [IPv6]
  port: number;                       // UDP port (1-65535)
  priority: number;                   // 0-255, lower = higher priority
  transport?: 'quic' | 'https';       // Default: quic
  relayId?: IdentityIdentifier;       // For relay endpoints
  observedAt?: Timestamp;             // When this endpoint was last verified
  metadata?: Record<string, string>;
}

interface RecoveryConfig {
  method: 'none' | 'social' | 'device-escrow' | 'threshold' | 'provider';
  config: SocialRecoveryConfig | DeviceEscrowConfig | ThresholdConfig | ProviderConfig | {};
}

interface Claims {
  name?: string;      // Max 64 chars
  avatar?: string;    // Content hash
  bio?: string;       // Max 256 chars
}

interface SignatureSet {
  current: Signature;
  previous: Signature | null;
}
```

## Identity Service Interface

The primary interface for identity operations.

```typescript
interface IdentityService {
  // === Identity Creation ===

  /**
   * Generate a new identity with fresh keys.
   * @returns New identity document and private keys (keys must be stored securely)
   */
  createIdentity(options?: CreateIdentityOptions): Promise<CreateIdentityResult>;

  // === Identity Retrieval ===

  /**
   * Get the current user's identity document.
   */
  getSelfIdentity(): Promise<IdentityDocument>;

  /**
   * Get a peer's identity document by IID.
   * Fetches from local cache, then network if needed.
   */
  getIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null>;

  /**
   * Fetch latest identity document from network, bypassing cache.
   */
  refreshIdentity(iid: IdentityIdentifier): Promise<IdentityDocument | null>;

  // === Identity Updates ===

  /**
   * Update claims (name, avatar, bio) in identity document.
   */
  updateClaims(claims: Partial<Claims>): Promise<IdentityDocument>;

  /**
   * Update endpoints in identity document.
   */
  updateEndpoints(endpoints: Endpoint[]): Promise<IdentityDocument>;

  /**
   * Update recovery configuration.
   */
  updateRecovery(config: RecoveryConfig): Promise<IdentityDocument>;

  // === Key Operations ===

  /**
   * Rotate signing key, encryption key, or both.
   */
  rotateKeys(options: RotateKeysOptions): Promise<RotateKeysResult>;

  /**
   * Revoke a compromised key and replace it.
   */
  revokeKey(options: RevokeKeyOptions): Promise<RevokeKeyResult>;

  /**
   * Permanently revoke the entire identity.
   */
  revokeIdentity(options: RevokeIdentityOptions): Promise<void>;

  // === Verification ===

  /**
   * Verify an identity document's signatures and structure.
   */
  verifyDocument(doc: IdentityDocument): VerificationResult;

  /**
   * Verify a document update (rotation, recovery, revocation).
   */
  verifyUpdate(oldDoc: IdentityDocument, newDoc: IdentityDocument): VerificationResult;

  // === Signing ===

  /**
   * Sign arbitrary data with the current signing key.
   * Used by other layers (messaging, sync) for authentication.
   */
  sign(data: Uint8Array): Promise<Signature>;

  /**
   * Verify a signature from a peer.
   */
  verify(iid: IdentityIdentifier, data: Uint8Array, signature: Signature): Promise<boolean>;
}
```

## Supporting Interfaces

### Create Identity

```typescript
interface CreateIdentityOptions {
  claims?: Partial<Claims>;
  endpoints?: Endpoint[];
  recovery?: RecoveryConfig;
}

interface CreateIdentityResult {
  document: IdentityDocument;
  signingKeyPair: KeyPair;
  encryptionKeyPair: KeyPair;
}

interface KeyPair {
  publicKey: PublicKey;
  privateKey: PrivateKey;
}
```

### Key Rotation

```typescript
interface RotateKeysOptions {
  rotateType: 'signing' | 'encryption' | 'full';
  urgency?: 'normal' | 'urgent';
  reason?: string;
}

interface RotateKeysResult {
  newDocument: IdentityDocument;
  newSigningKeyPair?: KeyPair;
  newEncryptionKeyPair?: KeyPair;
  propagationResult: PropagationResult;
}

interface PropagationResult {
  peersNotified: number;
  peersAcknowledged: number;
  directoryUpdated: boolean;
  errors: PropagationError[];
}

interface PropagationError {
  target: string;
  error: string;
  retryable: boolean;
}
```

### Revocation

```typescript
interface RevokeKeyOptions {
  keyType: 'signing' | 'encryption';
  reason: 'compromised' | 'lost' | 'superseded';
  newKeys?: { signing?: KeyPair; encryption?: KeyPair };
  recoveryProof?: RecoveryProof;
}

interface RevokeKeyResult {
  newDocument: IdentityDocument;
  revocationDocument: KeyRevocation;
  propagationResult: PropagationResult;
}

interface RevokeIdentityOptions {
  reason: 'compromised' | 'abandoned' | 'legal';
  message?: string;
  successorIid?: IdentityIdentifier;
}
```

### Verification

```typescript
interface VerificationResult {
  valid: boolean;
  error?: VerificationError;
  warnings?: string[];
}

type VerificationError =
  | 'INVALID_VERSION'
  | 'INVALID_IID'
  | 'INVALID_SIGNATURE'
  | 'SEQUENCE_REGRESSION'
  | 'MISSING_PREVIOUS_SIG'
  | 'DOCUMENT_TOO_LARGE'
  | 'MALFORMED_JSON'
  | 'EXPIRED_TIMESTAMP'
  | 'INVALID_RECOVERY_PROOF';
```

## Name Resolution Interface

```typescript
interface NameResolutionService {
  // === Resolution ===

  /**
   * Resolve a name to an IID through all configured systems.
   */
  resolve(name: string, options?: ResolveOptions): Promise<ResolveResult | null>;

  /**
   * Resolve using a specific system only.
   */
  resolveWith(name: string, system: NameSystem): Promise<ResolveResult | null>;

  /**
   * Reverse lookup: find names associated with an IID.
   */
  reverseResolve(iid: IdentityIdentifier): Promise<ReverseResolveResult>;

  // === Local Aliases ===

  /**
   * Add a local alias for an IID.
   */
  addAlias(alias: string, iid: IdentityIdentifier, options?: AliasOptions): Promise<void>;

  /**
   * Remove a local alias.
   */
  removeAlias(alias: string): Promise<void>;

  /**
   * Get all local aliases.
   */
  listAliases(): Promise<AliasEntry[]>;

  /**
   * Get alias for a specific IID.
   */
  getAliasFor(iid: IdentityIdentifier): Promise<string | null>;
}

type NameSystem = 'local' | 'dns' | string;

interface ResolveOptions {
  skipLocal?: boolean;
  skipDns?: boolean;
  timeout?: number;
  maxRegistries?: number;
}

interface ResolveResult {
  iid: IdentityIdentifier;
  name: string;
  source: NameSystem;
  verified: boolean;
  confidence: 'high' | 'medium' | 'low';
  cachedUntil?: Date;
}

interface ReverseResolveResult {
  localAlias?: string;
  dnsNames: string[];
  registryNames: Array<{ registry: string; name: string }>;
  selfReportedName?: string;
}

interface AliasOptions {
  displayName?: string;
  verifiedVia?: 'in_person' | 'qr_code' | 'trusted_intro' | 'unverified';
}

interface AliasEntry {
  alias: string;
  iid: IdentityIdentifier;
  displayName?: string;
  addedAt: Date;
  verifiedVia?: string;
}
```

## Recovery Interface

```typescript
interface RecoveryService {
  // === Configuration ===

  /**
   * Configure recovery method for the identity.
   */
  configureRecovery(config: RecoveryConfig): Promise<IdentityDocument>;

  /**
   * Get current recovery configuration.
   */
  getRecoveryConfig(): Promise<RecoveryConfig>;

  // === Recovery Initiation ===

  /**
   * Start a recovery process (generates new keys, prepares recovery request).
   */
  initiateRecovery(iid: IdentityIdentifier): Promise<PendingRecovery>;

  /**
   * Add a recovery proof (trustee attestation, escrow signature, etc.).
   */
  addRecoveryProof(pending: PendingRecovery, proof: RecoveryProof): Promise<PendingRecovery>;

  /**
   * Check if recovery has sufficient proofs.
   */
  isRecoveryReady(pending: PendingRecovery): boolean;

  /**
   * Execute recovery (publish new identity document).
   */
  executeRecovery(pending: PendingRecovery): Promise<RecoveryResult>;

  // === Contestation ===

  /**
   * Contest a pending recovery (if you still have your keys).
   */
  contestRecovery(pendingSequence: SequenceNumber, reason: string): Promise<ContestResult>;

  // === Trustee Operations ===

  /**
   * Sign a recovery attestation for a peer (as a trustee).
   */
  attestRecovery(
    subjectIid: IdentityIdentifier,
    newSigningKey: PublicKey,
    newEncryptionKey: PublicKey
  ): Promise<RecoveryAttestation>;
}

interface PendingRecovery {
  iid: IdentityIdentifier;
  newSigningKey: PublicKey;
  newEncryptionKey: PublicKey;
  initiatedAt: Date;
  proofs: RecoveryProof[];
  status: 'collecting' | 'ready' | 'cooldown' | 'executed' | 'contested';
}

type RecoveryProof =
  | { type: 'social'; attestations: RecoveryAttestation[] }
  | { type: 'device-escrow'; signature: Signature }
  | { type: 'threshold'; reconstructedSignature: Signature }
  | { type: 'provider'; providerAttestation: ProviderAttestation };

interface RecoveryAttestation {
  type: 'recovery_attestation';
  subjectIid: IdentityIdentifier;
  newSigningKey: PublicKey;
  newEncryptionKey: PublicKey;
  trusteeIid: IdentityIdentifier;
  timestamp: Timestamp;
  signature: Signature;
}

interface RecoveryResult {
  success: boolean;
  newDocument?: IdentityDocument;
  error?: string;
}

interface ContestResult {
  success: boolean;
  recoveryBlocked: boolean;
  error?: string;
}
```

## Key Storage Interface

```typescript
interface KeyStorage {
  // === Storage ===

  /**
   * Store a private key securely.
   * @param keyId Unique identifier (e.g., "signing:current", "encryption:seq:5")
   * @param keyType Type of key for proper handling
   * @param privateKey Raw 32-byte private key, Base64 encoded
   */
  storeKey(keyId: string, keyType: 'ed25519' | 'x25519', privateKey: PrivateKey): Promise<void>;

  /**
   * Delete a private key securely (zero memory before release).
   */
  deleteKey(keyId: string): Promise<void>;

  /**
   * List all stored key IDs.
   */
  listKeys(): Promise<string[]>;

  /**
   * Check if a key exists.
   */
  hasKey(keyId: string): Promise<boolean>;

  // === Ed25519 Signing (key never leaves storage) ===

  /**
   * Sign data using a stored Ed25519 key.
   * @param keyId ID of the signing key
   * @param data Data to sign
   * @returns Raw 64-byte Ed25519 signature, Base64 encoded
   */
  signWith(keyId: string, data: Uint8Array): Promise<Signature>;

  // === X25519 Key Agreement (for E2E encryption) ===
  // NOTE: X25519 is a Diffie-Hellman primitive, NOT direct encryption.
  // Use ECDH to derive a shared secret, then use that with AEAD.

  /**
   * Perform X25519 key agreement to derive a shared secret.
   * @param keyId ID of the local X25519 private key
   * @param peerPublicKey Peer's X25519 public key (raw 32 bytes, Base64)
   * @returns 32-byte shared secret (use with HKDF to derive encryption keys)
   */
  deriveSharedSecret(keyId: string, peerPublicKey: PublicKey): Promise<Uint8Array>;

  /**
   * Derive encryption keys from shared secret using HKDF.
   * @param sharedSecret From deriveSharedSecret()
   * @param info Context string (e.g., "message-encryption" or "session-key")
   * @param salt Optional salt (use conversation ID or nonce)
   * @returns Derived key material for AEAD (e.g., 32 bytes for ChaCha20-Poly1305)
   */
  deriveKey(sharedSecret: Uint8Array, info: string, salt?: Uint8Array): Promise<Uint8Array>;
}
```

### Encryption Scheme Contract

**X25519 is used for key agreement, not direct encryption.**

The identity layer provides X25519 key pairs. Messaging and other layers use them as follows:

1. **Key Agreement**: `shared_secret = X25519(my_private, peer_public)`
2. **Key Derivation**: `encryption_key = HKDF-SHA256(shared_secret, salt, info)`
3. **Encryption**: `ciphertext = ChaCha20-Poly1305(encryption_key, nonce, plaintext)`

| Component | Algorithm |
|-----------|-----------|
| Key agreement | X25519 |
| Key derivation | HKDF-SHA256 |
| Symmetric encryption | ChaCha20-Poly1305 |
| Nonce size | 12 bytes (96 bits) |

**Messaging layer** will specify the full encryption protocol including session keys, ratcheting, and forward secrecy.

## Events

```typescript
interface IdentityEvents {
  // Emitted when own identity is updated
  onSelfIdentityUpdated: Event<{ oldDoc: IdentityDocument; newDoc: IdentityDocument }>;

  // Emitted when a peer's identity is updated
  onPeerIdentityUpdated: Event<{ iid: IdentityIdentifier; oldDoc: IdentityDocument; newDoc: IdentityDocument }>;

  // Emitted when a peer's identity is revoked
  onPeerIdentityRevoked: Event<{ iid: IdentityIdentifier; revocation: KeyRevocation | IdentityRevocation }>;

  // Emitted when a recovery is initiated for own identity
  onRecoveryInitiated: Event<{ pending: PendingRecovery }>;

  // Emitted when someone requests recovery attestation
  onRecoveryAttestationRequested: Event<{ subjectIid: IdentityIdentifier; requester: IdentityIdentifier }>;

  // Emitted when key rotation is recommended
  onKeyRotationRecommended: Event<{ reason: string; keyType: 'signing' | 'encryption' | 'both' }>;
}
```

## Error Types

```typescript
class IdentityError extends Error {
  constructor(
    public code: IdentityErrorCode,
    message: string,
    public details?: Record<string, unknown>
  ) {
    super(message);
  }
}

type IdentityErrorCode =
  | 'IDENTITY_NOT_FOUND'
  | 'INVALID_DOCUMENT'
  | 'SIGNATURE_VERIFICATION_FAILED'
  | 'KEY_NOT_FOUND'
  | 'RECOVERY_NOT_CONFIGURED'
  | 'RECOVERY_THRESHOLD_NOT_MET'
  | 'RECOVERY_COOLDOWN_ACTIVE'
  | 'REVOCATION_FAILED'
  | 'PROPAGATION_FAILED'
  | 'NAME_RESOLUTION_FAILED'
  | 'ALIAS_ALREADY_EXISTS'
  | 'SEQUENCE_CONFLICT'
  | 'UNAUTHORIZED';
```

## Constants

```typescript
const IDENTITY_CONSTANTS = {
  // Document constraints
  MAX_DOCUMENT_SIZE: 16384,           // 16 KB
  MAX_ENDPOINTS: 10,
  MAX_CLAIMS_NAME_LENGTH: 64,
  MAX_CLAIMS_BIO_LENGTH: 256,
  MAX_EXTENSIONS_SIZE: 4096,          // 4 KB

  // Recovery constraints
  MAX_TRUSTEES: 10,
  MIN_RECOVERY_THRESHOLD: 2,
  DEFAULT_COOLDOWN_HOURS: 72,

  // Key constraints
  SIGNING_KEY_ALGORITHM: 'Ed25519',
  ENCRYPTION_KEY_ALGORITHM: 'X25519',

  // Retention
  OLD_ENCRYPTION_KEY_RETENTION_DAYS: 30,
  REVOCATION_RECORD_RETENTION_DAYS: 365,

  // Timeouts
  PROPAGATION_TIMEOUT_MS: 30000,
  RESOLUTION_TIMEOUT_MS: 10000,
} as const;
```

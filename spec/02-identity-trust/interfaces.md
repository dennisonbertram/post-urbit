# Identity & Trust Interfaces

## Overview

This document specifies the complete API surface for the Identity & Trust layer. These interfaces are used by other layers (Transport, Messaging, Apps) and by applications.

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
type SequenceNumber = number;
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
}

interface KeySet {
  signing: {
    current: PublicKey;
    previous: PublicKey | null;
  };
  encryption: {
    current: PublicKey;
    previous: PublicKey | null;
  };
}

interface Endpoint {
  type: 'direct' | 'relay' | 'mailbox';
  address: string;
  priority: number; // 0-255, lower = higher priority
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
   */
  storeKey(keyId: string, privateKey: PrivateKey): Promise<void>;

  /**
   * Retrieve a private key.
   */
  getKey(keyId: string): Promise<PrivateKey | null>;

  /**
   * Delete a private key.
   */
  deleteKey(keyId: string): Promise<void>;

  /**
   * List all stored key IDs.
   */
  listKeys(): Promise<string[]>;

  // === Signing (key never leaves storage) ===

  /**
   * Sign data using a stored key (key never exposed).
   */
  signWith(keyId: string, data: Uint8Array): Promise<Signature>;

  /**
   * Decrypt data using stored encryption key.
   */
  decryptWith(keyId: string, ciphertext: Uint8Array): Promise<Uint8Array>;
}
```

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

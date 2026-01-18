# Messaging & Sync Interfaces

## Overview

This document specifies the complete API surface for the Messaging & Sync layer.

## Wire Format vs TypeScript Convention

| Context | Convention | Example |
|---------|------------|---------|
| **On-wire JSON (PUSE plaintext)** | snake_case | `thread_id`, `reply_to`, `is_typing`, `target_message_id` |
| **TypeScript interfaces** | camelCase | `threadId`, `replyTo`, `isTyping`, `targetMessageId` |

**Normative rule**: On-wire plaintext JSON inside PUSE envelopes uses snake_case; TypeScript interfaces use camelCase for developer ergonomics. Implementations MUST: [REQ-MSG-016]
1. Serialize to snake_case when creating PUSE plaintext
2. Deserialize from snake_case when parsing received PUSE plaintext

**Field mapping** (TypeScript → Wire):
- `threadId` → `thread_id`
- `replyTo` → `reply_to`
- `expiresAt` → `expires_at`
- `receiptType` → `receipt_type`
- `messageIds` → `message_ids`
- `targetMessageId` → `target_message_id`
- `isTyping` → `is_typing`
- `mediaType` → `media_type`
- `senderIid` → `sender_iid`
- `groupId` → `group_id`
- `chainKey` → `chain_key`
- `keyId` → `key_id`

## Core Types

**Important:** These TypeScript types use **string representations** for developer ergonomics in app-level code. For wire encoding (sync CBOR, PUSE binary headers), see the **"API↔Wire Encoding Mapping"** section below which specifies fixed-length byte representations.

```typescript
// === In-memory/API string representations ===
// For wire encoding, see "API↔Wire Encoding Mapping" section

// Message identifier (UUID v4 string)
type MessageId = string;

// Conversation identifier (derived or random)
type ConversationId = string;

// Group identifier (32-char Base32, same format as IID)
type GroupId = string;

// Document identifier (UUID string or 64-char hex for content-addressed)
// Wire: 32-byte bstr (see mapping table)
type DocumentId = string;

// Identity identifier (32-char Crockford Base32)
// Wire: 20-byte bstr (raw IID bytes)
type IdentityIdentifier = string;

// Operation identifier (64-char lowercase hex)
// Wire: 32-byte bstr (raw SHA256 hash)
type OperationId = string;

// Public key (43-char Base64, no padding)
// Wire: 32-byte bstr (raw key bytes)
type PublicKey = string;

// Ed25519 signature (86-char Base64, no padding)
// Wire: 64-byte bstr (raw signature bytes)
type Signature = string;

// RFC 3339 timestamp string
type Timestamp = string;

// Decimal string (for uint64 safety)
type SequenceNumber = string;
```

## API↔Wire Encoding Mapping (Normative)

The TypeScript interfaces above are **in-memory/app-facing only**. When encoding for the wire (sync stream 0x04 CBOR payloads), the following conversions MUST be applied: [REQ-MSG-017]

| API Type | String Format | Wire Format (CBOR) |
|----------|---------------|-------------------|
| `DocumentId` | UUID string (`550e8400-...`) | 32-byte bstr: UUID bytes (16) + 16 zero bytes |
| `IdentityIdentifier` | Crockford Base32 (32 chars) | 20-byte bstr: raw IID bytes |
| `Signature` | Base64 string (86 chars) | 64-byte bstr: raw Ed25519 signature |
| `PublicKey` | Base64 string (43 chars) | 32-byte bstr: raw X25519/Ed25519 key |
| `OperationId` | Hex string (64 chars) | 32-byte bstr: raw SHA256 hash |
| `Timestamp` (messages) | RFC 3339 string | CBOR text string (same format) |
| `HybridLogicalClock` (sync) | Object `{physical, logical, origin}` | CBOR map: `{physical: uint, logical: uint, origin: 20-byte bstr}` |

**Timestamp types:** The `Timestamp` type (RFC 3339 string) is used for message timestamps, expiration times, and human-readable dates. The `HybridLogicalClock` type is used **only** for sync operation ordering (see sync-protocol.md). These are distinct types with different wire encodings.

**DocumentId padding rule:** Sync documents use a 32-byte identifier. For UUID-based IDs, the 16-byte UUID is followed by 16 zero bytes. For content-addressed documents, the full 32-byte SHA256 hash is used.

**Example:**
```
API: { documentId: "550e8400-e29b-41d4-a716-446655440000" }
Wire CBOR: { document_id: h'550e8400e29b41d4a71644665544000000000000000000000000000000000000' }
```

## Message Types

```typescript
// Base message structure (after decryption)
interface Message {
  id: MessageId;
  conversationId: ConversationId;
  senderId: IdentityIdentifier;
  timestamp: Timestamp;
  sequence: SequenceNumber;
  type: MessageType;
  content: MessageContent;
  threadId?: MessageId;
  replyTo?: MessageId;
  expiresAt?: Timestamp;
  metadata?: Record<string, unknown>;
}

// Message types per RFC-0003 §8.2
type MessageType =
  | 'text'
  | 'rich'
  | 'media'
  | 'reaction'
  | 'receipt'
  | 'typing'
  | 'app'
  // Reserved for future (not in RFC-0003 v1):
  | 'edit'     // Reserved: message editing
  | 'delete'   // Reserved: message deletion
  | 'system';  // Reserved: system notifications

// Type-specific content
type MessageContent =
  | TextContent
  | RichContent
  | MediaContent
  | ReactionContent
  | ReceiptContent
  | TypingContent
  | EditContent
  | DeleteContent
  | SystemContent
  | AppContent;

interface TextContent {
  text: string;
  mentions?: Mention[];
}

interface Mention {
  iid: IdentityIdentifier;
  offset: number;
  length: number;
}

interface RichContent {
  format: 'markdown' | 'html';
  text: string;
  mentions?: Mention[];
}

interface MediaContent {
  mediaType: string;           // MIME type
  size: number;                // Bytes
  width?: number;              // For images/video
  height?: number;
  duration?: number;           // For audio/video, seconds
  hash: string;                // Content hash (sha256:...)
  // Wire format: Base64 standard encoding (no padding), 32 bytes decoded
  // In-memory: may be decoded to Uint8Array for cryptographic operations
  key: string;                 // Base64-encoded 32-byte encryption key
  // Wire format: Base64 standard encoding (no padding), 12 bytes decoded
  nonce: string;               // Base64-encoded 12-byte nonce
  url: string;                 // Where to fetch encrypted media
  thumbnail?: {
    data: string;              // Base64 inline thumbnail
    width: number;
    height: number;
  };
}

// **Wire Encoding:** On the wire (JSON plaintext), `key` and `nonce` are Base64 standard
// (no padding) strings. Applications may decode to Uint8Array for cryptographic operations.

interface ReactionContent {
  targetMessageId: MessageId;
  emoji: string;
  action: 'add' | 'remove';
}

interface ReceiptContent {
  receiptType: 'delivered' | 'read';
  messageIds: MessageId[];
}

// Per RFC-0003 §8.2 - typing indicator content
interface TypingContent {
  isTyping: boolean;           // Note: expiration handled at message envelope level (Message.expiresAt), not in content
}

// Reserved for future - not in RFC-0003 v1
interface EditContent {
  targetMessageId: MessageId;
  newContent: TextContent | RichContent;
  editedAt: Timestamp;
}

// Reserved for future - not in RFC-0003 v1
interface DeleteContent {
  targetMessageId: MessageId;
  deleteType: 'for_me' | 'for_all';
}

// Reserved for future - not in RFC-0003 v1
interface SystemContent {
  event: string;
  data: Record<string, unknown>;
}

// Per RFC-0003 §8.2 - app-defined message content
// Wire format: { "app_id": string, "data": CBOR-decoded }
// App message content uses the CBOR↔JSON mapping defined in RFC-0003 §8.2.
// CBOR bytes use `~b` prefix, big integers use `~i` prefix, etc.
interface AppContent {
  appId: string;               // Application identifier (maps to wire "app_id")
  data: unknown;               // Application payload (CBOR-decoded, maps to wire "data")
}
```

## Messaging Service Interface

```typescript
interface MessagingService {
  // === Conversations ===

  /**
   * Start or retrieve a 1:1 conversation.
   */
  getOrCreateConversation(peerId: IdentityIdentifier): Promise<Conversation>;

  /**
   * List all conversations.
   */
  listConversations(options?: ListConversationsOptions): Promise<Conversation[]>;

  /**
   * Get a specific conversation.
   */
  getConversation(conversationId: ConversationId): Promise<Conversation | null>;

  /**
   * Archive a conversation.
   */
  archiveConversation(conversationId: ConversationId): Promise<void>;

  /**
   * Delete a conversation (local only).
   */
  deleteConversation(conversationId: ConversationId): Promise<void>;

  // === Messages ===

  /**
   * Send a message to a conversation.
   */
  sendMessage(
    conversationId: ConversationId,
    content: MessageContent,
    options?: SendMessageOptions
  ): Promise<Message>;

  /**
   * Get messages from a conversation.
   */
  getMessages(
    conversationId: ConversationId,
    options?: GetMessagesOptions
  ): Promise<Message[]>;

  /**
   * Edit a previously sent message.
   *
   * @reserved NOT IMPLEMENTED in v1. Calling this method will throw
   * MessagingError with code 'INVALID_MESSAGE_TYPE'. The 'edit' message type
   * and EditContent are reserved for future protocol versions.
   */
  editMessage(messageId: MessageId, newContent: TextContent | RichContent): Promise<Message>;

  /**
   * Delete a message.
   *
   * @reserved NOT IMPLEMENTED in v1. Calling this method will throw
   * MessagingError with code 'INVALID_MESSAGE_TYPE'. The 'delete' message type
   * and DeleteContent are reserved for future protocol versions.
   * Local-only deletion (removing from local storage) should use
   * deleteConversation() or direct database operations.
   */
  deleteMessage(messageId: MessageId, type: 'for_me' | 'for_all'): Promise<void>;

  /**
   * Send a reaction to a message.
   */
  react(messageId: MessageId, emoji: string): Promise<void>;

  /**
   * Remove a reaction from a message.
   */
  unreact(messageId: MessageId, emoji: string): Promise<void>;

  // === Status ===

  /**
   * Send a read receipt.
   */
  markAsRead(conversationId: ConversationId, upToMessageId: MessageId): Promise<void>;

  /**
   * Send typing indicator.
   */
  setTyping(conversationId: ConversationId, isTyping: boolean): Promise<void>;

  // === Events ===

  onMessageReceived: Event<{ message: Message }>;
  /** @reserved NOT IMPLEMENTED in v1. This event will never fire until edit support is added. */
  onMessageEdited: Event<{ messageId: MessageId; newContent: MessageContent }>;
  /** @reserved NOT IMPLEMENTED in v1. This event will never fire until delete support is added. */
  onMessageDeleted: Event<{ messageId: MessageId; conversationId: ConversationId }>;
  onReaction: Event<{ messageId: MessageId; senderId: IdentityIdentifier; emoji: string; action: 'add' | 'remove' }>;
  onTyping: Event<{ conversationId: ConversationId; senderId: IdentityIdentifier; isTyping: boolean }>;
  onReadReceipt: Event<{ conversationId: ConversationId; senderId: IdentityIdentifier; upToSequence: SequenceNumber }>;
}

interface Conversation {
  id: ConversationId;
  type: 'one_to_one' | 'group';
  participants: IdentityIdentifier[];
  lastMessage?: Message;
  lastActivity: Timestamp;
  unreadCount: number;
  archived: boolean;
  muted: boolean;
  muteUntil?: Timestamp;
}

interface ListConversationsOptions {
  includeArchived?: boolean;
  limit?: number;
  beforeTimestamp?: Timestamp;
}

interface SendMessageOptions {
  threadId?: MessageId;
  replyTo?: MessageId;
  expiresAt?: Timestamp;
  priority?: 'normal' | 'high';
}

interface GetMessagesOptions {
  limit?: number;
  beforeSequence?: SequenceNumber;
  afterSequence?: SequenceNumber;
  threadId?: MessageId;
}
```

## Group Service Interface

```typescript
interface GroupService {
  // === Group Management ===

  /**
   * Create a new group.
   */
  createGroup(options: CreateGroupOptions): Promise<Group>;

  /**
   * Get a group by ID.
   */
  getGroup(groupId: GroupId): Promise<Group | null>;

  /**
   * List groups the user is a member of.
   */
  listGroups(): Promise<Group[]>;

  /**
   * Update group metadata (admin only).
   */
  updateGroup(groupId: GroupId, updates: Partial<GroupMetadata>): Promise<Group>;

  /**
   * Delete/dissolve a group (owner only).
   */
  deleteGroup(groupId: GroupId): Promise<void>;

  // === Membership ===

  /**
   * Invite a user to a group.
   */
  inviteToGroup(groupId: GroupId, iid: IdentityIdentifier): Promise<void>;

  /**
   * Accept a group invitation.
   */
  acceptInvite(groupId: GroupId): Promise<Group>;

  /**
   * Decline a group invitation.
   */
  declineInvite(groupId: GroupId): Promise<void>;

  /**
   * Leave a group.
   */
  leaveGroup(groupId: GroupId): Promise<void>;

  /**
   * Remove a member from a group (admin only).
   */
  removeMember(groupId: GroupId, iid: IdentityIdentifier, reason?: string): Promise<void>;

  /**
   * Update a member's role (admin only).
   */
  updateMemberRole(groupId: GroupId, iid: IdentityIdentifier, role: GroupRole): Promise<void>;

  // === Events ===

  onGroupInvite: Event<{ groupId: GroupId; inviterId: IdentityIdentifier; group: GroupMetadata }>;
  onGroupJoined: Event<{ groupId: GroupId }>;
  onGroupLeft: Event<{ groupId: GroupId; iid: IdentityIdentifier }>;
  onGroupUpdated: Event<{ groupId: GroupId; updates: Partial<GroupMetadata> }>;
  onMemberAdded: Event<{ groupId: GroupId; iid: IdentityIdentifier }>;
  onMemberRemoved: Event<{ groupId: GroupId; iid: IdentityIdentifier; reason?: string }>;
  onMemberRoleChanged: Event<{ groupId: GroupId; iid: IdentityIdentifier; newRole: GroupRole }>;
}

interface Group {
  id: GroupId;
  metadata: GroupMetadata;
  members: GroupMember[];
  myRole: GroupRole;
  joined: boolean;
  createdAt: Timestamp;
  updatedAt: Timestamp;
}

interface GroupMetadata {
  name: string;
  description?: string;
  avatar?: string;
  settings: GroupSettings;
}

interface GroupSettings {
  joinRule: 'invite_only' | 'link' | 'open';
  historyVisibility: 'joined' | 'invited' | 'shared' | 'none';
  messageRetentionDays?: number;
  allowReactions: boolean;
  allowThreads: boolean;
}

interface GroupMember {
  iid: IdentityIdentifier;
  role: GroupRole;
  joinedAt: Timestamp;
  invitedBy: IdentityIdentifier;
  displayName?: string;
}

type GroupRole = 'owner' | 'admin' | 'moderator' | 'member';

interface CreateGroupOptions {
  name: string;
  description?: string;
  avatar?: string;
  settings?: Partial<GroupSettings>;
  initialMembers?: IdentityIdentifier[];
}
```

## Sync Service Interface

```typescript
interface SyncService {
  // === Document Management ===

  /**
   * Create a new sync document.
   */
  createDocument<T>(options: CreateDocumentOptions<T>): Promise<SyncDocument<T>>;

  /**
   * Open an existing document.
   */
  openDocument<T>(documentId: DocumentId): Promise<SyncDocument<T> | null>;

  /**
   * List documents of a specific type.
   */
  listDocuments(type: string): Promise<DocumentSummary[]>;

  /**
   * Delete a document (owner only).
   */
  deleteDocument(documentId: DocumentId): Promise<void>;

  // === Sync Control ===

  /**
   * Trigger sync for a document with specific peers.
   */
  syncDocument(documentId: DocumentId, peers?: IdentityIdentifier[]): Promise<SyncResult>;

  /**
   * Subscribe to updates for a document.
   */
  subscribeToDocument(documentId: DocumentId): SyncSubscription;

  /**
   * Get sync status for a document.
   */
  getSyncStatus(documentId: DocumentId): Promise<SyncStatus>;

  // === Events ===

  onDocumentCreated: Event<{ documentId: DocumentId; type: string }>;
  onDocumentUpdated: Event<{ documentId: DocumentId; operations: SyncOperation[] }>;
  onDocumentDeleted: Event<{ documentId: DocumentId }>;
  onSyncCompleted: Event<{ documentId: DocumentId; result: SyncResult }>;
  onSyncError: Event<{ documentId: DocumentId; error: SyncError }>;
}

interface SyncDocument<T> {
  id: DocumentId;
  type: string;
  owner: IdentityIdentifier;
  createdAt: Timestamp;
  access: AccessControl;

  // Read current state
  getState(): T;

  // Apply a CRDT operation
  apply(operation: CRDTOperation): Promise<void>;

  // Observe changes
  onChange(callback: (state: T) => void): Unsubscribe;

  // Close the document (release resources)
  close(): void;
}

interface CreateDocumentOptions<T> {
  type: string;
  initialState: T;
  access?: AccessControl;
}

interface AccessControl {
  owner: IdentityIdentifier;
  readers: IdentityIdentifier[] | 'public';
  writers: IdentityIdentifier[];
  admins: IdentityIdentifier[];
}

interface DocumentSummary {
  id: DocumentId;
  type: string;
  owner: IdentityIdentifier;
  createdAt: Timestamp;
  lastModified: Timestamp;
}

interface SyncResult {
  success: boolean;
  operationsSent: number;
  operationsReceived: number;
  peersReached: number;
  errors?: SyncError[];
}

interface SyncStatus {
  documentId: DocumentId;
  localVersion: HybridLogicalClock;
  peers: Map<IdentityIdentifier, PeerSyncStatus>;
}

interface PeerSyncStatus {
  lastSynced: Timestamp;
  peerVersion: HybridLogicalClock;
  pendingOperations: number;
}

interface SyncSubscription {
  unsubscribe(): void;
}

interface HybridLogicalClock {
  physical: number;
  logical: number;
  origin: IdentityIdentifier;
}

/**
 * Wire-level CRDT operations (matches sync-protocol.md CBOR schema).
 * Type codes match the Operation Type Registry in sync-protocol.md.
 * App-level APIs may use higher-level abstractions that translate to these wire operations.
 */
type CRDTOperation =
  | { type: 0; value: unknown }                                    // lww_set: set LWW-Register value
  | { type: 1; element: unknown; tag: Uint8Array }                 // orset_add: add to OR-Set (tag is 24-byte UniqueTag)
  | { type: 2; element: unknown; tags: Uint8Array[] }              // orset_remove: remove from OR-Set
  | { type: 3; amount: number }                                    // pncounter_inc: increment counter
  | { type: 4; amount: number }                                    // pncounter_dec: decrement counter
  | { type: 5; key: string; value: unknown | null }                // lwwmap_set: set map key-value (null to delete)
  | { type: 6; id: Uint8Array; parent: Uint8Array; value: unknown }// rga_insert: insert into RGA (24-byte IDs)
  | { type: 7; id: Uint8Array };                                   // rga_delete: delete from RGA

interface SyncOperation {
  id: string;
  documentId: DocumentId;
  origin: IdentityIdentifier;
  timestamp: HybridLogicalClock;
  operation: CRDTOperation;
  dependencies: string[];
  signature: Signature;
}

type SyncError =
  | { code: 'DOCUMENT_NOT_FOUND'; documentId: DocumentId }
  | { code: 'PERMISSION_DENIED'; documentId: DocumentId; requiredPermission: string }
  | { code: 'INVALID_OPERATION'; details: string }
  | { code: 'SYNC_TIMEOUT'; peer: IdentityIdentifier }
  | { code: 'NETWORK_ERROR'; details: string };

type Unsubscribe = () => void;
```

## Encryption Service Interface

```typescript
interface MessageEncryptionService {
  // === Key Management ===

  /**
   * Initialize ratchet session with a peer.
   */
  initializeSession(peerId: IdentityIdentifier): Promise<void>;

  /**
   * Check if a session exists with a peer.
   */
  hasSession(peerId: IdentityIdentifier): boolean;

  /**
   * Reset session (e.g., after desync).
   */
  resetSession(peerId: IdentityIdentifier): Promise<void>;

  // === Encryption (1:1) ===

  /**
   * Encrypt a message for a peer (1:1).
   * Returns a SealedEnvelope with full PUSE wire format bytes.
   */
  encryptMessage(
    peerId: IdentityIdentifier,
    plaintext: Uint8Array
  ): Promise<SealedEnvelope>;

  /**
   * Decrypt a message from a peer (1:1).
   * Accepts raw PUSE envelope bytes or parsed SealedEnvelope.
   */
  decryptMessage(
    senderId: IdentityIdentifier,
    envelope: Uint8Array | SealedEnvelope
  ): Promise<Uint8Array>;

  // === Group Encryption ===

  /**
   * Get or generate sender key for a group.
   */
  getSenderKey(groupId: GroupId): Promise<SenderKeyState>;

  /**
   * Share sender key with a peer (via 1:1 ratchet).
   */
  shareSenderKey(groupId: GroupId, peerId: IdentityIdentifier): Promise<void>;

  /**
   * Receive a sender key from a peer.
   */
  receiveSenderKey(groupId: GroupId, senderId: IdentityIdentifier, keyShare: SenderKeyShare): Promise<void>;

  /**
   * Encrypt a message for a group.
   * Returns a SealedEnvelope with group header extension (type 0x02).
   */
  encryptGroupMessage(groupId: GroupId, plaintext: Uint8Array): Promise<SealedEnvelope>;

  /**
   * Decrypt a message from a group.
   * Accepts raw PUSE envelope bytes or parsed SealedEnvelope.
   */
  decryptGroupMessage(
    groupId: GroupId,
    senderId: IdentityIdentifier,
    envelope: Uint8Array | SealedEnvelope
  ): Promise<Uint8Array>;
}

/**
 * Sealed envelope - the unified encrypted message format (PUSE wire format).
 * This matches the Secure Envelope wire format exactly.
 * See secure-envelope.md for complete specification.
 */
interface SealedEnvelope {
  /** Full PUSE envelope bytes (ready for transmission) */
  bytes: Uint8Array;

  /** Parsed header fields (for routing/filtering without decryption) */
  header: EnvelopeHeader;
}

interface EnvelopeHeader {
  magic: Uint8Array;            // 4 bytes: 0x50 0x55 0x53 0x45 ("PUSE")
  version: number;              // 1 byte
  flags: number;                // 1 byte
  senderIid: Uint8Array;        // 20 bytes (raw)
  recipientIid: Uint8Array;     // 20 bytes (raw) - IID or GroupId
  messageId: Uint8Array;        // 16 bytes (UUID)
  headerExtension: Uint8Array;  // Variable length (ratchet or group info)
}

/** Flags byte interpretation */
interface EnvelopeFlags {
  recipientType: 'direct' | 'group' | 'broadcast';  // bits 0-1
  requiresAck: boolean;                              // bit 2
  priority: 'normal' | 'high';                       // bit 3
  forwardable: boolean;                              // bit 4
}

/** Header extension types */
type HeaderExtensionType =
  | 0x00  // initial (X3DH)
  | 0x01  // ratchet (1:1 ongoing)
  | 0x02; // group (sender key)

/** Parsed ratchet header extension (type 0x01) */
interface RatchetHeaderExtension {
  type: 0x01;
  dhPublicKey: Uint8Array;       // 32 bytes
  previousChainLength: number;   // uint32
  chainIndex: number;            // uint32
}

/** Parsed group header extension (type 0x02) */
interface GroupHeaderExtension {
  type: 0x02;
  senderKeyId: Uint8Array;       // 16 bytes
  iteration: number;             // uint32
}

/** Parsed initial header extension (type 0x00) */
interface InitialHeaderExtension {
  type: 0x00;
  ephemeralPublicKey: Uint8Array; // 32 bytes
}

interface SenderKeyState {
  keyId: string;
  chainKey: Uint8Array;
  // NOTE: No separate signature key. Group message signatures use the sender's
  // identity signing key via the PUSE envelope signature. See group-messaging.md.
  iteration: number;
}

interface SenderKeyShare {
  keyId: string;
  chainKey: Uint8Array;
  // NOTE: No signaturePublicKey. Verify sender via PUSE envelope signature
  // against sender's identity document signing key.
  iteration: number;
}

interface KeyPair {
  publicKey: Uint8Array;
  privateKey: Uint8Array;
}
```

## Error Types

```typescript
class MessagingError extends Error {
  constructor(
    public code: MessagingErrorCode,
    message: string,
    public details?: Record<string, unknown>
  ) {
    super(message);
  }
}

type MessagingErrorCode =
  // Message errors
  | 'MESSAGE_NOT_FOUND'
  | 'MESSAGE_TOO_LARGE'
  | 'INVALID_MESSAGE_TYPE'
  | 'ENCRYPTION_FAILED'
  | 'DECRYPTION_FAILED'
  // Conversation errors
  | 'CONVERSATION_NOT_FOUND'
  | 'NOT_A_PARTICIPANT'
  // Group errors
  | 'GROUP_NOT_FOUND'
  | 'NOT_A_MEMBER'
  | 'INSUFFICIENT_PERMISSIONS'
  | 'GROUP_FULL'
  // Sync errors
  | 'DOCUMENT_NOT_FOUND'
  | 'SYNC_FAILED'
  | 'CONFLICT_DETECTED'
  // Session errors
  | 'SESSION_NOT_FOUND'
  | 'SESSION_DESYNC'
  | 'KEY_NOT_FOUND';
```

## Constants

```typescript
const MESSAGING_CONSTANTS = {
  // Message limits
  MAX_MESSAGE_SIZE: 1048576,         // 1 MB
  MAX_TEXT_LENGTH: 65536,            // 64 KB
  MAX_MENTIONS_PER_MESSAGE: 50,

  // Media limits
  MAX_MEDIA_SIZE: 104857600,         // 100 MB
  MAX_THUMBNAIL_SIZE: 65536,         // 64 KB

  // Conversation limits
  MAX_PARTICIPANTS_1_TO_1: 2,

  // Group limits
  MAX_GROUP_SIZE: 10000,
  MAX_GROUP_NAME_LENGTH: 100,
  MAX_GROUP_DESCRIPTION_LENGTH: 1000,

  // Timing
  TYPING_INDICATOR_TIMEOUT_MS: 10000,
  MESSAGE_EXPIRY_MIN_SECONDS: 60,
  MESSAGE_EXPIRY_MAX_DAYS: 365,

  // Sync
  SYNC_BATCH_SIZE: 100,
  SYNC_TIMEOUT_MS: 30000,
  MAX_OPERATIONS_PER_DOCUMENT: 1000000,

  // Encryption
  RATCHET_MAX_SKIP: 100,
  SKIPPED_KEY_TTL_DAYS: 7,
  SENDER_KEY_ROTATION_MESSAGES: 100,
  SENDER_KEY_ROTATION_DAYS: 7,
} as const;
```

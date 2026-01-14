# Messaging & Sync Interfaces

## Overview

This document specifies the complete API surface for the Messaging & Sync layer.

## Core Types

```typescript
// Message identifier (UUID v4)
type MessageId = string;

// Conversation identifier (derived or random)
type ConversationId = string;

// Group identifier (32-char Base32, same format as IID)
type GroupId = string;

// Document identifier (UUID or content-addressed hash)
type DocumentId = string;

// Reuse from identity layer
type IdentityIdentifier = string;
type PublicKey = string;
type Signature = string;
type Timestamp = string;
type SequenceNumber = string;
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

type MessageType =
  | 'text'
  | 'rich'
  | 'media'
  | 'reaction'
  | 'receipt'
  | 'typing'
  | 'edit'
  | 'delete'
  | 'system'
  | 'app';

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
  key: Uint8Array;             // Encryption key for media file
  nonce: Uint8Array;           // Nonce for decryption
  url: string;                 // Where to fetch encrypted media
  thumbnail?: {
    data: string;              // Base64 inline thumbnail
    width: number;
    height: number;
  };
}

interface ReactionContent {
  targetMessageId: MessageId;
  emoji: string;
  action: 'add' | 'remove';
}

interface ReceiptContent {
  receiptType: 'delivered' | 'read';
  messageIds: MessageId[];
}

interface TypingContent {
  isTyping: boolean;
  expiresAt: Timestamp;        // Auto-clear after this time
}

interface EditContent {
  targetMessageId: MessageId;
  newContent: TextContent | RichContent;
  editedAt: Timestamp;
}

interface DeleteContent {
  targetMessageId: MessageId;
  deleteType: 'for_me' | 'for_all';
}

interface SystemContent {
  event: string;
  data: Record<string, unknown>;
}

interface AppContent {
  appId: string;
  appVersion: string;
  payload: unknown;
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
   */
  editMessage(messageId: MessageId, newContent: TextContent | RichContent): Promise<Message>;

  /**
   * Delete a message.
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
  onMessageEdited: Event<{ messageId: MessageId; newContent: MessageContent }>;
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

type CRDTOperation =
  | { type: 'lww_set'; key: string; value: unknown; timestamp: HybridLogicalClock }
  | { type: 'counter_inc'; delta: number }
  | { type: 'counter_dec'; delta: number }
  | { type: 'set_add'; element: unknown; tag: string }
  | { type: 'set_remove'; element: unknown; tag: string }
  | { type: 'list_insert'; index: number; element: unknown; id: string }
  | { type: 'list_delete'; id: string };

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

  // === Encryption ===

  /**
   * Encrypt a message for a peer (1:1).
   */
  encryptMessage(
    peerId: IdentityIdentifier,
    plaintext: Uint8Array
  ): Promise<EncryptedMessage>;

  /**
   * Decrypt a message from a peer (1:1).
   */
  decryptMessage(
    senderId: IdentityIdentifier,
    envelope: EncryptedMessage
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
   */
  encryptGroupMessage(groupId: GroupId, plaintext: Uint8Array): Promise<GroupEncryptedMessage>;

  /**
   * Decrypt a message from a group.
   */
  decryptGroupMessage(groupId: GroupId, senderId: IdentityIdentifier, envelope: GroupEncryptedMessage): Promise<Uint8Array>;
}

interface EncryptedMessage {
  senderIid: Uint8Array;        // 20 bytes
  recipientIid: Uint8Array;     // 20 bytes
  ephemeralPublicKey: Uint8Array; // 32 bytes
  nonce: Uint8Array;            // 12 bytes
  ciphertext: Uint8Array;
  signature: Uint8Array;        // 64 bytes
}

interface GroupEncryptedMessage {
  senderIid: Uint8Array;
  groupId: Uint8Array;
  senderKeyId: Uint8Array;      // 16 bytes
  iteration: number;
  nonce: Uint8Array;
  ciphertext: Uint8Array;
  signature: Uint8Array;
}

interface SenderKeyState {
  keyId: string;
  chainKey: Uint8Array;
  signatureKeyPair: KeyPair;
  iteration: number;
}

interface SenderKeyShare {
  keyId: string;
  chainKey: Uint8Array;
  signaturePublicKey: Uint8Array;
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

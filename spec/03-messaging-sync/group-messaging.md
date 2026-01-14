# Group Messaging Protocol

## Overview

Group messaging enables encrypted multi-party conversations. Uses sender keys for efficiency - each sender encrypts once for the entire group.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Encryption scheme | Sender keys | O(1) encryption per message vs O(n) for recipient keys |
| Key distribution | Via 1:1 ratchet | Secure, no group-wide shared secret |
| Membership | Explicit invites | No permission-less joins |
| History | Optional sharing | Privacy vs UX tradeoff |

## Group Structure

### Group Identifier

```
group_id = Base32Lower(SHA256(creator_iid || random_32_bytes || creation_timestamp)[0:20])
```

32-character Base32 string, same format as IID.

### Group Metadata

```json
{
  "group_id": "<32-char-base32>",
  "name": "Project Team",
  "description": "Discussions about the project",
  "avatar": "sha256:abc123...",
  "created_at": "2025-01-13T12:00:00Z",
  "created_by": "<creator-iid>",
  "settings": {
    "join_rule": "invite_only|link|open",
    "history_visibility": "joined|invited|shared|none",
    "message_retention_days": 365,
    "allow_reactions": true,
    "allow_threads": true
  }
}
```

### Group Membership

```json
{
  "group_id": "<group-id>",
  "members": [
    {
      "iid": "<member-iid>",
      "role": "admin|moderator|member",
      "joined_at": "2025-01-13T12:00:00Z",
      "invited_by": "<inviter-iid>",
      "display_name": "Alice",
      "sender_key_id": "<current-sender-key-id>"
    }
  ],
  "version": "42",
  "updated_at": "2025-01-15T10:00:00Z"
}
```

## Sender Keys

### Concept

Each group member generates a "sender key" - a symmetric key used to encrypt all messages they send to the group. Other members receive this key via secure 1:1 channels.

### Sender Key Structure

```typescript
interface SenderKey {
  keyId: string;           // Unique identifier (16 bytes, base64)
  senderIid: string;       // IID of the sender (for KDF domain separation)
  chainKey: Uint8Array;    // 32-byte chain key
  signatureKey: KeyPair;   // Ed25519 for message signing
  createdAt: Timestamp;
  iteration: number;       // How many messages encrypted with this chain
}
```

### Sender Key Chain

Like the double ratchet sending chain, sender key chains forward for each message using `kdf_sender_key()` from `double-ratchet.md`:

```
def sender_key_encrypt(sender_key: SenderKey, group_id: bytes, plaintext: bytes) -> bytes:
    # Derive message key using kdf_sender_key from double-ratchet.md
    # This uses HMAC-SHA256 with domain separation binding to group/sender/key
    sender_key.chain_key, message_key = kdf_sender_key(
        chain_key=sender_key.chain_key,
        group_id=group_id,
        sender_iid=sender_key.sender_iid,
        key_id=sender_key.key_id
    )
    sender_key.iteration += 1

    # Encrypt
    nonce = generate_nonce()
    ciphertext = ChaCha20Poly1305(message_key, nonce, plaintext)

    # Sign the ciphertext
    signature = Ed25519Sign(sender_key.signature_key.private, ciphertext)

    return encode(sender_key.key_id, sender_key.iteration, nonce, ciphertext, signature)
```

## Key Distribution

### Initial Distribution

When Alice creates a group:

1. Alice generates her sender key
2. For each invited member Bob:
   - Alice sends `sender_key_share` message via 1:1 ratchet
   - Bob verifies Alice's identity and stores sender key

```json
{
  "type": "sender_key_share",
  "content": {
    "group_id": "<group-id>",
    "sender_iid": "<alice-iid>",
    "key_id": "<sender-key-id>",
    "chain_key": "<base64-32-bytes>",
    "signature_public_key": "<base64-32-bytes>",
    "iteration": 0
  }
}
```

### New Member Joining

When Bob is invited:

1. Inviter (admin/mod) sends invite to Bob
2. Each existing member sends their current sender key to Bob
3. Bob generates his sender key and shares with all members
4. Bob can now send and receive group messages

### Key Rotation

Sender keys should rotate periodically for forward secrecy:

1. Member generates new sender key
2. Member sends `sender_key_share` to all other members
3. New messages use new key
4. Old key retained briefly for late-arriving messages

Rotation triggers:
- Every 100 messages
- Every 7 days
- After member leaves (if security concern)

## Message Format

### Group Message Envelope

Group messages use the **unified Secure Envelope** (PUSE) format with a Group Header Extension. See `secure-envelope.md` for the full envelope structure.

Key differences from 1:1 messages:
- Recipient field contains `group_id` (not individual IID)
- Flags byte: recipient type = 0x01 (group)
- Header extension type = 0x02 (group)
- Encryption uses sender key chain, not double ratchet

```
Group Header Extension (inside Secure Envelope):
┌────────────────────────────────────────┐
│ Extension Type: 0x02 (group)           │ 1 byte
├────────────────────────────────────────┤
│ Sender Key ID                          │ 16 bytes
├────────────────────────────────────────┤
│ Sender Key Iteration                   │ 4 bytes (big-endian)
└────────────────────────────────────────┘
Total: 21 bytes
```

The entire envelope (including header extension) is signed by the sender's Ed25519 signing key, providing:
- Proof of sender identity
- Tamper detection for all header fields
- Protection against iteration/key-id manipulation

### Message Content

Same as 1:1 messages (see secure-envelope.md):

```json
{
  "type": "text",
  "id": "<message-uuid>",
  "timestamp": "2025-01-13T12:00:00Z",
  "sequence": "42",
  "thread_id": null,
  "reply_to": null,
  "content": {
    "text": "Hello, group!",
    "mentions": []
  }
}
```

## Membership Operations

### Group State Update Model

All membership changes are represented as **Group State Updates** - signed operations that modify the group's membership state.

```typescript
interface GroupStateUpdate {
  group_id: string;
  version: string;                 // Monotonically increasing (decimal string)
  actor_iid: IdentityIdentifier;   // Who performed this action
  action: GroupAction;
  timestamp: string;               // RFC3339
  previous_version: string;        // Version this update builds on
}

type GroupAction =
  | { type: 'member_added'; member_iid: string; role: string; invited_by: string }
  | { type: 'member_removed'; member_iid: string; removed_by: string; reason?: string }
  | { type: 'member_left'; member_iid: string }
  | { type: 'role_changed'; member_iid: string; old_role: string; new_role: string }
  | { type: 'settings_changed'; changes: Record<string, unknown> };
```

**Authentication**: Group state updates are sent as regular group messages. The PUSE envelope signature authenticates the actor.

**Authorization**: Recipients verify:
1. `actor_iid` matches envelope sender (PUSE signature)
2. Actor has permission for the action (role check against local state)

| Action | Required Role |
|--------|---------------|
| member_added | admin or moderator |
| member_removed | admin (or moderator if target is member) |
| member_left | self only |
| role_changed | admin only |
| settings_changed | admin only |

**Conflict Resolution** (deterministic):
1. Higher `version` wins
2. If same `version`: lexicographically smaller `actor_iid` wins
3. If same `actor_iid`: lexicographically smaller `action.type` wins

**Convergence**: All members eventually reach the same membership state by applying valid updates in version order.

### Invite

Admin sends invite via 1:1 channel:

```json
{
  "type": "group_invite",
  "content": {
    "group_id": "<group-id>",
    "group_name": "Project Team",
    "inviter_iid": "<admin-iid>",
    "role": "member",
    "expires_at": "2025-01-20T12:00:00Z"
  }
}
```

### Accept Invite

Invitee responds:

```json
{
  "type": "group_invite_response",
  "content": {
    "group_id": "<group-id>",
    "accepted": true
  }
}
```

Then existing members share their sender keys.

### Leave

Member sends to group:

```json
{
  "type": "group_event",
  "content": {
    "event": "member_left",
    "member_iid": "<leaving-iid>"
  }
}
```

### Remove (Kick)

Admin sends to group:

```json
{
  "type": "group_event",
  "content": {
    "event": "member_removed",
    "member_iid": "<removed-iid>",
    "removed_by": "<admin-iid>",
    "reason": "Policy violation"
  }
}
```

After removal, remaining members SHOULD rotate sender keys (removed member has old keys).

## History Visibility

### Options

| Setting | Behavior |
|---------|----------|
| `none` | New members see no history |
| `joined` | See messages from when they joined |
| `invited` | See messages from when they were invited |
| `shared` | New members receive full history (encrypted) |

### Sharing History

For `shared` visibility:

1. New member joins
2. Existing member (usually inviter) encrypts history chunks
3. Chunks sent via 1:1 channel to new member
4. New member integrates history into local view

```json
{
  "type": "group_history",
  "content": {
    "group_id": "<group-id>",
    "chunk_index": 0,
    "total_chunks": 5,
    "messages": [
      // Array of decrypted message objects
    ]
  }
}
```

## Delivery

### Fanout Options

| Method | Pros | Cons |
|--------|------|------|
| Direct fanout | Low latency | Sender does O(n) sends |
| Relay fanout | Single send | Relay sees group graph |
| DHT/gossip | Decentralized | Higher latency |

Default: Direct fanout with relay fallback for offline members.

### Delivery Flow

```
Alice sends group message:

1. Encrypt once with sender key
2. For each online member:
   - Send via QUIC message stream
3. For each offline member:
   - Determine mailbox from identity document
   - Send to mailbox

Members receive:
1. Receive via QUIC or mailbox
2. Look up Alice's sender key for this group
3. Verify iteration is valid (no replay)
4. Decrypt using sender key chain
5. Verify signature
```

## Conflict Resolution

### Membership Conflicts

If two admins make conflicting changes:

1. Changes include group `version` (sequence number)
2. Higher version wins
3. Same version: lexicographically smaller admin IID wins
4. Conflicts logged, may require manual resolution

### Message Ordering

Within a group, messages are ordered by:

1. Timestamp (primary)
2. Sender IID (secondary, for same timestamp)
3. Message ID (tertiary)

This provides consistent ordering across members.

## Security Considerations

### Sender Key Compromise

If Alice's sender key is compromised:

- Attacker can send fake messages as Alice (until rotation)
- Attacker can decrypt future messages (until rotation)
- Past messages remain secure (forward secrecy via chain)

Mitigation: Regular rotation, immediate rotation on suspected compromise.

### Removed Member

Removed member retains:

- All sender keys at time of removal
- Can decrypt future messages until rotation

Mitigation: Rotate all sender keys immediately after removing a member.

### Key Confirmation Attack

Malicious member could:

- Claim to have sent a message they didn't send
- Claim another member said something

Mitigation: Sender key includes signing key; verify signatures.

## Error Handling

| Error | Condition | Action |
|-------|-----------|--------|
| `UNKNOWN_SENDER_KEY` | Key ID not found | Request key from sender |
| `ITERATION_TOO_LOW` | Replay attempt | Reject message |
| `ITERATION_GAP` | Missed messages | Request resync from sender |
| `SIGNATURE_INVALID` | Signature verification failed | Reject message |
| `NOT_A_MEMBER` | Sender not in group | Reject message |
| `INSUFFICIENT_PERMISSIONS` | Action requires higher role | Reject action |

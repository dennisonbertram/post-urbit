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

### Group Identifier (Normative)

A Group Identifier (GroupId) is a 20-byte value encoded as a 32-character lowercase Crockford Base32 string (same encoding rules as IID/DID).

**Derivation inputs:**
- `creator_iid_raw`: 20 bytes, the creator's IID **raw bytes** (Crockford Base32 decoded)
- `random`: 32 bytes, cryptographically random
- `created_at_utf8`: canonical RFC3339 UTC timestamp string encoded as UTF-8, exactly 20 bytes: `YYYY-MM-DDTHH:MM:SSZ` (no fractional seconds)

**Derivation:**
```
group_id_raw = SHA256( creator_iid_raw || random || created_at_utf8 )[0:20]
group_id     = CrockfordBase32Lower(group_id_raw)  // 32 chars
```

**Notes:**
- Implementations MUST use the raw 20-byte IID for `creator_iid_raw`, not the Base32 string.
- Implementations MUST use the canonical timestamp form to ensure deterministic IDs.
- In cryptographic KDF inputs (e.g., sender-key KDF), use the raw 20-byte `group_id_raw`.

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
      "role": "owner|admin|moderator|member",
      "joined_at": "2025-01-13T12:00:00Z",
      "invited_by": "<inviter-iid>",
      "display_name": "Alice",
      "sender_key_id": "<current-sender-key-id>"
    }
  ],
  "version": "42.a1b2c3d4",
  "updated_at": "2025-01-15T10:00:00Z"
}
```

**Version Format (Normative):**
The `version` field in both `GroupMembership` snapshots and `GroupStateUpdate` messages MUST use the format `"<logical_clock>.<first_8_chars_of_actor_iid>"` where:
- `logical_clock`: decimal integer (e.g., "42")
- `first_8_chars_of_actor_iid`: first 8 characters of the Crockford Base32-encoded IID of the actor who created this version (e.g., "a1b2c3d4")

The genesis version (when group is created) MUST use `"0.<first_8_chars_of_creator_iid>"` with `previous_version` set to `null`.

## Sender Keys

### Concept

Each group member generates a "sender key" - a symmetric key used to encrypt all messages they send to the group. Other members receive this key via secure 1:1 channels.

### Sender Key Structure

```typescript
interface SenderKey {
  keyId: string;           // Unique identifier (16 bytes raw, stored as base64)
  senderIid: string;       // IID of the sender (20 bytes raw, stored as Base32)
  chainKey: Uint8Array;    // 32-byte chain key
  createdAt: Timestamp;
  iteration: number;       // How many messages encrypted with this chain
}
```

**KDF encoding note:** When calling `kdf_sender_key()`, string fields MUST be decoded to raw bytes:
- `keyId`: Base64 decode → 16 bytes
- `senderIid`: Crockford Base32 decode → 20 bytes
- `group_id`: Crockford Base32 decode → 20 bytes (from group context)

See `double-ratchet.md` "Group Sender Key KDF" for normative encoding requirements.

**Signature model:** Group messages are signed using the **PUSE envelope signature** with the sender's identity signing key (from their identity document). There is no separate sender-key signature. This provides:
- Non-repudiation tied to identity
- Consistent verification model with 1:1 messages
- Simpler key management (no per-sender-key signature keys)

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

    # Note: Signing is done at the PUSE envelope level using the sender's
    # identity signing key, not here. See secure-envelope.md.

    return encode(sender_key.key_id, sender_key.iteration, nonce, ciphertext)
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

**Sender Key Iteration (Normative):**
- The `iteration` field in the group header extension is **1-indexed**: the first encrypted group message uses iteration=1
- Value **0 is invalid**; implementations MUST reject group envelopes with `iteration = 0`
- This differs from `sender_key_share.content.iteration = 0` which is a state hint (pre-first-message), not a wire value
- See RFC-0003 §3.4.4 for the authoritative iteration counter specification

The entire envelope (including header extension) is signed by the sender's Ed25519 signing key, providing:
- Proof of sender identity
- Tamper detection for all header fields
- Protection against iteration/key-id manipulation

### Message Content

Same as 1:1 messages (see secure-envelope.md):

```json
{
  "type": "text",
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

**Note:** The `message_id` is in the envelope header, NOT in the plaintext (see secure-envelope.md).

## Membership Operations

### Group State Update Model

All membership changes are represented as **Group State Updates** - signed operations that modify the group's membership state.

**Wire Format: See RFC-0003 §8.6 (Authoritative)**

The `group_state_update` PUSE message content MUST follow RFC-0003 §8.6:

```typescript
// Wire format (content of PUSE plaintext)
interface GroupStateUpdateContent {
  action: 'add_member' | 'remove_member' | 'promote_admin' | 'demote_admin' | 'update_info' | 'rotate_sender_key';
  group_id: string;      // 32-char Crockford Base32 (20 raw bytes encoded)
  target_iid?: string;   // For member actions: IID of affected member
  version: string;       // Format: "<logical_clock>.<actor_suffix>" where actor_suffix is first 8 chars of actor's IID
}

// Version format: Each actor maintains a local logical clock.
// On update: version = max(local_clock, max_seen_version) + 1
// Full version string: "<logical_clock>.<first_8_chars_of_actor_iid>"
// This prevents collisions without requiring coordination.

// Internal model (MAY include additional fields for local state tracking)
interface GroupStateUpdateInternal extends GroupStateUpdateContent {
  actor_iid: string;       // Derived from PUSE sender IID
  timestamp: string;       // RFC3339, from PUSE plaintext
  previous_version?: string; // For local DAG tracking (not on wire)
}
```

**Authentication**: Group state updates are authenticated via the **PUSE envelope signature**. There is no separate content-level signature. The actor IID is derived from the PUSE sender.

**Authorization**: Recipients verify:
1. PUSE sender has permission for the action (role check against local state)
2. Actor has required role per table below

| Wire Action | Required Role |
|-------------|---------------|
| add_member | owner, admin, or moderator |
| remove_member | owner or admin (or moderator if target is member) |
| promote_admin | owner only |
| demote_admin | owner only |
| update_info | owner or admin |
| rotate_sender_key | owner or admin |

**Conflict Resolution (Normative):**

Version comparison MUST use numeric ordering for the logical clock component:

1. Parse version strings as `(logical_clock: int, actor_suffix: string)`
2. Compare `logical_clock` as **unsigned integers** (NOT lexicographically)
3. If same `logical_clock`: lexicographically smaller `actor_suffix` wins (case-sensitive)
4. If same `version`: lexicographically smaller `actor_iid` wins
5. If same `actor_iid`: lexicographically smaller `action.type` wins

**Example:** Version `"10.a1b2c3d4"` > `"2.z9y8x7w6"` because `10 > 2` numerically.

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

### Leave (via Remove Self)

Member sends `group_state_update` PUSE message to remove themselves. Per RFC-0003 §8.6, the wire format is:

```json
{
  "type": "group_state_update",
  "timestamp": "2025-01-15T12:00:00Z",
  "sequence": "43",
  "content": {
    "action": "remove_member",
    "group_id": "abzy73bycgb9ybrg12tynyxgkfzyh3bk",
    "target_iid": "k5xq7z4mj3c9yfv0kh2lpm6ngqbya8rx",
    "version": "43.a1b2c3d4"
  }
}
```

**Note:** "Leave" is implemented as `remove_member` where `target_iid == PUSE sender`. Authorization rules allow self-removal.

### Remove (Kick)

Admin sends `group_state_update` PUSE message. Per RFC-0003 §8.6:

```json
{
  "type": "group_state_update",
  "timestamp": "2025-01-15T12:01:00Z",
  "sequence": "44",
  "content": {
    "action": "remove_member",
    "group_id": "abzy73bycgb9ybrg12tynyxgkfzyh3bk",
    "target_iid": "removed3bycgb9ybrg12tynyxgkfzyh3",
    "version": "44.a1b2c3d4"
  }
}
```

After removal, remaining members SHOULD rotate sender keys (removed member has old keys).

**Wire Format (Normative per RFC-0003 §8.6):**

- All group membership operations MUST be sent as PUSE messages with `"type": "group_state_update"`
- The ad-hoc `"type": "group_event"` format is deprecated and MUST NOT be used
- Implementations MUST reject messages with `"type": "group_event"`
- No separate signature field exists in the content - authentication is via the PUSE envelope signature
- Action types MUST match RFC-0003 §8.6: `add_member`, `remove_member`, `promote_admin`, `demote_admin`, `update_info`, `rotate_sender_key`

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

Default: Direct fanout with mailbox fallback for offline members.

### Delivery Flow

```
Alice sends group message:

1. Encrypt once with sender key
   - PUSE envelope recipient field = group_id
   - Flags recipient_type = 0x01 (group)
2. For each online member:
   - Send via QUIC message stream (same envelope to each)
3. For each offline member:
   - Determine mailbox from member's identity document
   - POST to mailbox: POST /messages/{member_iid}
   - The same PUSE envelope (with group_id as recipient) is stored
     under each member's individual inbox

Members receive:
1. Receive via QUIC or retrieve from own mailbox
2. Parse PUSE envelope:
   - Check flags recipient_type = 0x01 (group)
   - Extract group_id from recipient field
3. Look up Alice's sender key for this group
4. Verify iteration is valid (no replay)
5. Decrypt using sender key chain
6. Verify PUSE envelope signature
```

### Mailbox Delivery for Group Messages (Normative)

**Problem:** For group messages, the PUSE envelope's `recipient` field contains the `group_id`, not individual member IIDs. Mailboxes index messages by inbox owner IID for retrieval.

**Solution:** The mailbox store API accepts an explicit `inbox_owner_iid` path parameter separate from the PUSE envelope:

```
POST /messages/{member_iid}
Content-Type: application/octet-stream

<PUSE envelope with group_id as recipient>
```

The sender fans out to each offline member's mailbox individually:

1. **Same envelope, multiple stores:** The identical PUSE envelope (encrypted once with sender key) is stored in multiple mailboxes
2. **Storage keyed by member IID:** Each mailbox stores the envelope under the `member_iid` from the URL path
3. **PUSE envelope unchanged:** The group_id remains in the envelope's recipient field for decryption context
4. **Retrieval by inbox owner:** Each member retrieves from their own inbox using `token.iid`

See RFC-0003 §7.4.1 "Storage Keying and Routing" for the authoritative specification.

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

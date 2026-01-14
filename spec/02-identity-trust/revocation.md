# Key Revocation

## Overview

Revocation declares that a key (or entire identity) has been compromised and should no longer be trusted. Unlike rotation (which maintains continuity), revocation is an emergency measure.

## Revocation Types

| Type | Scope | Use Case |
|------|-------|----------|
| **Key revocation** | Specific key version | Suspected compromise of one key |
| **Identity revocation** | Entire identity | Complete compromise, abandoning identity |

## Key Revocation

Revoke a specific key while maintaining identity continuity (equivalent to emergency rotation).

### Revocation Document

```json
{
  "type": "key_revocation",
  "iid": "<identity>",
  "revoked_key": "<base64-public-key-being-revoked>",
  "revoked_key_type": "signing|encryption",
  "reason": "compromised|lost|superseded",
  "effective_at": "<RFC3339-timestamp>",
  "replacement_document": { <new-identity-document> },
  "signatures": {
    "by_revoked_key": "<sig-by-key-being-revoked>|null",
    "by_new_key": "<sig-by-new-key>",
    "by_recovery": { <recovery-proof-if-used> }
  }
}
```

### Revocation Scenarios

#### Scenario A: Key holder initiates (normal rotation path)

User still has access to the compromised key:

1. Generate new keys
2. Create revocation document
3. Sign with BOTH old and new keys
4. Publish immediately (no cooldown)

```json
{
  "signatures": {
    "by_revoked_key": "<valid-signature>",
    "by_new_key": "<valid-signature>",
    "by_recovery": null
  }
}
```

#### Scenario B: Key lost, use recovery

User lost access to the compromised key:

1. Generate new keys
2. Initiate recovery (see recovery-mechanisms.md)
3. Create revocation document with recovery proof
4. Subject to recovery cooldown

```json
{
  "signatures": {
    "by_revoked_key": null,
    "by_new_key": "<valid-signature>",
    "by_recovery": {
      "method": "social",
      "attestations": [...]
    }
  }
}
```

## Identity Revocation

Permanently abandon an identity. This is a terminal state.

### Use Cases

- Complete compromise with no recovery possible
- Legal requirement to abandon identity
- User wants to "disappear" this identity

### Revocation Document

```json
{
  "type": "identity_revocation",
  "iid": "<identity>",
  "reason": "compromised|abandoned|legal",
  "message": "<optional-human-readable-message>",
  "effective_at": "<RFC3339-timestamp>",
  "successor_iid": "<optional-new-identity-if-migrating>",
  "signature": "<sig-by-current-signing-key>"
}
```

### Effects of Identity Revocation

1. **Identity is permanently dead** - no further updates accepted
2. **Peers should stop messaging** this identity
3. **Successor hint** allows migration path if desired
4. **Historical messages** remain readable but identity is marked revoked

## Revocation Propagation

Revocations must propagate quickly to prevent attackers from impersonating.

### Propagation Protocol

1. **Immediate push** to all connected peers
2. **DHT/directory update** with revocation notice
3. **Relay notification** to stop forwarding to old keys
4. **Gossip protocol** for network-wide propagation

### Revocation Envelope

```json
{
  "type": "revocation_notice",
  "priority": "critical",
  "document": { <revocation-document> },
  "ttl": 86400
}
```

### Peer Behavior on Receiving Revocation

```
function handle_revocation(revocation):
    # Verify revocation is valid
    if not verify_revocation(revocation):
        return REJECT

    # Update local identity cache
    mark_revoked(revocation.iid, revocation.effective_at)

    # If key revocation, update to new document
    if revocation.type == "key_revocation":
        store_identity(revocation.replacement_document)

    # Propagate to other peers
    gossip(revocation)

    # Warn user if they have active conversations
    if has_conversation_with(revocation.iid):
        notify_user("Contact's identity has changed")
```

## Revocation Verification

```typescript
function verifyKeyRevocation(revocation: KeyRevocation): VerificationResult {
  const { iid, revoked_key, replacement_document, signatures } = revocation;

  // Verify new document is valid
  const newDocValid = verifyIdentityDocument(replacement_document);
  if (!newDocValid) return { valid: false, error: 'INVALID_REPLACEMENT' };

  // Verify IID matches
  if (replacement_document.iid !== iid) {
    return { valid: false, error: 'IID_MISMATCH' };
  }

  // Verify sequence increased
  const oldDoc = getStoredIdentity(iid);
  if (replacement_document.sequence <= oldDoc.sequence) {
    return { valid: false, error: 'SEQUENCE_REGRESSION' };
  }

  // Verify authorization (at least one valid path)
  const hasOldKeyAuth = signatures.by_revoked_key &&
    verify(revoked_key, revocation, signatures.by_revoked_key);

  const hasNewKeyAuth = signatures.by_new_key &&
    verify(replacement_document.keys.signing.current, revocation, signatures.by_new_key);

  const hasRecoveryAuth = signatures.by_recovery &&
    verifyRecoveryProof(oldDoc, signatures.by_recovery);

  if (hasOldKeyAuth && hasNewKeyAuth) {
    // Normal revocation path
    return { valid: true, path: 'key_holder' };
  }

  if (hasNewKeyAuth && hasRecoveryAuth) {
    // Recovery path
    return { valid: true, path: 'recovery' };
  }

  return { valid: false, error: 'INSUFFICIENT_AUTHORIZATION' };
}
```

## Revocation Lists

Nodes maintain a local revocation list for quick lookup:

```typescript
interface RevocationList {
  // Check if a key is revoked
  isKeyRevoked(publicKey: string): boolean;

  // Check if an identity is revoked
  isIdentityRevoked(iid: string): boolean;

  // Get revocation details
  getRevocation(iid: string): RevocationRecord | null;

  // Add new revocation
  addRevocation(revocation: KeyRevocation | IdentityRevocation): void;

  // Prune old revocations (keep for 1 year, then archive)
  prune(olderThan: Date): void;
}

interface RevocationRecord {
  iid: string;
  type: 'key' | 'identity';
  revokedAt: Date;
  reason: string;
  revokedKeys: string[];
  successorIid?: string;
}
```

## Security Considerations

### Race Conditions

Attacker with compromised key may try to revoke the legitimate user's recovery.

**Mitigation**:
- Recovery cooldown gives legitimate user time to counter-revoke
- Multiple signatures required (harder for attacker to forge)
- Out-of-band notification to trustees

### Replay Attacks

Old revocation documents could be replayed.

**Mitigation**:
- Timestamp + sequence number
- Revocation is permanent (can't be "un-revoked")

### Gossip Amplification

Malicious actors could flood network with fake revocations.

**Mitigation**:
- Verify signature before gossiping
- Rate limit revocation gossip
- Require valid identity chain

## State Transitions

```
┌─────────────┐
│   ACTIVE    │ ← Normal operation
└──────┬──────┘
       │
       │ key_revocation
       ▼
┌─────────────┐
│   ACTIVE    │ ← New keys, old revoked
│ (new keys)  │
└──────┬──────┘
       │
       │ identity_revocation
       ▼
┌─────────────┐
│   REVOKED   │ ← Terminal state
└─────────────┘
```

## Interfaces

```typescript
interface RevocationManager {
  // Revoke a specific key (emergency rotation)
  revokeKey(
    currentDoc: IdentityDocument,
    keyToRevoke: 'signing' | 'encryption',
    reason: RevocationReason,
    newKeys: { signing?: KeyPair; encryption?: KeyPair },
    signingPrivate: PrivateKey
  ): Promise<KeyRevocation>;

  // Revoke using recovery (no access to old key)
  revokeKeyWithRecovery(
    currentDoc: IdentityDocument,
    keyToRevoke: 'signing' | 'encryption',
    reason: RevocationReason,
    newKeys: { signing: KeyPair; encryption: KeyPair },
    recoveryProof: RecoveryProof
  ): Promise<KeyRevocation>;

  // Permanently revoke entire identity
  revokeIdentity(
    currentDoc: IdentityDocument,
    reason: RevocationReason,
    message?: string,
    successorIid?: string,
    signingPrivate: PrivateKey
  ): Promise<IdentityRevocation>;

  // Verify a revocation
  verifyRevocation(
    revocation: KeyRevocation | IdentityRevocation
  ): VerificationResult;

  // Propagate revocation to network
  propagateRevocation(
    revocation: KeyRevocation | IdentityRevocation
  ): Promise<PropagationResult>;
}

type RevocationReason = 'compromised' | 'lost' | 'superseded' | 'abandoned' | 'legal';
```

## Test Scenarios

1. **Key revocation with old key**: User has compromised key, revokes it, peers accept new key
2. **Key revocation via recovery**: User lost key, uses social recovery to revoke, cooldown period observed
3. **Identity revocation**: User permanently abandons identity, peers stop messaging
4. **Revocation race**: Attacker tries to revoke before legitimate user, legitimate user wins via recovery
5. **Propagation**: Revocation reaches all peers within expected time
6. **Stale revocation rejected**: Old revocation document is rejected due to sequence number

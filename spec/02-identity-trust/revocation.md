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
    "by_current_signing_key": "<sig-by-current-signing-key>",
    "by_new_signing_key": "<sig-by-new-signing-key>|null"
  },
  "recovery_proof": null
}
```

**Signature Requirements by Key Type:**

| Revoking | Has Old Key | Required Signatures |
|----------|-------------|---------------------|
| Signing key | Yes | `by_current_signing_key` (old) + `by_new_signing_key` (new) |
| Signing key | No | `by_new_signing_key` + `recovery_proof` |
| Encryption key | Yes | `by_current_signing_key` only (X25519 can't sign) |
| Encryption key | No | `by_new_signing_key` + `recovery_proof` |

**Note:** X25519 keys cannot create signatures. Encryption key revocation is always authorized by the signing key. When recovery is used, `recovery_proof` contains the standard recovery proof structure (see `identity-document-schema.md`). The `replacement_document` also contains `recovery_proof` per the standard identity document format.

### Revocation Scenarios

#### Scenario A: Signing key revocation (has old key)

User still has access to the compromised signing key:

1. Generate new signing key
2. Create revocation document
3. Sign with BOTH old signing key and new signing key
4. Publish immediately (no cooldown)

```json
{
  "revoked_key_type": "signing",
  "signatures": {
    "by_current_signing_key": "<sig-by-old-signing-key>",
    "by_new_signing_key": "<sig-by-new-signing-key>"
  },
  "recovery_proof": null
}
```

#### Scenario B: Signing key revocation (key lost)

User lost access to the compromised signing key:

1. Generate new signing key
2. Initiate recovery (see recovery-mechanisms.md)
3. Create revocation document with recovery proof
4. Subject to recovery cooldown

```json
{
  "revoked_key_type": "signing",
  "signatures": {
    "by_current_signing_key": null,
    "by_new_signing_key": "<sig-by-new-signing-key>"
  },
  "recovery_proof": {
    "method": "social",
    "initiated_at": "<RFC3339-timestamp>",
    "cooldown_expires_at": "<RFC3339-timestamp>",
    "status": "pending",
    "proof_data": {
      "attestations": [...]
    }
  }
}
```

#### Scenario C: Encryption key revocation

User wants to revoke an encryption key. X25519 keys cannot sign, so authorization is always via the signing key:

1. Generate new encryption key
2. Create revocation document
3. Sign with current signing key only
4. Publish immediately (no cooldown)

```json
{
  "revoked_key_type": "encryption",
  "signatures": {
    "by_current_signing_key": "<sig-by-current-signing-key>",
    "by_new_signing_key": null
  },
  "recovery_proof": null
}
```

**Note:** If the signing key is also compromised (and encryption key revocation is needed), revoke the signing key first using recovery, then revoke the encryption key.

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

### DHT Storage (Normative)

Revocations MUST be stored in DHT for later discovery by nodes that were offline during propagation.

**DHT Key for Revocations:**
```
Key = SHA256("post-urbit:revocation:" || iid)  # iid is 32-char Crockford Base32 string, UTF-8 encoded
```

**DHT Value:**
- For `key_revocation`: Store the full `key_revocation` document (JCS-canonical JSON), which includes the `replacement_document` and `signatures` fields
- For `identity_revocation`: Store the `identity_revocation` document (JCS-canonical JSON)

See RFC-0001 §10 for the exact document schemas and signature construction.

**Storage Rules:**
- DHT nodes MUST verify revocation signatures before storing:
  - For `key_revocation`: Verify `signatures.by_current_signing_key` and/or `signatures.by_new_signing_key` per RFC-0001 §10.1
  - For `identity_revocation`: Verify `signature` field per RFC-0001 §10.2
- TTL: 365 days (revocations are long-lived)
- Multiple revocations for same IID: keep **earliest** `effective_at` timestamp (security-conservative, per RFC-0001 §12.7)

**Lookup Behavior:**
When verifying an identity document, implementations SHOULD:
1. Fetch identity from `SHA256("post-urbit:identity:" || iid)`
2. Also fetch any revocation from `SHA256("post-urbit:revocation:" || iid)`
3. If revocation exists and is valid, treat identity as revoked

This ensures nodes that come online later can discover revocations even if they missed the gossip propagation.

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
  const { iid, revoked_key, revoked_key_type, replacement_document, signatures, recovery_proof } = revocation;

  // Verify new document is valid
  const newDocValid = verifyIdentityDocument(replacement_document);
  if (!newDocValid) return { valid: false, error: 'INVALID_REPLACEMENT' };

  // Verify IID matches
  if (replacement_document.iid !== iid) {
    return { valid: false, error: 'IID_MISMATCH' };
  }

  // Verify sequence increased (use BigInt for numeric comparison of decimal strings)
  const oldDoc = getStoredIdentity(iid);
  if (BigInt(replacement_document.sequence) <= BigInt(oldDoc.sequence)) {
    return { valid: false, error: 'SEQUENCE_REGRESSION' };
  }

  // Verify revoked_key matches stored document
  if (revoked_key_type === 'signing') {
    if (revoked_key !== oldDoc.keys.signing.current &&
        !oldDoc.keys.signing.history?.some(h => h.key === revoked_key)) {
      return { valid: false, error: 'REVOKED_KEY_NOT_FOUND' };
    }
  } else if (revoked_key_type === 'encryption') {
    if (revoked_key !== oldDoc.keys.encryption.current &&
        !oldDoc.keys.encryption.previous?.some(h => h.key === revoked_key)) {
      return { valid: false, error: 'REVOKED_KEY_NOT_FOUND' };
    }
  }

  // Verify authorization based on key type
  if (revoked_key_type === 'signing') {
    // Signing key revocation requires either:
    // - Both old and new signing key signatures, OR
    // - New signing key signature + recovery proof
    const hasOldKeyAuth = signatures.by_current_signing_key &&
      verify(oldDoc.keys.signing.current, revocation, signatures.by_current_signing_key);

    const hasNewKeyAuth = signatures.by_new_signing_key &&
      verify(replacement_document.keys.signing.current, revocation, signatures.by_new_signing_key);

    const hasRecoveryAuth = recovery_proof &&
      verifyRecoveryProof(oldDoc, recovery_proof);

    if (hasOldKeyAuth && hasNewKeyAuth) {
      return { valid: true, path: 'key_holder' };
    }

    if (hasNewKeyAuth && hasRecoveryAuth) {
      // NOTE: recovery_proof.status is informational; cooldown is determined solely by cooldown_expires_at
      return { valid: true, path: 'recovery', cooldown: Date.now() < new Date(recovery_proof.cooldown_expires_at).getTime() };
    }

  } else if (revoked_key_type === 'encryption') {
    // Encryption key revocation: X25519 can't sign, so we need current signing key
    const hasSigningAuth = signatures.by_current_signing_key &&
      verify(oldDoc.keys.signing.current, revocation, signatures.by_current_signing_key);

    if (hasSigningAuth) {
      return { valid: true, path: 'signing_key_auth' };
    }

    // If signing key is also being revoked (via recovery), allow recovery path
    const hasRecoveryAuth = recovery_proof &&
      verifyRecoveryProof(oldDoc, recovery_proof);

    const hasNewKeyAuth = signatures.by_new_signing_key &&
      verify(replacement_document.keys.signing.current, revocation, signatures.by_new_signing_key);

    if (hasNewKeyAuth && hasRecoveryAuth) {
      return { valid: true, path: 'recovery', cooldown: Date.now() < new Date(recovery_proof.cooldown_expires_at).getTime() };
    }
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

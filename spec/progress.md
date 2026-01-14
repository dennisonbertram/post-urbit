# Specification Progress

## Iteration: 2
## Mode: HOLISTIC REVIEW (even iteration)
## Status: 25/100 completeness estimate

### Fully Specified
- (none yet - 02-identity-trust is comprehensive but needs final polish)

### In Progress
- **02-identity-trust**: Holistic review complete, major fixes applied
  - ✅ overview.md
  - ✅ identity-document-schema.md (updated with safer conflict resolution)
  - ✅ key-rotation.md
  - ✅ recovery-mechanisms.md
  - ✅ revocation.md
  - ✅ name-resolution.md
  - ✅ interfaces.md (major updates: genesis key, recovery proof, X25519 clarification)
  - ✅ caching-policy.md (NEW - TOFU, TTL, resolution sources)

### Not Yet Started
- 01-transport-connectivity: Ready to start (next priority)
- 03-messaging-sync: Blocked by identity + transport
- 04-app-runtime: Blocked by messaging/sync primitives
- 05-ux-packaging: Can proceed in parallel
- 06-rfcs: Can draft RFC-0001 (identity document)
- 07-implementation: Blocked by component specs
- 08-security: Can proceed in parallel
- 09-governance: Can proceed in parallel

### GPT-5.2 Review Log
- **Iteration 1 (deep dive)**: 02-identity-trust - initial component creation
- **Iteration 2 (holistic)**: Cross-cutting review findings and fixes:

  **High-Priority Fixes Applied:**
  1. ✅ Added `keys.signing.genesis` to TypeScript interfaces
  2. ✅ Added `recoveryProof` to IdentityDocument type
  3. ✅ Fixed KeyStorage - renamed `decryptWith` to `deriveSharedSecret` (X25519 is ECDH, not encryption)
  4. ✅ Added encryption scheme contract (X25519 + HKDF + ChaCha20-Poly1305)
  5. ✅ Fixed conflict resolution - removed gameable hash tiebreaker, added TOFU + manual resolution
  6. ✅ Added caching-policy.md with full resolution/TTL/TOFU specification
  7. ✅ Added multiple encryption.previous keys support (EncryptionKeyHistory[])

  **Remaining Issues (to address in future iterations):**
  - Endpoint normalization rules (hostname/IP canonicalization)
  - Concrete test vectors (not just placeholders)
  - Transport integration contract needs Transport layer spec
  - Timestamp validation for revocations/attestations (currently only on identity docs)

### Holistic Health Check
- [x] All interfaces align across components (fixed genesis key, recovery proof, key storage)
- [x] No contradictions between specs (fixed conflict resolution)
- [x] Dependencies form a DAG (no cycles)
- [x] Core vision preserved ("no permanent root keys" clarified as "no permanent secret keys")
- [x] Appropriate level of detail (added caching policy, still need test vectors)

### Next Priority
**Iteration 3 will be DEEP DIVE on 01-transport-connectivity**

Transport is the next foundational layer:
- Identity depends on it for propagation
- Messaging depends on it for delivery
- Without transport, identity is not actually deployable

Focus areas:
- QUIC integration
- NAT traversal (STUN/TURN equivalent)
- Relay protocol
- Peer handshake binding to identity

### Critical Path Analysis
```
Identity Document Format (02) ← READY FOR TRANSPORT
    ↓
Peer Handshake (01) ← NEXT PRIORITY
    ↓
Secure Envelope (03)
    ↓
1:1 Messaging (03) → Group Protocol (03)
    ↓
Sync Protocol (03)
    ↓
App Runtime (04)
    ↓
Packaging (05)
```

### Specification Checklist: 02-identity-trust
- [x] Data structures defined
- [x] Interfaces specified (with holistic fixes)
- [x] State machines documented
- [x] Wire formats locked
- [x] Error handling complete
- [x] Dependencies mapped (transport contract defined)
- [ ] Test scenarios written (concrete vectors needed)
- [x] Security considerations documented
- [x] GPT-5.2 review passed (holistic review complete)

### Files Modified This Iteration
1. `interfaces.md` - Added genesis key, recovery proof, fixed X25519 usage
2. `identity-document-schema.md` - Fixed conflict resolution to prevent hash-tiebreaker attack
3. `caching-policy.md` - NEW file with TOFU, TTL, resolution sources

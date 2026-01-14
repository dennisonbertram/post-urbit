# Specification Progress

## Iteration: 7
## Mode: DEEP DIVE (04-app-runtime)
## Status: 85/100 completeness estimate

### Fully Specified
- **02-identity-trust**: Core identity system + Device Identifiers (DID)
- **01-transport-connectivity**: Transport layer + multi-device support
- **00-shared**: Layer integration specs + mailbox auth token
- **03-messaging-sync**: Messaging and sync layer (complete)
- **04-app-runtime**: Application runtime (complete pending final review)

### In Progress
- **04-app-runtime**: Deep dive complete, issues fixed
  - ✅ overview.md - Layer architecture + document index
  - ✅ abi.md - **NEW** Authoritative ABI specification
  - ✅ wasm-sandbox.md - Sandbox config, WASI, execution model
  - ✅ capability-system.md - Permission model + authoritative mapping
  - ✅ api-surface.md - Host API methods + subscription lifecycle
  - ✅ manifest-schema.md - Package format + file integrity hashes
  - ✅ interfaces.md - TypeScript interfaces

### Not Yet Started
- 05-ux-packaging: NEXT
- 06-rfcs: Can draft RFC-0001 through RFC-0003
- 07-implementation: Blocked by component specs
- 08-security: Can proceed in parallel
- 09-governance: Can proceed in parallel

### GPT-5.2 Review Log
- **Iteration 1-4**: Identity and Transport layers refined
- **Iteration 5 (deep dive)**: 03-messaging-sync initial specs
- **Iteration 6 (holistic)**: Cross-layer consistency fixes
- **Iteration 7 (deep dive)**: 04-app-runtime created + reviewed

  **BLOCKING Issues Fixed (Iteration 7):**
  1. ✅ B1: WASM ABI unified in new `abi.md` (imports, exports, signatures)
  2. ✅ B2: Async model specified (polling via host.poll, no callbacks)
  3. ✅ B3: Invocation ABI complete (packed i64 return, memory ownership)
  4. ✅ B4: WASI bypass fixed (clock/random disabled, use Host API)
  5. ✅ B5: Package signature binds to file hashes in manifest
  6. ✅ B6: Storage model clarified (Host API primary, no /data FS)
  7. ✅ B7: Method-to-capability mapping made authoritative (snake_case)

  **HIGH Issues Fixed (Iteration 7):**
  1. ✅ H8: Result envelope format unified (ok/error CBOR)
  2. ✅ H9: Subscription lifecycle defined (persistence, delivery, cleanup)
  3. ✅ H10: Messaging capability simplified (subscribe implies receive)
  4. ✅ H11: Deterministic vs crypto random split into two APIs
  5. ✅ H12: Inter-app invocation semantics (call depth, fuel, exports)
  6. ✅ H13: Revocation behavior specified (no reprompt, defined cleanup)

### Critical Path Analysis
```
Identity (02) ← COMPLETE + DID
    ↓
Transport (01) ← COMPLETE + multi-device
    ↓
Layer Integration (00) ← COMPLETE + mailbox auth
    ↓
Messaging & Sync (03) ← COMPLETE
    ↓
App Runtime (04) ← COMPLETE (pending iteration 8 review)
    ↓
Packaging (05) ← NEXT
```

### Specification Checklist: 04-app-runtime
- [x] WASM sandbox execution model
- [x] Authoritative ABI specification (abi.md)
- [x] Host-to-guest and guest-to-host interfaces
- [x] Async model (polling, no callbacks)
- [x] Capability system with authoritative mapping
- [x] Permission workflow (install, runtime, revocation)
- [x] Storage architecture (Host API primary, no /data FS)
- [x] Manifest schema with file integrity hashes
- [x] Subscription lifecycle semantics
- [x] Inter-app invocation model
- [x] Deterministic vs crypto randomness
- [ ] Complete test vectors
- [ ] SDK interface examples

### Next Priority
**Iteration 8 will be HOLISTIC REVIEW** of all layers including new 04-app-runtime

Focus areas:
- Cross-layer consistency with messaging/sync
- API naming conventions alignment
- Error handling uniformity
- Security model completeness

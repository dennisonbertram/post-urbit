## Iteration 64: HOLISTIC REVIEW

### Cross-Document Check
- **Relay allocation binding semantics conflict (RFC vs layer doc):**
  - `spec/06-rfcs/RFC-0002-transport.md` §7.8 specifies a **two-step** model: HTTPS `/allocate` returns `(allocation_id, token)` but **UDP binding is established by the first UDP PURL packet** (and later updated by REBIND).
  - `spec/01-transport-connectivity/relay-protocol.md` “Relay verification” + “Allocation Binding and Mobility” states allocation is **bound to source IP:port at creation time** (implying binding at HTTPS request time), which is incompatible with RFC-0002’s explicit “HTTPS is TCP, not UDP” binding model.
- **App Runtime method registry inconsistency (reserved vs implemented):**
  - `spec/04-app-runtime/abi.md` explicitly labels several methods as **reserved** (must return `NOT_IMPLEMENTED`), e.g. `storage.shared.get/set`, `messaging.unsubscribe`, `messaging.list_groups`, `sync.get_document`, `sync.subscribe`, `sync.share`, `notifications.cancel`, `contacts.get`, etc.
  - `spec/04-app-runtime/api-surface.md` and `spec/04-app-runtime/capability-system.md` list many of these in the “Methods” tables / `CAPABILITY_MAP` as if they are part of the usable API surface, without consistently marking them “reserved” and without schemas in `api-surface.md`. This creates ambiguity for both host implementers and app authors.

### Blocking Issues (B1, B2, etc.)
**B1 — Relay allocation UDP binding timing mismatch**
- **Why blocking:** Relay implementers following `01/relay-protocol.md` can bind tokens to the wrong 5-tuple (TCP source port), breaking real relay delivery and rebind behavior.
- **Fix:** Update `spec/01-transport-connectivity/relay-protocol.md` to match RFC-0002:
  - Allocation created in “pending UDP bind” state.
  - UDP bind established on first valid PURL packet (token-based).
  - REBIND updates bind; enforce “token must match + signed payload” per RFC-0002.

**B2 — App Runtime API: reserved-method handling inconsistent across ABI/API/capabilities**
- **Why blocking:** Host implementers cannot know whether to implement these methods or return `NOT_IMPLEMENTED`; app developers cannot rely on method availability.
- **Fix (choose one path and apply consistently):**
  1. **Keep reserved:** Mark these methods as **reserved** in `api-surface.md` method tables (and optionally in `CAPABILITY_MAP` comments) and explicitly state they return `{ ok:false, error:{code:"NOT_IMPLEMENTED"} }` until specified; **or**
  2. **Promote to v1:** Provide full schemas + semantics in `api-surface.md` and move them from “reserved” to “v1” in `abi.md`.

### Minor Issues (M1, M2, etc.)
**M1 — Mailbox “cursor vs timestamp” parameter naming mismatch**
- `spec/00-shared/layer-integration.md` `MailboxService.retrieve(sinceCursor?: string)` implies an opaque cursor, while `spec/06-rfcs/RFC-0003-messaging.md` §7.4 uses `since=<timestamp>`. Align terminology (either define cursor as timestamp string or specify an opaque cursor in RFC).

**M2 — Handshake challenge IID/DID encoding clarity in non-authoritative doc**
- `spec/01-transport-connectivity/peer-handshake.md` challenge construction text is less explicit about **Base32→raw 20-byte decoding** than RFC-0002. Since RFC-0002 is authoritative, this is mostly editorial, but worth tightening to prevent implementer mistakes.

**M3 — Minor naming drift: “X3DH” references in some non-RFC interfaces/comments**
- E.g., `spec/03-messaging-sync/interfaces.md` header extension type comment still says “initial (X3DH)” while RFC-0003 positions it as 2DH (domain separator remains `post-urbit-x3dh-v1`). Purely naming/clarity.

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — 2 blocking issues require fixes

The spec is very close: core wire formats, byte sizes, and crypto constructions are largely consistent across layers and RFCs, but the two contradictions above (relay binding semantics; app runtime method registry) can lead to incompatible independent implementations and must be resolved to be implementation-ready.

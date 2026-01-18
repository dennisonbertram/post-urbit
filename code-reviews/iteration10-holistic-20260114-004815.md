## BLOCKING (must fix before implementation can be coherent)

1) **App package format and signing are contradictory across layers**
- **Where:** `05-ux-packaging/app-distribution.md` (ZIP `.postapp` + `SIGNATURE` file) vs `04-app-runtime/manifest-schema.md` (tar.gz `.pkg.tar.gz` + `manifest.signature` + `manifest.files`) and parts of `05-ux-packaging/overview.md`.
- **Problem:** Two incompatible package containers and two incompatible signature binding models:
  - Packaging layer: signature is a separate text file over *manifest hash*.
  - Runtime layer: signature is embedded in manifest over *canonical manifest without signature field*, and `files` hashes are inside manifest.
- **Impact:** Install/update verification, repository hosting, and tooling cannot interoperate.
- **Fix direction:** Choose exactly one:
  - **Option A (recommended):** `.postapp` (ZIP) as canonical; require `manifest.json` to contain `files.hashes`; `SIGNATURE` becomes optional/removed (or becomes the embedded manifest signature material), and signature payload format becomes normative in one place.
  - **Option B:** `.pkg.tar.gz` canonical; update `05` specs and Admin API upload endpoints accordingly.

2) **Admin authentication contract is inconsistent (cookie vs bearer, token returned vs not, TLS requirements)**
- **Where:** `05-ux-packaging/node-daemon.md` (cookie-based browser auth; *login returns session metadata and sets cookie*; local HTTP allowed), vs `05-ux-packaging/interfaces.md` (`LoginResponse` includes `token`; bearer token for UI), vs `05-ux-packaging/admin-ui.md` (claims “Secure cookies always, even localhost uses TLS”).
- **Problem:** Three different models:
  - Browser: cookie+CSRF (double-submit).
  - UI TypeScript: bearer token in response body.
  - Local mode: HTTP allowed but cookie security varies.
- **Impact:** You can’t implement client/server auth once without breaking another spec.
- **Fix direction:** Make one normative browser model:
  - If **cookie is primary**: remove `token` from `LoginResponse` in `interfaces.md` and update `admin-ui.md` sample client to not rely on bearer; clarify local HTTP cookie settings.
  - If **bearer is primary**: remove CSRF requirements and cookie session model (or scope it to optional mode only).

3) **Endpoint schema is duplicated and incompatible (Identity/Transport vs UX/Admin types)**
- **Where:** Normative endpoint in `02-identity-trust/identity-document-schema.md` (`type: direct|relay|mailbox`, `host`, `port`, `transport`, `priority`, `relay_id`) vs `05-ux-packaging/interfaces.md` “Missing Types” `Endpoint` (`type: quic|relay|https`, `address`).
- **Problem:** Different field names, different type enums, different meaning of transport/port.
- **Impact:** Admin API cannot display/edit endpoints without lossy mapping; transport selection logic can diverge from identity truth.
- **Fix direction:** Delete the UX “Endpoint” shadow type and reuse the canonical endpoint schema everywhere (or define a single explicit mapping DTO with documented conversion).

4) **Relay allocation model doesn’t integrate cleanly with identity publishing / endpoint discovery**
- **Where:** `01-transport-connectivity/relay-protocol.md` (allocations return `allocated_port` and expire ~1h), vs identity publishing defaults (`publish_interval_hours = 24`), vs endpoint schema (relay endpoint is just `host/port`).
- **Problem:** If peers must contact `relay.example.com:<allocated_port>`, the identity document (or DHT record) must be refreshed roughly hourly. Current publish cadence is 24h, and identity updates are “heavy” (sequence increments, signatures).
- **Impact:** Relay connectivity will often fail or require out-of-band discovery not specified.
- **Fix direction:** Pick one consistent design:
  - **Stable relay port** (recommended): peers always send to relay’s stable port; relay routes by destination IID (no per-allocation port exposure), allocation token authenticates sender only.
  - Or keep per-allocation port but specify **fast endpoint publishing** and how it avoids identity sequence churn (e.g., separate “reachability record” in DHT distinct from IDOC).

5) **Device discovery/DHT keys are underspecified or inconsistent with DHT hashing conventions**
- **Where:** `02-identity-trust/identity-document-schema.md` says store device docs in DHT as key = `"device:" + did"` and discover via `"devices-for:" + iid"` prefix queries; but `00-shared/layer-integration.md` defines DHT keys as `SHA256("post-urbit:identity:" || iid)` and transport DHT interfaces return `PeerEndpoints`.
- **Problem:** No canonical hashed key format for device docs, no prefix-query mechanics specified for a hashed-key DHT, and no signature/storage rule for device records.
- **Impact:** Multi-device is not implementable consistently across nodes.
- **Fix direction:** Define *normative* DHT record formats for:
  - `post-urbit:device:<did>` → signed Device Document envelope
  - `post-urbit:devices-for:<iid>` → an index record (or explicitly state the DHT supports prefix iteration and how)
  - Include TTL, signature rules, and verification steps (like IDOC).

---

## HIGH (serious; will cause bugs, insecurity, or interoperability failures)

6) **Signing key history retention conflicts with long-lived verification needs (apps + mailbox + offline)**
- **Where:** `02-identity-trust/identity-document-schema.md` signing history retention “max 3 entries or 14 days”, while:
  - `05-ux-packaging/app-distribution.md` wants verifying package signatures long after signing time (“old signatures valid”, even “2 years” mentioned).
  - Mailbox/offline delivery can plausibly exceed 14 days; secure envelope verification only checks `current`/`previous`.
- **Impact:** Old packages and delayed messages become unverifiable, causing false “invalid signature” errors.
- **Fix direction:** Align on a single retention policy:
  - Either extend signing history retention materially (e.g., 1–2 years or N rotations),
  - or change verification semantics so archived identity versions are retrievable/verifiable (e.g., store historical IDOCs in DHT by sequence hash).

7) **Group sender-key model contradicts interfaces (extra signature keypair)**
- **Where:** `03-messaging-sync/group-messaging.md` explicitly says signatures are done at PUSE envelope level and “no separate sender-key signature keys”, but `03-messaging-sync/interfaces.md` defines `SenderKeyState.signatureKeyPair` and `SenderKeyShare.signaturePublicKey`.
- **Impact:** Implementations diverge; extra keys complicate trust and rotation.
- **Fix direction:** Remove sender-key signature keys from interfaces (or revise group spec to justify and specify them). Prefer “PUSE signature only” for consistency.

8) **Revocation propagation/storage is not fully integrated**
- **Where:** `02-identity-trust/revocation.md` defines revocation docs and “DHT/directory update with revocation notice”, but `00-shared/layer-integration.md` only defines DHT storage for IDOC.
- **Problem:** No canonical DHT keys/records for revocations, no conflict rules vs identity updates, no “latest revocation” lookup rule.
- **Impact:** Compromised keys may remain usable because peers don’t learn revocations reliably.
- **Fix direction:** Define DHT record(s) for revocations, e.g.:
  - `post-urbit:revocation:<iid>` → latest revocation notice (signed)
  - Optionally `post-urbit:revocation:<iid>:<revoked_key_hash>` for key-specific
  - Specify cache/TTL/gossip behavior and precedence vs identity docs.

9) **RecoveryConfig schema drifts across Identity vs Admin/UX types**
- **Where:** canonical recovery config is `recovery: { method, config }` in `identity-document-schema.md` and detailed in `recovery-mechanisms.md`, but `05-ux-packaging/interfaces.md` defines a different `RecoveryConfig` shape (flattened fields like `trustees?`, `threshold?`, `escrowDevice?`, etc.).
- **Impact:** Admin API cannot round-trip or validate recovery configs; recovery proofs may not match config.
- **Fix direction:** Reuse the identity-layer RecoveryConfig schema in Admin API types, with method-specific `config` payloads.

10) **Encryption key “previous” retention contradicts rotation text**
- **Where:** `02-identity-trust/key-rotation.md` says keep encryption previous public key “indefinitely”, while `identity-document-schema.md` specifies retention max 5 keys or 30 days.
- **Impact:** Senders may rely on previous keys longer than receivers advertise; offline peers break.
- **Fix direction:** Make one rule: either bounded retention everywhere, or indefinite everywhere (bounded is more realistic).

11) **Secure Envelope verification ignores signing key history**
- **Where:** `03-messaging-sync/secure-envelope.md` verification pseudocode checks only `current` and `previous`.
- **Impact:** Messages signed shortly before rotation (or during multiple rotations) may fail verification; mailbox-delivered messages fail more often.
- **Fix direction:** Verification should check:
  - `current`, `previous`, and `keys.signing.history[]` (within expiry windows),
  - and/or cached historical identity docs (per caching policy’s history limit).

12) **Type drift for sequences and timestamps (string vs number) appears in examples and algorithms**
- **Where:** Identity schema mandates `sequence` is a decimal string; some rotation/recovery examples show numeric or omit quoting.
- **Impact:** JSON canonicalization (JCS) and signature verification will fail across implementations if one side encodes as number.
- **Fix direction:** Add a lint rule: any example or pseudocode must show `sequence` as `"N"` string; add explicit “reject numeric sequence” validation.

---

## MEDIUM (won’t block prototyping, but will create sharp edges later)

13) **DHT interface return types don’t match “full IDOC in DHT” claim**
- **Where:** `01-transport-connectivity/overview.md` and `00-shared/layer-integration.md` say DHT stores full IDOC; `DiscoveryService.lookupPeer()` returns `PeerEndpoints` not a document.
- **Impact:** Callers may implement DHT as “endpoints only” while others store IDOC.
- **Fix direction:** Either:
  - make `lookupPeer()` return the identity document (or a union containing it),
  - or provide a second explicit method `fetchIdentityDoc(iid)` at transport layer.

14) **Mailbox protocol is minimal but missing operational limits and deletion semantics alignment**
- **Where:** `00-shared/layer-integration.md` mailbox API.
- **Gaps:** message TTL, max message size, batching semantics for retrieve, dedup rules, and mapping to message ack/retry model in messaging.
- **Fix direction:** Add: max envelope size, retention defaults, pagination/cursors for retrieve, and retry/ack guidance.

15) **Sync protocol hashing / operation_id encoding is under-specified**
- **Where:** `03-messaging-sync/sync-protocol.md` uses `SHA256(origin || timestamp || operation_bytes)` but doesn’t define canonical byte encoding of HLC or op bytes.
- **Impact:** Different implementations derive different operation IDs; Merkle trees won’t match.
- **Fix direction:** Define canonical CBOR encoding for HLC + operation, and operation_id = SHA256(canonical_cbor).

---

## LOW (cleanup / paper cuts / future-proofing)

16) **Multiple parallel “interfaces.md” files redefine overlapping common types**
- **Where:** identity/transport/messaging/app-runtime/ux all define their own `Timestamp`, `Endpoint`, etc.
- **Impact:** Drift over time (already happening).
- **Fix direction:** Create a single `00-shared/types.md` (and TS package) and import/re-export.

17) **Naming inconsistencies: “post-urbit” vs “postnode” vs “post-urbit
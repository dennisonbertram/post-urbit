## Holistic Review — Prioritized Issues & Fixes

### BLOCKING (must fix before implementation; contradictions/security-critical)

1) **Multi-device (DID) is specified, but Transport handshake never authenticates/negotiates a DID**
- **Where:** `identity-document-schema.md` (Device Document / DID), `transport/interfaces.md` (PeerId includes `did`, connection stores `peerDeviceDocument`), `peer-handshake.md` (no DID anywhere).
- **Problem:** You can’t safely support per-device sessions or dedup rules “per (iid,did)” if the connection handshake only proves IID control. A malicious device could claim a DID without proof, or you can’t bind sessions to a device at all.
- **Actionable fix:**
  - Extend `peer-handshake.md` messages to include `client_did`, `server_did` (optional) and attach **DeviceDocument** + proof.
  - Add a **device challenge signature** using `device_signing_key` or `device_transport_key` and verify `signature_by_identity` on the Device Document against identity’s current signing key.
  - Decide whether the QUIC/TLS layer is bound to **device_transport_key** (ideal) or only to identity signing key via the post-TLS handshake.

2) **Secure Envelope header-extension location contradicts Double Ratchet + examples**
- **Where:** `secure-envelope.md` (ratchet params are in **header extension AAD**, unencrypted), `double-ratchet.md` (ratchet header described as part of encrypted plaintext JSON under `"ratchet": {...}`).
- **Problem:** Implementation can’t be consistent: receivers need ratchet header **before** decryption to derive the correct message key; if it’s inside ciphertext, decryption is circular.
- **Actionable fix:**
  - Make `double-ratchet.md` normative that DH public key / chain indexes live in the **PUSE header extension** (type `0x01`), not plaintext JSON.
  - Remove the plaintext `"ratchet"` object from message examples, or explicitly mark it as “redundant copy forbidden”.

3) **Group messaging signature model is internally inconsistent (identity signature vs sender-key signature)**
- **Where:** `group-messaging.md` (sender key includes an Ed25519 `signatureKey`, signs ciphertext), `secure-envelope.md` (single 64-byte signature verified with **identity document signing key**).
- **Problem:** Which signature does the recipient verify? If both exist, you have redundant/conflicting authentication and unclear non-repudiation properties.
- **Actionable fix (pick one):**
  - **Option A (recommended, simpler):** Remove `signatureKey` from sender keys entirely; group messages are authenticated by the **PUSE signature using identity signing key**.
  - **Option B:** PUSE signature is verified using the **sender-key signature public key** from `sender_key_share` (and identity signature becomes optional/absent). This is a larger change: update `secure-envelope.md` verification, key lookup rules, and non-repudiation statement.

4) **App runtime async model contradicts itself (no callbacks vs callbacks)**
- **Where:** `abi.md` (polling, *no callbacks*, `handle()` is entrypoint), `api-surface.md` (“Apps yield and receive callbacks”), `messaging.subscribe` includes `callback_entry`, `HostBridge` has `registerCallback/invokeCallback`.
- **Problem:** This prevents a coherent WASM SDK/host implementation. If only `handle()` is called, `callback_entry` is meaningless.
- **Actionable fix:**
  - Make `abi.md` authoritative (it already claims to be): remove callback concepts from `api-surface.md` and `HostBridge`.
  - Replace `callback_entry` in subscription APIs with a `subscription_id` only; delivery is via `handle()` with `type: 'message'` and `callback_id`/`subscription_id`.
  - If you truly want multiple entrypoints, explicitly allow host to call other exported functions and update ABI accordingly (but that breaks the “no callbacks” claim).

5) **Capabilities: “messaging:receive” exists in manifests/docs but is explicitly removed elsewhere**
- **Where:** `capability-system.md` (“no separate receive; subscribe implies receive”), but `manifest-schema.md` examples and required lists include `messaging:receive`.
- **Problem:** Install-time permission UI and enforcement will diverge; apps will request a capability the system says doesn’t exist.
- **Actionable fix:**
  - Remove `messaging:receive` everywhere and update examples to use `messaging:subscribe`.
  - Add a manifest validator rule: `messaging:receive` → **INVALID_CAPABILITY** (or migration alias to `messaging:subscribe` for backwards compatibility).

6) **Recovery proof schema drift across documents**
- **Where:** `identity-document-schema.md` defines `recovery_proof` embed with `{method, initiated_at, cooldown_expires_at, status, proof_data}`, while `recovery-mechanisms.md` shows a different structure (e.g., `recovery_proof.attestations` directly; missing status fields), and `interfaces.md` uses camelCase embed types that don’t match the JSON examples.
- **Problem:** Peers can’t verify recovery consistently; implementations will fork.
- **Actionable fix:**
  - Declare one canonical on-wire `recovery_proof` shape (prefer the richer one in `identity-document-schema.md`) and update *all* examples + verification pseudocode to match.
  - Ensure TypeScript ↔ wire mapping (snake_case) is applied consistently.

7) **Identity↔Transport DHT storage format and APIs don’t match**
- **Where:** `identity-caching-policy.md` expects `dhtPut/dhtGet` of docs; `transport/interfaces.md` `DiscoveryService.lookupPeer()` returns endpoints; `00-shared/layer-integration.md` says DHT stores full IDOC and also adds an extra record-level signature/TTL not present in transport interfaces.
- **Problem:** You cannot implement both sets without inventing missing types (TTL, signature) and clarifying whether DHT stores endpoints vs full documents.
- **Actionable fix:**
  - Decide one: **DHT stores full IDOC** (recommended) or **DHT stores endpoints only**.
  - Update `transport/interfaces.md` to expose the same primitives identity needs (e.g., `dhtPut(key,value,ttl)` and include multi-value results with metadata).
  - If you keep record-level signatures, specify how they’re encoded and returned by `dhtGet`; otherwise drop them and rely on IDOC self-signature + storage policy.

---

### HIGH (significant correctness/security/implementation risk)

8) **Signing key rotation retention vs offline/mailbox delivery is underspecified**
- **Where:** `identity-document-schema.md` keeps `keys.signing.previous` for one rotation cycle; `secure-envelope.md` allows verifying with previous key only; mailbox can deliver messages much later.
- **Problem:** A message signed with an older signing key may become unverifiable for offline recipients after multiple rotations.
- **Actionable fix:**
  - Add a **signing key history** (bounded, like encryption history) or allow a limited set of previous signing keys with validity windows.
  - Alternatively include a **signing key id** in PUSE header extension and require identity docs to retain keys referenced by unexpired messages.

9) **Group membership update “version” mechanism requires coordination but claims multi-admin concurrency**
- **Where:** `group-messaging.md` uses `version` as monotonically increasing string and “same version tie-breakers”.
- **Problem:** Two admins cannot safely choose the next monotonic version without coordination; frequent same-version conflicts are expected.
- **Actionable fix:**
  - Use an HLC/Lamport-style version (`(hlc, actor_iid)`), or
  - Make membership state itself a CRDT (OR-Set of members/roles), or
  - Enforce a single-writer model (one admin designated sequencer).

10) **Secure Envelope plaintext examples contradict “message_id only in header”**
- **Where:** `secure-envelope.md` test vector plaintext includes `"id":"test-1"` but later says plaintext does NOT include an `id` field.
- **Problem:** Breaks message dedupe/references; app APIs use `Message.id` heavily.
- **Actionable fix:**
  - Make it normative that message ID is **header-only**; remove `id` from all plaintext examples and update messaging interfaces to clarify the mapping.

11) **Sync security model conflicts with presence of `sync_op` inside PUSE message types**
- **Where:** `messaging-sync overview` says Sync stream (0x04) is not PUSE; `secure-envelope.md` lists `sync_op` as a PUSE message type.
- **Problem:** Unclear whether sync ops can arrive over messaging/mailbox paths and how they’re validated.
- **Actionable fix:**
  - Either remove `sync_op` from PUSE entirely, or explicitly define when sync operations may be encapsulated in PUSE (e.g., mailbox fallback) and how that interacts with operation signatures and access control.

12) **Endpoint port semantics differ between Identity schema and Transport schema**
- **Where:** `identity-document-schema.md` endpoint object says `port` is UDP; mailbox uses HTTPS/TCP; `transport/interfaces.md` clarifies “UDP for quic, TCP for https”.
- **Problem:** Implementations will reject valid mailbox endpoints or mis-handle ports.
- **Actionable fix:** Make endpoint `port` always “service port”, and interpret protocol via `transport` (`quic`=UDP, `https`=TCP). Update Identity schema normative text.

13) **Capability mapping type mismatch**
- **Where:** `capability-system.md` declares `Record<string, string | null>` but uses arrays (e.g., `messaging.send_group`: `['messaging:send','messaging:group']`).
- **Problem:** Type-level mismatch becomes real code bugs in enforcement.
- **Actionable fix:** Define mapping as `string | string[] | null | {anyOf?:...; allOf?:...}` and make evaluation rules explicit.

---

### MEDIUM (gaps/ambiguities that will slow implementation or cause edge-case bugs)

14) **DHT key and IID encoding ambiguities**
- **Where:** `00-shared/layer-integration.md` uses `sha256("post-urbit:identity:" || iid)` but doesn’t state whether `iid` is ASCII bytes, decoded 20 bytes, or normalized base32.
- **Actionable fix:** Specify exact byte construction (e.g., UTF-8 of lowercase base32 IID) and require normalization before hashing.

15) **Revocation vs “emergency rotation” overlaps create two parallel update paths**
- **Where:** `revocation.md` key revocation includes replacement identity document; `key-rotation.md` also handles urgent rotation; `interfaces.md` has both rotateKeys and revokeKey.
- **Problem:** Two mechanisms for “replace key now” complicate verification and state machines.
- **Actionable fix:** Collapse into one model:
  - Either “revocation is a signed wrapper around an identity update” (and identity doc update rules remain primary), or
  - “urgent rotation is represented only as identity doc update with reason + flags,” and reserve revocation for identity-death only.

16) **Mailbox auth token encoding not fully specified**
- **Where:** `00-shared/layer-integration.md` says “Base64-encoded JSON” but doesn’t specify base64 vs base64url, padding rules, canonicalization for the encoded payload, etc.
- **Actionable fix:** Specify: base64url-no-padding for the outer token, and JCS canonical JSON bytes for signing, and whether the bearer value is the encoded full object including signature.

17) **Messaging multi-device delivery semantics unclear**
- **Where:** PUSE flags include “Forwardable” bit; messaging interfaces and group delivery talk about fanout to members, but device fanout rules aren’t defined.
- **Actionable fix:** Define whether messages are addressed to:
  - (iid) and the recipient forwards to all devices, or
  - (iid,did) and sender fans out to devices, or
  - both (with forwardable semantics). Then update PUSE header fields accordingly.

18) **Time-skew rules differ (identity docs vs handshake/relay) without explicit rationale**
- **Where:** Identity timestamp validation allows +24h, handshake/relay require ±5 minutes.
- **Actionable fix:** Add an explicit “time requirements” section: identity docs are archival (sequence is primary), but handshake/relay are anti-replay (tight skew). Make implementers treat these separately.

---

### LOW (polish/consistency, not blocking)

19) **Terminology and domain-separator drift**
- **Where:** “post-urbit”, “post_urbit”, various signature input strings across relay/handshake/ratchet.
- **Actionable fix:** Centralize all domain separators in a single registry doc (or `00-shared/constants.md`) and reference it everywhere.

20) **Extra record-level DHT signature may be redundant**
- **Where:** `00-shared/layer-integration.md` adds record signature; IDOC already self-signed.
- **Actionable fix:** If you keep it, justify (anti-spam storage gating) and specify return path; otherwise simplify.

21) **Missing test vectors / conformance fixtures across critical crypto formats**
- **Where:** Many docs say “test vectors to be generated”.
- **Actionable fix:** Add a `conformance/` suite with:
  - IID derivation vectors (raw pubkey → iid)
  - JCS canonicalization vectors
  - PUSE parse/sign/encrypt/decrypt vectors
  - Handshake exporter binding vectors (library-specific caveats)

---

## Biggest “integration points missing” (cross-layer glue you should add)
- **Device-aware handshake** (BLOCKING): identity+device binding at transport.
- **Unified subscription delivery model** (BLOCKING): remove callback_entry; deliver via `handle()` events.
- **One authoritative schema for recovery proofs** (BLOCKING): unify verification and cooldown/contest semantics.
- **One definitive stance on “Sync over PUSE vs Sync stream only”** (HIGH): especially for mailbox/offline.
- **Message addressing model for multi-device** (MEDIUM/HIGH): must be defined to avoid later redesign.

If you want, I can propose concrete patched message schemas for (a) DID-aware handshake, (b) corrected PUSE header extension usage for ratchet+groups, and (c) a unified recovery proof schema, all in the same style as your existing docs.

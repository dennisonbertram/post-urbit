## Holistic Review — Prioritized Issues & Readiness

Overall: the architecture is cohesive, layering is clean, and the “glue” docs (esp. `00-shared/layer-integration.md`) do a lot of the heavy lifting. That said, there are **4 BLOCKING items** that would cause implementers to build incompatible systems unless resolved. After those, you’re very close to RFC-ready.

---

# BLOCKING (implementation-breaking contradictions / missing normative decisions)

## 1) Multi-device messaging is not fully specified end-to-end (esp. with mailbox/offline)
**Where:**
- `00-shared/layer-integration.md` (device DHT discovery + mailbox is identity-addressed)
- `02-identity-trust/identity-document-schema.md` (DID + DeviceDocument fields)
- `03-messaging-sync/*` (ratchet says “per device”, interfaces don’t)
- `03-messaging-sync/secure-envelope.md` (PUSE has no recipient DID field)
- `03-messaging-sync/interfaces.md` (`MessagingService`/`MessageEncryptionService` accept only IID)

**Why it blocks:**  
You simultaneously specify:
- transport connections can be per `(iid, did)` (good),
- device discovery exists (good),
- but **PUSE + mailbox addressing are identity-level** (`recipient_iid` only), and **messaging APIs don’t allow DID targeting**.

This leaves a fundamental ambiguity: *when an identity has multiple devices, what key is used and which device(s) can decrypt mailbox-stored messages?* Without a normative choice, implementations will diverge.

**Concrete options (pick one and make it normative):**
1. **Identity-level messaging keys (shared across devices)**: simplest for mailbox; undermines the stated “separate sessions per device” goal.
2. **Per-device messaging keys + per-device delivery**: requires either:
   - mailbox endpoints keyed by DID (`/store/{recipient_did}`), or
   - PUSE recipient field extended to include DID (or add a header extension), plus mailbox stores multiple copies or a multi-recipient envelope.
3. **Single “primary” inbox device**: mailbox delivers to one device; node forwards internally to other devices (then you must specify the forwarding protocol and trust/crypto boundaries; PUSE `forwardable` flag hints at this but isn’t defined).

**Minimum spec fix:** Decide the model and update:
- PUSE envelope routing fields and/or mailbox API,
- `MessageEncryptionService` / `MessagingService` to accept `PeerId { iid, did? }` or specify internal fanout rules,
- clarify what `device_transport_key` is used for (transport only vs messaging/X3DH).

---

## 2) Relay protocol contradicts itself about “stable port model”
**Where:**
- `01-transport-connectivity/relay-protocol.md` (“Stable Relay Port Model” section vs later “Relay Data Flow” diagram)
- `01-transport-connectivity/interfaces.md` (`RelayAllocation` has `allocatedPort`)

**Why it blocks:**  
You normatively argue for **stable port** (e.g., relay always on `:4433`), but the later diagram shows **per-allocation ports** (`52341`, `52342`). Those are incompatible models and would drive different endpoint publishing, NAT/firewall behavior, and allocation refresh.

**Fix:** Make one model canonical. If keeping stable-port (as recommended in your own text):
- Update the diagram to show all clients send to `relay:4433` and routing is by `(dest IID, token)`.
- Clarify what `allocatedPort` means in `RelayAllocation`:
  - either always the stable relay port, or
  - remove it and use `relay.port`.

---

## 3) QUIC stream “type byte” framing is inconsistent across docs (per-stream vs per-message)
**Where:**
- `01-transport-connectivity/quic-integration.md` (stream type byte written once at stream start)
- `01-transport-connectivity/peer-handshake.md` (explicitly: stream type written once)
- `00-shared/layer-integration.md` (“Identity Update Stream” diagram appears to include stream type per message)

**Why it blocks:**  
Implementations will disagree about whether every identity update message begins with `0x02`, or only the stream does. That breaks framing and interop.

**Fix:** Make a single global rule (recommended: **stream type written once**, then message frames follow) and update `layer-integration.md` diagrams to match:
- Stream begins with `0x02`
- Then repeated frames: `message_type (1) || length (4) || payload`

(Your PUSE “no inner length for header extension” work is good; this is the remaining similar-class framing mismatch.)

---

## 4) App manifest + package signing schema is inconsistent across 04/05
**Where:**
- `04-app-runtime/manifest-schema.md` (claims `files` required; signature semantics vary in-text)
- `04-app-runtime/interfaces.md` (`AppManifest.signature` exists, but `files` field not represented)
- `05-ux-packaging/app-distribution.md` (introduces `files: { hashes: ... }` + `SIGNATURE` file as primary)
- `04-app-runtime/manifest-schema.md` vs `05-ux-packaging/app-distribution.md` disagree on the structure of `files`

**Why it blocks:**  
Installers/verifiers won’t agree on:
- whether `manifest.json` must contain an embedded Ed25519 signature,
- whether the authoritative signature is the package `SIGNATURE` file or manifest field,
- and whether `files` is a flat map (`path -> sha256`) or an object (`{ hashes, total_size }`).

**Fix (choose one canonical model and propagate everywhere):**
- Define the **canonical manifest schema** (exact JSON shape) and reflect it in:
  - `manifest-schema.md`
  - `interfaces.md` `AppManifest`
  - `app-distribution.md` signing/verification steps
- If you want both signature mechanisms (manifest-embedded + SIGNATURE file), explicitly define:
  - which is REQUIRED vs OPTIONAL,
  - whether both must verify and what happens if they conflict.

---

# HIGH (serious interop/security gaps; not always immediately blocking but likely to cause bugs or insecurity)

## 5) PUSE signature verification doesn’t incorporate `keys.signing.history`
**Where:**
- `03-messaging-sync/secure-envelope.md` verification pseudocode (current/previous only)
- `02-identity-trust/identity-document-schema.md` explicitly introduces extended signing key history for long-lived verification

**Impact:**  
Mailbox-delayed messages, archived messages, and long-lived app package signatures may be unverifiable even though the identity layer claims they should be.

**Fix:** Update secure-envelope verification rules to try:
1) `current`
2) `previous`
3) `keys.signing.history[]` entries that are valid for the message time/sequence (you’ll need a normative selector: envelope nonce timestamp, plaintext timestamp, or “accept if key not expired and within validity window”).

---

## 6) Device document DHT signature authority is contradictory
**Where:**
- `00-shared/layer-integration.md` “Device Document DHT Format” text says “signed by device key” but table says signature by identity signing key; JSON structure includes both.

**Impact:**  
Implementers won’t know what to verify/store, and relays/DHT nodes won’t know spam-prevention rule.

**Fix:** Make it explicit whether:
- the **DHT record signature** is by the identity signing key (recommended for authorization), and/or
- the **device document itself** includes a device-key signature for proof-of-possession (optional, but then specify exact signed bytes).

---

## 7) Encoding conventions: Base64 vs Base64url drift
**Where:**
- Global conventions in `00-shared/layer-integration.md` say Base64 standard no padding for “keys/signatures”
- `01-transport-connectivity/relay-protocol.md` uses Base64url for allocation tokens

**Impact:**  
Small but real interop failures (“invalid token” due to decoder mismatch).

**Fix:** Add a global convention line: tokens MAY use Base64url; keys/signatures MUST use Base64 (standard). Or normalize tokens to standard Base64 too.

---

## 8) DHT record “signature” vs embedded IDOC signature is redundant/unclear
**Where:**
- `00-shared/layer-integration.md` DHT record format + code sample

**Impact:**  
DHT node verification behavior is ambiguous: verify document signature only? record signature only? both? signed bytes include envelope headers or JSON only?

**Fix:** Either:
- remove the extra DHT record signature and rely solely on IDOC’s internal signature, **or**
- define record signature exactly (bytes = full `IDOC` envelope) and require DHT nodes verify *both* record signature and internal document signature (and why both are necessary).

---

# MEDIUM (non-fatal but will cause confusion, drift, or implementation variance)

## 9) Peer-handshake resumption introduces messages not in the message-type registry
**Where:**
- `01-transport-connectivity/peer-handshake.md` (“resume_accepted” appears but not in formats/table)

**Fix:** Add explicit message definitions and state machine transitions for resumption (and whether ServerHello is skipped or abbreviated).

---

## 10) Test vectors coverage gaps vs “97% complete” claim
**Where:**
- `00-shared/test-vectors.md` has placeholders for DID vector (Test Vector 8)
- Some docs still contain “test vectors to be generated” placeholders:
  - `02-identity-trust/identity-document-schema.md`
  - `03-messaging-sync/secure-envelope.md`

**Fix:** Either complete the missing vectors or explicitly scope them out and point to the authoritative `test-vectors.md`.

---

## 11) App-level messaging vs core message types needs one bridging paragraph
**Where:**
- `04-app-runtime/api-surface.md` uses `message_type` strings (e.g., `com.example.*`)
- `03-messaging-sync/secure-envelope.md` message `type` enum includes `app`

**Fix:** Add a normative mapping: host wraps app messages in PUSE plaintext `type: "app"` and includes `{app_id, app_version, message_type, payload}` (or similar), so both layers line up.

---

# LOW (polish, clarity, doc hygiene)

## 12) Diagrams and naming consistency
- Update relay diagram (see BLOCKING #2).
- Consider renaming `device_transport_key` → `device_encryption_key` if it’s used for messaging sessions; otherwise explicitly state it is transport-only.

## 13) Minor duplication / outdated sections
- Remove or deprecate placeholder “Test Vector” stubs in non-authoritative docs in favor of `00-shared/test-vectors.md`.

---

# RFC Readiness Assessment

If you resolve the **4 BLOCKING items**, the spec is effectively **RFC-ready** (you can start RFC-0001..0003) because:
- the layering and dependencies form a DAG,
- wire formats are mostly explicit and consistent,
- the test vectors framework is strong (HKDF salt handling is correctly nailed down),
- operational surface (daemon/admin/app packaging) is unusually well specified for this stage.

If you want, I can propose a concrete RFC breakdown (e.g., RFC-0001 Identity+IDOC+DHT, RFC-0002 QUIC+Handshake+Relay, RFC-0003 PUSE+Ratchet+Mailbox) and list the minimum normative deltas needed in each to close the BLOCKING items.

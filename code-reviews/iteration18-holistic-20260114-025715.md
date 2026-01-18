Below is a prioritized alignment review based on the material you provided (layer docs + progress log excerpts). I’m treating the RFCs as “complete” but checking the *system* for contradictions/mismatches that would cause independent implementations of RFC‑0001/2/3 to fail to interop.

---

## BLOCKING (must fix for interop)

### 1) **Key revocation schema assumes the revoked key can sign (breaks for X25519)**
**Where:** `revocation.md` → `key_revocation` document:  
```json
"signatures": { "by_revoked_key": "...", "by_new_key": "..." }
"revoked_key_type": "signing|encryption"
```
**Why blocking:** If `revoked_key_type == "encryption"`, the revoked key is **X25519**, which **cannot sign**. The field `by_revoked_key` is nonsensical for encryption-key revocation, and verification logic will diverge across implementations.

**Fix recommendation:**
- Redefine authorization rules for `key_revocation`:
  - If revoking **signing** key: require `(sig_by_old_signing AND sig_by_new_signing)` OR `(sig_by_new_signing AND recovery_proof)`.
  - If revoking **encryption** key: require `sig_by_current_signing` over the revocation doc + replacement identity doc (no “sig by revoked encryption key” path).
- Update schema accordingly, e.g.:
```json
"signatures": {
  "by_current_signing_key": "<sig>",
  "by_new_signing_key": "<sig>|null"
}
```
…and make `by_new_signing_key` required only when the signing key is changing.

---

### 2) **DHT authentication/signature model is inconsistent across docs & interfaces**
**Where:**
- `01-transport-connectivity/overview.md` says DHT record includes a **separate signature** field.
- `identity-caching-resolution-policy.md` + `interfaces.md` define `dhtPut(... signature ...)`.
- Progress log claims: “DHT authentication simplified (uses internal IDOC signature only)”.

**Why blocking:** Implementations will either:
- require an external DHT signature (and reject “IDOC-only”), or
- store/accept IDOCs without that extra signature (and reject “signature-required”).

**Fix recommendation:**
- Make **one** model normative in `00-shared/layer-integration.md` and enforce it everywhere:
  - **Option A (recommended):** DHT stores **IDOC bytes only**; validators parse IDOC and verify `signatures.current` (and rotation/recovery rules).
    - Remove `signature` parameter from `dhtPut()` in all interfaces.
    - Update Transport overview text to remove “Signature:” line.
  - **Option B:** keep a DHT wrapper signature (then specify *exactly* what is signed, by which key, and how it’s encoded).
- Also ensure the **DHT key derivation** is specified in exactly one place and referenced everywhere (some docs say “keyed by IID”, others specify `SHA256("post-urbit:identity:"||iid)`).

---

### 3) **Mailbox is a required capability but Transport interfaces don’t expose it**
**Where:**
- Identity endpoints include `"type": "mailbox", "transport": "https"`.
- Transport overview includes Mailbox as a first-class path.
- `TransportService` interface exposes direct/relay/NAT/DHT, but **no mailbox send/receive API**.

**Why blocking:** RFC‑0003 “store-and-forward” can’t be implemented consistently if mailbox is “Transport responsibility” but Transport offers no contract.

**Fix recommendation:**
- Either:
  - Move mailbox clearly into Messaging layer (and remove “Mailbox” from Transport layer overview/interfaces), **or**
  - Add a `MailboxService` (or Transport sub-API) with normative operations, e.g.:
    - `mailboxPut(recipientIid, envelopeBytes, auth)`
    - `mailboxPoll(sinceCursor, auth)`
    - `mailboxAck(messageId, auth)`
  - Then ensure RFC‑0003 normatively references that API and specifies authentication (identity signature? device token? HTTP auth?).

---

### 4) **`keys.encryption.previous` type is contradictory across schema vs rotation/recovery examples**
**Where:**
- Canonical schema: `keys.encryption.previous` is an **array of history entries** (with validity windows).
- `key-rotation.md` example uses `"previous": "<K_enc_old>"` (string).
- `recovery-mechanisms.md` recovery document example sets encryption `previous: null`.

**Why blocking:** Senders choosing fallback encryption keys and receivers attempting decryption will implement different parsing logic.

**Fix recommendation:**
- Make the schema authoritative:
  - `keys.encryption.previous` MUST be an array (possibly empty).
- Update all examples and any RFC‑0003 references accordingly.
- Ensure RFC‑0003 (2DH/session init) specifies how to select among:
  - `keys.encryption.current`
  - any non-expired entries in `keys.encryption.previous`

---

### 5) **Domain separator “byte-length” guidance is inconsistent with the literal strings shown**
**Where:** progress log mentions fixed byte lengths (e.g., “relay-alloc: 25, rebind: 20, device: 20”), but the strings shown in docs (e.g. `"post-urbit-relay-allocate-v1"`, `"post-urbit-device-handshake-v1"`) do not match those lengths.

**Why blocking:** If any RFC states “prefix length = X bytes” and an implementation uses that to build/parse, it will fail verification across implementations.

**Fix recommendation:**
- Create a **single Domain Separator Registry** (normative) listing:
  - exact ASCII bytes (string literal)
  - whether the signed payload is `prefix || data` or `SHA256(prefix || data)`
  - which encoding is used for fields (UTF‑8 strings vs raw bytes)
- Remove manual byte-length assertions from narrative text; derive lengths mechanically in test vectors instead.

---

## HIGH (very likely to break correctness or cause divergent implementations)

### 6) **Identity test vectors/examples contradict “raw 32-byte key” rule**
**Where:** `identity-document-schema.md` test vector shows base64 like `MCowBQYDK2Vw...` (looks like DER/SPKI), while the spec says **raw 32-byte Ed25519 public key** base64.

**Why high:** Implementers may accidentally accept DER keys (or generate IIDs from the wrong bytes), breaking identity stability and interop.

**Fix recommendation:**
- Replace all test vectors with **real raw 32-byte** key material:
  - show raw pubkey bytes (hex) + base64 (no padding)
  - show derived IID from those raw bytes
- Add a negative test: “DER/SPKI MUST be rejected”.

---

### 7) **Stream framing rules are clear for Control/Handshake but underspecified for Message/PUSE delivery**
**Where:**
- `peer-handshake.md` defines: first bidi stream, 1-byte stream type, then **4-byte length-prefixed JSON** frames.
- QUIC integration mentions stream type then payload, but not the exact framing for message streams.
- Progress log says “JSON for 0x01–0x02, binary for 0x03–0x05”, but the binary framing isn’t spelled out here.

**Why high:** RFC‑0003 PUSE envelope delivery needs a deterministic framing (length prefix? varint? one envelope per stream?).

**Fix recommendation:**
- In RFC‑0002 (Transport), define normative framing for stream types:
  - Control (0x01): length-prefixed JSON (already done)
  - Identity (0x02): length-prefixed JSON (implied, ensure explicit)
  - Message (0x03): **length-prefixed binary frames** (define prefix size/endianness or QUIC varint)
- In RFC‑0003, reference that framing explicitly (e.g., “PUSE envelope bytes are sent as one binary frame on stream type 0x03”).

---

### 8) **Signing key history limits contradict themselves**
**Where:** `identity-document-schema.md` optional fields table says `keys.signing.history` “Max 3 entries”, later says retention “max 10 previous signing keys or 2 years”.

**Why high:** Verifying older package signatures / delayed mailbox messages depends on history retention. Divergent limits break verification across nodes.

**Fix recommendation:**
- Pick one:
  - Update table to match retention (10 / 2 years), or
  - Reduce retention text to match “max 3”.
- Ensure RFC‑0003 and app signing rules rely on the chosen limit (or specify fallback behavior when history is insufficient).

---

### 9) **Revocation verification pseudocode has logic gaps/bugs**
**Where:** `verifyKeyRevocation()` references `recovery_proof` without destructuring; doesn’t confirm `revoked_key` matches stored doc; doesn’t handle encryption-key revocation semantics (see BLOCKING #1).

**Why high:** Different implementations will accept/reject different revocations.

**Fix recommendation:**
- Tighten verification steps:
  - Ensure `revoked_key` equals the appropriate key in the *stored* doc (signing current/previous/history, encryption current/previous list).
  - Ensure replacement document’s relevant key actually changes.
  - Ensure `effective_at` ordering and sequence ordering rules are explicit.
- Fix the code sample to be internally consistent (variable scope, required fields).

---

### 10) **Error-code registries are fragmented and may collide**
**Where:**
- QUIC application error codes (0x100+) defined in `quic-integration.md`
- Relay error codes are separate (0x01–0x07) inside relay ERROR packet
- Handshake “error.code” uses strings (`IDENTITY_MISMATCH|...`)
- Messaging RFC‑0003 likely has its own errors (not visible here)

**Why high:** Implementations need a single mapping for:
- QUIC application close codes
- per-protocol error payload codes (relay, mailbox, messaging)

**Fix recommendation:**
- Create `00-shared/error-code-registry.md` (normative):
  - QUIC application close codes (numeric)
  - Relay ERROR packet codes
  - Messaging protocol-level error codes (if any)
  - Handshake failure reasons: either standardize on numeric codes or clearly mark as JSON-only diagnostic strings not used for protocol decisions.

---

## MEDIUM (security/usability gaps that will matter in production)

### 11) **2DH (RFC‑0003) must explicitly bind to the recipient’s X25519 key version**
**Focus item you asked:** “Does RFC‑0003’s 2DH use `keys.encryption.current` correctly?”

**Risk:** If the recipient rotates encryption keys and publishes multiple `keys.encryption.previous` entries, a sender/receiver needs an unambiguous way to know *which* static key was used for the 2DH calculation.

**Fix recommendation (normative in RFC‑0003):**
- Include in the session-init/PUSE header either:
  - the recipient static X25519 public key bytes used, or
  - a “recipient encryption key id” (e.g., hash of the 32 bytes), or
  - the recipient identity document `sequence` that was used for key selection.
- Define selection rules: prefer `current`, fallback to any non-expired `previous` entry.

---

### 12) **Recovery and “recovery config updates” are an attack surface**
**Risk:** An attacker with the signing key can update recovery config to weaken recovery, then rotate. Current docs don’t specify any stabilization rule (cooldown for recovery-config changes, trustee notifications, etc.).

**Fix recommendation:**
- Add a rule: recovery config changes either:
  - require a cooldown before becoming active, or
  - require an additional confirmation (e.g., device escrow or trustees), or
  - trigger automatic out-of-band notifications to trustees with a contest window.

---

### 13) **Identity revocation is a DoS vector**
**Risk:** If an attacker obtains the signing key briefly, they can publish a terminal `identity_revocation` and permanently kill the identity.

**Fix recommendation:**
- Consider making `identity_revocation` require either:
  - recovery authorization, or
  - a cooldown + contestation (similar to recovery), or
  - a “two-step tombstone” (pending → final).

---

## LOW (docs polish / consistency)

### 14) Snake_case vs camelCase is mostly consistent, but examples drift
You’re already explicit about snake_case on-wire and JCS on the snake_case form. Audit remaining examples (rotation/recovery/revocation) to ensure they use snake_case and correct field names.

### 15) Terminology drift: “post-urbit” vs “postnode” vs “postapp”
Not inherently wrong, but domain separators and protocol labels should be centralized so implementers don’t guess which prefix applies in which context.

---

# Summary: the “must-fix” set
If you fix only a handful to get to interop, do these first:

1) **Revocation schema for encryption keys** (X25519 can’t sign)  
2) **Single authoritative DHT record/signature model + update all interfaces**  
3) **Mailbox layering/API contract** (Transport vs Messaging responsibility)  
4) **Normalize `keys.encryption.previous` across all docs/examples**  
5) **Centralize domain separators (and remove/repair byte-length claims)**  
6) **Replace identity test vectors with true raw-key vectors**

If you want, paste RFC‑0003’s concrete 2DH + PUSE header definitions (the parts that mention the identity doc fields), and I can do a line-by-line check specifically for: key selection, encoding, signature inputs, and transport framing assumptions.

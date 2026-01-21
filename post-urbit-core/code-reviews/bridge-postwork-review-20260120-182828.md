## Overall rating: **7/10**

Strong core design (single entry point, identity from infrastructure, default-deny registry, session binding, strict size limits, replay cache semantics). The main gaps are (a) **CBOR strictness details are underspecified/possibly unenforceable with common decoders**, (b) **replay caching is not safe under concurrency and is checked too early**, (c) **timestamp “anti-replay” is weaker than implied**, (d) **event long-poll implementation is DoS-prone**, and (e) a few **crypto/format hardening** items.

---

## 1) CBOR profile: is it strict enough?

### What’s good
- Explicitly rejects indefinite-length items and unknown tags (good for parser-bomb defense).
- Enforces payload size and structural limits (depth/collection length/string length).

### Issues / ambiguities
- **Duplicate map keys**: many Rust CBOR stacks won’t reliably detect duplicates *after* decoding into `serde`/`Value` (the “last wins” behavior can silently occur). If you truly “MUST reject duplicate map keys”, you need a decoder/validator that *tracks keys during parse*, not after materialization.
- **Non-UTF8 text strings**: CBOR text strings are required to be UTF‑8 by spec, but some decoders accept invalid UTF‑8 by mapping to replacement characters or erroring inconsistently. You need explicit enforcement at the byte level.
- **Tag handling**: “unknown tags rejected unless whitelisted” is good, but you need an explicit whitelist (even if empty). Otherwise implementers will drift.
- **Canonical/normalized encoding** is not mentioned. Not strictly required, but without it you can get multiple byte representations of “the same” semantic request, which complicates logging, request hashing, and any future signing/dedup schemes.

### Improvements
1. **Use a streaming CBOR validator** that:
   - walks tokens, enforces definite lengths, depth, max items, max bytes
   - enforces **unique map keys per map** (track a hash set of key encodings)
   - rejects NaN/Inf floats and disallowed simple values/tags
2. Consider requiring **Canonical CBOR (RFC 8949 §4.2.1)** at least for:
   - map key ordering
   - preferred integer encodings
   This reduces ambiguity and helps deterministic request hashing.
3. Reduce attack surface by avoiding `ciborium::Value` for `params` where feasible:
   - prefer **typed per-method param structs** (`#[serde(deny_unknown_fields)]`)
   - at minimum, enforce `params` max size/complexity *per method*

---

## 2) Session binding: is it bulletproof?

### What’s good
- Identity derived from `webview.label()` (good “identity from infrastructure”).
- Session bound to **webview label** and app_id derived from that label.
- Collapsing auth failures to `UNAUTHORIZED` reduces probing.

### Gaps
- **`app_id` parsing accepts empty**: `label = "app-"` yields `app_id = ""` and currently passes the prefix check.
- **Label/app_id character policy** is not defined. If app_id is later used for filesystem paths, DB keys, logs, metrics labels, etc., you risk injection/path traversal/log forging unless you constrain it.
- **Uniqueness/immutability assumptions** about webview labels should be stated explicitly:
  - Labels must be **minted by the backend**, not controllable by app content.
  - Labels must be **unique per webview** and not re-bindable.
- Spec implies a “create_session (shell-only)” handshake, but does not specify **how the token/session is delivered** to the app webview securely (e.g., init script before any untrusted JS runs). That delivery mechanism is often where real-world bypasses occur.

### Improvements
1. Enforce `app_id` constraints at extraction:
   - non-empty, length bounds (e.g., 1..64)
   - strict charset (e.g., `^[a-z0-9][a-z0-9_-]{0,63}$`)
2. Bind session to an additional **non-forgeable instance identifier** if available (defense-in-depth):
   - webview internal id/handle (not just label), or
   - a backend-issued “webview instance nonce” stored server-side at creation
3. Specify the session bootstrap explicitly:
   - session is created by backend, token injected via **initialization script** before app JS executes (or equivalent)
   - prohibit app-controlled navigation from reusing the session in a different origin/context unless intended

---

## 3) Anti-replay with cached response: is it correct?

### What’s good
- Returning cached response for duplicate `(session_id, request_id)` is a solid “exactly-once within window” behavior **for retries**.
- Bounding window/entries is the right direction.

### Critical issues
1. **Replay check occurs before auth/session validation** in the example flow.  
   If an attacker ever obtains a `(session_id, request_id)` pair (logs, leakage, side channels), they could potentially retrieve cached responses without presenting a valid token/webview binding *if you return cached responses prior to auth*. Even if session_id is high entropy, don’t rely on that.
2. **Not concurrency-safe for in-flight duplicates**:
   - If the same `(session,id)` arrives twice concurrently, both can execute because cache is populated only after completion.
   - This breaks the “can’t force re-execution” claim.
3. **Cache key ignores request bytes**:
   - If the same `(session,id)` is reused with *different* `method/params`, you’ll return a response for the earlier request (confusing at best; occasionally exploitable depending on method semantics).

### Improvements
1. Move processing order to:
   **parse → validate envelope fields → validate session binding → validate token → rate limit → replay/singleflight → execute**
2. Implement **singleflight/in-flight dedupe**:
   - On first sight of `(session,id)` insert a placeholder “InProgress”
   - Subsequent duplicates await the first completion and get the same response
3. Store and verify a **request digest**:
   - cache key: `(session_id, request_id)`
   - cached metadata: `sha256(request_bytes_canonical)` (or canonical CBOR hash)
   - if same key but different digest → `INVALID_REQUEST` (or collapsed error)
4. Consider **not caching certain transient failures** (optional, policy-dependent):
   - If you cache `TIMEOUT` but the underlying task wasn’t actually cancelled, clients may never learn the real outcome.
   - Prefer making handlers cancellation-safe, or persist idempotency outcomes for non-idempotent methods.

---

## 4) Error code collapsing / oracle resistance

### What’s good
- Auth failures collapse to `UNAUTHORIZED` with no details: correct.
- Avoids leaking internal diagnostics in `details`.

### Remaining oracle surfaces
- **Ordering/timing**: if you look up method specs or do heavy param decoding before auth, you create timing differences that can reveal:
  - whether a session id exists
  - whether a method is valid
- `PERMISSION_DENIED` currently includes the missing capability string. That’s not as severe as auth oracles, but it *does* enable method/capability enumeration and fingerprinting.

### Improvements
1. Guarantee processing order: **authenticate first**, then method existence/authorization, then expensive parsing.
2. Consider collapsing permission errors to a generic message:
   - `PERMISSION_DENIED` but message “Permission denied”
   - keep the missing capability only in server logs
3. Normalize error latency (where practical) for auth failures (small random jitter can help, but be cautious).

---

## 5) Event subscription model security

### What’s good
- App-initiated long-poll is simpler and avoids push channels.
- Subscription is bound to session_id on poll: good.

### Security/robustness problems
- The provided `poll()` loop is a **busy-wait** with a fixed sleep. With many sessions/subscriptions, this becomes a CPU DoS vector.
- No explicit rule that **topics are namespaced/authorized**. Without that, an app could subscribe to global/system topics and learn cross-app/system signals.
- Response size constraints must be enforced: events can accumulate and exceed 256KB unless chunking/pagination rules are explicit.
- Missing “**one outstanding poll per subscription/session**” rule. An app can tie up all 16 concurrent request slots using `events.poll` (30s) and starve other operations.

### Improvements
1. Replace polling loop with async notifications:
   - `tokio::sync::Notify`, channels, or condition variables keyed per subscription
2. Enforce topic authorization:
   - require `events.subscribe` to check `topic` against an allowlist and capabilities
   - strongly consider per-app namespace: `app.{app_id}.…` (backend enforces)
3. Enforce **one in-flight poll per subscription** (or per session), and separate concurrency budgets:
   - e.g., allow 1–2 long polls max, reserve concurrency for short calls
4. Enforce event payload bounds:
   - per-event max bytes
   - per-poll max bytes in addition to `max_events`
   - drop policy should be explicit and observable (`dropped_count`, `reset_seq` semantics)

---

## 6) Cryptographic issues / missed attack vectors

### What’s good
- HMAC-SHA256 with OS-keystore secret is reasonable for a local bridge.
- Key rotation fields exist.
- Constant-time compare is attempted.

### Crypto/format hardening issues
1. **Ambiguous HMAC payload formatting**: `format!("...:{}:{}:...")` risks delimiter ambiguity if any field can contain `:`. Today your label/app_id may be controlled by infrastructure, but don’t rely on “probably doesn’t contain ':'”.
2. **Token is effectively a bearer credential** for the session. If app JS is compromised (XSS/supply-chain), attacker can call any allowed method anyway—this is inherent—but the spec should be explicit that the token is not meant to protect against a malicious app, only cross-webview confusion and unauthorized invocation.
3. `constant_time_eq` early-returns on length mismatch. Length is not secret, but you should still prefer vetted primitives (`subtle` crate) and enforce tight token length bounds.
4. No explicit **token/session string length limits** beyond generic CBOR string limits (65KB). A 65KB `token` field is unnecessary and can become a CPU/memory DoS primitive.

### Improvements
1. HMAC over structured bytes:
   - CBOR-encode a small array `[v, session_id, app_id, webview_label, created_at, nonce]` in canonical form, then HMAC that
   - or use length-prefix binary encoding
2. Enforce strict token syntax:
   - `kid` charset/length
   - signature must be base64url, and **expected length** for 32-byte HMAC (43 chars no-pad)
   - overall token length cap (e.g., ≤ 128)
3. Add explicit max lengths for `session`, `id`, `method`, `trace_id`, etc. (tight, not 65KB).
4. Ensure sensitive logging rules:
   - never log `token`, raw `params`, or large blobs
   - log request metadata + hashes
5. Rate-limit and quota-limit **error_id generation/logging** to prevent log amplification.

---

## Summary of highest-priority changes
1. **Authenticate before replay-cache return**; include webview/app binding in replay decision.
2. Add **singleflight/in-flight dedupe** and **request digest** validation for `(session,id)`.
3. Implement a **real streaming CBOR validator** with duplicate-key rejection (or require canonical CBOR + a decoder that enforces uniqueness).
4. Fix app label parsing (`app-` empty) and constrain `app_id` charset/length.
5. Replace event long-poll busy-wait with async notify; enforce topic authorization + 1 outstanding poll policy.
6. Harden token formatting (structured HMAC payload, strict token length/syntax).

If you want, I can propose a concrete “correct” request handling pipeline (step-by-step) and the exact data structures for singleflight + digest binding that preserve your current envelope format.

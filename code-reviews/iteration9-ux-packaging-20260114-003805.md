## BLOCKING (would prevent a correct/secure implementation)

### 1) TLS/session model contradictions (can’t implement as written)
- Spec says **`secure: true` always (even localhost uses TLS)**, but deployment + first-run UX says Admin UI at **`http://localhost:8080`** and Docker exposes **8080** plaintext.
  - **Action:** Decide one:
    - (A) default Admin UI is HTTPS everywhere (even localhost), ship self-signed cert + trust-on-first-use UX; or
    - (B) allow HTTP on localhost and set `secure=false` for localhost *only*; or
    - (C) split “local dev mode” vs “production mode” explicitly.
  - Also clarify whether the Admin UI is served from the same origin as the Admin API (currently implied) and how mixed-content is avoided.

### 2) Authentication flow is inconsistent (cookie vs bearer token vs admin token vs password)
- Login returns `{ session, token }` and API client uses `Authorization: Bearer <token>`, **but session cookie is also defined** (`postnode_session`, httpOnly).
- Daemon “AuthConfig” says Bearer token can be **admin token, API key, session cookie, app token**—but cookies aren’t Bearer tokens.
- “Admin password” exists in UI, but daemon config uses `POSTNODE_ADMIN_TOKEN_HASH` and headless setup generates an **admin token**, not a password.
  - **Action:** Choose a single coherent browser auth mechanism:
    - Option 1: browser uses **httpOnly session cookie** only; login returns session metadata but **no token**; CSRF protection required.
    - Option 2: browser stores **access token** (but then `httpOnly` cookie session is redundant); must define secure storage strategy (not localStorage), refresh, rotation.
  - Define relationship between:
    - “admin password” (interactive setup),
    - “admin token” (headless / automation),
    - “API keys” (scoped),
    - “sessions” (browser).
  - Explicitly define auth for **CLI** vs **Admin UI** vs **third-party clients**.

### 3) CSRF is mentioned but not specified (implementation-blocking for cookie auth)
- You claim “CSRF tokens” in Security Model, but there is no:
  - token issuance endpoint,
  - header/cookie naming convention,
  - validation rules,
  - which endpoints require it.
- SameSite=Strict reduces risk, but does not replace CSRF for all browser scenarios (and breaks some embedded/redirect flows).
  - **Action:** Specify CSRF strategy:
    - Double-submit cookie (`csrf_token` + `X-CSRF-Token`) or same-origin `Origin`/`Sec-Fetch-Site` enforcement.
    - Add endpoints/headers and error codes.

### 4) API surface doesn’t match the defined HTTP endpoints (can’t implement client/server consistently)
Examples:
- `AdminApiClient.installApp(source: AppSource)` has no `appId`, but daemon endpoints show `POST /apps/{app_id}/install`.
- Backups:
  - System API: `restoreBackup(file: File)` (upload)
  - AdminApiClient: `restoreBackup(backupId: string, password?: string)` (restore by id)
  - HTTP endpoints only list `POST /backup` (create) and no list/restore endpoints in daemon section.
- Identity rotation endpoints: daemon shows `/identity/rotate` but UI client splits rotate signing/encryption separately.
  - **Action:** Produce a single authoritative REST map (paths + methods + request/response schema) that matches the TypeScript interfaces, including:
    - install by URL/file/repo,
    - backup list/download/upload/restore,
    - key rotation endpoints,
    - logs query parameters,
    - device add activation flow.

### 5) Package install size limits conflict with HTTP server request limits (file install impossible)
- `.postapp` total size limit = **100MB**.
- HTTP server `max_request_body_bytes` = **10MB**.
  - **Action:** Either:
    - increase request body limit to ≥ package max, or
    - support **streaming upload** (chunked) / resumable upload, or
    - require URL-based install for large packages and define UI behavior.

### 6) WebSocket URL/auth is contradictory and underspecified
- Spec shows `wss://localhost:8080/admin/v1/events` and also client uses `new WebSocket('/admin/v1/events')`.
- CSP allows `connect-src 'self' wss:;` which is overly broad.
- No statement whether WS uses:
  - session cookie,
  - Bearer token query param,
  - Sec-WebSocket-Protocol.
  - **Action:** Define:
    - exact WS URL (relative vs absolute),
    - authentication mechanism,
    - reconnect/backoff behavior,
    - event ordering/at-least-once semantics,
    - whether missed events can be replayed (cursor/lastEventId).

### 7) Signature “timestamp max 7 days” breaks normal distribution/update use cases
- Verification step rejects signatures older than 7 days. That makes it impossible to:
  - install older versions,
  - install from cold storage,
  - use long-lived releases.
  - **Action:** Replace with a meaningful check:
    - allow old signatures, but ensure key-valid-at-time and optionally apply *optional* freshness checks only for “direct URL downloads” to mitigate replay, or
    - enforce timestamp only against **repository/update manifests**, not package signatures, or
    - define “build timestamp” vs “sign timestamp” semantics.

---

## HIGH (major gaps/edge cases; security or major functionality risk)

### 1) Missing type definitions referenced throughout
- `RecoveryConfig` is referenced in multiple places but never defined.
- `Endpoint` in `IdentityInfo.endpoints` is referenced but undefined.
- `MessageSummary` and message export types referenced but undefined.
- `AppSettings` collision: you have `NodeSettings.apps: AppSettings` and also `AppsApi.getSettings(appId): Promise<AppSettings>`.
  - **Action:** Add the missing types and rename to avoid collision (e.g., `NodeAppSettings` vs `PerAppSettings`).

### 2) “Remember this device (30 days)” not specified end-to-end
- UI has remember device, `Session.deviceId?`, and `DeviceAddResult.activationCode` exists, but flows aren’t connected:
  - How is a remembered browser mapped to a “device”?
  - Is it a **device credential** separate from a session?
  - What happens on device removal—does it revoke remembered sessions?
  - **Action:** Specify:
    - persistent device token format, rotation, revocation,
    - storage location (cookie vs platform keychain),
    - admin UI page for “Active sessions” and “Remembered devices” (currently only “Devices”).

### 3) Sensitive-operation reauth rules are partial and inconsistent
- Uninstall app: “No” reauth, but clear data: “Yes”. Uninstall can be equally destructive.
- “View settings: No / Change settings: Yes” — but some settings (API keys, allowlist, recovery) are more sensitive than others.
  - **Action:** Define a sensitivity classification per endpoint and enforce on server:
    - e.g., `requires_fresh_auth: true` in endpoint metadata.
  - Define reauth mechanism: prompt password? token step-up? TOTP?

### 4) Repository manifest signing is underspecified (can’t validate “repository compromise” mitigation)
- `repository.json` has `"signature": "<repository-operator-signature>"` but no:
  - canonicalization rules,
  - signing payload,
  - algorithm,
  - key discovery (operator identity doc? pinned key? TLS-only?).
- Same for `updates.json` signature format (what exactly is signed?).
  - **Action:** Define signing exactly like `.postapp`:
    - canonical JSON, hash, payload prefix, signature type, key source and pinning rules.

### 5) App install/update flows omit important failure and rollback details
- Update flow says “Backup current app data” and “Rollback if startup fails” but doesn’t define:
  - what constitutes “startup fails” (health check? exit code? timeout?),
  - where backups are stored and retention,
  - migration contract (who triggers, what API, version gating),
  - atomicity of install switch-over.
  - **Action:** Define an app lifecycle/update state machine and what Admin API returns during long-running operations (job IDs, progress events).

### 6) API error handling assumptions don’t match HTTP realities
- `ApiClient.request()` always does `return response.json()`; breaks for:
  - `204 No Content` (e.g., DELETE),
  - non-JSON error bodies (reverse proxies),
  - large downloads (backup).
  - **Action:** Define consistent response semantics:
    - success envelopes vs raw,
    - which endpoints return `204`,
    - error response always JSON (even on 5xx), or client must fall back to text.

### 7) Installing from “file path” is not realistic for browser Admin UI
- `AppSource { type:'file', value: string /* file path */ }` works for CLI, not web.
  - **Action:** Split sources by client type:
    - Browser uses `File` upload (multipart/form-data) or File System Access API (still yields a blob).
    - CLI uses filesystem paths.
  - Reflect this in Admin API endpoints.

### 8) Trust model for “Verified author” is unclear
- UI shows “✓ Verified”, contacts have trust, repositories have trust, identity docs fetched from DHT—but “verified” criteria is not defined.
  - **Action:** Define verification sources and precedence:
    - local trust marks,
    - repository endorsement,
    - web-of-trust (contacts),
    - enterprise allowlist.
  - Provide a deterministic `verificationStatus` field in API for UI to display.

---

## MEDIUM (important clarifications; likely bugs or UX issues if unspecified)

### 1) Pagination/sorting contracts are vague
- `PaginationOptions.sortBy?: string` is free-form; server must whitelist.
  - **Action:** Define allowed sort fields per endpoint (`contacts: ['displayName','addedAt','lastSeen','trustLevel']` etc).

### 2) Logs API lacks streaming / cursor semantics
- `getLogs(options)` returns `LogEntry[]` only; no paging/cursor, no “tail -f”.
  - **Action:** Add:
    - pagination cursor or `offset` for logs,
    - WS event type `log_entry` optional,
    - download diagnostic bundle endpoint.

### 3) Identity export and backup download/upload flows are missing from Admin UI spec
- “Export identity” action exists, but no API method or file format.
- Backups return `path` which is server-local; UI needs download URLs.
  - **Action:** Add endpoints:
    - `GET /admin/v1/identity/export` (returns encrypted blob),
    - `POST /admin/v1/backups` create,
    - `GET /admin/v1/backups/{id}` download,
    - `POST /admin/v1/backups/upload` and `POST /admin/v1/backups/{id}/restore`.

### 4) App permissions model unclear (granted/denied/pending)
- What does it mean to “updateAppPermissions(Partial<AppPermissions>)”?
  - Can caller move capability from pending→granted by including in `granted`?
  - What about removing from `denied`?
  - **Action:** Use an explicit patch model:
    - `{ grant: Capability[], deny: Capability[], resetToDefault?: boolean }`.

### 5) CSP policy and asset strategy are incomplete
- CSP has `style-src 'unsafe-inline'` which contradicts “security first” posture (though you only forbade inline scripts).
- `connect-src 'self' wss:;` allows connecting to any WSS host; weakens CSP.
  - **Action:** Tighten CSP:
    - `connect-src 'self' wss://<same-host>` or `'self'` plus explicit allowed repo domains if needed.
  - Clarify if Tailwind is embedded at build-time (no external CDN).

### 6) Mobile “essential operations only” is undefined
- Which operations are essential? Login? restart? key rotation?
  - **Action:** Provide a list and acceptance criteria (what pages/actions available on mobile).

### 7) Device removal edge cases
- Can you remove the current device? What if it’s the last device?
- How do “devices” relate to transport keys vs admin UI sessions?
  - **Action:** Define invariants:
    - minimum one authorized device,
    - removing current device requires adding another first or immediate lockout confirmation flow.

### 8) Update URL trust and takeover
- If `distribution.update_url` is attacker-controlled (e.g., compromised hosting), signature verification helps, but:
  - how do you ensure updates are from same `author_iid` and same `app_id`?
  - what if update manifest points to a different package with same author but different app_id?
  - **Action:** Require update manifest to include and match:
    - `app_id`, `author_iid`, and current installed app’s `author_iid` and `id`.

---

## LOW (polish, consistency, maintainability)

### 1) Naming collisions and clarity
- `AppSettings` used for node-level and per-app settings.
- `SettingsApi.get<K extends keyof NodeSettings>(key: K)` exists, but AdminApiClient uses only `getSettings(): Promise<NodeSettings>`.
  - **Action:** Standardize; keep one “public” client interface and generate TS types from OpenAPI.

### 2) UI pages list includes `Messages.tsx` but Admin UI says “read-only”; permissions include `send:messages`
- **Action:** Clarify whether Admin UI ever sends messages (Quick Action shows “New Message”).
  - Either remove send from Admin UI or define which admin surfaces allow sending.

### 3) Error code catalogs differ between sections
- Daemon error codes omit `VALIDATION_ERROR` which appears later.
  - **Action:** Single source of truth for error codes + HTTP mapping.

### 4) Docker / nginx examples don’t mention WS and CSP implications
- Nginx snippet includes WS headers (good), but CSP `connect-src` must allow that origin.
  - **Action:** Document recommended headers when reverse-proxied and how UI discovers WS URL.

---

## Biggest “interface gaps” to close next (concrete deliverables)

1) **Authoritative API definition** (OpenAPI) that reconciles:
   - endpoints, payloads, auth, errors, long-running jobs, downloads/uploads.
2) **Auth spec** (one page) covering:
   - password vs admin token vs API key, cookie vs bearer, CSRF, reauth/step-up.
3) **Repository + update signing spec** matching `.postapp` rigor.
4) **Backup/install transfer protocols** supporting 100MB+ via streaming and resumability.
5) **Missing types** (`RecoveryConfig`, `Endpoint`, message/export types, per-app settings types) plus naming cleanup.

If you want, I can propose a concrete REST map (paths/methods) that exactly matches your `AdminApiClient` plus the minimal additions needed for file upload, backup download, WS auth, and long-running operations.

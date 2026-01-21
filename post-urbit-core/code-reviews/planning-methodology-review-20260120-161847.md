## 1) Are the 10 domains comprehensive and correctly ordered?

### Coverage
The 10 domains are a strong backbone for an “app platform” frontend: you’ve captured the main vertical slices (shell → sandbox → bridge → permissions → SDK → lifecycle → storage → resource mgmt → hardening → testing).

That said, **they’re not fully comprehensive for a production desktop platform**, mainly because several cross-cutting “platform” concerns don’t have an explicit home (details in section 2).

### Ordering
The ordering is *mostly* correct, but there are two practical issues:

1) **Security hardening is too late given your known risks**  
You already know the existential risk is “untrusted app can reach Tauri IPC / privileged plugins” and that isolation choice (iframe vs multi-webview) drives everything. Waiting until Domain 9 to fully specify IPC lockdown, navigation controls, exfil paths, etc. is likely to cause rewrites.

**Actionable adjustment:** treat “Security Hardening” as a **parallel track that starts immediately after Domain 2** (or even folded into Domains 1–3 as required sections). Keep Domain 9 as a consolidation doc, but don’t defer the actual decisions.

2) **Memory/resource management needs to be pulled forward if multi-webview is default**  
Multi-webview is explicitly recommended for untrusted apps, but it materially affects UX (load time), architecture (LRU unloading), and even API design (subscriptions/backpressure). Waiting until Domain 8 to quantify limits is risky.

**Actionable adjustment:** after Domain 2 (isolation decision), run Domain 8 as an early “constraints spec” (targets + budgets + LRU policy skeleton), then iterate later.

A workable revised dependency flow often looks like:
- **(0) Spike gates** → 1 Shell → 2 Sandbox → (8 Resource constraints) → 3 Bridge → 4 Permissions → 5 SDK → 6 Lifecycle/trust → 7 Storage → 9 Hardening consolidation → 10 Testing.

## 2) Missing domains or concerns (specific gaps)

These are the biggest “no clear owner” items I see:

### A. **App trust, signing, keys, revocation, and update security** (needs a first-class home)
You mention signing in review notes, and Domain 6 includes “verification,” but in practice this is large enough to deserve either:
- a dedicated domain, or
- an explicit sub-section template required in Domain 6.

**What’s missing if it stays implicit:**
- trust store format (developer keys, rotation)
- revocation mechanism and UX
- downgrade/replay protection for updates
- transparency/logging story (even minimal)

**Action:** Add **Domain 6A: Package Trust & Updates** (or explicitly enumerate these as Domain 6 deliverables with acceptance criteria).

### B. **Protocol/ABI governance + versioning (machine-readable)**
You already have naming inconsistencies in the source docs. That’s a symptom of missing “one canonical registry.”

**Action:** Add a “Protocol Registry” artifact (could live under Domain 3 or 5) that is:
- machine-readable (JSON/YAML)
- generates Rust structs + TS types
- includes version negotiation and deprecation rules

This is the single highest leverage way to prevent drift.

### C. **Observability & diagnostics**
Desktop platforms need: audit logs (you have), but also:
- crash reporting strategy (even local-only)
- structured logs with per-app/session correlation IDs
- performance metrics (webview creation time, bridge latency, memory per app)
- user-facing diagnostics bundle (“export logs”)

**Action:** Add a small domain or a required section in Domains 1/8/9/10:
- “Logging/metrics/events: schema + storage + redaction rules + viewer UI.”

### D. **Navigation/external-open/download policy (explicit platform policy doc)**
You list this as a security concern, but it tends to sprawl across shell, sandbox, and hardening.

**Action:** Make “External interaction policy” a **named deliverable** (with a matrix):
- allowed URL schemes
- `window.open` handling
- file downloads
- clipboard policy
- drag-and-drop boundaries

### E. **Post-Urbit core integration contract**
FRONTEND_ARCHITECTURE references a “Post-Urbit core” and optional “HTTP bridge,” but none of the domains explicitly pins down:
- what’s embedded vs remote
- how identity is sourced
- offline-first rules, sync boundaries
- how errors propagate

**Action:** Add a “Core Integration Contract” section to Domains 3/7 (or a dedicated domain) defining:
- minimal required core APIs
- serialization formats
- failure modes and retries

### F. **UX system + accessibility + theming + i18n**
You mention shadcn/Tailwind but there’s no explicit deliverable for:
- accessibility targets (keyboard, screen reader)
- theming tokens and app theming constraints
- localization strategy (even “not in v1” should be explicit)

**Action:** Add a “UX & Accessibility” slice under Domain 1 (shell) and Domain 5 (SDK, for app-facing theming hooks).

## 3) Is the review cadence appropriate?

### What’s good
- **Pre + Post review per domain** is a solid quality loop (prevents the “write spec then discover contradictions” pattern).
- **System reviews every 5 loops** is reasonable because by Domain 5 you’ve defined most of the external surface area (bridge + permissions + SDK), so it’s the right checkpoint.

### What I would change
1) **Add a “Gate Review” before Domain 1–3 specs lock in**
Given your existential risk, you need a *proof step*, not just a spec review.

**Action:** Insert “Phase 0 / Gating Spikes” *before* Loop 1, with acceptance criteria like:
- untrusted app cannot call privileged Tauri APIs (prove with a malicious test app)
- chosen isolation model works on Win/macOS/Linux with baseline memory numbers
- CSP/header injection works reliably in the custom protocol

2) **Make reviews checklist-driven and artifact-based**
Right now the cadence is time-based (every loop), but you’ll get more consistency if every review checks the same invariants.

**Action:** Create a standard review checklist used in *every* domain review:
- naming aligns with protocol registry
- threat model section included
- platform matrix addressed (Win/macOS/Linux deltas)
- “What must be enforced in Rust vs UI” explicitly stated
- test plan + acceptance criteria included

## 4) Suggestions to improve the planning process (actionable)

### 1) Add ADRs (Architecture Decision Records) and require them for key forks
You have known forks (iframe vs multi-webview, static capabilities vs runtime enforcement, embedding vs not). Specs alone won’t prevent re-litigation.

**Action:** Add `/docs/adrs/` with a lightweight template:
- decision
- status
- options considered
- security/perf implications
- rollback plan

### 2) Establish a single source of truth for the app/platform ABI
This directly addresses the naming contradictions called out in your review.

**Action:** Add `docs/specs/PROTOCOL_REGISTRY.yaml` and mandate:
- Domain 3 updates it
- Domain 5 SDK types are generated from it
- Rust bridge decoding validates against it

### 3) Convert each domain into “implementation-ready” by enforcing a Definition of Done
Right now “implementation-ready” is stated, but not enforced.

**Action:** For each domain, require:
- sequence diagrams for key flows
- data models + storage schema + migrations
- explicit Rust/TS interfaces (function signatures)
- security invariants (“must be enforced in Rust”)
- acceptance criteria + test cases

### 4) Track risks explicitly with owners and retirement criteria
You already identified derailers (IPC escape, memory blowups). Make them first-class.

**Action:** Add `docs/RISK_REGISTER.md` with:
- risk, severity, likelihood
- owner
- mitigation
- “retired when…” measurable condition (e.g., E2E test exists + passes on 3 OSes)

### 5) Plan “vertical slice milestones” instead of only domain completion
Domain completion can still produce integration surprises.

**Action:** Define 2–3 milestone slices that must work end-to-end:
- Slice A: install app → launch → handshake → `storage.get/set` → permission prompt → audit log
- Slice B: app-to-app call with permission mediation
- Slice C: update app with signature verification + rollback

---

### Net assessment
- **Domains:** strong foundation but missing several platform-grade concerns (trust/update, ABI governance, observability, core integration contract, UX accessibility).
- **Order:** mostly right, but security/resource constraints should start earlier and run in parallel.
- **Cadence:** good structure; add gating spikes + checklist discipline to reduce rework.

If you want, I can propose a concrete “Phase 0 (1–2 weeks) gating spike plan” with exact experiments and pass/fail criteria aligned to the top existential risks.

Below is what **Domain 6.5: Package Trust & Updates** must cover given the current spec set, plus the **gaps** and the **cross-spec dependencies that need resolution**.

---

## 1) App package format (`.postapp`) and manifest schema

### What the Package Trust spec needs to define
**Package container + canonical layout**
- Whether `.postapp` is ZIP “as-is” or a constrained ZIP profile:
  - disallow symlinks, hardlinks
  - disallow absolute paths / `..` path traversal
  - define compression method allowlist (e.g., deflate/store only)
  - define duplicate-path handling (fail closed)
  - define max compression ratio / unzip-bomb defenses
- Canonical directory layout (align with `postapp://{app_id}/...` handler):
  - `manifest.json` (required)
  - `SIGNATURE` / `SIGNATURES` (required outside dev mode)
  - `ui/` subtree served via `postapp://`
  - `main.wasm` (if it’s truly required) and how/when it is executed (or if it is inert for now)

**Manifest schema (normative)**
- Required fields: `app.id`, `app.version`, `app.name`, entrypoints, `min_platform_version`, etc.
- Capability declaration (must align with Permission System):
  - required vs optional capabilities
  - user-facing reasons (length limits already exist in Shell spec)
- Integrity metadata:
  - per-file hashes and sizes (strongly recommended)
  - total uncompressed size
  - build metadata (optional) but must be non-authoritative for identity/trust
- Update metadata hooks:
  - update channel / feed URL / repository ID (if marketplace-driven)
  - whether local updates are allowed

### Gaps in current specs
- **No canonical manifest schema exists** (08 references `manifest.json` but doesn’t define it).
- **No integrity model exists** (hash list, sizes, canonicalization rules).
- `.postapp` layout in 08 is plausible, but **not fully reconciled** with:
  - the `postapp://` protocol handler path expectations in 02 (`apps_dir/{app_id}/ui/{path}`)
  - Shell “manifest rendering policy” constraints (01) which are UI rules, not install-time enforcement.
- `main.wasm` is treated as required in 08 but **no execution/trust semantics are defined** anywhere.

---

## 2) Developer/publisher identity and signing

### What needs to be specified
**Identity model**
- What is a “publisher/developer identity”?
  - Today you have fields like `author_iid` (01, 08) but no cryptographic definition.
- How identity binds to keys:
  - key IDs (`kid`) format
  - key ownership proof (marketplace verification, Web-of-Trust, self-asserted, etc.)

**Signing**
- What exactly is signed:
  - whole ZIP bytes (fragile across repackaging) vs
  - a canonical “package descriptor” (manifest + file hash table) (preferred)
- Signature algorithm and encoding (Ed25519 recommended in 00; also used for extensions in 05).
- Multi-signature support (publisher + marketplace endorsement).
- Key rotation mechanism:
  - allowed key transition rules (old key signs new key? marketplace signs key binding? both?)

**Trust store**
- Storage format for trusted publisher keys / marketplace roots:
  - file format + integrity protection + update mechanism
- Where it lives and who can modify it (Rust-only invariant)

**Revocation**
- Revocation list vs transparency log vs TUF-style roles (root/targets/timestamp/snapshot).
- How revocations are distributed and cached offline.

### Gaps in current specs
- `author_iid` exists but **has no cryptographic meaning** yet.
- 08 introduces `MarketplaceSignature` but **doesn’t define**:
  - what it signs (package? metadata?)
  - how it chains to trust roots
- **No trust store format** exists.
- **No key rotation** rules exist (only HMAC rotation exists for bridge tokens in 04; unrelated).
- **No revocation mechanism** exists.

---

## 3) Trust levels and permission escalation on update

### What needs to be specified
**Trust levels**
- At minimum: marketplace-installed, locally installed, developer mode.
- Potentially: verified publisher vs unverified; “allow unsigned local” policy, etc.
- What each trust level enables/blocks:
  - installation allowed?
  - auto-update allowed?
  - permission defaults tightened/loosened? (careful—shouldn’t silently grant new power)

**Permission escalation policy on update**
- Determine what happens when an update introduces new requested capabilities:
  - must trigger update-time prompt (08 mentions this)
  - must preserve prior grants/denials (06 has escalation logic sketch)
- Define what happens when:
  - publisher identity changes
  - signing key rotates
  - package becomes revoked after install

### Gaps / conflicts
- **Permission tier semantics are inconsistent across specs**:
  - 06 says `GrantOnce` = “prompt on first use, remember decision”
  - 06 also has an **install-time prompt flow** for “GrantOnce capabilities”
  - 08 assumes escalation prompts at update time
  → Domain 6.5 depends on resolving when prompts happen (install vs first use vs update) for each tier.
- No policy exists for **publisher key change** during update (is that an escalation event?).

---

## 4) Update mechanisms (marketplace vs local)

### What needs to be specified
**Marketplace update system**
- How update metadata is discovered:
  - signed index feed, repository snapshot, transparency log, etc.
- Required protections:
  - anti-downgrade and anti-freeze (rollback/freeze attacks are common)
  - pinning / channel semantics (stable/beta/dev)
- Offline behavior:
  - caching metadata and packages
  - safe failure modes

**Local update**
- Allowed at all? Under what user confirmations?
- Must still enforce downgrade prevention unless explicitly overridden in dev mode.

### Gaps
- `AppSource::Marketplace { repository_url, app_id, version, signature }` exists (08) but **no protocol** for:
  - version discovery
  - metadata signing
  - transport security assumptions
- No definition of **auto-update vs manual** behavior or UX.
- No anti-freeze / repository compromise story.

---

## 5) Signature verification pipeline

### What needs to be specified
A **Rust-authoritative**, fail-closed pipeline with explicit ordering:
1. Acquire bytes (download / file read)
2. Pre-parse safety checks (size limits, ZIP structure sanity)
3. Parse manifest *without trusting it*
4. Verify:
   - signature(s)
   - trust chain to trust store
   - revocation status
   - integrity hash table vs archive contents
   - app_id format matches (and matches host used in `postapp://`)
5. Only then extract to staging (with path traversal defenses)
6. Atomic commit
7. Persist “measured install” record (hash, signer key id, timestamp)
8. Emit audit events + shell state changes

Also needs to define how verification differs by source:
- Marketplace: require full chain + metadata signatures
- Local file: require signature OR strong warning + explicit user confirmation (policy decision)
- Developer mode: allow unsigned, but must clearly mark as untrusted

### Gaps
- 08 has `verify_package(&parsed, &source)` but **doesn’t define**:
  - what is verified
  - what “signature_verified” truly means
- 02’s runtime protocol handler prevents path traversal at request time, but **install-time extraction hardening is not specified**.
- No link to revocation/trust store because those don’t exist yet.

---

## 6) Rollback and integrity checking

### What needs to be specified
**Rollback policy**
- When rollback is allowed:
  - install/upgrade failure
  - post-upgrade health check failure
- How rollback interacts with downgrade prevention:
  - rollback to *previous installed version* might be a “downgrade” but necessary for reliability
  - must specify constraints: rollback only to last-known-good that is still trusted + not revoked

**Integrity checking**
- What is re-checked and when:
  - at install time (mandatory)
  - at launch time (recommended: verify manifest + critical assets hash)
  - periodic/background integrity scan (optional)
- Corruption handling:
  - mark app `Corrupted` (08 has this state) and block launch until repaired/reinstalled

### Gaps
- 08 sketches rollback triggers but **no cryptographic integrity model** exists to support it.
- No policy for “installed app later becomes revoked”:
  - do we block launch immediately?
  - do we allow user override offline?
- No “measured install” record schema (hashes, signer identity, provenance) is defined.

---

## 7) Security boundaries for untrusted packages

### What needs to be specified
**Hard boundary rule:** *A package is untrusted until verification completes.*
- Shell must not render any package-provided active content pre-verification.
- Even “preview” metadata must be sanitized and treated as attacker-controlled.

**Untrusted input handling requirements**
- ZIP parsing must be resilient:
  - resource limits, timeouts, max entries, max path length
  - reject symlinks and weird file modes
- UI assets constraints enforced at install time (not just in React rendering):
  - SVG banned (01), icon size caps, etc.
- Ensure update metadata cannot cause arbitrary file writes.
- Ensure staging/backup directories are permissioned correctly.

### Gaps
- Shell spec (01) has strong UI sanitization rules, but **those are not enforced as install-time validation rules** yet.
- No explicit “do not load app UI for preview before verification” invariant exists.
- No ZIP bomb / pathological archive limits are specified.

---

# Cross-spec dependencies that need resolution (highest impact)

1) **Manifest schema must be defined once and referenced everywhere**
- Needed by: 08 (install/update), 06 (capabilities + reasons), 01 (rendering policy), 02 (`postapp://` serving), 07 (SDK bootstrap app_id/version), 06.5 (signing target).

2) **Signature policy inconsistency**
- 00/06.5 says “Signature verification before installation”
- 08 says LocalFile signature is “Optional”
→ Decide: are unsigned local installs allowed outside dev mode? If yes, what warnings and restrictions apply?

3) **Permission prompt timing semantics conflict**
- 06’s `GrantOnce` meaning conflicts with install-time prompting flows.
→ Decide:  
- install-time prompt for some tiers?  
- first-use prompt only?  
- update-time escalation prompt?  
…and ensure 06 + 08 + 06.5 agree.

4) **Publisher identity (`author_iid`) is referenced but undefined**
- Must define: cryptographic binding of `author_iid` to signing keys, or remove/replace it.
- Also impacts UI trust display (“who published this?”).

5) **Marketplace update metadata and anti-rollback/freeze protections**
- No spec exists. Domain 6.5 must either:
  - adopt a known model (TUF-like), or
  - define a simpler but explicitly-scoped model (and accept risks in Risk Register).

6) **Untrusted archive extraction hardening**
- Runtime path traversal defenses exist (02) but install-time extraction defenses do not.
- Must align with Resource Constraints limits (03) for package size and decompressed size.

7) **Unify signing across “apps” and “extensions”**
- 05 already defines `.postmod` signatures (Ed25519).
- Decide whether `.postapp` and `.postmod` share:
  - trust roots
  - key IDs
  - revocation distribution
  - signature file format

---

## What’s “missing” in the current architecture view (summary)
- A normative **manifest.json schema**
- A normative **what-is-signed** definition + canonicalization rules
- A **trust store + key rotation + revocation** mechanism
- A **marketplace update metadata** design with downgrade/freeze resistance
- Install-time **archive safety limits** (zip bombs, symlinks, duplicates)
- A coherent, cross-spec policy for **permission escalation on update** and **prompt timing**
- A coherent policy for **rollback vs downgrade prevention** and **revoked-installed-app behavior**

If you want, I can turn this into an explicit **Domain 6.5 spec outline** (sections, required data models, sequence diagrams, Rust/TS interfaces, acceptance tests) that matches the “Definition of Done (Per Domain)” checklist in `00-PLANNING_DOMAINS.md`.

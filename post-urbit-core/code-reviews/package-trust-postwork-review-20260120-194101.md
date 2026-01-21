## Score: **6 / 10**

Strong breadth (ZIP hardening, per-file hashes, multi-signature, rotation-aware verification, anti-downgrade/freeze intent). The main deficits are **(a)** cryptographic canonicalization/signing rigor, **(b)** trust-chain authority clarity (who signs what, and why that is sufficient), and **(c)** update-metadata security (freshness/rollback of metadata) in a way that resists **replay/freeze** attacks without hand-wavy assumptions.

Below are specific issues by the requested review categories.

---

## 1) Cryptographic protocol gaps

### 1.1 Canonicalization is underspecified / nonstandard (high)
You define custom JSON canonicalization rules:

> “Unicode escaped (\uXXXX)”, “Numbers represented as-is”, “keys sorted”, etc.

This is very likely to diverge across implementations (Rust/TS/Go) and can cause **signature verification failures** or (worse) inconsistent hashing if any component “normalizes” numbers/strings differently.

**Fix**: Adopt an existing canonicalization standard:
- **RFC 8785 JCS (JSON Canonicalization Scheme)** for `manifest.json` + `FILES`, or
- Sign a **canonical CBOR** encoding (preferred long-term, since you already have CBOR discipline elsewhere).

Also explicitly require JSON parsing to **reject duplicate keys** (a common signature-bypass vector in lax parsers).

---

### 1.2 Signing payload construction is fragile and not formally specified (medium/high)
You propose:

`"{domain}:{manifest_hash}:{files_hash}:{timestamp}"`

But `manifest_hash` / `files_hash` are `"sha256:{hex}"` which themselves contain `:`. If an implementation ever tries to parse fields back out, ambiguity appears. Even if you never parse, this is an attractive footgun.

**Fix**: Define the signature input as a structured encoding:
- CBOR map like `{domain, manifest_hash, files_hash, package_id, version, ts, sig_version}` encoded canonically; or
- JCS JSON of a `SignedPayload` struct with RFC8785 canonicalization.

Also include `package_id`, `package_type`, `version`, `signature_version` explicitly in the signed payload even if they’re “implicitly” bound by the manifest hash—this reduces implementation mistakes.

---

### 1.3 Timestamp semantics are unsafe for key compromise/revocation (high)
You require:

> “key MUST NOT be revoked at the signature timestamp”

This allows **backdating** signatures to a time before revocation. If revocation is for **key compromise**, you often want one of:
- “revoke all signatures ever made by that key” (hard revoke), or
- “revoke from a compromise_time that is not controlled by the attacker”.

Right now, an attacker with a stolen key can set `timestamp` arbitrarily in the past and pass your check unless you have an external trusted timestamping/freshness mechanism.

**Fix**:
- Define revocation semantics per `RevocationReason`:
  - `KeyCompromise` should typically invalidate **all** signatures by that key (or at least from the compromise time, signed by a stronger authority than the compromised key).
- Add **freshness** requirements for signature timestamp (e.g., must be within ±X days of marketplace release time for marketplace installs).
- Consider TUF-style **timestamp/snapshot metadata** (see §5).

---

### 1.4 Base64 / encoding requirements not fully normative (medium)
You say “base64” in multiple places; you don’t define whether it’s padded/unpadded, standard vs URL-safe, etc.

**Fix**: Normatively specify encoding:
- public keys: base64 **standard** with padding (or no padding), but be explicit;
- signatures: base64 standard; length checks (`Ed25519 sig = 64 bytes`).

---

## 2) Trust chain weaknesses

### 2.1 Authority model for identity docs + DHT is implied, not proven (high)
You rely on `fetch_identity()` and “IID derived from genesis signing key”, but don’t specify the **self-authentication** property and verification procedure end-to-end:
- What exactly is an IID? Hash of what (raw pubkey bytes? multicodec?)?
- How is the identity document signed and chained across rotations?
- What prevents a malicious DHT response from swapping in attacker keys?

**Fix**: Add a normative “Identity Document Verification” section:
- `IID = HASH(genesis_pubkey_bytes)` with exact hash and encoding
- identity doc includes `current_key`, `previous_keys`, rotations signed by prior key (or by genesis)
- DHT responses are accepted only if the chain verifies to the IID

Without this, the marketplace/publisher signature checks are conceptually incomplete.

---

### 2.2 Marketplace certificate / attestation format is undefined (high)
`MarketplaceCertificate.platform_attestation: String` exists, but:
- what is signed?
- by which platform root?
- canonicalization?
- expiry handling?
- key rotation?

**Fix**: Define an explicit certificate payload and signature scheme (again: JCS/CBOR) and require verification against embedded platform roots.

---

### 2.3 Publisher identity continuity across updates is not mandated (high)
For an installed `package_id`, an attacker could attempt to deliver an update signed by a different IID if the marketplace is malicious/compromised and the UX is weak.

**Fix**:
- Enforce: `installed.publisher_iid` must match update’s `publisher.iid` **unless** a defined “transfer of ownership” protocol occurs (requires strong co-signing + explicit user confirmation).

---

### 2.4 “Trust store integrity_hash” is not anchored (medium)
A hash stored alongside data is not integrity protection unless it is anchored (OS keystore / signed / compared to embedded constants).

**Fix**:
- Platform roots: validate DB contents against **compiled-in** roots at startup.
- For mutable trust objects: either store an authenticated log (append-only + signature) or anchor a MAC/signature in OS keystore.

---

## 3) Archive attack coverage (zip bombs, symlinks, traversal)

You’re in good shape conceptually, but there are important edge cases.

### 3.1 Path normalization edge cases (high)
ZIP entry names can bypass naive checks via:
- backslashes (`..\..\windows\system32`) on Windows
- drive letters (`C:\...`)
- UNC paths (`\\server\share`)
- mixed separators, repeated slashes
- UTF-8 normalization tricks
- NUL bytes in filenames (some libs truncate)
- case-insensitive collisions on Windows/macOS default FS

**Fix**:
- Normalize entry paths using a strict algorithm:
  - reject any `\` separators (or convert then re-check),
  - reject `:` and drive prefixes,
  - reject NUL,
  - enforce NFC normalization (or reject non-UTF8),
  - detect collisions under **case-insensitive** comparison for target platforms.
- Enforce extraction using “openat”-style safe joins where possible (Rust: validate components, then create dirs/files without following symlinks).

---

### 3.2 Symlink/hardlink detection depends on ZIP metadata correctness (medium)
Attackers can craft archives with weird extra fields; `unix_mode()` and external attributes aren’t always sufficient across all ZIP writers.

**Fix**:
- During extraction, open files with “do not follow symlinks” semantics where available.
- After extraction, re-walk the staging directory and verify there are **no symlinks** (best-effort on Windows) and that all files live under the intended root.

---

### 3.3 Nested archive detection by extension is bypassable (medium)
You reject `.zip/.postapp/.postmod` “inside”, but an attacker can embed a ZIP with a different extension. That’s not always a security issue, but it can be used for **decompression bombs inside later parsing** if any later component treats it as an archive.

**Fix**:
- If you truly want “no nested archives”, detect ZIP magic bytes too.
- Otherwise, define the threat boundary: “we only parse the outer archive; inner blobs are treated as inert data”.

---

## 4) Revocation distribution completeness

### 4.1 RevocationSource is referenced but not defined (high)
`revocation_sources: Vec<RevocationSource>` is in the trust store but no schema/verification rules are provided.

**Fix**: Add:
- source types (DHT, HTTPS endpoint, transparency log),
- signature requirements (which root/key signs revocations),
- update cadence and caching rules,
- replay protection (monotonic version / signed timestamp).

---

### 4.2 Revocation signing authority is ambiguous (high)
You mix:
- platform trust root purpose `RevocationSigning`,
- `fetch_revocations(&dht, iid)` (implies publisher-supplied revocations),
- marketplace certificate revocation (platform/marketplace-supplied)

If revocations are signed by the compromised publisher key, they are not reliable in key compromise scenarios.

**Fix**:
- For `KeyCompromise` and `Malware`: require revocation to be signed by **platform revocation root** and/or **trusted marketplace** (or a transparency log operator).
- For “publisher-requested”: allow publisher-signed revocation, but only if chained to an uncompromised key (rotation doc signed by previous key, etc.).

---

### 4.3 Installed-but-revoked handling needs tighter coupling to update + rollback (medium/high)
You define severity-based actions, but you don’t state:
- whether rollback is allowed to a revoked version (it should not),
- whether “freeze” is overridden by revocation (it should be),
- what happens offline if the app is already known revoked but cannot phone home.

**Fix**: Add explicit precedence rules:
`Revocation > Forced Security Update > User Freeze > Normal Auto-update`

---

## 5) Update protocol security (rollback, freeze attacks)

### 5.1 UpdateManifest freshness / replay protection is insufficient (high)
A classic freeze attack is: attacker replays an **old but validly signed** update manifest forever. Your `max_staleness_days` helps only if:
- the client has a trusted clock, and
- it checks signed timestamps and refuses old metadata.

But you do not specify:
- storing “last seen update manifest timestamp/version” and rejecting older,
- a TUF-like snapshot/timestamp chain to prevent indefinite replay.

**Fix (recommended)**:
Adopt **TUF concepts** (even a simplified variant):
- `timestamp.json` signed frequently (short expiry)
- `snapshot.json` binds versions of targets metadata
- `targets.json` lists available versions + hashes
This gives you strong replay/freeze resistance and clear root/role separation.

At minimum: persist `last_update_manifest_timestamp` and reject any update manifest with `timestamp <= stored_timestamp` (with careful clock-skew handling).

---

### 5.2 Rollback conflicts with anti-downgrade (high)
You store `highest_installed_version` and block installs below it—good for downgrade defense, but **rollback is a controlled downgrade**.

Right now the spec doesn’t define how rollback works without punching a downgrade-sized hole.

**Fix**:
- Allow rollback **only** to locally created backups produced by the platform (not arbitrary downloads).
- Keep `highest_installed_version` unchanged after rollback.
- Disallow rollback to:
  - revoked versions,
  - versions listed as vulnerable in a currently trusted advisory set,
  - versions below marketplace minimum_version (unless offline recovery mode with explicit user override).

---

### 5.3 “Equal version” handling is inconsistent (medium)
Your test cases expect “Install equal version → Blocked”, but `can_install_version()` currently allows equality.

Security-wise, “same semver, different content” is a common supply-chain attack.

**Fix**:
- Allow reinstall of the same version only if `manifest_hash` and `files_hash` exactly match the installed record (repair scenario).
- Otherwise require explicit “developer mode” or block.

---

### 5.4 Key-change UX policy needs cryptographic gates (medium/high)
You have `require_rotation_proof` which is good, but you need to define what “rotation proof” is:
- which document,
- signed by which key,
- and how it’s fetched and authenticated.

Also: if key change is “unproven”, allowing user override is risky for marketplace installs.

**Fix**:
- Marketplace source: block on unproven key change unless marketplace provides an attestation + platform policy allows override.
- Local file source: allow override with a strong warning and require user to trust the publisher key fingerprint.

---

## 6) Cross-spec integration (permission, lifecycle, bridge)

### 6.1 Package format is not coherent with Domain 6 (high)
`08-APP_LIFECYCLE_MANAGEMENT.md` defines `.postapp` with `manifest.json` + `SIGNATURE` but **does not include `FILES`** or your expanded SIGNATURE JSON structure. This is a hard coherence issue: implementation will drift or be ambiguous.

**Fix**:
- Update Domain 6 package format section to reference **this** canonical format:
  - presence and meaning of `FILES`,
  - JSON schema for `SIGNATURE`,
  - install verification order (“verify before extract/commit”, etc).

---

### 6.2 Permission escalation flow must use the Permission System API contract (medium)
Your update prompt flow shows direct calls like:
`Shell -> PermissionStore: grant_capabilities(new_caps)`

But `06-PERMISSION_SYSTEM.md` specifies shell-only bridge methods (`permission.grant`, prompt IDs, audit log semantics, scopes). Update-time prompts should follow the same pipeline so auditing, cooldowns, and UX are consistent.

**Fix**:
- Specify update-time escalation uses the same `shell://permissions/prompt` + `permission.grant(...)` flow (or an equivalent shell-only command) and produces audit events.

---

### 6.3 Bridge/Protocol Registry extension signing is parallel but not unified (medium)
`05-PROTOCOL_REGISTRY.md` describes extension packages and “Ed25519 signature” but does not align with:
- `FILES` manifest,
- multi-signature/cosign,
- publisher IID binding and key rotation.

**Fix**:
- Make extension verification reuse the same `PackageSignatureDocument` + `FileIntegrityManifest` machinery (or clearly define a smaller but compatible subset).

---

### 6.4 Network/secrets fields contradict the sandbox CSP (medium)
`PackageManifest` includes `secrets` and `network` configuration, but the sandbox spec enforces `connect-src 'none'`. This is either:
- future-facing (fine), or
- an inconsistency that will confuse implementers and reviewers.

**Fix**:
- Mark `secrets`/`network` as **reserved / not active in v1**, or define the permission + CSP relaxation model needed to make it real.

---

## Summary of the most important fixes (priority order)

1. **Standardize canonicalization & signing payload** (RFC8785 JCS or canonical CBOR; reject JSON dup keys).
2. **Define identity/DHT trust chain verification** (IID derivation + rotation doc validation).
3. **Harden update metadata against replay/freeze** (TUF-like timestamp/snapshot or persistent monotonic metadata timestamp with rejection rules).
4. **Resolve rollback vs anti-downgrade** with strict “rollback only to local backups + not revoked/vulnerable”.
5. **Unify package format across Domain 6 and 6.5** (FILES + SIGNATURE JSON everywhere).
6. **Clarify revocation authority and distribution** (RevocationSource schema + who signs what, especially for key compromise).

If you want, I can draft an “Addendum: Minimal TUF-ish Update Metadata” section that fits your current structures (marketplace-signed manifest + publisher-signed packages) without requiring a full TUF implementation on day one.

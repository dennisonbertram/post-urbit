# PRD: Network Access Capability for Post-Urbit Apps

**Status:** Implemented
**Author:** Post-Urbit Team
**Date:** 2026-01-20
**Target Version:** 0.2.0
**Implementation Date:** 2026-01-20

---

## 1. Overview

### 1.1 Problem Statement

Post-Urbit apps currently run in a fully sandboxed WASM environment with no network access. While this maximizes security, it severely limits app functionality. Apps cannot:

- Query external APIs (LLMs, weather, maps, search)
- Fetch web content (RSS feeds, news, data)
- Integrate with external services (calendars, social networks)
- Sync with cloud services

This makes Post-Urbit apps feel like isolated toys rather than capable agents that can act on behalf of users.

### 1.2 Solution

Add a **capability-controlled network access system** that allows apps to make HTTP/HTTPS requests to explicitly declared domains, with user approval at install time and host-level enforcement at runtime.

### 1.3 Goals

1. Enable apps to make outbound HTTP/HTTPS requests
2. Maintain strong security through domain allowlists
3. Keep API keys/secrets secure (never exposed to app code)
4. Provide user transparency and control
5. Support common patterns: REST APIs, webhooks, RSS feeds

### 1.4 Non-Goals

1. WebSocket/long-lived connections (future work)
2. Raw TCP/UDP socket access
3. Localhost/private network access
4. Acting as an HTTP server (apps don't listen for inbound)
5. mTLS client certificates (future work)

---

## 2. User Experience

### 2.1 App Installation Flow

When a user installs an app that requires network access:

```
┌─────────────────────────────────────────────────────────┐
│  Install "WeatherAgent"?                                │
│                                                         │
│  This app requests the following permissions:           │
│                                                         │
│  ◉ Network Access                                       │
│    • api.anthropic.com - AI model queries               │
│    • api.weather.gov - Weather data                     │
│    • *.openweathermap.org - Weather data                │
│                                                         │
│  ◉ Storage (app-scoped)                                 │
│  ◉ Notifications                                        │
│                                                         │
│  ⚠ This app will be able to send data to these domains  │
│                                                         │
│  [Cancel]                      [Review Domains] [Allow] │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Secret Configuration

For APIs requiring authentication, users configure secrets at the node level:

```
┌─────────────────────────────────────────────────────────┐
│  Configure Secrets for "WeatherAgent"                   │
│                                                         │
│  This app needs the following API keys:                 │
│                                                         │
│  Anthropic API Key                                      │
│  "Used for AI model queries"                            │
│  [•••••••••••••••••••••••••]  [Paste] [Show]           │
│  ↳ Will only be sent to: api.anthropic.com              │
│                                                         │
│  OpenWeatherMap Key (optional)                          │
│  "Used for weather forecasts"                           │
│  [                          ]  [Paste]                  │
│  ↳ Will only be sent to: *.openweathermap.org           │
│                                                         │
│  [Cancel]                                       [Save]  │
└─────────────────────────────────────────────────────────┘
```

### 2.3 Runtime Audit Log

Users can view all network requests made by apps:

```
Network Activity - WeatherAgent
─────────────────────────────────────────────────────────
2026-01-20 14:32:15  POST api.anthropic.com/v1/messages     200  1.2s
2026-01-20 14:32:14  GET  api.weather.gov/points/39,-77     200  0.3s
2026-01-20 14:30:01  GET  api.weather.gov/alerts/active     200  0.2s
─────────────────────────────────────────────────────────
Total requests today: 47 | Data sent: 12 KB | Data received: 156 KB
```

---

## 3. Technical Design

### 3.1 Manifest Schema

Apps declare network requirements in `manifest.json`:

```json
{
  "id": "weather-agent",
  "version": "1.0.0",
  "name": "Weather Agent",

  "capabilities": {
    "required": [
      "storage:app",
      "network:https:api.weather.gov",
      "network:https:api.anthropic.com"
    ],
    "optional": [
      "network:https:*.openweathermap.org",
      "notifications:show"
    ]
  },

  "secrets": {
    "anthropic_api_key": {
      "description": "Anthropic API key for Claude model access",
      "required": true,
      "inject": {
        "domains": ["api.anthropic.com"],
        "header": "x-api-key"
      }
    },
    "openweathermap_key": {
      "description": "OpenWeatherMap API key",
      "required": false,
      "inject": {
        "domains": ["*.openweathermap.org"],
        "query_param": "appid"
      }
    }
  },

  "network": {
    "rate_limits": {
      "api.anthropic.com": {
        "requests_per_minute": 60,
        "requests_per_day": 1000
      }
    }
  }
}
```

### 3.2 Capability String Format

```
network:<protocol>:<domain_pattern>

Examples:
  network:https:api.anthropic.com       # Exact domain, HTTPS only
  network:https:*.example.com           # Wildcard subdomain
  network:http+https:legacy.api.com     # Both HTTP and HTTPS (rare)
```

**Rules:**
- Protocol is required (`https`, `http`, or `http+https`)
- HTTPS should be strongly preferred; HTTP only for legacy APIs
- Wildcards only at subdomain level (`*.example.com`), not in TLD
- No IP addresses allowed (must use domain names)
- No localhost, 127.0.0.1, or private IP ranges (10.x, 192.168.x, 172.16-31.x)

### 3.3 Host API

#### 3.3.1 `network.fetch`

Primary method for making HTTP requests.

**Request:**
```rust
network.fetch({
    // Required
    url: String,              // Full URL including path and query

    // Optional
    method: String,           // GET, POST, PUT, DELETE, PATCH, HEAD (default: GET)
    headers: Map<String, String>,  // Request headers
    body: Bytes,              // Request body (for POST/PUT/PATCH)

    // Timeouts
    timeout_ms: u32,          // Request timeout (default: 30000, max: 300000)

    // Response handling
    max_response_bytes: u32,  // Max response size (default: 10MB, max: 50MB)
})
```

**Response:**
```rust
{
    ok: bool,
    value: {
        status: u16,                    // HTTP status code
        status_text: String,            // HTTP status text
        headers: Map<String, String>,   // Response headers
        body: Bytes,                    // Response body
        url: String,                    // Final URL (after redirects)
        redirected: bool,               // Whether request was redirected
    }
}
```

**Errors:**
```rust
{
    ok: false,
    error: {
        code: String,  // Error code (see below)
        message: String,
    }
}
```

| Error Code | Description |
|------------|-------------|
| `PERMISSION_DENIED` | Domain not in app's allowlist |
| `INVALID_URL` | Malformed URL |
| `BLOCKED_DOMAIN` | Attempted localhost/private IP |
| `TIMEOUT` | Request exceeded timeout |
| `RESPONSE_TOO_LARGE` | Response exceeded max_response_bytes |
| `RATE_LIMITED` | App exceeded rate limit for domain |
| `SECRET_NOT_CONFIGURED` | Required secret not set by user |
| `NETWORK_ERROR` | Connection failed, DNS error, etc. |
| `TLS_ERROR` | Certificate validation failed |

#### 3.3.2 `network.fetch_json` (Convenience Method)

Wrapper that parses JSON response.

**Request:**
```rust
network.fetch_json({
    url: String,
    method: String,
    headers: Map<String, String>,
    body: Value,  // Will be JSON-serialized
    timeout_ms: u32,
})
```

**Response:**
```rust
{
    ok: bool,
    value: {
        status: u16,
        headers: Map<String, String>,
        body: Value,  // Parsed JSON
    }
}
```

Additional error: `JSON_PARSE_ERROR` if response isn't valid JSON.

### 3.4 Secret Injection

Secrets are **never exposed to app code**. The host injects them at request time.

**Flow:**
1. App calls `network.fetch({url: "https://api.anthropic.com/v1/messages", ...})`
2. Host checks: Does app have `anthropic_api_key` secret configured?
3. Host checks: Is `api.anthropic.com` in the secret's allowed domains?
4. Host injects: Adds `x-api-key: <actual_key>` header to request
5. Request is made with secret attached
6. App receives response (never sees the key)

**Injection Methods:**

```json
{
  "inject": {
    "domains": ["api.example.com"],
    "header": "Authorization",           // Inject as header
    "header_prefix": "Bearer "           // Optional prefix
  }
}

{
  "inject": {
    "domains": ["api.example.com"],
    "query_param": "api_key"             // Inject as URL query param
  }
}

{
  "inject": {
    "domains": ["api.example.com"],
    "basic_auth": true                   // Inject as Basic Auth (secret is password)
  }
}
```

### 3.5 Rate Limiting

**Default Limits (per app, per domain):**
- 100 requests per minute
- 10,000 requests per day
- 100 MB data transfer per day

**App-Declared Limits:**
Apps can declare lower limits in manifest (not higher):
```json
{
  "network": {
    "rate_limits": {
      "api.anthropic.com": {
        "requests_per_minute": 10
      }
    }
  }
}
```

**User Override:**
Users can further restrict (but not expand beyond defaults):
```
Settings > Apps > WeatherAgent > Network Limits
  api.anthropic.com: [10] req/min  [100] req/day
```

### 3.6 Request/Response Limits

| Limit | Default | Maximum | Configurable |
|-------|---------|---------|--------------|
| Request body size | 10 MB | 50 MB | Per-request |
| Response body size | 10 MB | 50 MB | Per-request |
| Request timeout | 30 sec | 5 min | Per-request |
| Max redirects | 5 | 10 | No |
| Header size | 64 KB | 64 KB | No |
| URL length | 8 KB | 8 KB | No |

### 3.7 Redirect Handling

- Redirects are followed automatically (up to limit)
- **Cross-domain redirects**: Only followed if target domain is also in allowlist
- Response includes `redirected: true` and final `url` if redirected
- 3xx responses are never returned directly (either followed or error)

---

## 4. Security Considerations

### 4.1 Threat Model

| Threat | Mitigation |
|--------|------------|
| Data exfiltration to arbitrary servers | Domain allowlist enforced by host |
| API key theft | Secrets never exposed to app, injected by host |
| SSRF (Server-Side Request Forgery) | Block localhost, private IPs, link-local |
| DNS rebinding | Validate IP at connection time, not just DNS lookup |
| Request smuggling | Use well-tested HTTP client (reqwest), validate headers |
| DoS via excessive requests | Rate limiting per app per domain |
| Man-in-the-middle | TLS required (HTTPS), certificate validation |
| Tracking/fingerprinting | User can audit all requests, revoke access |

### 4.2 Blocked Destinations

The following are **always blocked**, regardless of allowlist:

```
# Localhost
localhost, 127.0.0.0/8, ::1

# Private networks
10.0.0.0/8
172.16.0.0/12
192.168.0.0/16

# Link-local
169.254.0.0/16
fe80::/10

# AWS metadata
169.254.169.254

# Cloud metadata endpoints
metadata.google.internal
metadata.azure.com

# Unix sockets
file://, unix://
```

### 4.3 DNS Rebinding Protection

1. Resolve DNS before making request
2. Check resolved IP against blocklist
3. Pin resolved IP for duration of connection
4. Re-validate on redirect

### 4.4 Certificate Validation

- Full certificate chain validation required
- No option to skip certificate checks
- System CA store used
- HSTS respected for known domains

### 4.5 Audit Logging

All network requests are logged locally:

```rust
struct NetworkAuditEntry {
    timestamp: DateTime<Utc>,
    app_id: String,

    // Request
    method: String,
    url: String,  // Secrets redacted
    request_size: u64,

    // Response
    status: Option<u16>,
    response_size: Option<u64>,
    duration_ms: u64,

    // Outcome
    outcome: NetworkOutcome,  // Success, Error, Blocked, RateLimited
    error_code: Option<String>,
}
```

Logs retained for 30 days, accessible via admin API.

---

## 5. Implementation Plan

### 5.1 Phase 1: Core Infrastructure

**Files to modify:**
- `src/runtime_wasm.rs` - Add network capability registration and host calls
- `src/runtime.rs` - Add network types and validation

**New files:**
- `src/network.rs` - HTTP client wrapper, security validation, rate limiting
- `src/secrets.rs` - Secret storage and injection

**Tasks:**
1. Define `NetworkCapability` struct and parsing
2. Implement domain allowlist validation
3. Implement IP blocklist checking
4. Create `HttpClient` wrapper around `reqwest` with security checks
5. Implement rate limiter (token bucket per app per domain)
6. Add `network.fetch` host call handler
7. Unit tests for all security validations

### 5.2 Phase 2: Secret Management

**Tasks:**
1. Define secret schema in manifest
2. Implement secret storage (encrypted at rest)
3. Implement secret injection logic
4. Add secret configuration API (for UI)
5. Never log or expose secrets in any output

### 5.3 Phase 3: Manifest & Install Flow

**Files to modify:**
- `src/app_store.rs` - Parse network capabilities and secrets from manifest

**Tasks:**
1. Extend manifest schema validation
2. Parse and validate network capabilities
3. Parse and validate secret declarations
4. Update app installation to prompt for network permissions
5. Update app installation to prompt for secret configuration

### 5.4 Phase 4: Audit & Observability

**New files:**
- `src/network_audit.rs` - Audit logging

**Tasks:**
1. Implement audit log storage (SQLite or append-only file)
2. Log all network requests with outcomes
3. Add admin API endpoints for querying logs
4. Add per-app network statistics

### 5.5 Phase 5: Convenience Methods & Polish

**Tasks:**
1. Add `network.fetch_json` convenience method
2. Add configurable timeout handling
3. Add user-configurable rate limit overrides
4. Documentation and examples

---

## 6. API Reference Summary

### 6.1 New Capabilities

| Capability | Description |
|------------|-------------|
| `network:https:<domain>` | HTTPS access to specific domain |
| `network:http:<domain>` | HTTP access (discouraged) |
| `network:http+https:<domain>` | Both protocols |

### 6.2 New Host Calls

| Method | Capability Required | Description |
|--------|---------------------|-------------|
| `network.fetch` | `network:*:<domain>` | Make HTTP request |
| `network.fetch_json` | `network:*:<domain>` | Make HTTP request, parse JSON |

### 6.3 New Manifest Fields

```json
{
  "secrets": {
    "<secret_name>": {
      "description": "string",
      "required": "boolean",
      "inject": {
        "domains": ["string"],
        "header": "string",
        "header_prefix": "string",
        "query_param": "string",
        "basic_auth": "boolean"
      }
    }
  },
  "network": {
    "rate_limits": {
      "<domain>": {
        "requests_per_minute": "number",
        "requests_per_day": "number"
      }
    }
  }
}
```

---

## 7. Testing Requirements

### 7.1 Unit Tests

- [ ] Domain pattern parsing and matching
- [ ] Wildcard domain matching (`*.example.com`)
- [ ] IP blocklist validation (all private ranges)
- [ ] DNS rebinding protection
- [ ] Rate limiter (token bucket behavior)
- [ ] Secret injection (header, query param, basic auth)
- [ ] Request/response size limits
- [ ] Timeout handling
- [ ] Redirect following (same domain)
- [ ] Cross-domain redirect blocking

### 7.2 Integration Tests

- [ ] Full request/response cycle to mock server
- [ ] Secret injection end-to-end
- [ ] Rate limiting triggers correctly
- [ ] Audit log entries created
- [ ] App without capability gets PERMISSION_DENIED
- [ ] Cross-domain redirect to allowed domain succeeds
- [ ] Cross-domain redirect to disallowed domain fails

### 7.3 Security Tests

- [ ] Cannot reach localhost via any method
- [ ] Cannot reach private IPs via any method
- [ ] Cannot reach AWS metadata endpoint
- [ ] DNS rebinding attack fails
- [ ] Secrets never appear in logs
- [ ] Secrets never appear in error messages
- [ ] Secrets never returned to app
- [ ] Invalid TLS certificate rejected

### 7.4 Fuzzing

- [ ] URL parser fuzzing
- [ ] Domain pattern fuzzing
- [ ] Header injection fuzzing

---

## 8. Open Questions

1. **WebSocket support?** - Useful for real-time APIs. Could add `network:wss:<domain>` capability. Deferred to future version.

2. **Streaming responses?** - Large responses (LLM streaming) benefit from streaming. Current design buffers full response. Consider adding `network.fetch_stream` later.

3. **Retry policy?** - Should host auto-retry on 5xx/network errors? Leaning toward no - let app decide retry logic.

4. **Certificate pinning?** - Allow apps to pin certificates for extra security? Adds complexity, probably not needed for v1.

5. **Proxy support?** - Should node support HTTP proxy for all outbound requests? Useful for corporate environments. Could add to node config.

---

## 9. Success Criteria

1. Apps can make HTTP requests to declared domains
2. Secrets are never exposed to app code
3. All requests are auditable by user
4. No way to bypass domain allowlist
5. No way to reach localhost or private networks
6. Rate limiting prevents runaway apps
7. Performance: <10ms overhead per request (excluding network time)

---

## Appendix A: Example App Using Network Capability

```json
{
  "id": "llm-assistant",
  "version": "1.0.0",
  "name": "LLM Assistant",
  "description": "An AI assistant powered by Claude",
  "author": "did:post:abc123...",

  "capabilities": {
    "required": [
      "storage:app",
      "network:https:api.anthropic.com"
    ],
    "optional": [
      "notifications:show"
    ]
  },

  "secrets": {
    "anthropic_api_key": {
      "description": "Your Anthropic API key from console.anthropic.com",
      "required": true,
      "inject": {
        "domains": ["api.anthropic.com"],
        "header": "x-api-key"
      }
    }
  }
}
```

**App code (WASM/Rust):**
```rust
fn query_claude(prompt: &str) -> Result<String, Error> {
    let response = host_call("network.fetch_json", json!({
        "url": "https://api.anthropic.com/v1/messages",
        "method": "POST",
        "headers": {
            "content-type": "application/json",
            "anthropic-version": "2023-06-01"
        },
        "body": {
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": prompt}
            ]
        }
    }))?;

    // Note: x-api-key header was injected by host
    // App never sees the actual API key

    let content = response["body"]["content"][0]["text"]
        .as_str()
        .ok_or(Error::ParseError)?;

    Ok(content.to_string())
}
```

---

## Appendix B: Capability Registry Additions

```rust
// In runtime_wasm.rs, add to register_default_capabilities():

// Network capabilities - dynamically registered based on manifest
// Pattern: "network.fetch" -> "network:https:api.example.com"
// The actual domain check happens at runtime based on URL

registry.register("network.fetch", "network:http");  // Base capability
registry.register("network.fetch_json", "network:http");
```

```rust
// New validation in handle_network_fetch():

fn handle_network_fetch(
    app_id: &str,
    granted_capabilities: &[String],
    request: NetworkFetchRequest,
) -> Result<NetworkFetchResponse, HostError> {
    // 1. Parse URL
    let url = Url::parse(&request.url)?;

    // 2. Check scheme
    let scheme = url.scheme();
    if scheme != "https" && scheme != "http" {
        return Err(HostError::InvalidUrl("Only HTTP(S) supported"));
    }

    // 3. Extract domain
    let domain = url.host_str().ok_or(HostError::InvalidUrl("No host"))?;

    // 4. Check against blocklist (localhost, private IPs, etc.)
    validate_not_blocked(domain, &url)?;

    // 5. Check app has capability for this domain
    let required_cap = format!("network:{}:{}", scheme, domain);
    if !has_matching_capability(granted_capabilities, &required_cap) {
        return Err(HostError::PermissionDenied(format!(
            "App does not have network access to {}", domain
        )));
    }

    // 6. Check rate limit
    check_rate_limit(app_id, domain)?;

    // 7. Inject secrets
    let request = inject_secrets(app_id, domain, request)?;

    // 8. Make request
    let response = http_client.execute(request).await?;

    // 9. Log to audit
    audit_log.record(app_id, &request, &response);

    // 10. Return response
    Ok(response)
}
```

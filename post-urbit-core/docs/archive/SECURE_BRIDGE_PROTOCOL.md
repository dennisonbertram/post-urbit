# Post-Urbit Secure Bridge Protocol Specification

## Overview

This document specifies the secure communication protocol between sandboxed app iframes and the Post-Urbit Tauri/Rust backend. The protocol addresses critical security gaps identified in initial architecture review:

1. **Forgeable app identity** - Solved via unforgeable HMAC tokens minted by Rust
2. **Insecure postMessage** - Solved via MessageChannel/MessagePort isolation
3. **Navigation exfiltration** - Addressed via CSP and navigation blocking

## Security Properties

### Guarantees Provided

1. **App Identity Binding**: Each app session is bound to a cryptographically signed token that cannot be forged by other apps or the shell
2. **Replay Protection**: Request IDs prevent replay attacks with configurable window
3. **Session Isolation**: Apps cannot access other apps' sessions or data
4. **Capability Enforcement**: Backend validates permissions before executing any method
5. **Channel Isolation**: MessagePort ensures messages cannot be intercepted by other iframes

### Attack Vectors Mitigated

| Attack | Mitigation |
|--------|------------|
| App impersonation | HMAC-signed session tokens validated by backend |
| Message interception | MessageChannel instead of global postMessage |
| Replay attacks | Request ID tracking with time window |
| Session hijacking | Tokens bound to app_id, expire after timeout |
| Privilege escalation | Capability check on every request |
| Data exfiltration via navigation | CSP + webview navigation hooks |

### Threat Model

**Trusted:**
- Tauri/Rust backend
- Shell React application (but defense in depth applied)

**Untrusted:**
- All third-party apps running in iframes
- Network (all traffic assumed potentially malicious)

**Assumptions:**
- System webview is not compromised
- Rust backend secret key is not leaked
- User's device is not rooted/jailbroken

---

## 1. Handshake Flow

### Sequence Diagram

```
    Shell                         Rust Backend                    App (iframe)
      |                                |                              |
      |-- create_app_session(app_id) ->|                              |
      |                                |                              |
      |<-- {session_id, token, caps} --|                              |
      |                                |                              |
      |-- Create iframe, MessageChannel                               |
      |                                                               |
      |-- postMessage(handshake, [port2]) --------------------------->|
      |                                                               |
      |                                                          (App receives
      |                                                           port2 and
      |                                                           session info)
      |                                                               |
      |<=============== All future communication via port1/port2 ====>|
      |                                |                              |
      |-- bridge_request(cbor) ------->|                              |
      |<-- bridge_response(cbor) ------|                              |
```

### Step 1: Shell Requests Session from Rust

```typescript
// Shell creates session when launching app
const session = await invoke('create_app_session', { appId: 'com.example.app' });

// Response:
interface CreateSessionResponse {
  session_id: string;      // UUID v4
  token: string;           // HMAC-signed token
  capabilities: string[];  // Granted permissions
  expires_at: string;      // ISO 8601 timestamp
  platform_version: string;
}
```

### Step 2: Shell Creates MessageChannel and Iframe

```typescript
// Create isolated channel
const channel = new MessageChannel();

// Create sandboxed iframe
const iframe = document.createElement('iframe');
iframe.src = `postapp://${appId}/index.html`;
iframe.sandbox.add('allow-scripts');
// Note: allow-same-origin can be added if needed for specific APIs
```

### Step 3: Shell Sends Handshake with Port Transfer

```typescript
// One-time postMessage to transfer port
const handshake = {
  type: 'post_urbit_handshake',
  version: 1,
  session_id: session.session_id,
  token: session.token,
  app_id: appId,
  capabilities: session.capabilities,
  platform_version: session.platform_version,
};

// Transfer port2 to app - this is the ONLY postMessage we send
iframe.contentWindow.postMessage(handshake, '*', [channel.port2]);

// All future communication via channel.port1
channel.port1.onmessage = handleAppMessage;
channel.port1.start();
```

### Step 4: App Receives Handshake and Stores Context

```typescript
// In app's SDK initialization
window.addEventListener('message', (event) => {
  if (event.data?.type !== 'post_urbit_handshake') return;

  // Store session info
  bridgeState.sessionId = event.data.session_id;
  bridgeState.token = event.data.token;
  bridgeState.appId = event.data.app_id;
  bridgeState.capabilities = event.data.capabilities;

  // Store the port for all future communication
  bridgeState.port = event.ports[0];
  bridgeState.port.onmessage = handlePlatformMessage;
  bridgeState.port.start();

  // Remove the listener - we only need handshake once
  window.removeEventListener('message', this);
}, { once: true });
```

---

## 2. Message Schema

All messages use CBOR encoding for efficiency and binary support.

### Request Envelope

```typescript
interface BridgeRequest {
  v: 1;                    // Protocol version
  id: string;              // Unique request ID (UUID v4)
  ts: number;              // Timestamp in milliseconds
  session: string;         // Session ID from handshake
  token: string;           // HMAC token from handshake
  method: string;          // API method name
  params: CborValue;       // Method-specific parameters
  batch_id?: string;       // Optional batch identifier
}
```

### Response Envelope

```typescript
interface BridgeResponse {
  v: 1;                    // Protocol version
  id: string;              // Echoed request ID
  ts: number;              // Response timestamp
  ok: boolean;             // Success flag
  result?: CborValue;      // Method result (if ok=true)
  error?: BridgeError;     // Error details (if ok=false)
}

interface BridgeError {
  code: string;            // Machine-readable error code
  message: string;         // Human-readable message
  details?: CborValue;     // Additional context
  retryable: boolean;      // Whether retry might succeed
}
```

### Error Codes

| Code | Description | Retryable |
|------|-------------|-----------|
| `INVALID_REQUEST` | Malformed request | No |
| `INVALID_SESSION` | Session not found or expired | No |
| `INVALID_TOKEN` | Token validation failed | No |
| `PERMISSION_DENIED` | Capability not granted | No |
| `NOT_FOUND` | Resource not found | No |
| `CONFLICT` | Version mismatch | Yes |
| `RATE_LIMITED` | Too many requests | Yes |
| `INTERNAL_ERROR` | Backend error | Yes |
| `TIMEOUT` | Request timed out | Yes |

### Subscription Messages

```typescript
// Subscribe request
interface SubscribeRequest {
  v: 1;
  id: string;
  ts: number;
  session: string;
  token: string;
  method: 'subscribe';
  params: {
    topic: string;         // e.g., 'messaging', 'storage'
    filter?: CborValue;    // Topic-specific filter
  };
}

// Event pushed from backend
interface SubscriptionEvent {
  v: 1;
  type: 'event';
  subscription_id: string;
  seq: number;             // Sequence number for ordering
  ts: number;
  topic: string;
  data: CborValue;
}

// Unsubscribe
interface UnsubscribeRequest {
  v: 1;
  id: string;
  ts: number;
  session: string;
  token: string;
  method: 'unsubscribe';
  params: {
    subscription_id: string;
  };
}
```

### Batch Operations

```typescript
interface BatchRequest {
  v: 1;
  id: string;              // Batch ID
  ts: number;
  session: string;
  token: string;
  method: 'batch';
  params: {
    requests: Array<{
      id: string;          // Individual request ID
      method: string;
      params: CborValue;
    }>;
    atomic?: boolean;      // If true, all-or-nothing
  };
}

interface BatchResponse {
  v: 1;
  id: string;              // Batch ID
  ts: number;
  ok: boolean;
  results: Array<{
    id: string;            // Individual request ID
    ok: boolean;
    result?: CborValue;
    error?: BridgeError;
  }>;
}
```

---

## 3. Token System

### Token Generation (Rust)

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct SessionManagerConfig {
    /// Secret key for HMAC signing (32 bytes)
    pub secret: [u8; 32],
    /// Session timeout duration
    pub session_timeout: Duration,
    /// Maximum sessions per app
    pub max_sessions_per_app: usize,
    /// Replay protection window
    pub replay_window: Duration,
}

pub struct AppSession {
    pub session_id: String,
    pub app_id: String,
    pub token: String,
    pub capabilities: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub nonce: String,
}

impl AppSessionManager {
    pub async fn create_session(&self, app_id: &str) -> Result<CreateSessionResponse> {
        let session_id = Uuid::new_v4().to_string();
        let nonce = generate_nonce();  // 16 random bytes, hex encoded
        let created_at = Utc::now();
        let expires_at = created_at + self.config.session_timeout;

        // Generate HMAC token
        let payload = format!(
            "post-urbit:app-session:v1:{}:{}:{}:{}",
            session_id, app_id, created_at.timestamp(), nonce
        );

        let mut mac = HmacSha256::new_from_slice(&self.config.secret)?;
        mac.update(payload.as_bytes());
        let token = base64_url_encode(&mac.finalize().into_bytes());

        // Store session
        self.sessions.insert(session_id.clone(), AppSession {
            session_id: session_id.clone(),
            app_id: app_id.to_string(),
            token: token.clone(),
            capabilities: self.get_app_capabilities(app_id).await,
            created_at,
            expires_at,
            nonce,
        });

        Ok(CreateSessionResponse {
            session_id,
            token,
            capabilities,
            expires_at: expires_at.to_rfc3339(),
        })
    }
}
```

### Token Validation (Rust)

```rust
impl AppSessionManager {
    pub async fn validate_request(
        &self,
        session_id: &str,
        token: &str,
        request_id: &str,
        timestamp_ms: u64,
    ) -> Result<AppSession> {
        // 1. Check for replay
        if self.seen_request_ids.contains(request_id) {
            return Err(PostUrbitError::InvalidInput("replay detected"));
        }
        self.seen_request_ids.insert(request_id.to_string(), Instant::now());

        // 2. Validate timestamp (within 5 minutes)
        let now_ms = Utc::now().timestamp_millis() as u64;
        let time_diff = now_ms.abs_diff(timestamp_ms);
        if time_diff > 5 * 60 * 1000 {
            return Err(PostUrbitError::InvalidInput("timestamp out of range"));
        }

        // 3. Get session
        let session = self.sessions.get(session_id)
            .ok_or(PostUrbitError::InvalidInput("session not found"))?;

        // 4. Check expiration
        if Utc::now() > session.expires_at {
            self.sessions.remove(session_id);
            return Err(PostUrbitError::InvalidInput("session expired"));
        }

        // 5. Validate token
        let expected_token = self.generate_token(
            &session.session_id,
            &session.app_id,
            session.created_at.timestamp(),
            &session.nonce,
        )?;

        if !constant_time_eq(token.as_bytes(), expected_token.as_bytes()) {
            return Err(PostUrbitError::InvalidInput("invalid token"));
        }

        Ok(session.clone())
    }
}
```

---

## 4. TypeScript SDK

### SecureBridge Class

```typescript
import { encode as cborEncode, decode as cborDecode } from 'cbor-x';

interface BridgeState {
  sessionId: string;
  token: string;
  appId: string;
  capabilities: string[];
  port: MessagePort;
  ready: boolean;
}

class SecureBridge {
  private state: BridgeState | null = null;
  private pendingRequests = new Map<string, {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
    timeout: ReturnType<typeof setTimeout>;
  }>();
  private subscriptions = new Map<string, Set<(data: any) => void>>();
  private readyPromise: Promise<void>;
  private readyResolve!: () => void;

  constructor() {
    this.readyPromise = new Promise((resolve) => {
      this.readyResolve = resolve;
    });
    this.initHandshake();
  }

  private initHandshake() {
    window.addEventListener('message', (event) => {
      if (event.data?.type !== 'post_urbit_handshake') return;

      this.state = {
        sessionId: event.data.session_id,
        token: event.data.token,
        appId: event.data.app_id,
        capabilities: event.data.capabilities,
        port: event.ports[0],
        ready: true,
      };

      this.state.port.onmessage = this.handleMessage.bind(this);
      this.state.port.start();
      this.readyResolve();
    }, { once: true });
  }

  private handleMessage(event: MessageEvent) {
    const data = cborDecode(new Uint8Array(event.data));

    // Handle subscription events
    if (data.type === 'event') {
      const callbacks = this.subscriptions.get(data.subscription_id);
      callbacks?.forEach(cb => cb(data.data));
      return;
    }

    // Handle request responses
    const pending = this.pendingRequests.get(data.id);
    if (pending) {
      clearTimeout(pending.timeout);
      this.pendingRequests.delete(data.id);

      if (data.ok) {
        pending.resolve(data.result);
      } else {
        pending.reject(new BridgeError(data.error));
      }
    }
  }

  async call<T>(
    method: string,
    params: any = {},
    options: { timeout?: number; signal?: AbortSignal } = {}
  ): Promise<T> {
    await this.readyPromise;

    if (!this.state) {
      throw new Error('Bridge not initialized');
    }

    const requestId = crypto.randomUUID();
    const request = {
      v: 1,
      id: requestId,
      ts: Date.now(),
      session: this.state.sessionId,
      token: this.state.token,
      method,
      params,
    };

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pendingRequests.delete(requestId);
        reject(new Error('Request timeout'));
      }, options.timeout ?? 30000);

      // Handle abort signal
      options.signal?.addEventListener('abort', () => {
        clearTimeout(timeout);
        this.pendingRequests.delete(requestId);
        reject(new Error('Request aborted'));
      });

      this.pendingRequests.set(requestId, { resolve, reject, timeout });

      // Send via MessagePort
      const encoded = cborEncode(request);
      this.state!.port.postMessage(encoded.buffer, [encoded.buffer]);
    });
  }

  async batch<T>(
    requests: Array<{ method: string; params?: any }>,
    options: { atomic?: boolean; timeout?: number } = {}
  ): Promise<T[]> {
    const result = await this.call<{ results: Array<{ ok: boolean; result?: any; error?: any }> }>(
      'batch',
      {
        requests: requests.map((r, i) => ({
          id: `batch-${i}`,
          method: r.method,
          params: r.params ?? {},
        })),
        atomic: options.atomic ?? false,
      },
      { timeout: options.timeout }
    );

    return result.results.map((r, i) => {
      if (!r.ok) {
        throw new BridgeError(r.error);
      }
      return r.result;
    });
  }

  subscribe(
    topic: string,
    callback: (data: any) => void,
    filter?: any
  ): () => void {
    const subscriptionId = crypto.randomUUID();

    // Register callback
    if (!this.subscriptions.has(subscriptionId)) {
      this.subscriptions.set(subscriptionId, new Set());
    }
    this.subscriptions.get(subscriptionId)!.add(callback);

    // Send subscribe request
    this.call('subscribe', { topic, filter, subscription_id: subscriptionId });

    // Return unsubscribe function
    return () => {
      this.subscriptions.get(subscriptionId)?.delete(callback);
      if (this.subscriptions.get(subscriptionId)?.size === 0) {
        this.subscriptions.delete(subscriptionId);
        this.call('unsubscribe', { subscription_id: subscriptionId });
      }
    };
  }

  get capabilities(): string[] {
    return this.state?.capabilities ?? [];
  }

  hasCapability(cap: string): boolean {
    return this.capabilities.includes(cap);
  }
}

export const bridge = new SecureBridge();
```

### React Hooks

```typescript
// @post-urbit/sdk/react

import { bridge } from './bridge';

export function useIdentity() {
  const [identity, setIdentity] = useState<Identity | null>(null);

  useEffect(() => {
    bridge.call<Identity>('system.get_identity').then(setIdentity);
  }, []);

  return identity;
}

export function useStore<T>(key: string, defaultValue?: T) {
  const [value, setValue] = useState<T | undefined>(defaultValue);
  const [version, setVersion] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    bridge.call<{ value: T | null; version: number }>('storage.get', { key })
      .then(result => {
        if (result.value !== null) {
          setValue(result.value);
          setVersion(result.version);
        }
        setLoading(false);
      });
  }, [key]);

  const update = useCallback(async (newValue: T) => {
    const result = await bridge.call<{ version: number }>('storage.set', {
      key,
      value: newValue,
      expected_version: version,
    });
    setValue(newValue);
    setVersion(result.version);
  }, [key, version]);

  return { value, update, loading, version };
}

export function useMessaging() {
  const send = useCallback(async (
    recipient: string,
    messageType: string,
    content: any
  ) => {
    return bridge.call<{ message_id: string; sent_at: string }>('messaging.send', {
      recipient,
      message_type: messageType,
      content,
    });
  }, []);

  const subscribe = useCallback((
    filter: { message_types?: string[]; senders?: string[] },
    callback: (message: Message) => void
  ) => {
    return bridge.subscribe('messaging', callback, { filter });
  }, []);

  return { send, subscribe };
}

export function useContacts(options?: { limit?: number }) {
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    bridge.call<{ contacts: Contact[] }>('contacts.list', options)
      .then(result => {
        setContacts(result.contacts);
        setLoading(false);
      });
  }, [options?.limit]);

  return { contacts, loading };
}

export function usePermissions() {
  const request = useCallback(async (permissions: string[]) => {
    return bridge.call<{ granted: string[]; denied: string[] }>(
      'permission.request',
      { permissions }
    );
  }, []);

  const check = useCallback((permission: string) => {
    return bridge.hasCapability(permission);
  }, []);

  return { request, check, capabilities: bridge.capabilities };
}
```

---

## 5. Flow Control

### Rate Limiting

Backend enforces per-session rate limits:

```rust
pub struct RateLimiter {
    max_rps: u32,
    window: Duration,
    counts: HashMap<String, Vec<Instant>>,
}

impl RateLimiter {
    pub fn check(&mut self, session_id: &str) -> Result<()> {
        let now = Instant::now();
        let cutoff = now - self.window;

        let entry = self.counts.entry(session_id.to_string()).or_default();
        entry.retain(|ts| *ts > cutoff);

        if entry.len() >= self.max_rps as usize {
            return Err(PostUrbitError::InvalidInput("rate limited"));
        }

        entry.push(now);
        Ok(())
    }
}
```

### Subscription Backpressure

Events include sequence numbers. If client falls behind:

```rust
pub struct SubscriptionState {
    pub id: String,
    pub session_id: String,
    pub topic: String,
    pub last_delivered_seq: u64,
    pub pending_events: VecDeque<SubscriptionEvent>,
    pub max_pending: usize,  // Default: 100
}

impl SubscriptionState {
    pub fn enqueue(&mut self, event: SubscriptionEvent) -> bool {
        if self.pending_events.len() >= self.max_pending {
            // Drop oldest, signal backpressure
            self.pending_events.pop_front();
            return false;  // Backpressure
        }
        self.pending_events.push_back(event);
        true
    }
}
```

### Timeout and Cancellation

SDK-side cancellation via AbortController:

```typescript
const controller = new AbortController();

// With timeout
const result = await bridge.call('slow_operation', params, {
  signal: controller.signal,
  timeout: 60000,
});

// Manual cancellation
controller.abort();
```

---

## 6. Implementation Priority

### Phase 1: Core Security (Critical)

1. Implement `AppSessionManager` with HMAC tokens
2. Add `create_app_session` Tauri command
3. Replace global postMessage with MessageChannel
4. Add CSP headers to protocol handler
5. Block navigation in webview

### Phase 2: Message Layer

1. CBOR request/response handling
2. Request ID replay protection
3. Basic methods (storage, identity)
4. Subscription infrastructure

### Phase 3: SDK

1. SecureBridge class
2. React hooks
3. Batch operations
4. Timeout/cancellation

### Phase 4: Hardening

1. Rate limiting
2. Subscription backpressure
3. Comprehensive error handling
4. Security audit

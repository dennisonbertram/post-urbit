# Capability System

## Overview

The capability system controls what resources and operations applications can access. Applications declare required capabilities in their manifest; users grant permissions at install time or on first use.

## Design Principles

### Explicit Consent

Users must explicitly grant each capability. No default permissions beyond basic execution.

### Minimal Surface

Capabilities are fine-grained. Apps request only what they need.

### Revocable

Users can revoke any permission at any time. Apps must handle permission denial gracefully.

### Auditable

All capability grants and uses are logged for user review.

## Capability Categories

### Storage Capabilities

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `storage:app` | Read/write app's own data directory | Low |
| `storage:shared:{namespace}` | Access shared data namespace | Medium |
| `storage:quota:{size}` | Request specific storage quota | Low |

### Messaging Capabilities

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `messaging:send` | Send messages to contacts | High |
| `messaging:subscribe` | Subscribe to and receive messages | Medium |
| `messaging:group` | Create and manage groups | High |

**Note:** Receiving messages requires `messaging:subscribe`. There is no separate `messaging:receive` capability; subscribing implies the ability to receive matching messages.

### Contact Capabilities

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `contacts:read` | Read contact list | Medium |
| `contacts:read:limited` | Read only contacts who use this app | Low |
| `contacts:write` | Add/modify contacts | High |

### Sync Capabilities

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `sync:documents` | Sync documents across devices | Medium |
| `sync:peers:{iid}` | Sync with specific peer | Medium |
| `sync:all_peers` | Sync with all contacts | High |

### Notification Capabilities

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `notifications:show` | Display notifications | Low |
| `notifications:badge` | Update app badge | Low |
| `notifications:sound` | Play notification sounds | Low |

### System Capabilities

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `system:time` | Access current time | Low |
| `system:random` | Access cryptographic randomness | Low |
| `system:identity:read` | Read user's identity info | Medium |
| `system:background` | Run background tasks | Medium |

### Inter-App Capabilities

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `app:invoke:{app_id}` | Invoke specific other app | Medium |
| `app:invoke:any` | Invoke any installed app | High |
| `app:share:{app_id}` | Share data with specific app | Medium |

## Capability Specification

### Manifest Declaration

Apps declare capabilities in their manifest:

```json
{
  "capabilities": {
    "required": [
      "storage:app",
      "messaging:send",
      "messaging:receive"
    ],
    "optional": [
      "contacts:read:limited",
      "notifications:show"
    ]
  }
}
```

### Capability Format

```
capability = category ":" action [":" parameter]

category = "storage" | "messaging" | "contacts" | "sync" |
           "notifications" | "system" | "app"

action = identifier

parameter = identifier | "*"
```

### Parameterized Capabilities

Some capabilities accept parameters:

```
storage:shared:photos       # Specific namespace
storage:quota:100mb         # Specific quota
sync:peers:k5xq7z4m2n...    # Specific peer IID
app:invoke:com.example.chat # Specific app
```

## Permission Model

### Permission States

```typescript
type PermissionState =
  | 'NOT_REQUESTED'   // App hasn't asked for this
  | 'PENDING'         // Waiting for user decision
  | 'GRANTED'         // User approved
  | 'DENIED'          // User denied
  | 'REVOKED';        // User revoked after granting
```

### Permission Storage

```typescript
interface PermissionRecord {
  appId: string;
  capability: string;
  state: PermissionState;
  grantedAt?: Timestamp;
  grantedBy: 'install' | 'prompt' | 'setting';
  expiresAt?: Timestamp;      // For temporary grants
  usageCount: number;
  lastUsed?: Timestamp;
}
```

### Permission Grants

```typescript
interface PermissionGrant {
  capability: string;
  scope?: PermissionScope;
  duration?: GrantDuration;
}

type PermissionScope =
  | { type: 'all' }
  | { type: 'specific'; values: string[] }
  | { type: 'pattern'; regex: string };

type GrantDuration =
  | { type: 'permanent' }
  | { type: 'session' }           // Until app restart
  | { type: 'timed'; seconds: number }
  | { type: 'use_count'; count: number };
```

## Permission Workflow

### At Install Time

```
1. Parse app manifest
2. Display required capabilities with explanations
3. User approves or cancels installation
4. If approved, grant all required capabilities
5. Store permission records
```

### At Runtime (Optional Capabilities)

```
1. App calls host API requiring optional capability
2. Host checks permission state
3. If NOT_REQUESTED:
   a. Show permission prompt to user
   b. User approves or denies
   c. Update permission state
4. If GRANTED: proceed with operation
5. If DENIED or REVOKED: return PERMISSION_DENIED error (no reprompt)
```

**Important:** Once denied or revoked, capabilities are NOT automatically re-prompted. Users must explicitly re-grant via settings UI.

### Revocation Behavior

When a capability is revoked mid-session:

| Scenario | Behavior |
|----------|----------|
| Pending host call | Fails with `PERMISSION_DENIED` |
| Active subscription | Deleted; queued messages dropped |
| In-progress invocation | Continues until next host call requiring the capability |
| Shared storage access | Immediately denied |

**Required vs Optional capability revocation:**
- Required capabilities: App disabled if revoked (cannot function without them)
- Optional capabilities: App continues with reduced functionality

### Permission Prompt UI

```typescript
interface PermissionPrompt {
  appId: string;
  appName: string;
  appIcon: string;
  capability: string;
  capabilityDescription: string;
  riskLevel: 'low' | 'medium' | 'high';
  reason?: string;              // Why app needs this (from manifest)
  options: PromptOption[];
}

interface PromptOption {
  action: 'allow' | 'deny' | 'allow_once' | 'allow_session';
  label: string;
  isDefault: boolean;
}
```

## Capability Enforcement

### Enforcement Points

```typescript
interface CapabilityEnforcer {
  // Check if app has capability
  hasCapability(appId: string, capability: string): boolean;

  // Request capability (may prompt user)
  requestCapability(
    appId: string,
    capability: string,
    reason?: string
  ): Promise<PermissionState>;

  // Assert capability (throws if denied)
  assertCapability(appId: string, capability: string): void;

  // Get all capabilities for app
  getCapabilities(appId: string): PermissionRecord[];

  // Revoke capability
  revokeCapability(appId: string, capability: string): void;
}
```

### Host API Integration

Every host API call checks capabilities:

```typescript
async function handleHostCall(
  appId: string,
  method: string,
  args: unknown
): Promise<unknown> {
  // Get required capability for this method
  const required = getRequiredCapability(method);

  if (required) {
    // Check current state (do NOT auto-prompt for revoked/denied)
    const state = enforcer.hasCapability(appId, required);

    if (!state) {
      // Check if this is an optional capability that hasn't been requested yet
      const isOptional = isOptionalCapability(appId, required);
      const wasRequested = wasCapabilityRequested(appId, required);

      if (isOptional && !wasRequested) {
        // First-time request for optional capability: prompt user
        const granted = await enforcer.requestCapability(appId, required);
        if (!granted) {
          throw new PermissionDeniedError(required);
        }
      } else {
        // Required capability missing, or optional was denied/revoked: fail without prompt
        throw new PermissionDeniedError(required);
      }
    }
  }

  // Proceed with operation
  return executeMethod(appId, method, args);
}
```

### Method-to-Capability Mapping

This mapping is **authoritative**. Method names use `snake_case` as defined in `abi.md`.

```typescript
const CAPABILITY_MAP: Record<string, string | null> = {
  // Storage (all require storage:app)
  'storage.get': 'storage:app',
  'storage.set': 'storage:app',
  'storage.delete': 'storage:app',
  'storage.list': 'storage:app',
  'storage.shared.get': 'storage:shared:*',  // * replaced with namespace
  'storage.shared.set': 'storage:shared:*',

  // Messaging
  'messaging.send': 'messaging:send',
  'messaging.send_group': ['messaging:send', 'messaging:group'],  // Requires both
  'messaging.subscribe': 'messaging:subscribe',
  'messaging.unsubscribe': 'messaging:subscribe',
  'messaging.create_group': 'messaging:group',
  'messaging.list_groups': 'messaging:group',

  // Contacts
  'contacts.list': 'contacts:read',
  'contacts.get': 'contacts:read',
  'contacts.list_app_users': 'contacts:read:limited',

  // Sync
  'sync.create_document': 'sync:documents',
  'sync.get_document': 'sync:documents',
  'sync.apply_operation': 'sync:documents',
  'sync.subscribe': 'sync:documents',
  'sync.share': 'sync:documents',

  // Notifications
  'notifications.show': 'notifications:show',
  'notifications.set_badge': 'notifications:badge',
  'notifications.cancel': 'notifications:show',

  // System
  'system.get_time': 'system:time',
  'system.get_random': 'system:random',
  'system.get_deterministic_random': null,  // No capability required
  'system.get_identity': 'system:identity:read',
  'system.get_app_info': null,  // No capability required

  // Inter-App
  'app.invoke': 'app:invoke:*',  // * replaced with target app_id
  'app.share': 'app:share:*',
};
```

**Notes:**
- `null` means no capability required (always allowed)
- `*` is replaced with the actual parameter value (namespace, app_id, etc.)
- Array values require ALL listed capabilities
- Unknown methods return `METHOD_NOT_FOUND` error

## Capability Delegation

### App-to-App Capability Sharing

Apps can delegate capabilities to other apps:

```typescript
interface CapabilityDelegation {
  fromApp: string;
  toApp: string;
  capability: string;
  constraints?: DelegationConstraints;
  expiresAt?: Timestamp;
}

interface DelegationConstraints {
  // Limit scope
  scope?: PermissionScope;
  // Limit operations
  operations?: string[];
  // Limit usage
  maxUses?: number;
}
```

### Delegation Rules

1. App can only delegate capabilities it has
2. Delegated capabilities are subset of original
3. User must approve delegation (or app has `app:delegate` capability)
4. Delegation is revocable

## Resource Quotas

### Quota Capabilities

Storage quotas are capabilities:

```
storage:quota:10mb     # 10 megabytes
storage:quota:100mb    # 100 megabytes
storage:quota:1gb      # 1 gigabyte
```

### Quota Enforcement

```typescript
interface QuotaEnforcer {
  // Check current usage
  getUsage(appId: string): QuotaUsage;

  // Check if operation would exceed quota
  checkQuota(appId: string, additionalBytes: number): boolean;

  // Request quota increase
  requestQuotaIncrease(
    appId: string,
    requestedBytes: number
  ): Promise<boolean>;
}

interface QuotaUsage {
  used: number;        // Bytes used
  limit: number;       // Current limit
  reserved: number;    // Reserved for pending ops
}
```

## Audit Logging

### Audit Events

```typescript
interface AuditEvent {
  timestamp: Timestamp;
  appId: string;
  capability: string;
  action: AuditAction;
  details?: Record<string, unknown>;
}

type AuditAction =
  | 'REQUESTED'
  | 'GRANTED'
  | 'DENIED'
  | 'USED'
  | 'REVOKED'
  | 'EXPIRED';
```

### Audit Log Queries

```typescript
interface AuditLog {
  // Get events for app
  getAppEvents(
    appId: string,
    filter?: AuditFilter
  ): AuditEvent[];

  // Get events for capability
  getCapabilityEvents(
    capability: string,
    filter?: AuditFilter
  ): AuditEvent[];

  // Get summary
  getSummary(
    appId: string,
    since: Timestamp
  ): AuditSummary;
}

interface AuditFilter {
  since?: Timestamp;
  until?: Timestamp;
  actions?: AuditAction[];
  limit?: number;
}

interface AuditSummary {
  capabilityUsage: Map<string, number>;
  deniedRequests: number;
  revokedCapabilities: string[];
}
```

## Security Considerations

### Permission Fatigue

Risk: Users blindly approve all permissions.

Mitigations:
- Clear, non-technical descriptions
- Risk level indicators
- Usage statistics shown before granting
- Periodic permission reviews

### Capability Creep

Risk: Apps request more than needed over time.

Mitigations:
- Required capabilities locked at install
- Optional capability requests are logged
- Users can review and revoke

### Covert Channels

Risk: Apps communicate through shared resources.

Mitigations:
- Shared storage namespaces require explicit capability
- Inter-app communication is logged
- No timing-based channels (fuel normalization)

### Privilege Escalation

Risk: App gains capabilities it wasn't granted.

Mitigations:
- Capability checks at every enforcement point
- No ambient authority
- Capability tokens are unforgeable

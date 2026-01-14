# App Runtime Interfaces

## Overview

This document specifies the complete TypeScript interface for the App Runtime layer, including app lifecycle management, capability enforcement, and the host bridge.

## Core Types

```typescript
// App identifier (reverse domain notation)
type AppId = string;

// Package content hash
type PackageHash = string;

// App version (semver)
type AppVersion = string;

// Timestamps
type Timestamp = string;  // RFC3339

// Binary data
type Bytes = Uint8Array;
```

## App Metadata Types

```typescript
interface AppManifest {
  manifestVersion: number;
  app: AppInfo;
  runtime: RuntimeConfig;
  capabilities: CapabilitiesConfig;
  storage: StorageConfig;
  ui: UIConfig;
  handlers: HandlersConfig;
  background: BackgroundConfig;
  dependencies: DependenciesConfig;
  signature: SignatureConfig;
}

interface AppInfo {
  id: AppId;
  name: string;
  version: AppVersion;
  description: string;
  author: AuthorInfo;
  license: string;
  homepage?: string;
  repository?: string;
}

interface AuthorInfo {
  name: string;
  iid?: string;
  url?: string;
}

interface RuntimeConfig {
  entry: string;
  memory?: MemoryConfig;
  fuel?: FuelConfig;
}

interface MemoryConfig {
  initialPages: number;
  maximumPages: number;
}

interface FuelConfig {
  userAction?: number;
  backgroundTask?: number;
  appStart?: number;
}

interface CapabilitiesConfig {
  required: string[];
  optional?: string[];
  reasons?: Record<string, string>;
}

interface StorageConfig {
  quota?: string;
  sharedNamespaces?: string[];
}

interface UIConfig {
  icon: string;
  screenshots?: string[];
  category: AppCategory;
  contentRating: ContentRating;
}

type AppCategory =
  | 'social'
  | 'productivity'
  | 'utilities'
  | 'games'
  | 'media'
  | 'finance'
  | 'health'
  | 'education'
  | 'other';

type ContentRating = 'everyone' | 'teen' | 'mature';

interface HandlersConfig {
  messageTypes?: string[];
  fileTypes?: string[];
  urlSchemes?: string[];
}

interface BackgroundConfig {
  enabled: boolean;
  triggers: BackgroundTrigger[];
}

type BackgroundTrigger =
  | { type: 'interval'; intervalSeconds: number }
  | { type: 'message'; messageType: string }
  | { type: 'sync'; documentType: string }
  | { type: 'boot' };

interface DependenciesConfig {
  nodeVersion?: string;
  apiVersion: string;
  apps?: Record<AppId, string>;
}

interface SignatureConfig {
  algorithm: 'ed25519';
  publicKey: string;
  signature: string;
}
```

## App Instance Types

```typescript
interface AppInstance {
  appId: AppId;
  state: InstanceState;
  loadedAt?: Timestamp;
  lastInvocation?: Timestamp;
  fuelRemaining: number;
  memoryUsed: number;
  invocationCount: number;
}

type InstanceState =
  | 'INACTIVE'
  | 'LOADING'
  | 'READY'
  | 'RUNNING';

interface InvocationContext {
  invocationId: string;
  appId: AppId;
  entryPoint: string;
  fuel: number;
  startedAt: Timestamp;
}
```

## Installed App Types

```typescript
interface InstalledApp {
  appId: AppId;
  manifest: AppManifest;
  packageHash: PackageHash;
  installedAt: Timestamp;
  updatedAt?: Timestamp;
  installedBy: string;  // 'user' | 'system' | app_id
  state: InstalledAppState;
  permissions: PermissionState[];
  storageUsed: number;
}

type InstalledAppState =
  | 'ACTIVE'
  | 'DISABLED'
  | 'UPDATING'
  | 'UNINSTALLING';

interface PermissionState {
  capability: string;
  state: PermissionStatus;
  grantedAt?: Timestamp;
  grantedBy?: string;
}

type PermissionStatus =
  | 'NOT_REQUESTED'
  | 'PENDING'
  | 'GRANTED'
  | 'DENIED'
  | 'REVOKED';
```

## App Manager Service

```typescript
interface AppManagerService {
  // === Installation ===

  /**
   * Install an app from a package.
   */
  install(
    packageBytes: Bytes,
    options?: InstallOptions
  ): Promise<Result<InstalledApp, InstallError>>;

  /**
   * Update an installed app.
   */
  update(
    appId: AppId,
    packageBytes: Bytes
  ): Promise<Result<InstalledApp, UpdateError>>;

  /**
   * Uninstall an app.
   */
  uninstall(
    appId: AppId,
    options?: UninstallOptions
  ): Promise<Result<void, UninstallError>>;

  // === Queries ===

  /**
   * Get installed app info.
   */
  getApp(appId: AppId): InstalledApp | null;

  /**
   * List all installed apps.
   */
  listApps(filter?: AppFilter): InstalledApp[];

  /**
   * Search available apps (from app sources).
   */
  searchApps(query: string): Promise<AppSearchResult[]>;

  // === State Management ===

  /**
   * Enable/disable an app.
   */
  setEnabled(appId: AppId, enabled: boolean): Promise<void>;

  /**
   * Get app storage usage.
   */
  getStorageUsage(appId: AppId): StorageUsage;

  /**
   * Clear app data.
   */
  clearData(
    appId: AppId,
    options?: ClearDataOptions
  ): Promise<void>;

  // === Events ===

  onAppInstalled: Event<{ app: InstalledApp }>;
  onAppUpdated: Event<{ app: InstalledApp; previousVersion: AppVersion }>;
  onAppUninstalled: Event<{ appId: AppId }>;
  onAppStateChanged: Event<{ appId: AppId; state: InstalledAppState }>;
}

interface InstallOptions {
  autoGrant?: boolean;        // Auto-grant required capabilities
  source?: string;            // Source identifier
}

interface UninstallOptions {
  keepData?: boolean;         // Preserve app data
}

interface AppFilter {
  state?: InstalledAppState[];
  category?: AppCategory[];
  hasCapability?: string[];
}

interface AppSearchResult {
  appId: AppId;
  name: string;
  version: AppVersion;
  description: string;
  source: string;
  downloadUrl: string;
}

interface StorageUsage {
  total: number;
  data: number;
  cache: number;
  quota: number;
}

interface ClearDataOptions {
  preserveSettings?: boolean;
  clearCache?: boolean;
}

type InstallError =
  | 'INVALID_PACKAGE'
  | 'INVALID_MANIFEST'
  | 'INVALID_SIGNATURE'
  | 'ALREADY_INSTALLED'
  | 'INCOMPATIBLE_VERSION'
  | 'STORAGE_ERROR';

type UpdateError =
  | 'APP_NOT_FOUND'
  | 'INVALID_PACKAGE'
  | 'INVALID_SIGNATURE'
  | 'VERSION_DOWNGRADE'
  | 'MIGRATION_FAILED';

type UninstallError =
  | 'APP_NOT_FOUND'
  | 'APP_RUNNING'
  | 'SYSTEM_APP';
```

## App Runtime Service

```typescript
interface AppRuntimeService {
  // === Instance Management ===

  /**
   * Load app into memory.
   */
  load(appId: AppId): Promise<Result<void, LoadError>>;

  /**
   * Unload app from memory.
   */
  unload(appId: AppId): Promise<void>;

  /**
   * Get instance info.
   */
  getInstance(appId: AppId): AppInstance | null;

  /**
   * List all loaded instances.
   */
  listInstances(): AppInstance[];

  // === Invocation ===

  /**
   * Invoke app entry point.
   */
  invoke(
    appId: AppId,
    invocation: AppInvocation
  ): Promise<Result<InvocationResult, InvocationError>>;

  /**
   * Cancel running invocation.
   */
  cancel(invocationId: string): Promise<void>;

  // === Background Tasks ===

  /**
   * Schedule background task.
   */
  scheduleTask(
    appId: AppId,
    task: BackgroundTask
  ): Promise<string>;  // Task ID

  /**
   * Cancel scheduled task.
   */
  cancelTask(taskId: string): Promise<void>;

  /**
   * List scheduled tasks.
   */
  listTasks(appId?: AppId): ScheduledTask[];

  // === Events ===

  onInstanceLoaded: Event<{ appId: AppId }>;
  onInstanceUnloaded: Event<{ appId: AppId }>;
  onInvocationStarted: Event<{ context: InvocationContext }>;
  onInvocationCompleted: Event<{ context: InvocationContext; success: boolean }>;
}

interface AppInvocation {
  entryPoint: string;         // WASM export name
  args: Bytes;                // CBOR-encoded arguments
  fuel?: number;              // Override default fuel
  timeout?: number;           // Timeout in ms
}

interface InvocationResult {
  result: Bytes;              // CBOR-encoded result
  fuelUsed: number;
  durationMs: number;
}

interface BackgroundTask {
  entryPoint: string;
  args?: Bytes;
  trigger: BackgroundTrigger;
}

interface ScheduledTask {
  taskId: string;
  appId: AppId;
  entryPoint: string;
  trigger: BackgroundTrigger;
  nextRun?: Timestamp;
  lastRun?: Timestamp;
}

type LoadError =
  | 'APP_NOT_FOUND'
  | 'APP_DISABLED'
  | 'INVALID_WASM'
  | 'MEMORY_LIMIT'
  | 'ALREADY_LOADED';

type InvocationError =
  | 'NOT_LOADED'
  | 'ENTRY_NOT_FOUND'
  | 'FUEL_EXHAUSTED'
  | 'TRAPPED'
  | 'TIMEOUT'
  | 'CANCELLED'
  | 'PERMISSION_DENIED';
```

## Capability Service

```typescript
interface CapabilityService {
  // === Permission Checks ===

  /**
   * Check if app has capability.
   */
  hasCapability(appId: AppId, capability: string): boolean;

  /**
   * Get all capabilities for app.
   */
  getCapabilities(appId: AppId): PermissionState[];

  // === Permission Requests ===

  /**
   * Request capability (may prompt user).
   */
  requestCapability(
    appId: AppId,
    capability: string,
    reason?: string
  ): Promise<PermissionStatus>;

  /**
   * Request multiple capabilities.
   */
  requestCapabilities(
    appId: AppId,
    capabilities: string[],
    reasons?: Record<string, string>
  ): Promise<Record<string, PermissionStatus>>;

  // === Permission Management ===

  /**
   * Grant capability (admin action).
   */
  grantCapability(
    appId: AppId,
    capability: string,
    options?: GrantOptions
  ): Promise<void>;

  /**
   * Revoke capability.
   */
  revokeCapability(appId: AppId, capability: string): Promise<void>;

  /**
   * Revoke all capabilities.
   */
  revokeAllCapabilities(appId: AppId): Promise<void>;

  // === Audit ===

  /**
   * Get audit log.
   */
  getAuditLog(filter?: AuditFilter): AuditEvent[];

  // === Events ===

  onCapabilityGranted: Event<{ appId: AppId; capability: string }>;
  onCapabilityRevoked: Event<{ appId: AppId; capability: string }>;
  onCapabilityUsed: Event<{ appId: AppId; capability: string; method: string }>;
}

interface GrantOptions {
  duration?: GrantDuration;
  scope?: PermissionScope;
}

type GrantDuration =
  | { type: 'permanent' }
  | { type: 'session' }
  | { type: 'timed'; seconds: number }
  | { type: 'useCount'; count: number };

type PermissionScope =
  | { type: 'all' }
  | { type: 'specific'; values: string[] }
  | { type: 'pattern'; regex: string };

interface AuditFilter {
  appId?: AppId;
  capability?: string;
  actions?: AuditAction[];
  since?: Timestamp;
  until?: Timestamp;
  limit?: number;
}

interface AuditEvent {
  timestamp: Timestamp;
  appId: AppId;
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

## Host Bridge Interface

```typescript
/**
 * The bridge between WASM apps and the host.
 * Implements all Host API methods.
 */
interface HostBridge {
  // === Call Handling ===

  /**
   * Handle a host call from WASM.
   */
  handleCall(
    context: InvocationContext,
    method: string,
    args: Bytes
  ): Promise<Result<Bytes, HostError>>;

  // === Callbacks ===

  /**
   * Register callback for app.
   */
  registerCallback(
    appId: AppId,
    name: string,
    entryPoint: string
  ): void;

  /**
   * Invoke callback.
   */
  invokeCallback(
    appId: AppId,
    name: string,
    args: Bytes
  ): Promise<void>;

  // === Subscriptions ===

  /**
   * Get active subscriptions for app.
   */
  getSubscriptions(appId: AppId): Subscription[];

  /**
   * Cancel subscription.
   */
  cancelSubscription(subscriptionId: string): void;
}

interface Subscription {
  subscriptionId: string;
  appId: AppId;
  type: SubscriptionType;
  filter: unknown;
  callbackEntry: string;
}

type SubscriptionType =
  | 'message'
  | 'sync'
  | 'contact';

type HostError =
  | 'PERMISSION_DENIED'
  | 'INVALID_ARGUMENT'
  | 'NOT_FOUND'
  | 'QUOTA_EXCEEDED'
  | 'INTERNAL_ERROR'
  | 'TIMEOUT';
```

## App Storage Interface

```typescript
interface AppStorageService {
  // === Per-App Storage ===

  /**
   * Get value from app storage.
   */
  get(appId: AppId, key: string): Promise<StoredValue | null>;

  /**
   * Set value in app storage.
   */
  set(
    appId: AppId,
    key: string,
    value: Bytes,
    options?: SetOptions
  ): Promise<number>;  // Returns version

  /**
   * Delete from app storage.
   */
  delete(appId: AppId, key: string): Promise<boolean>;

  /**
   * List keys in app storage.
   */
  list(
    appId: AppId,
    prefix: string,
    options?: ListOptions
  ): Promise<StorageListResult>;

  // === Shared Storage ===

  /**
   * Get from shared namespace.
   */
  getShared(
    namespace: string,
    key: string
  ): Promise<StoredValue | null>;

  /**
   * Set in shared namespace.
   */
  setShared(
    appId: AppId,  // For tracking ownership
    namespace: string,
    key: string,
    value: Bytes
  ): Promise<number>;

  // === Quota Management ===

  /**
   * Get storage usage for app.
   */
  getUsage(appId: AppId): StorageUsage;

  /**
   * Check if operation fits in quota.
   */
  checkQuota(appId: AppId, additionalBytes: number): boolean;
}

interface StoredValue {
  value: Bytes;
  version: number;
  storedAt: Timestamp;
  size: number;
}

interface SetOptions {
  expectedVersion?: number;   // For optimistic concurrency
}

interface ListOptions {
  cursor?: string;
  limit?: number;
}

interface StorageListResult {
  keys: string[];
  cursor?: string;
  hasMore: boolean;
}
```

## Error Types

```typescript
class AppRuntimeError extends Error {
  constructor(
    public code: AppRuntimeErrorCode,
    message: string,
    public details?: Record<string, unknown>
  ) {
    super(message);
  }
}

type AppRuntimeErrorCode =
  // Installation errors
  | 'INVALID_PACKAGE'
  | 'INVALID_MANIFEST'
  | 'INVALID_SIGNATURE'
  | 'ALREADY_INSTALLED'
  | 'INCOMPATIBLE_VERSION'

  // Runtime errors
  | 'APP_NOT_FOUND'
  | 'APP_DISABLED'
  | 'NOT_LOADED'
  | 'ENTRY_NOT_FOUND'
  | 'INVALID_WASM'

  // Execution errors
  | 'FUEL_EXHAUSTED'
  | 'MEMORY_LIMIT'
  | 'TRAPPED'
  | 'TIMEOUT'
  | 'CANCELLED'

  // Permission errors
  | 'PERMISSION_DENIED'
  | 'CAPABILITY_NOT_FOUND'

  // Storage errors
  | 'QUOTA_EXCEEDED'
  | 'STORAGE_ERROR'
  | 'VERSION_MISMATCH'

  // System errors
  | 'INTERNAL_ERROR';
```

## Event Types

```typescript
// Generic event type
type Event<T> = {
  subscribe(callback: (data: T) => void): Unsubscribe;
};

type Unsubscribe = () => void;

// Specific event payloads
interface AppInstalledEvent {
  app: InstalledApp;
}

interface AppUpdatedEvent {
  app: InstalledApp;
  previousVersion: AppVersion;
}

interface AppUninstalledEvent {
  appId: AppId;
}

interface InvocationStartedEvent {
  context: InvocationContext;
}

interface InvocationCompletedEvent {
  context: InvocationContext;
  success: boolean;
  error?: AppRuntimeErrorCode;
}

interface CapabilityUsedEvent {
  appId: AppId;
  capability: string;
  method: string;
  allowed: boolean;
}
```

## Result Type

```typescript
type Result<T, E> =
  | { ok: true; value: T }
  | { ok: false; error: E };

// Helper functions
function ok<T>(value: T): Result<T, never> {
  return { ok: true, value };
}

function err<E>(error: E): Result<never, E> {
  return { ok: false, error };
}
```

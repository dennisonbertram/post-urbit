# Resource Constraints Specification

## Overview

This specification defines resource management for the Post-Urbit multi-webview architecture. Each app runs in a separate webview (50-350MB each), requiring careful resource orchestration to maintain shell responsiveness across varying device capabilities.

### Design Principles

1. **Device-Aware Defaults** - Automatically tune limits based on physical RAM
2. **Graceful Degradation** - Progressive resource reduction before forced eviction
3. **Predictable Eviction** - Deterministic LRU algorithm with auditable decisions
4. **App Cooperation** - Pressure signaling enables apps to self-regulate
5. **Shell Priority** - Shell responsiveness always protected over app resources

### Related Documents

- [Domain 2: App Sandbox & Isolation](./02-APP_SANDBOX_ISOLATION.md) - Webview lifecycle states
- [ADR-003: Multi-webview Architecture](../adrs/ADR-003-multiwebview-isolation.md) - Memory overhead justification
- [Tauri Multi-webview Research](../TAURI_MULTIWEBVIEW_RESEARCH.md) - Platform memory baselines

---

## Resource Model Data Structures

### Rust Types

```rust
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};

/// Device capability classification based on physical RAM
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    /// <= 8GB RAM: constrained mobile/low-end desktop
    Constrained,
    /// 16GB RAM: typical desktop
    Standard,
    /// >= 32GB RAM: high-end workstation
    Performance,
}

impl DeviceClass {
    pub fn from_physical_ram(ram_bytes: u64) -> Self {
        let ram_gb = ram_bytes / (1024 * 1024 * 1024);
        match ram_gb {
            0..=8 => DeviceClass::Constrained,
            9..=24 => DeviceClass::Standard,
            _ => DeviceClass::Performance,
        }
    }
}

/// Global resource limits derived from device class
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResourceLimitsConfig {
    /// Device classification
    pub device_class: DeviceClass,

    /// Maximum number of hot (visible) webviews
    pub max_hot_webviews: usize,

    /// Maximum number of warm (hidden but alive) webviews
    pub max_warm_webviews: usize,

    /// Total RSS budget for all app webviews (bytes)
    pub total_app_memory_budget_bytes: u64,

    /// System memory pressure thresholds (0.0-1.0)
    pub memory_pressure_warn_threshold: f64,
    pub memory_pressure_critical_threshold: f64,

    /// Shell memory budgets (bytes)
    pub shell_memory_target_bytes: u64,
    pub shell_memory_warn_bytes: u64,
    pub shell_memory_critical_bytes: u64,

    /// Per-app memory limits (bytes)
    pub per_app_memory_soft_cap_bytes: u64,
    pub per_app_memory_hard_cap_bytes: u64,

    /// CPU limits (percentage of one core, 100 = one full core)
    pub foreground_cpu_warn_percent: u32,
    pub foreground_cpu_warn_duration_secs: u64,
    pub background_cpu_warn_percent: u32,
    pub background_cpu_warn_duration_secs: u64,
    pub background_cpu_critical_percent: u32,
    pub background_cpu_critical_duration_secs: u64,

    /// Warm timeout before eviction to cold (seconds)
    pub warm_timeout_secs: u64,

    /// Hysteresis gap to prevent eviction thrashing
    pub pressure_hysteresis: f64,

    /// Eviction grace period (milliseconds)
    pub eviction_grace_period_ms: u64,

    /// Maximum state blob from prepare_for_unload (bytes)
    pub max_unload_state_bytes: usize,
}

impl ResourceLimitsConfig {
    /// Create device-appropriate defaults
    pub fn for_device_class(class: DeviceClass) -> Self {
        match class {
            DeviceClass::Constrained => Self {
                device_class: class,
                max_hot_webviews: 2,
                max_warm_webviews: 2,
                total_app_memory_budget_bytes: 1_536 * 1024 * 1024, // 1.5 GB
                memory_pressure_warn_threshold: 0.75,
                memory_pressure_critical_threshold: 0.85,
                shell_memory_target_bytes: 200 * 1024 * 1024,
                shell_memory_warn_bytes: 250 * 1024 * 1024,
                shell_memory_critical_bytes: 350 * 1024 * 1024,
                per_app_memory_soft_cap_bytes: 300 * 1024 * 1024,
                per_app_memory_hard_cap_bytes: 500 * 1024 * 1024,
                foreground_cpu_warn_percent: 120,
                foreground_cpu_warn_duration_secs: 30,
                background_cpu_warn_percent: 10,
                background_cpu_warn_duration_secs: 30,
                background_cpu_critical_percent: 25,
                background_cpu_critical_duration_secs: 15,
                warm_timeout_secs: 300,
                pressure_hysteresis: 0.05,
                eviction_grace_period_ms: 1500,
                max_unload_state_bytes: 65536, // 64 KB
            },
            DeviceClass::Standard => Self {
                device_class: class,
                max_hot_webviews: 3,
                max_warm_webviews: 4,
                total_app_memory_budget_bytes: 3 * 1024 * 1024 * 1024, // 3 GB
                memory_pressure_warn_threshold: 0.80,
                memory_pressure_critical_threshold: 0.90,
                shell_memory_target_bytes: 200 * 1024 * 1024,
                shell_memory_warn_bytes: 250 * 1024 * 1024,
                shell_memory_critical_bytes: 350 * 1024 * 1024,
                per_app_memory_soft_cap_bytes: 300 * 1024 * 1024,
                per_app_memory_hard_cap_bytes: 500 * 1024 * 1024,
                foreground_cpu_warn_percent: 120,
                foreground_cpu_warn_duration_secs: 30,
                background_cpu_warn_percent: 10,
                background_cpu_warn_duration_secs: 30,
                background_cpu_critical_percent: 25,
                background_cpu_critical_duration_secs: 15,
                warm_timeout_secs: 300,
                pressure_hysteresis: 0.05,
                eviction_grace_period_ms: 1500,
                max_unload_state_bytes: 65536,
            },
            DeviceClass::Performance => Self {
                device_class: class,
                max_hot_webviews: 4,
                max_warm_webviews: 6,
                total_app_memory_budget_bytes: 5 * 1024 * 1024 * 1024, // 5 GB
                memory_pressure_warn_threshold: 0.85,
                memory_pressure_critical_threshold: 0.92,
                shell_memory_target_bytes: 200 * 1024 * 1024,
                shell_memory_warn_bytes: 250 * 1024 * 1024,
                shell_memory_critical_bytes: 350 * 1024 * 1024,
                per_app_memory_soft_cap_bytes: 300 * 1024 * 1024,
                per_app_memory_hard_cap_bytes: 500 * 1024 * 1024,
                foreground_cpu_warn_percent: 120,
                foreground_cpu_warn_duration_secs: 30,
                background_cpu_warn_percent: 10,
                background_cpu_warn_duration_secs: 30,
                background_cpu_critical_percent: 25,
                background_cpu_critical_duration_secs: 15,
                warm_timeout_secs: 300,
                pressure_hysteresis: 0.05,
                eviction_grace_period_ms: 1500,
                max_unload_state_bytes: 65536,
            },
        }
    }
}

/// Bridge (IPC) rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BridgeLimitsConfig {
    /// Maximum payload size per request (bytes)
    pub max_payload_bytes: usize,

    /// Sustained requests per second (token bucket refill rate)
    pub rate_limit_sustained_rps: u32,

    /// Burst capacity (token bucket size)
    pub rate_limit_burst: u32,

    /// Maximum concurrent in-flight requests
    pub max_concurrent_requests: usize,
}

impl Default for BridgeLimitsConfig {
    fn default() -> Self {
        Self {
            max_payload_bytes: 256 * 1024, // 256 KB
            rate_limit_sustained_rps: 50,
            rate_limit_burst: 200,
            max_concurrent_requests: 16,
        }
    }
}

/// Storage quota configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StorageLimitsConfig {
    /// Installed package size limits (bytes)
    pub package_soft_limit_bytes: u64,
    pub package_hard_limit_bytes: u64,

    /// Default runtime data quota per app (bytes)
    pub default_runtime_quota_bytes: u64,

    /// Storage warning threshold (0.0-1.0)
    pub storage_warn_threshold: f64,

    /// Per-app log retention (bytes)
    pub per_app_log_limit_bytes: u64,

    /// Global log retention (bytes)
    pub global_log_limit_bytes: u64,
}

impl Default for StorageLimitsConfig {
    fn default() -> Self {
        Self {
            package_soft_limit_bytes: 75 * 1024 * 1024,  // 75 MB
            package_hard_limit_bytes: 150 * 1024 * 1024, // 150 MB
            default_runtime_quota_bytes: 256 * 1024 * 1024, // 256 MB
            storage_warn_threshold: 0.80,
            per_app_log_limit_bytes: 20 * 1024 * 1024,  // 20 MB
            global_log_limit_bytes: 100 * 1024 * 1024,  // 100 MB
        }
    }
}

/// Resource pressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureLevel {
    /// Resources plentiful, no action needed
    Normal,
    /// Approaching limits, apps should reduce usage
    Constrained,
    /// At or near limits, aggressive reduction required
    Critical,
}

/// Eviction reason for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvictionReason {
    /// Exceeded max warm/hot count
    CountLimit { state: String, current: usize, max: usize },
    /// Exceeded warm timeout
    WarmTimeout { warm_duration_secs: u64 },
    /// System memory pressure
    MemoryPressure { level: PressureLevel, system_usage: f64 },
    /// Per-app memory exceeded
    AppMemoryExceeded { rss_bytes: u64, limit_bytes: u64 },
    /// Background CPU exceeded critical threshold
    CpuExceeded { cpu_percent: u32, duration_secs: u64 },
    /// User-initiated close
    UserRequested,
    /// Shell entering low-resource mode
    LowResourceMode,
    /// App crashed or terminated unexpectedly
    Crashed { crash_count: u32 },
}

/// Per-app resource usage snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppResourceUsage {
    pub app_id: String,
    pub session_id: String,
    pub state: String, // "hot" | "warm" | "cold"

    /// Estimated RSS in bytes (best effort)
    pub estimated_rss_bytes: u64,

    /// CPU usage (rolling averages, percentage of one core)
    pub cpu_percent_5s: f32,
    pub cpu_percent_30s: f32,

    /// Bridge statistics
    pub bridge_requests_total: u64,
    pub bridge_requests_last_minute: u32,
    pub bridge_bytes_in: u64,
    pub bridge_bytes_out: u64,
    pub bridge_errors: u64,

    /// Lifecycle metrics
    pub eviction_count: u32,
    pub last_eviction_reason: Option<EvictionReason>,
    pub crash_count: u32,
    pub launch_time_ms: u64,

    /// Time tracking
    pub last_active: String, // ISO 8601
    pub created_at: String,  // ISO 8601
    pub time_in_hot_secs: u64,
    pub time_in_warm_secs: u64,
    pub time_in_cold_secs: u64,

    /// Storage usage
    pub storage_used_bytes: u64,
    pub storage_quota_bytes: u64,
}

/// Global resource snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResourceSnapshot {
    pub timestamp: String, // ISO 8601
    pub device_class: DeviceClass,

    /// Shell metrics
    pub shell_rss_bytes: u64,
    pub shell_cpu_percent: f32,

    /// App aggregate metrics
    pub total_app_rss_bytes: u64,
    pub total_app_cpu_percent: f32,
    pub hot_count: usize,
    pub warm_count: usize,
    pub cold_count: usize,

    /// Pressure state
    pub pressure_level: PressureLevel,
    pub system_memory_usage: f64,
    pub evictions_last_minute: u32,

    /// Low-resource mode
    pub low_resource_mode_active: bool,
}

/// Persisted state when app is evicted to Cold
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PersistedAppState {
    pub app_id: String,
    pub session_id: String,
    pub evicted_at: String, // ISO 8601
    pub eviction_reason: EvictionReason,

    /// Shell-captured state (always persisted)
    pub last_url: String,
    pub geometry: WindowGeometry,
    pub scroll_position: Option<ScrollPosition>,

    /// App-provided state (from prepare_for_unload, size-capped)
    pub app_state_blob: Option<Vec<u8>>,
    pub app_state_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WindowGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScrollPosition {
    pub scroll_x: f64,
    pub scroll_y: f64,
}
```

---

## Device Class Detection and Defaults

### Detection Algorithm

```rust
impl ResourceManager {
    pub fn detect_device_class() -> DeviceClass {
        let ram_bytes = get_physical_memory();
        DeviceClass::from_physical_ram(ram_bytes)
    }
}

#[cfg(target_os = "windows")]
fn get_physical_memory() -> u64 {
    use windows::Win32::System::SystemInformation::{
        GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };

    unsafe {
        GlobalMemoryStatusEx(&mut status).ok();
    }

    status.ullTotalPhys
}

#[cfg(target_os = "macos")]
fn get_physical_memory() -> u64 {
    use std::process::Command;

    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok();

    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(8 * 1024 * 1024 * 1024) // Default 8GB
}

#[cfg(target_os = "linux")]
fn get_physical_memory() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("MemTotal:"))
                .and_then(|line| {
                    line.split_whitespace()
                        .nth(1)
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|kb| kb * 1024)
                })
        })
        .unwrap_or(8 * 1024 * 1024 * 1024)
}
```

### Device Class Defaults Summary

| Physical RAM | Device Class | Max Hot | Max Warm | Total Budget | Warn | Critical |
|---:|---|---:|---:|---:|---|---|
| <= 8 GB | Constrained | 2 | 2 | 1.5 GB | 0.75 | 0.85 |
| 16 GB | Standard | 3 | 4 | 3.0 GB | 0.80 | 0.90 |
| >= 32 GB | Performance | 4 | 6 | 5.0 GB | 0.85 | 0.92 |

### Shell Budget

| Metric | Target | Warn | Critical Action |
|--------|--------|------|-----------------|
| Shell RSS | <= 200 MB | 250 MB | 350 MB: Enter low-resource mode |

---

## Per-App Resource Limits

### Memory Limits

| Limit | Value | Action |
|-------|-------|--------|
| Soft cap | 300 MB RSS | Emit warning, deprioritize in eviction |
| Hard cap | 500 MB RSS | Begin eviction countdown if not focused; if focused, show memory warning banner |

### CPU Limits

| Context | Threshold | Duration | Action |
|---------|-----------|----------|--------|
| Foreground (focused) | > 120% one core | > 30s | Warn user |
| Background (warm) | > 10% one core | > 30s | Warn, consider eviction |
| Background critical | > 25% one core | > 15s | Force evict to Cold |

### Bridge (IPC) Limits

| Limit | Value | Enforcement |
|-------|-------|-------------|
| Payload max | 256 KB | Hard reject |
| Sustained rate | 50 req/s | Token bucket (refill) |
| Burst | 200 requests | Token bucket (capacity) |
| Concurrent in-flight | 16 | Backpressure |

---

## Storage Quota System

### Quota Categories

| Category | Soft Limit | Hard Limit | Notes |
|----------|------------|------------|-------|
| Installed package (UI bundle) | 75 MB | 150 MB | Reject install unless developer mode |
| Runtime data (IndexedDB/localStorage) | - | 256 MB default | Expandable via quota request |
| Per-app logs | - | 20 MB | Rotating |
| Global logs | - | 100 MB | Rotating |

### Storage Events

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StorageUsage {
    pub app_id: String,
    pub used_bytes: u64,
    pub quota_bytes: u64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageEvent {
    /// Approaching quota (>80%)
    QuotaWarning { usage: StorageUsage },
    /// At quota (100%)
    QuotaExceeded { usage: StorageUsage },
    /// Quota increased via user approval
    QuotaIncreased { old_quota: u64, new_quota: u64 },
}
```

---

## LRU Eviction Algorithm

### State Machine

```
                  ┌─────────────┐
                  │    Cold     │
                  │ (destroyed) │
                  └──────▲──────┘
                         │ Eviction
                         │ (warm_timeout OR memory_pressure)
                  ┌──────┴──────┐
          show()  │    Warm     │  hide()
      ┌──────────►│  (hidden)   │◄──────────┐
      │           └──────▲──────┘           │
      │                  │                  │
      │                  │ count_limit      │
      │           ┌──────┴──────┐           │
      │           │     Hot     │           │
      │           │  (visible)  │           │
      │           └─────────────┘           │
      │                                     │
      └─────────────────────────────────────┘
```

### Eviction Triggers (Ordered by Priority)

1. **Count-based**
   - If `hot_count > max_hot`: Demote LRU hot (excluding focused) to Warm
   - If `warm_count > max_warm`: Evict LRU warm to Cold

2. **Time-based**
   - If warm for `> warm_timeout_secs` (default 300s): Warm to Cold

3. **Memory-pressure**
   - If `system_pressure >= warn_threshold`: Begin soft eviction (Warm to Cold)
   - If `system_pressure >= critical_threshold`: Hard eviction (all warm; demote non-focused hot)

### Hysteresis

Eviction continues until pressure falls below `warn_threshold - hysteresis` (default 0.05) to prevent thrashing.

### Eviction Candidate Selection

```rust
/// Eviction candidate with computed score
#[derive(Debug, Clone)]
pub struct EvictionCandidate {
    pub app_id: String,
    pub state: String,  // "hot" | "warm"
    pub last_active: Instant,
    pub estimated_rss_bytes: u64,
    pub created_at: Instant,
    pub is_focused: bool,
    pub is_pinned: bool,
}

impl EvictionCandidate {
    /// Compute eviction priority (lower = evict first)
    /// Returns None if candidate should never be evicted
    pub fn eviction_score(&self, under_memory_pressure: bool) -> Option<(u8, u128, u64, u128)> {
        // Never evict focused or pinned
        if self.is_focused || self.is_pinned {
            return None;
        }

        // State priority: Warm=1, Hot=2
        let state_priority = match self.state.as_str() {
            "warm" => 1u8,
            "hot" => 2u8,
            _ => return None, // Cold or unknown
        };

        // LRU: older last_active = higher elapsed = evict first
        let lru_score = self.last_active.elapsed().as_nanos();

        // Memory: larger = evict first (only under pressure)
        let memory_score = if under_memory_pressure {
            u64::MAX - self.estimated_rss_bytes
        } else {
            u64::MAX
        };

        // Creation time tiebreaker: older = evict first
        let creation_score = self.created_at.elapsed().as_nanos();

        Some((state_priority, lru_score, memory_score, creation_score))
    }
}

impl ResourceManager {
    /// Select next eviction candidate
    pub fn select_eviction_candidate(
        &self,
        under_memory_pressure: bool,
    ) -> Option<String> {
        let mut candidates: Vec<EvictionCandidate> = self
            .apps
            .values()
            .filter_map(|app| self.to_eviction_candidate(app))
            .collect();

        // Sort by eviction score (ascending = evict first)
        candidates.sort_by(|a, b| {
            let score_a = a.eviction_score(under_memory_pressure);
            let score_b = b.eviction_score(under_memory_pressure);
            score_a.cmp(&score_b)
        });

        candidates.first().map(|c| c.app_id.clone())
    }
}
```

### Selection Criteria (Lexicographic)

1. **Never evict**: Focused webview, shell, pinned apps
2. **State priority**: Warm (1) before Hot (2)
3. **LRU**: Oldest `last_active` first
4. **Memory-aware** (under pressure): Largest RSS first
5. **Tiebreaker**: Oldest `created_at` first

### Graceful Eviction Handshake

```
    ResourceManager                    App Webview
          │                                │
          │ ─── app://resource/evicting ──►│
          │     { deadline_ms: 1500,       │
          │       reason: ... }            │
          │                                │
          │◄── resource.prepare_for_unload │
          │    (optional, ≤64KB, timeboxed)│
          │                                │
          │ [After deadline OR response]   │
          │                                │
          │ ─── Destroy webview ──────────►│
          │                                │
```

### Persistence on Eviction

Shell persists (regardless of app cooperation):
- Window geometry (`x`, `y`, `width`, `height`)
- Last URL (always `postapp://{app_id}/...`)
- Scroll position (if capturable)
- Session ID for bookkeeping

App-provided blob (via `prepare_for_unload`):
- Max 64 KB
- Timeboxed to grace period (1500ms)
- Stored encrypted/integrity-protected

---

## Metrics Collection

### Per-App Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `state` | enum | hot/warm/cold |
| `estimated_rss_bytes` | u64 | Memory usage (best effort) |
| `cpu_percent_5s` | f32 | 5-second rolling average |
| `cpu_percent_30s` | f32 | 30-second rolling average |
| `bridge_invoke_rate` | f32 | Requests per second |
| `bridge_bytes_in` | u64 | Total bytes received |
| `bridge_bytes_out` | u64 | Total bytes sent |
| `bridge_errors` | u64 | Total error count |
| `eviction_count` | u32 | Times evicted |
| `last_eviction_reason` | enum | Why last evicted |
| `crash_count` | u32 | Abnormal terminations |
| `launch_time_ms` | u64 | Cold start latency |
| `time_in_hot_secs` | u64 | Total time in hot state |
| `time_in_warm_secs` | u64 | Total time in warm state |

### Global Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `shell_rss_bytes` | u64 | Shell memory usage |
| `shell_cpu_percent` | f32 | Shell CPU usage |
| `total_app_rss_bytes` | u64 | Sum of all app memory |
| `total_app_cpu_percent` | f32 | Sum of all app CPU |
| `hot_count` | usize | Number of hot webviews |
| `warm_count` | usize | Number of warm webviews |
| `cold_count` | usize | Number of cold apps |
| `pressure_level` | enum | normal/constrained/critical |
| `evictions_per_minute` | f32 | Eviction throughput |

### OpenTelemetry Spans

| Span Name | Attributes | Purpose |
|-----------|------------|---------|
| `app.launch` | `app_id`, `cold_start`, `duration_ms` | Launch timing |
| `webview.create` | `app_id`, `platform`, `duration_ms` | Webview creation |
| `protocol.serve_file` | `app_id`, `path`, `size_bytes`, `duration_ms` | Protocol handler |
| `eviction.run` | `app_id`, `reason`, `target_state`, `graceful` | Eviction tracking |
| `resource.pressure_changed` | `level`, `system_usage`, `app_count` | Pressure transitions |

### Thrash Detection

```rust
#[derive(Debug, Clone)]
pub struct ThrashEvent {
    pub app_id: String,
    pub evict_relaunch_within_secs: u64,
    pub occurred_at: Instant,
}

impl ResourceManager {
    /// Detect if app is being thrashed (evict/relaunch cycle)
    pub fn detect_thrash(&self, app_id: &str) -> bool {
        let recent_evictions = self
            .eviction_log
            .iter()
            .filter(|e| e.app_id == app_id)
            .filter(|e| e.occurred_at.elapsed().as_secs() < 300)
            .count();

        recent_evictions >= 3
    }
}
```

---

## Pressure Signaling Protocol

### Event: `app://resource/pressure`

Emitted to each app when pressure state changes.

```typescript
interface ResourcePressureEvent {
  level: 'normal' | 'constrained' | 'critical';
  signals: {
    memory: {
      system_usage: number;      // 0.0-1.0
      app_rss_bytes: number;     // This app's RSS
      total_rss_bytes: number;   // All apps RSS
    };
    cpu: {
      app_percent: number;       // This app's CPU
      sustained_high: boolean;   // Above threshold for duration
    };
    storage: {
      used_bytes: number;
      quota_bytes: number;
      usage_percent: number;
    };
  };
  budgets: {
    memory_soft_bytes: number;
    memory_hard_bytes: number;
    storage_quota_bytes: number;
  };
  recommended_actions: string[];
}
```

### Recommended Actions by Level

| Level | `recommended_actions` |
|-------|----------------------|
| `constrained` | `["clearCaches", "reduceAnimations", "stopPolling"]` |
| `critical` | `["persistDrafts", "releaseBuffers", "stopTimers", "prepareForEviction"]` |

### Event: `app://resource/evicting`

Emitted immediately before eviction begins.

```typescript
interface EvictingEvent {
  deadline_ms: number;         // Time to respond (1500ms)
  reason: EvictionReason;
  target_state: 'cold';        // Always cold for now
}
```

### Broadcast vs Targeted

| Scenario | Event Target |
|----------|--------------|
| System pressure rises | Broadcast to all apps |
| Single app breaches per-app threshold | Targeted to that app |
| Pre-eviction warning | Targeted to app being evicted |

### Expected App Behaviors

| Event | Expected Response |
|-------|------------------|
| `constrained` | Reduce caches, stop non-essential work, lower animation rate |
| `critical` | Persist state, release large buffers, stop background timers |
| `evicting` | Return quickly from `prepare_for_unload`; no blocking operations |

---

## Shell Commands (Rust)

### Introspection Commands

```rust
/// Get global resource snapshot
#[tauri::command]
pub async fn shell_get_resource_snapshot(
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<ResourceSnapshot, String> {
    verify_shell_only(&webview)?;
    Ok(state.resource_manager.get_snapshot())
}

/// Get per-app resource usage
#[tauri::command]
pub async fn shell_get_app_resource_usage(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
) -> Result<AppResourceUsage, String> {
    verify_shell_only(&webview)?;
    state.resource_manager.get_app_usage(&app_id)
        .ok_or_else(|| "App not found".to_string())
}

/// Get storage usage for app
#[tauri::command]
pub async fn shell_get_storage_usage(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
) -> Result<StorageUsage, String> {
    verify_shell_only(&webview)?;
    state.storage_manager.get_usage(&app_id)
        .ok_or_else(|| "App not found".to_string())
}
```

### Policy Configuration Commands

```rust
/// Patch resource limits (partial update)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResourceLimitsPatch {
    pub max_hot_webviews: Option<usize>,
    pub max_warm_webviews: Option<usize>,
    pub warm_timeout_secs: Option<u64>,
    pub per_app_memory_soft_cap_bytes: Option<u64>,
    pub per_app_memory_hard_cap_bytes: Option<u64>,
}

#[tauri::command]
pub async fn shell_set_resource_limits(
    webview: Webview,
    state: State<'_, AppState>,
    patch: ResourceLimitsPatch,
) -> Result<(), String> {
    verify_shell_only(&webview)?;
    state.resource_manager.apply_limits_patch(patch);
    Ok(())
}

/// App priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppPriority {
    /// Never auto-evict (user pinned)
    Pinned,
    /// Normal eviction rules
    Normal,
    /// Evict first when under pressure
    Background,
}

#[tauri::command]
pub async fn shell_set_app_priority(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
    priority: AppPriority,
) -> Result<(), String> {
    verify_shell_only(&webview)?;
    state.resource_manager.set_app_priority(&app_id, priority);
    Ok(())
}

#[tauri::command]
pub async fn shell_set_storage_quota(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
    quota_bytes: u64,
) -> Result<(), String> {
    verify_shell_only(&webview)?;
    state.storage_manager.set_quota(&app_id, quota_bytes);
    Ok(())
}
```

### Control Commands

```rust
/// Target state for manual eviction
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvictionTargetState {
    Warm,
    Cold,
}

#[tauri::command]
pub async fn shell_evict_app(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
    target_state: EvictionTargetState,
    reason: String,
) -> Result<(), String> {
    verify_shell_only(&webview)?;
    state.resource_manager.evict_app(
        &app_id,
        target_state,
        EvictionReason::UserRequested,
    ).await
}

/// Scope for data clearing
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearDataScope {
    Cache,
    Storage,
    All,
}

#[tauri::command]
pub async fn shell_clear_app_data(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
    scope: ClearDataScope,
) -> Result<(), String> {
    verify_shell_only(&webview)?;
    state.storage_manager.clear_data(&app_id, scope).await
}

#[tauri::command]
pub async fn shell_enter_low_resource_mode(
    webview: Webview,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    verify_shell_only(&webview)?;
    state.resource_manager.set_low_resource_mode(enabled).await;
    Ok(())
}
```

### Shell Events (Rust to Shell UI)

| Event | Payload | Description |
|-------|---------|-------------|
| `shell://resources/pressure_changed` | `{ level, system_usage }` | Pressure level changed |
| `shell://resources/snapshot_changed` | `ResourceSnapshot` | Throttled (1Hz) state update |
| `shell://resources/eviction` | `{ app_id, reason, timestamp }` | Eviction audit entry |
| `shell://resources/low_resource_mode` | `{ enabled, reason }` | Low-resource mode toggled |

---

## App Bridge APIs

### TypeScript Interfaces

```typescript
// Resource namespace in bridge API
interface ResourceBudget {
  memory_soft_bytes: number;
  memory_hard_bytes: number;
  storage_quota_bytes: number;
  storage_used_bytes: number;
  current_level: 'normal' | 'constrained' | 'critical';
}

interface QuotaIncreaseRequest {
  bytes: number;
  reason: string;
}

interface QuotaIncreaseResponse {
  approved: boolean;
  new_quota_bytes?: number;
  denial_reason?: string;
}

interface PrepareForUnloadResponse {
  state_blob: Uint8Array;  // Max 64KB
}
```

### Bridge Methods

```typescript
// @post-urbit/sdk resource namespace

/**
 * Get current resource budget for this app
 */
async function getBudget(): Promise<ResourceBudget>;

/**
 * Subscribe to pressure events
 */
function onPressure(
  callback: (event: ResourcePressureEvent) => void
): () => void;  // Returns unsubscribe function

/**
 * Prepare state for unload (called during eviction grace period)
 * Must complete within deadline_ms (typically 1500ms)
 * Returns max 64KB of state to restore on relaunch
 */
async function prepareForUnload(): Promise<PrepareForUnloadResponse>;

/**
 * Request storage quota increase (prompts user)
 */
async function requestQuotaIncrease(
  request: QuotaIncreaseRequest
): Promise<QuotaIncreaseResponse>;

/**
 * Get current storage usage
 */
async function getStorageUsage(): Promise<{
  used_bytes: number;
  quota_bytes: number;
}>;
```

### SDK Implementation

```typescript
// @post-urbit/sdk/resource.ts

import { bridge } from './bridge';

export const resource = {
  async getBudget(): Promise<ResourceBudget> {
    return bridge.call('resource.get_budget');
  },

  onPressure(callback: (event: ResourcePressureEvent) => void): () => void {
    return bridge.subscribe('resource.pressure', callback);
  },

  async prepareForUnload(): Promise<PrepareForUnloadResponse> {
    // App implements this handler
    const handler = (window as any).__postUrbitPrepareForUnload;
    if (typeof handler === 'function') {
      const blob = await handler();
      if (blob && blob.byteLength > 65536) {
        console.warn('prepareForUnload blob exceeds 64KB, truncating');
        return { state_blob: blob.slice(0, 65536) };
      }
      return { state_blob: blob || new Uint8Array() };
    }
    return { state_blob: new Uint8Array() };
  },

  async requestQuotaIncrease(
    request: QuotaIncreaseRequest
  ): Promise<QuotaIncreaseResponse> {
    return bridge.call('resource.request_quota_increase', request);
  },

  async getStorageUsage(): Promise<{ used_bytes: number; quota_bytes: number }> {
    return bridge.call('resource.get_storage_usage');
  },
};
```

### React Hooks

```typescript
// @post-urbit/sdk/react/useResourcePressure.ts

import { useState, useEffect } from 'react';
import { resource, ResourcePressureEvent, PressureLevel } from '../resource';

export function useResourcePressure() {
  const [level, setLevel] = useState<PressureLevel>('normal');
  const [event, setEvent] = useState<ResourcePressureEvent | null>(null);

  useEffect(() => {
    const unsubscribe = resource.onPressure((e) => {
      setLevel(e.level);
      setEvent(e);
    });
    return unsubscribe;
  }, []);

  return { level, event };
}

// @post-urbit/sdk/react/useStorageQuota.ts

import { useState, useEffect, useCallback } from 'react';
import { resource } from '../resource';

export function useStorageQuota() {
  const [usage, setUsage] = useState<{ used: number; quota: number } | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    resource.getStorageUsage().then((u) => {
      setUsage({ used: u.used_bytes, quota: u.quota_bytes });
      setLoading(false);
    });
  }, []);

  const requestIncrease = useCallback(async (bytes: number, reason: string) => {
    const result = await resource.requestQuotaIncrease({ bytes, reason });
    if (result.approved && result.new_quota_bytes) {
      setUsage((prev) => prev ? { ...prev, quota: result.new_quota_bytes! } : null);
    }
    return result;
  }, []);

  return { usage, loading, requestIncrease };
}
```

---

## Platform-Specific Considerations

### Windows (WebView2)

| Aspect | Behavior | Implementation |
|--------|----------|----------------|
| Process model | Browser + renderer + GPU processes | Sum RSS across all WebView2-associated PIDs |
| Memory signal | `GlobalMemoryStatusEx` | System commit + working set |
| Warm optimization | `TrySuspend()` API | Materially reduces CPU/memory for hidden webviews |
| Hard enforcement | Job Objects | Can cap CPU/memory if child processes are reliably enumerated |
| Termination | `EXCEPTION_ACCESS_VIOLATION` handling | Detect and handle unexpected termination |

### macOS (WKWebView)

| Aspect | Behavior | Implementation |
|--------|----------|----------------|
| Process model | Native multi-process (jetsam) | OS may terminate under memory pressure |
| Memory signal | `vm_statistics64` / `host_statistics64` | System pressure + jetsam events |
| Termination | `webContentProcessDidTerminate` delegate | Must handle as normal lifecycle path |
| Memory accounting | Shared processes | Prefer system pressure over per-webview hard caps |
| Warm behavior | May not free as much as expected | Consider lower warm counts on older Macs |

### Linux (WebKitGTK)

| Aspect | Behavior | Implementation |
|--------|----------|----------------|
| Process model | Multi-process (WebKit2) | Read `/proc/[pid]/statm` for RSS |
| Memory signal | `/proc/meminfo` | `MemAvailable` / `MemTotal` |
| Hard enforcement | cgroups v2 (optional) | `cpu.max` / `memory.high` if supported |
| Correlation | Subprocess to webview | Less direct than WebView2 |
| Variance | Distro/version differences | Test across target distros in CI |

### Platform Memory Reading

```rust
#[cfg(target_os = "windows")]
fn get_process_rss(pid: u32) -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ
    };

    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        ).ok()?;

        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ).ok()?;

        Some(counters.WorkingSetSize as u64)
    }
}

#[cfg(target_os = "macos")]
fn get_process_rss(pid: i32) -> Option<u64> {
    use std::process::Command;

    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;

    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}

#[cfg(target_os = "linux")]
fn get_process_rss(pid: i32) -> Option<u64> {
    let statm = std::fs::read_to_string(format!("/proc/{}/statm", pid)).ok()?;
    let rss_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 };
    Some(rss_pages * page_size)
}
```

---

## Low-Resource Mode

### Trigger Conditions

- Shell RSS exceeds `shell_memory_critical_bytes` (350MB)
- System pressure exceeds critical threshold for > 60s
- Manual activation via `shell_enter_low_resource_mode(true)`

### Low-Resource Mode Behavior

| Change | Normal Mode | Low-Resource Mode |
|--------|-------------|-------------------|
| Max hot webviews | Device default | 1 |
| Warm pool | Enabled | Disabled (all evicted) |
| Warm timeout | 300s | N/A (immediate eviction) |
| Animations | Normal | Reduced motion |
| Background refresh | Enabled | Disabled |

### Recovery

Exit low-resource mode when:
- System pressure < `warn_threshold - hysteresis` for > 30s
- Shell RSS < `shell_memory_warn_bytes` for > 30s
- Manual deactivation

---

## Acceptance Criteria

### Resource Detection

- [ ] Device class correctly detected on Windows, macOS, Linux
- [ ] Defaults applied based on device class
- [ ] Settings persist across restarts

### Memory Management

- [ ] Shell RSS stays under 200MB target in normal operation
- [ ] Per-app memory soft cap (300MB) triggers warning
- [ ] Per-app memory hard cap (500MB) triggers eviction countdown
- [ ] System pressure thresholds trigger appropriate evictions

### LRU Eviction

- [ ] Count-based eviction maintains max hot/warm limits
- [ ] Time-based eviction triggers after warm timeout
- [ ] Focused webview never evicted
- [ ] Pinned apps never auto-evicted
- [ ] Eviction reason logged with full context

### Graceful Handshake

- [ ] `app://resource/evicting` emitted before eviction
- [ ] Apps have 1500ms to respond with `prepare_for_unload`
- [ ] State blob capped at 64KB
- [ ] Eviction proceeds after deadline regardless of response

### Bridge Limits

- [ ] Payloads > 256KB rejected
- [ ] Rate limiting enforced (50 rps sustained, 200 burst)
- [ ] Concurrent requests capped at 16

### Storage

- [ ] Package install rejected if > 150MB (unless developer mode)
- [ ] Runtime quota enforced (default 256MB)
- [ ] Quota increase requires user approval

### Metrics

- [ ] Per-app metrics collected and queryable
- [ ] Global snapshot available at 1Hz
- [ ] Eviction audit trail maintained

### Platform

- [ ] Memory reading works on all 3 platforms
- [ ] System pressure detection works on all 3 platforms
- [ ] Crash/termination handled as normal lifecycle

---

## Test Cases

### Memory Management

| Test | Setup | Expected Result |
|------|-------|-----------------|
| Soft cap warning | App allocates 350MB | Warning emitted, app deprioritized |
| Hard cap eviction | Non-focused app at 550MB | Eviction countdown started |
| Focused protection | Focused app at 550MB | Warning banner shown, NOT evicted |
| System pressure soft | System at 82% usage (standard device) | Warm apps evicted to cold |
| System pressure critical | System at 92% usage | All warm evicted, hot demoted |

### LRU Eviction

| Test | Setup | Expected Result |
|------|-------|-----------------|
| Hot count limit | 4 hot apps on standard device (max 3) | LRU hot demoted to warm |
| Warm count limit | 5 warm apps on standard device (max 4) | LRU warm evicted to cold |
| Warm timeout | App warm for 301 seconds | App evicted to cold |
| Pinned protection | Pinned app is LRU | Next LRU evicted instead |

### Graceful Handshake

| Test | Setup | Expected Result |
|------|-------|-----------------|
| App responds | App returns 32KB state blob in 500ms | State persisted, eviction proceeds |
| App too slow | App takes 2000ms to respond | Eviction proceeds at 1500ms deadline |
| App returns too much | App returns 128KB blob | Truncated to 64KB |
| App doesn't respond | App has no `prepare_for_unload` handler | Eviction proceeds, no app state saved |

### Bridge Limits

| Test | Setup | Expected Result |
|------|-------|-----------------|
| Payload rejection | Send 512KB request | Request rejected with `PAYLOAD_TOO_LARGE` |
| Rate limiting | Send 300 requests in 1 second | After ~200, requests start failing with `RATE_LIMITED` |
| Backpressure | Fire 20 requests without waiting | 4 queued, rest rejected |

### Platform

| Test | Platform | Expected Result |
|------|----------|-----------------|
| Memory reading | Windows | WebView2 process RSS correctly summed |
| Memory reading | macOS | WKWebView RSS approximated |
| Memory reading | Linux | `/proc/[pid]/statm` parsed correctly |
| Termination | macOS | `webContentProcessDidTerminate` handled gracefully |

---

## Implementation Checklist

### Phase 1: Core Infrastructure

- [ ] Define `ResourceLimitsConfig` with device class defaults
- [ ] Implement device class detection for all platforms
- [ ] Create `ResourceManager` struct with state tracking
- [ ] Add `BridgeLimitsConfig` and token bucket rate limiter
- [ ] Implement platform-specific memory reading

### Phase 2: Eviction Engine

- [ ] Implement eviction candidate scoring
- [ ] Add count-based eviction trigger
- [ ] Add time-based eviction trigger
- [ ] Add memory-pressure eviction trigger
- [ ] Implement hysteresis logic
- [ ] Add graceful eviction handshake with deadline

### Phase 3: State Persistence

- [ ] Define `PersistedAppState` schema
- [ ] Capture window geometry on eviction
- [ ] Capture scroll position on eviction
- [ ] Store app-provided state blob (encrypted)
- [ ] Restore state on app relaunch

### Phase 4: Shell Integration

- [ ] Implement shell introspection commands
- [ ] Implement shell policy commands
- [ ] Implement shell control commands
- [ ] Add shell events for pressure/eviction
- [ ] Create resource dashboard UI

### Phase 5: App Bridge

- [ ] Implement `resource.get_budget` bridge method
- [ ] Implement `resource.on_pressure` subscription
- [ ] Implement `resource.prepare_for_unload` handler
- [ ] Implement `resource.request_quota_increase` with prompt
- [ ] Create SDK React hooks

### Phase 6: Metrics & Observability

- [ ] Collect per-app metrics
- [ ] Collect global metrics
- [ ] Add OpenTelemetry spans
- [ ] Implement thrash detection
- [ ] Create diagnostics export

### Phase 7: Platform Testing

- [ ] Test on Windows with WebView2
- [ ] Test on macOS with WKWebView
- [ ] Test on Linux with WebKitGTK
- [ ] Verify memory reading accuracy
- [ ] Verify crash containment

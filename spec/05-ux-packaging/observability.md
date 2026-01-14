# Observability

## Overview

Observability covers logging, metrics, health checks, and alerting for the node daemon. The goal is enabling operators to understand node behavior and diagnose issues.

## Logging

### Log Levels

| Level | Usage | Example |
|-------|-------|---------|
| `error` | Failures requiring attention | "Failed to connect to relay" |
| `warn` | Unexpected but handled | "Retrying DHT lookup" |
| `info` | Significant events | "App installed: com.example.app" |
| `debug` | Detailed operation | "Processing message from k5xq7z4m..." |
| `trace` | Very verbose | "QUIC packet received, 1234 bytes" |

### Log Format

**Structured JSON (default for production):**
```json
{
  "timestamp": "2025-01-15T12:00:00.123Z",
  "level": "info",
  "target": "postnode::messaging",
  "message": "Message delivered",
  "fields": {
    "message_id": "abc123",
    "recipient": "k5xq7z4m...",
    "size_bytes": 1234,
    "duration_ms": 45
  },
  "span": {
    "name": "send_message",
    "id": "span-123"
  }
}
```

**Human-readable (for development):**
```
2025-01-15T12:00:00.123Z  INFO postnode::messaging: Message delivered message_id=abc123 recipient=k5xq7z4m... size_bytes=1234 duration_ms=45
```

### Log Configuration

```toml
[logging]
# Log level (error, warn, info, debug, trace)
level = "info"

# Per-module overrides
[logging.modules]
"postnode::transport" = "debug"
"postnode::apps" = "info"
"quinn" = "warn"  # External dependency

# Output format
format = "json"  # or "text"

# Output destination
[logging.output]
type = "file"  # or "stdout", "stderr", "syslog"
path = "/var/log/postnode/postnode.log"

# Rotation
[logging.rotation]
max_size_mb = 100
max_files = 10
compress = true
```

### Log Categories

| Category | Target | Content |
|----------|--------|---------|
| Core | `postnode::core` | Startup, shutdown, config |
| Identity | `postnode::identity` | Key operations, document updates |
| Transport | `postnode::transport` | Connections, NAT, relays |
| Messaging | `postnode::messaging` | Send/receive, encryption |
| Sync | `postnode::sync` | Document sync, CRDT ops |
| Apps | `postnode::apps` | Install, run, permissions |
| Admin | `postnode::admin` | API calls, auth |
| HTTP | `postnode::http` | Request/response |

### Sensitive Data Handling

**Never log:**
- Private keys
- Full message content
- Passwords/tokens

**Redact or hash:**
- IIDs (show first 8 chars: `k5xq7z4m...`)
- Message IDs (show first 8 chars)
- IP addresses (configurable)

```toml
[logging.privacy]
redact_iids = true
redact_ips = true
redact_message_content = true
```

## Metrics

### Prometheus Metrics

The node exposes Prometheus-format metrics at `/metrics`.

### Metric Categories

#### Node Metrics

```prometheus
# Node uptime
postnode_uptime_seconds{} 86400

# Node info
postnode_info{version="1.0.0", iid="k5xq7z4m..."} 1

# Resource usage
postnode_memory_bytes{type="heap"} 134217728
postnode_memory_bytes{type="resident"} 268435456
postnode_cpu_seconds_total{} 1234.56
postnode_open_file_descriptors{} 156
```

#### Transport Metrics

```prometheus
# Connection counts
postnode_connections_total{type="direct"} 42
postnode_connections_total{type="relay"} 5
postnode_connections_active{} 47

# Connection events
postnode_connection_events_total{event="opened"} 1234
postnode_connection_events_total{event="closed"} 1187
postnode_connection_events_total{event="failed"} 23

# Bandwidth
postnode_bytes_sent_total{} 1234567890
postnode_bytes_received_total{} 9876543210

# Latency histogram
postnode_connection_latency_seconds_bucket{le="0.01"} 100
postnode_connection_latency_seconds_bucket{le="0.05"} 450
postnode_connection_latency_seconds_bucket{le="0.1"} 480
postnode_connection_latency_seconds_bucket{le="0.5"} 495
postnode_connection_latency_seconds_bucket{le="1"} 500
postnode_connection_latency_seconds_bucket{le="+Inf"} 500
```

#### Messaging Metrics

```prometheus
# Message counts
postnode_messages_sent_total{type="direct"} 1234
postnode_messages_sent_total{type="group"} 567
postnode_messages_received_total{type="direct"} 2345
postnode_messages_received_total{type="group"} 678

# Message processing time
postnode_message_processing_seconds_bucket{operation="encrypt", le="0.001"} 1000
postnode_message_processing_seconds_bucket{operation="decrypt", le="0.001"} 950

# Queue depths
postnode_message_queue_depth{queue="outgoing"} 5
postnode_message_queue_depth{queue="incoming"} 0
```

#### App Runtime Metrics

```prometheus
# App counts
postnode_apps_installed_total{} 12
postnode_apps_running{} 3

# App invocations
postnode_app_invocations_total{app_id="com.example.app", type="user_action"} 456
postnode_app_invocations_total{app_id="com.example.app", type="background"} 123

# App resource usage
postnode_app_memory_bytes{app_id="com.example.app"} 16777216
postnode_app_storage_bytes{app_id="com.example.app"} 52428800
postnode_app_fuel_consumed_total{app_id="com.example.app"} 987654321

# App errors
postnode_app_errors_total{app_id="com.example.app", error="timeout"} 2
postnode_app_errors_total{app_id="com.example.app", error="permission_denied"} 0
```

#### Storage Metrics

```prometheus
# Database sizes
postnode_storage_bytes{database="identity"} 1048576
postnode_storage_bytes{database="messages"} 104857600
postnode_storage_bytes{database="sync"} 52428800
postnode_storage_bytes{database="apps"} 209715200

# Database operations
postnode_db_operations_total{database="messages", operation="read"} 12345
postnode_db_operations_total{database="messages", operation="write"} 6789

# Database latency
postnode_db_operation_seconds_bucket{database="messages", operation="read", le="0.001"} 12000
```

### Metrics Configuration

```toml
[metrics]
enabled = true
listen_addr = "127.0.0.1:9090"  # Or expose via /metrics on main HTTP
path = "/metrics"

# Authentication (optional)
require_auth = false
# auth_token_hash = "<sha256-hash>"

# Metric selection
[metrics.collectors]
node = true
transport = true
messaging = true
apps = true
storage = true
go_runtime = true  # If using Go
```

## Health Checks

### Endpoints

| Endpoint | Purpose | Checks |
|----------|---------|--------|
| `/health/live` | Liveness | Process is running |
| `/health/ready` | Readiness | Ready to serve requests |
| `/health` | Detailed | Full health status |

### Liveness Check

```
GET /health/live

Response (200 OK):
{
  "status": "alive"
}

Response (503 Service Unavailable):
{
  "status": "dead",
  "reason": "shutting_down"
}
```

### Readiness Check

```
GET /health/ready

Response (200 OK):
{
  "status": "ready"
}

Response (503 Service Unavailable):
{
  "status": "not_ready",
  "reason": "initializing",
  "details": {
    "identity": "loaded",
    "transport": "starting",
    "messaging": "waiting",
    "apps": "waiting"
  }
}
```

### Detailed Health

```
GET /health

Response (200 OK):
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 86400,
  "checks": {
    "identity": {
      "status": "healthy",
      "iid": "k5xq7z4m...",
      "last_published": "2025-01-15T12:00:00Z"
    },
    "transport": {
      "status": "healthy",
      "connections": 47,
      "relays_connected": 2
    },
    "messaging": {
      "status": "healthy",
      "queue_depth": 5,
      "sessions_active": 23
    },
    "storage": {
      "status": "healthy",
      "disk_used_bytes": 1073741824,
      "disk_free_bytes": 10737418240
    },
    "apps": {
      "status": "healthy",
      "installed": 12,
      "running": 3
    }
  }
}

Response (503 Service Unavailable):
{
  "status": "unhealthy",
  "checks": {
    "storage": {
      "status": "unhealthy",
      "error": "disk_full"
    }
    // ... other checks
  }
}
```

### Health Check Thresholds

```toml
[health]
# Check intervals
check_interval_seconds = 30

# Thresholds for "unhealthy"
[health.thresholds]
disk_free_min_bytes = 104857600  # 100 MB
memory_max_percent = 90
connection_queue_max = 1000
message_queue_max = 10000
```

## Alerting

### Alert Rules

Define alert conditions that trigger notifications:

```toml
[alerts]
enabled = true

[[alerts.rules]]
name = "high_memory"
condition = "memory_percent > 85"
severity = "warning"
message = "Memory usage is high: {{ memory_percent }}%"

[[alerts.rules]]
name = "disk_full"
condition = "disk_free_bytes < 100000000"
severity = "critical"
message = "Disk space low: {{ disk_free_mb }} MB remaining"

[[alerts.rules]]
name = "relay_disconnected"
condition = "relays_connected == 0"
for = "5m"  # Must be true for 5 minutes
severity = "warning"
message = "No relay servers connected"

[[alerts.rules]]
name = "app_crash_loop"
condition = "app_restarts{app_id=~'.*'} > 3"
for = "10m"
severity = "warning"
message = "App {{ app_id }} is crash-looping"
```

### Alert Destinations

```toml
[alerts.destinations]

# Local notification (shown in admin UI)
[[alerts.destinations.local]]
enabled = true

# Email
[[alerts.destinations.email]]
enabled = true
smtp_server = "smtp.example.com:587"
from = "node@example.com"
to = ["admin@example.com"]
auth_user = "node@example.com"
auth_password_env = "SMTP_PASSWORD"

# Webhook
[[alerts.destinations.webhook]]
enabled = true
url = "https://hooks.slack.com/services/..."
method = "POST"
headers = { "Content-Type" = "application/json" }
template = '''
{
  "text": "Alert: {{ name }} - {{ message }}",
  "severity": "{{ severity }}"
}
'''

# PagerDuty
[[alerts.destinations.pagerduty]]
enabled = false
integration_key_env = "PAGERDUTY_KEY"
severity_map = { critical = "critical", warning = "warning" }
```

## Tracing

### Distributed Tracing

For debugging complex flows across components:

```toml
[tracing]
enabled = true
service_name = "postnode"

# OpenTelemetry exporter
[tracing.otlp]
endpoint = "http://localhost:4317"
protocol = "grpc"  # or "http"

# Sampling
[tracing.sampling]
strategy = "probabilistic"
rate = 0.1  # 10% of traces
```

### Trace Context

Traces include:
- Span ID
- Trace ID (for distributed tracing)
- Parent span ID
- Operation name
- Duration
- Tags (IID, message ID, app ID, etc.)
- Events (within span)

### Key Traces

| Operation | Spans |
|-----------|-------|
| Message send | `send_message` → `encrypt` → `deliver` |
| Message receive | `receive_message` → `decrypt` → `process` |
| App invoke | `app_invoke` → `load_instance` → `execute` → `handle_result` |
| Sync operation | `sync_op` → `apply_crdt` → `propagate` |

## Diagnostics

### Diagnostic Dump

Create a diagnostic bundle for troubleshooting:

```bash
postnode diagnostics dump --output diag.tar.gz

# Includes:
# - Recent logs (redacted)
# - Current metrics snapshot
# - Health check output
# - Configuration (sensitive values redacted)
# - Connection state
# - App state
# - System info (OS, memory, disk)
```

### Runtime Inspection

```bash
# Send SIGUSR1 to dump diagnostics to log
kill -USR1 $(pidof postnode)

# Or via CLI
postnode diagnostics status

# Output includes:
# - Active connections
# - Pending operations
# - Memory breakdown
# - Goroutine/task count
# - Open file descriptors
```

### Debug Mode

```toml
[debug]
enabled = false  # Enable for troubleshooting only
pprof_enabled = false  # Go pprof endpoint
heap_dump_on_oom = true
```

## Audit Logging

Separate from operational logs, audit logs track security-relevant events:

```json
{
  "timestamp": "2025-01-15T12:00:00Z",
  "event": "admin_login",
  "actor": {
    "type": "admin",
    "ip": "192.168.1.100",
    "user_agent": "Mozilla/5.0..."
  },
  "result": "success",
  "details": {
    "session_id": "sess-123"
  }
}
```

### Audit Events

| Event | Description |
|-------|-------------|
| `admin_login` | Admin UI login |
| `admin_logout` | Admin UI logout |
| `admin_login_failed` | Failed login attempt |
| `app_install` | App installation |
| `app_uninstall` | App removal |
| `permission_grant` | Permission granted to app |
| `permission_revoke` | Permission revoked |
| `key_rotation` | Identity key rotated |
| `device_add` | Device authorized |
| `device_remove` | Device deauthorized |
| `backup_create` | Backup created |
| `backup_restore` | Backup restored |
| `config_change` | Configuration changed |

### Audit Log Configuration

```toml
[audit]
enabled = true
path = "/var/log/postnode/audit.log"
format = "json"

# Retention
max_size_mb = 100
max_age_days = 365
compress = true

# Events to log (default: all)
events = ["*"]
# Or specific: ["admin_login", "app_install", "key_rotation"]
```

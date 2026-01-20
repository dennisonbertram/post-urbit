# Post-Urbit HTTP API Reference

This document provides a comprehensive reference for the Post-Urbit Node HTTP API. The API is divided into two main services:

1. **Admin API** - Node administration, identity management, contacts, apps, and settings
2. **Mailbox API** - Mailbox message storage and retrieval for inter-node messaging

## Table of Contents

- [Authentication](#authentication)
- [Health and Status](#health-and-status)
- [Identity Endpoints](#identity-endpoints)
- [Contacts Endpoints](#contacts-endpoints)
- [Apps Endpoints](#apps-endpoints)
- [Settings Endpoints](#settings-endpoints)
- [Backups Endpoints](#backups-endpoints)
- [API Keys Endpoints](#api-keys-endpoints)
- [Logs Endpoints](#logs-endpoints)
- [Events (WebSocket)](#events-websocket)
- [Mailbox Endpoints](#mailbox-endpoints)
- [Error Codes](#error-codes)

---

## Authentication

The Admin API supports three authentication methods:

### 1. Admin Token (Bearer Token)

Set via environment variable or configuration. Provides full administrative access.

```
Authorization: Bearer <admin_token>
```

### 2. API Keys

Scoped access tokens with specific permissions. Created via the `/admin/v1/api-keys` endpoint.

```
Authorization: Bearer <api_key_secret>
```

### 3. Session Cookies

Browser-based authentication using password login. Sessions are established via the `/admin/v1/auth/login` endpoint.

**Cookies Set:**
- `postnode_session` - Session identifier (HttpOnly, SameSite=Strict)
- `postnode_csrf` - CSRF token (SameSite=Strict)

**CSRF Protection:**
State-changing requests (POST, PUT, PATCH, DELETE) with cookie authentication require the `X-CSRF-Token` header matching the `postnode_csrf` cookie value.

### Fresh Authentication

Some sensitive operations (key rotation, device management, data clearing) require "fresh authentication" - authentication performed within the last 5 minutes. Use the `/admin/v1/auth/reauth` endpoint to refresh.

### Permissions

API keys can be scoped with the following permissions:

| Permission | Description |
|------------|-------------|
| `read:identity` | Read identity information |
| `write:identity` | Modify identity, rotate keys |
| `read:contacts` | Read contact list |
| `write:contacts` | Add/modify/remove contacts |
| `read:messages` | Read messages |
| `send:messages` | Send messages |
| `read:apps` | List installed apps |
| `manage:apps` | Install/uninstall/configure apps |
| `read:settings` | Read node settings |
| `write:settings` | Modify node settings |
| `admin:full` | Full administrative access |

---

## Health and Status

### GET /health

Returns comprehensive health status of the node. **No authentication required.**

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "checks": {
    "identity": {
      "status": "healthy",
      "iid": "CROCKFORD32_IID",
      "last_published": "2025-01-15T00:00:00Z"
    },
    "transport": {
      "status": "healthy",
      "connections": 5,
      "relays_connected": 2
    },
    "messaging": {
      "status": "healthy",
      "queue_depth": 0,
      "sessions_active": 3
    },
    "storage": {
      "status": "healthy",
      "disk_used_bytes": 1073741824,
      "disk_free_bytes": 107374182400
    },
    "apps": {
      "status": "healthy",
      "installed": 2,
      "running": 2
    }
  }
}
```

**Status Codes:**
- `200 OK` - Node is healthy
- `503 Service Unavailable` - Node is unhealthy

---

### GET /health/live

Kubernetes-style liveness probe. **No authentication required.**

**Response (Healthy):**
```json
{
  "status": "alive"
}
```

**Response (Shutting Down):**
```json
{
  "status": "dead",
  "reason": "shutting_down"
}
```

**Status Codes:**
- `200 OK` - Node is alive
- `503 Service Unavailable` - Node is shutting down

---

### GET /health/ready

Kubernetes-style readiness probe. **No authentication required.**

**Response (Ready):**
```json
{
  "status": "ready"
}
```

**Response (Not Ready):**
```json
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

**Status Codes:**
- `200 OK` - Node is ready
- `503 Service Unavailable` - Node is not ready

---

### GET /metrics

Prometheus-format metrics. **No authentication required** unless `metrics.require_auth` is enabled in settings.

**Headers (if auth required):**
```
Authorization: Bearer <metrics_token>
```

**Response:**
```
# HELP postnode_uptime_seconds Node uptime in seconds
# TYPE postnode_uptime_seconds gauge
postnode_uptime_seconds 3600

# HELP postnode_contacts_total Total number of contacts
# TYPE postnode_contacts_total gauge
postnode_contacts_total 15
...
```

**Status Codes:**
- `200 OK` - Metrics returned
- `401 Unauthorized` - Auth required but not provided
- `404 Not Found` - Metrics disabled

---

### GET /admin/v1/status

Detailed node status. **Requires:** `read:settings` permission.

**Response:**
```json
{
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "status": "healthy",
  "identity": {
    "iid": "CROCKFORD32_IID",
    "last_published": "2025-01-15T00:00:00Z",
    "device_count": 2
  },
  "network": {
    "connections_active": 5,
    "connections_direct": 3,
    "connections_relay": 2,
    "relays_connected": 2,
    "bytes_sent": 1048576,
    "bytes_received": 2097152,
    "external_addr_detected": "203.0.113.1:4433"
  },
  "storage": {
    "data_used_bytes": 1073741824,
    "data_free_bytes": 107374182400,
    "messages_count": 500,
    "documents_count": 100
  },
  "apps": {
    "installed": 3,
    "running": 2,
    "total_storage_used": 52428800
  }
}
```

---

## Authentication Endpoints

### POST /admin/v1/auth/login

Authenticate with password and create a session.

**Request:**
```json
{
  "password": "your_password",
  "remember_device": true
}
```

**Response:**
```json
{
  "session": {
    "id": "abc123...",
    "created_at": "2025-01-15T10:00:00Z",
    "expires_at": "2025-01-16T10:00:00Z",
    "last_activity": "2025-01-15T10:00:00Z",
    "user_agent": "Mozilla/5.0...",
    "ip_address": "192.168.1.100",
    "device_id": "dev123",
    "requires_fresh_auth": false
  },
  "csrf_token": "csrf_token_value"
}
```

**Set-Cookie Headers:**
- `postnode_session=<signed_session>; Path=/admin; HttpOnly; SameSite=Strict`
- `postnode_csrf=<csrf_token>; Path=/admin; SameSite=Strict`

---

### POST /admin/v1/auth/logout

End the current session. **Requires:** Valid session cookie.

**Response:** `204 No Content`

---

### POST /admin/v1/auth/refresh

Extend the current session expiration. **Requires:** Valid session cookie.

**Response:**
```json
{
  "id": "abc123...",
  "created_at": "2025-01-15T10:00:00Z",
  "expires_at": "2025-01-16T12:00:00Z",
  "last_activity": "2025-01-15T12:00:00Z",
  "user_agent": "Mozilla/5.0...",
  "ip_address": "192.168.1.100",
  "device_id": "dev123",
  "requires_fresh_auth": false
}
```

---

### POST /admin/v1/auth/reauth

Re-authenticate to enable sensitive operations. **Requires:** Valid session cookie.

**Request:**
```json
{
  "password": "your_password"
}
```

**Response:**
```json
{
  "id": "abc123...",
  "created_at": "2025-01-15T10:00:00Z",
  "expires_at": "2025-01-16T10:00:00Z",
  "last_activity": "2025-01-15T12:00:00Z",
  "user_agent": "Mozilla/5.0...",
  "ip_address": "192.168.1.100",
  "device_id": "dev123",
  "requires_fresh_auth": false
}
```

---

## Identity Endpoints

### GET /admin/v1/identity

Get identity information. **Requires:** `read:identity` permission.

**Response:**
```json
{
  "iid": "CROCKFORD32_IDENTITY_ID",
  "genesis_key_fingerprint": "sha256:abc123...",
  "current_signing_key_fingerprint": "sha256:def456...",
  "current_encryption_key_fingerprint": "sha256:789abc...",
  "created_at": "2025-01-01T00:00:00Z",
  "last_key_rotation": "2025-01-10T00:00:00Z",
  "recovery_method": "social",
  "endpoints": [
    {
      "type": "quic",
      "url": "quic://node.example.com:4433",
      "priority": 1
    }
  ],
  "profile": {
    "display_name": "Alice",
    "avatar": "data:image/png;base64,...",
    "bio": "Hello, I'm Alice!"
  }
}
```

---

### PUT /admin/v1/identity/profile

Update public profile. **Requires:** `write:identity` permission + fresh auth.

**Request:**
```json
{
  "display_name": "Alice",
  "avatar": "data:image/png;base64,...",
  "bio": "Updated bio"
}
```

**Response:** Same as `GET /admin/v1/identity`

---

### POST /admin/v1/identity/rotate/signing

Rotate signing key. **Requires:** `write:identity` permission + fresh auth.

**Response:**
```json
{
  "success": true,
  "new_key_fingerprint": "sha256:newkey...",
  "previous_key_fingerprint": "sha256:oldkey...",
  "rotated_at": "2025-01-15T12:00:00Z"
}
```

---

### POST /admin/v1/identity/rotate/encryption

Rotate encryption key. **Requires:** `write:identity` permission + fresh auth.

**Response:**
```json
{
  "success": true,
  "new_key_fingerprint": "sha256:newkey...",
  "previous_key_fingerprint": "sha256:oldkey...",
  "rotated_at": "2025-01-15T12:00:00Z"
}
```

---

### GET /admin/v1/identity/recovery

Get recovery configuration. **Requires:** `read:identity` permission.

**Response:**
```json
{
  "method": "social",
  "config": {
    "threshold": 3,
    "trustees": ["IID1", "IID2", "IID3", "IID4", "IID5"]
  }
}
```

---

### PUT /admin/v1/identity/recovery

Update recovery configuration. **Requires:** `write:identity` permission + fresh auth.

**Request:**
```json
{
  "method": "social",
  "config": {
    "threshold": 3,
    "trustees": ["IID1", "IID2", "IID3"]
  }
}
```

**Response:** Same as request

---

### GET /admin/v1/identity/export

Export identity backup (encrypted). **Requires:** `read:identity` permission.

**Response:** Binary data (application/octet-stream)

---

### GET /admin/v1/devices

List linked devices. **Requires:** `read:identity` permission.

**Response:**
```json
[
  {
    "did": "device_id_1",
    "name": "Desktop",
    "created_at": "2025-01-01T00:00:00Z",
    "last_seen": "2025-01-15T10:00:00Z",
    "is_current": true,
    "platform": "macos"
  }
]
```

---

### POST /admin/v1/devices

Add a new device. **Requires:** `write:identity` permission + fresh auth.

**Request:**
```json
{
  "name": "Mobile Phone"
}
```

**Response:**
```json
{
  "did": "new_device_id",
  "name": "Mobile Phone",
  "activation_code": "ACTIVATION_CODE",
  "expires_at": "2025-01-16T12:00:00Z"
}
```

---

### DELETE /admin/v1/devices/{did}

Remove a device. **Requires:** `write:identity` permission + fresh auth.

**Response:** `204 No Content`

**Error:** Cannot remove current device (409 Conflict)

---

## Contacts Endpoints

### GET /admin/v1/contacts

List contacts with pagination. **Requires:** `read:contacts` permission.

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | 50 | Max results (1-1000) |
| `offset` | integer | 0 | Skip first N results |
| `sort_by` | string | `display_name` | Sort field: `display_name`, `added_at`, `last_seen`, `trust_level` |
| `sort_order` | string | `asc` | Sort order: `asc`, `desc` |

**Response:**
```json
{
  "items": [
    {
      "iid": "CONTACT_IID",
      "display_name": "Bob",
      "avatar": "data:image/png;base64,...",
      "trust_level": "verified",
      "is_blocked": false,
      "is_online": true,
      "last_seen": "2025-01-15T11:00:00Z",
      "added_at": "2025-01-01T00:00:00Z",
      "added_by": "manual",
      "notes": "Met at conference",
      "tags": ["friends", "work"],
      "shared_groups": ["group1"]
    }
  ],
  "total": 25,
  "limit": 50,
  "offset": 0
}
```

---

### POST /admin/v1/contacts

Add a contact. **Requires:** `write:contacts` permission.

**Request:**
```json
{
  "iid": "CONTACT_IID",
  "display_name": "Bob",
  "trust_level": "unverified"
}
```

**Trust Levels:** `unknown`, `unverified`, `verified`, `trusted`

**Response:** Full contact object

---

### GET /admin/v1/contacts/{iid}

Get a specific contact. **Requires:** `read:contacts` permission.

**Response:** Full contact object

---

### PUT /admin/v1/contacts/{iid}

Update a contact. **Requires:** `write:contacts` permission.

**Request:**
```json
{
  "display_name": "Robert",
  "notes": "Updated notes",
  "tags": ["friends"],
  "trust_level": "verified"
}
```

**Response:** Updated contact object

---

### DELETE /admin/v1/contacts/{iid}

Remove a contact. **Requires:** `write:contacts` permission.

**Response:** `204 No Content`

---

### POST /admin/v1/contacts/{iid}/block

Block a contact. **Requires:** `write:contacts` permission.

**Response:** `204 No Content`

---

### DELETE /admin/v1/contacts/{iid}/block

Unblock a contact. **Requires:** `write:contacts` permission.

**Response:** `204 No Content`

---

## Apps Endpoints

### GET /admin/v1/apps

List installed apps. **Requires:** `read:apps` permission.

**Response:**
```json
[
  {
    "id": "com.example.app",
    "name": "Example App",
    "version": "1.0.0",
    "author_iid": "AUTHOR_IID",
    "author_name": "Example Developer",
    "description": "An example application",
    "icon": "data:image/png;base64,...",
    "installed_at": "2025-01-10T00:00:00Z",
    "last_opened": "2025-01-15T10:00:00Z",
    "update_available": "1.1.0",
    "status": "installed",
    "permissions": {
      "granted": ["contacts:read"],
      "denied": ["network:outbound"],
      "pending": ["storage:unlimited"]
    },
    "storage_used": 1048576,
    "storage_quota": 104857600
  }
]
```

**App Status:** `installed`, `running`, `disabled`, `error`

---

### GET /admin/v1/apps/{app_id}

Get a specific app. **Requires:** `read:apps` permission.

**Response:** Full app object

---

### POST /admin/v1/apps/install

Install an app from URL or repository. **Requires:** `manage:apps` permission.

**Request (from URL):**
```json
{
  "source": {
    "type": "url",
    "value": "https://example.com/app.postapp"
  }
}
```

**Request (from repository):**
```json
{
  "source": {
    "type": "repository",
    "value": "official:com.example.app"
  }
}
```

**Response:**
```json
{
  "app": { /* full app object */ },
  "permissions_requested": ["contacts:read", "storage:unlimited"],
  "permissions_granted": ["contacts:read"]
}
```

---

### POST /admin/v1/apps/install/upload

Install an app from uploaded file. **Requires:** `manage:apps` permission.

**Request:** `multipart/form-data` with `file` field containing `.postapp` package

**Response:** Same as `/admin/v1/apps/install`

---

### POST /admin/v1/apps/{app_id}/update

Update an app to latest version. **Requires:** `manage:apps` permission.

**Response:**
```json
{
  "app": { /* updated app object */ },
  "previous_version": "1.0.0",
  "new_permissions": ["new:capability"]
}
```

---

### DELETE /admin/v1/apps/{app_id}

Uninstall an app. **Requires:** `manage:apps` permission.

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `keepData` | boolean | false | Preserve app data after uninstall |

**Response:** `204 No Content`

---

### GET /admin/v1/apps/{app_id}/permissions

Get app permissions. **Requires:** `read:apps` permission.

**Response:**
```json
{
  "granted": ["contacts:read"],
  "denied": ["network:outbound"],
  "pending": ["storage:unlimited"]
}
```

---

### PATCH /admin/v1/apps/{app_id}/permissions

Update app permissions. **Requires:** `manage:apps` permission.

**Request:**
```json
{
  "grant": ["storage:unlimited"],
  "deny": ["network:outbound"],
  "reset": ["camera:access"]
}
```

**Response:** Updated permissions object

---

### GET /admin/v1/apps/{app_id}/settings

Get app-specific settings. **Requires:** `read:apps` permission.

**Response:** App-specific JSON object

---

### PUT /admin/v1/apps/{app_id}/settings

Update app-specific settings. **Requires:** `manage:apps` permission.

**Request:** App-specific JSON object

**Response:** Updated settings object

---

### POST /admin/v1/apps/{app_id}/clear-data

Clear all app data. **Requires:** `manage:apps` permission + fresh auth.

**Response:** `204 No Content`

---

### App UI and API Proxy

Apps are served at `/apps/{app_id}/` and API requests are proxied via `/apps/{app_id}/api/`.

| Path | Description |
|------|-------------|
| `GET /apps/{app_id}/` | Serves `ui/index.html` |
| `GET /apps/{app_id}/assets/*` | Serves static assets from `ui/assets/` |
| `* /apps/{app_id}/api/*` | Proxied to app's configured `api_base_url` |

---

## Settings Endpoints

### GET /admin/v1/settings

Get all node settings. **Requires:** `read:settings` permission.

**Response:**
```json
{
  "network": {
    "listen_addr": "0.0.0.0:4433",
    "admin_listen_addr": "127.0.0.1:8080",
    "enable_upnp": true,
    "external_addr": null,
    "relay_servers": ["relay1.example.com:4433"],
    "bandwidth_limit_mbps": null
  },
  "admin": {
    "enabled": true,
    "require_tls": true,
    "session_timeout_hours": 24,
    "ip_allowlist": []
  },
  "apps": {
    "auto_update": true,
    "allow_sideload": true,
    "default_storage_quota": "100MB",
    "trusted_repositories": [
      {
        "id": "official",
        "operator_iid": "OPERATOR_IID",
        "operator_key_fingerprint": "sha256:...",
        "url": "https://apps.example.com",
        "trust_level": "full",
        "auto_update": true,
        "added_at": "2025-01-01T00:00:00Z"
      }
    ]
  },
  "privacy": {
    "publish_identity_hours": 24,
    "show_online_status": true,
    "send_read_receipts": true,
    "share_analytics": false
  },
  "storage": {
    "data_dir": "/var/lib/postnode/data",
    "log_dir": "/var/log/postnode",
    "backup_enabled": true,
    "backup_schedule": "0 2 * * *",
    "backup_retention_days": 30
  },
  "notifications": {
    "enabled": true,
    "sound_enabled": true,
    "quiet_hours_start": "22:00",
    "quiet_hours_end": "07:00"
  },
  "logging": {
    "redact_iids": true,
    "redact_ips": true,
    "redact_message_content": true
  },
  "metrics": {
    "enabled": true,
    "require_auth": false,
    "auth_token_hash": null
  },
  "health": {
    "disk_free_min_bytes": 104857600,
    "memory_max_percent": 90,
    "connection_queue_max": 1000,
    "message_queue_max": 10000
  }
}
```

---

### GET /admin/v1/settings/{section}

Get a specific settings section. **Requires:** `read:settings` permission.

**Sections:** `network`, `admin`, `apps`, `privacy`, `storage`, `notifications`, `logging`, `metrics`, `health`

**Response:** Section-specific JSON object

---

### PATCH /admin/v1/settings

Update settings (partial). **Requires:** `write:settings` permission + fresh auth.

**Request:**
```json
{
  "privacy": {
    "show_online_status": false
  },
  "notifications": {
    "sound_enabled": false
  }
}
```

**Response:** Complete updated settings object

---

### POST /admin/v1/settings/reset

Reset settings to defaults. **Requires:** `write:settings` permission + fresh auth.

**Request:**
```json
{
  "section": "privacy"
}
```

Or omit `section` to reset all settings.

**Response:** Complete settings object after reset

---

## Backups Endpoints

### GET /admin/v1/backups

List available backups. **Requires:** `read:settings` permission.

**Response:**
```json
[
  {
    "id": "backup123",
    "created_at": "2025-01-15T02:00:00Z",
    "size": 52428800,
    "path": "/var/lib/postnode/data/backups/backup123.pusb",
    "type": "full"
  }
]
```

**Backup Types:** `full`, `identity`, `data`

---

### POST /admin/v1/backups

Create a new backup. **Requires:** `write:settings` permission.

**Request (optional):**
```json
{
  "type": "full"
}
```

**Response:**
```json
{
  "id": "backup456",
  "created_at": "2025-01-15T12:00:00Z",
  "size": 52428800,
  "path": "/var/lib/postnode/data/backups/backup456.pusb",
  "encrypted": true
}
```

---

### POST /admin/v1/backups/upload

Upload a backup file. **Requires:** `write:settings` permission.

**Request:** `multipart/form-data` with `file` field containing `.pusb` backup

**Response:** Backup list entry

---

### GET /admin/v1/backups/{id}

Download a backup file. **Requires:** `read:settings` permission.

**Response:** Binary data (application/octet-stream)

---

### POST /admin/v1/backups/{id}/restore

Restore from a backup. **Requires:** `write:settings` permission + fresh auth.

**Request:**
```json
{
  "password": "backup_password"
}
```

**Response:**
```json
{
  "success": true,
  "restored_at": "2025-01-15T12:30:00Z",
  "identity": "RESTORED_IID",
  "contacts_restored": 25,
  "messages_restored": 500,
  "apps_restored": 3,
  "warnings": []
}
```

---

### DELETE /admin/v1/backups/{id}

Delete a backup. **Requires:** `write:settings` permission.

**Response:** `204 No Content`

---

## API Keys Endpoints

### GET /admin/v1/api-keys

List API keys. **Requires:** `read:settings` permission.

**Response:**
```json
[
  {
    "id": "key123",
    "name": "CI Integration",
    "permissions": ["read:identity", "read:contacts"],
    "created_at": "2025-01-10T00:00:00Z",
    "expires_at": "2025-04-10T00:00:00Z",
    "last_used": "2025-01-15T10:00:00Z"
  }
]
```

Note: The secret is never returned after creation.

---

### POST /admin/v1/api-keys

Create a new API key. **Requires:** `write:settings` permission + fresh auth.

**Request:**
```json
{
  "name": "CI Integration",
  "permissions": ["read:identity", "read:contacts"],
  "expires_in_days": 90
}
```

**Response:**
```json
{
  "key": {
    "id": "key456",
    "name": "CI Integration",
    "permissions": ["read:identity", "read:contacts"],
    "created_at": "2025-01-15T12:00:00Z",
    "expires_at": "2025-04-15T12:00:00Z",
    "last_used": null
  },
  "secret": "pk_live_xxxxxxxxxxxxxxxxxxxx"
}
```

**Important:** The `secret` is only returned once. Store it securely.

---

### DELETE /admin/v1/api-keys/{id}

Revoke an API key. **Requires:** `write:settings` permission.

**Response:** `204 No Content`

---

## Logs Endpoints

### GET /admin/v1/logs

Query node logs. **Requires:** `read:settings` permission.

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | 100 | Max entries (1-1000) |
| `cursor` | string | - | Pagination cursor |
| `level` | string | - | Filter by level: `debug`, `info`, `warn`, `error` |
| `target` | string | - | Filter by target: `postnode::admin`, `postnode::identity`, etc. |
| `search` | string | - | Search in message text |
| `since` | ISO8601 | - | Entries after this time |
| `until` | ISO8601 | - | Entries before this time |

**Response:**
```json
{
  "entries": [
    {
      "timestamp": "2025-01-15T12:00:00Z",
      "level": "info",
      "target": "postnode::admin",
      "message": "admin login",
      "fields": {
        "device_id": "dev123"
      }
    }
  ],
  "cursor": "100",
  "has_more": true
}
```

---

## Control Endpoints

### POST /admin/v1/restart

Gracefully restart the node. **Requires:** `write:settings` permission.

**Response:** `202 Accepted`

---

### POST /admin/v1/shutdown

Gracefully shutdown the node. **Requires:** `write:settings` permission.

**Response:** `202 Accepted`

---

## Events (WebSocket)

### GET /admin/v1/events

WebSocket endpoint for real-time events. **Requires:** Authentication via query param or cookies.

**Connection:**
```
ws://localhost:8080/admin/v1/events?token=<api_key>
```

Or with session cookies:
```
ws://localhost:8080/admin/v1/events
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `token` | string | API key for authentication |
| `lastEventId` | integer | Resume from event ID |

**Default Subscribed Events:**
- `status_change`
- `contact_online`
- `message_received`
- `app_installed`
- `app_updated`
- `app_error`
- `sync_progress`
- `error`

**Subscription Management (Client to Server):**
```json
{
  "type": "subscribe",
  "events": ["log_entry", "backup_created"]
}
```

```json
{
  "type": "unsubscribe",
  "events": ["sync_progress"]
}
```

**Event Format (Server to Client):**
```json
{
  "id": 12345,
  "type": "message_received",
  "timestamp": "2025-01-15T12:00:00Z",
  "data": {
    "from_iid": "SENDER_IID",
    "message_id": "msg123"
  }
}
```

---

## Mailbox Endpoints

The Mailbox API is used for inter-node message delivery. It runs on a separate port and uses identity-based authentication.

### Authentication

Mailbox endpoints require a **Mailbox Token** in the `Authorization` header:

```
Authorization: Bearer <base64url_encoded_mailbox_token>
```

The mailbox token is a signed JSON object containing:
- `iid` - The sender/owner's identity ID
- `mailbox_url` - The mailbox URL this token is valid for
- `expires_at` - Token expiration timestamp
- `nonce` - Unique token identifier
- `signature` - Ed25519 signature by the identity's signing key

### POST /mailbox/token/{recipient_iid}

Request a bearer token to store messages in a recipient's mailbox.

**Headers:**
```
Authorization: Bearer <sender_mailbox_token>
Content-Type: application/json
```

**Request:**
```json
{
  "sender_iid": "SENDER_IID",
  "validity_hours": 24
}
```

**Response:**
```json
{
  "token": "bearer_token_value",
  "expires_at": "2025-01-16T12:00:00Z",
  "recipient_iid": "RECIPIENT_IID",
  "sender_iid": "SENDER_IID"
}
```

**Status Codes:**
- `200 OK` - Token generated
- `400 Bad Request` - Invalid request
- `401 Unauthorized` - Invalid mailbox token
- `403 Forbidden` - sender_iid mismatch
- `501 Not Implemented` - Bearer tokens disabled

---

### POST /messages/{recipient_iid}

Store a message in a recipient's mailbox.

**Headers:**
```
Authorization: Bearer <sender_mailbox_token>
X-Mailbox-Bearer-Token: <bearer_token>:<expires_at>
Content-Type: application/octet-stream
```

**Request Body:** PUSE envelope (binary, max 1MB)

**Response:**
```json
{
  "message_id": "msg_uuid",
  "stored_at": "2025-01-15T12:00:00Z",
  "expires_at": "2025-02-14T12:00:00Z"
}
```

**Status Codes:**
- `201 Created` - Message stored
- `400 Bad Request` - Invalid envelope
- `401 Unauthorized` - Invalid mailbox token
- `403 Forbidden` - Invalid/missing bearer token or sender mismatch
- `413 Payload Too Large` - Envelope > 1MB

---

### GET /messages

Retrieve messages from your mailbox.

**Headers:**
```
Authorization: Bearer <owner_mailbox_token>
```

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `cursor` | string | - | Pagination cursor (base64url encoded offset) |
| `limit` | integer | 100 | Max messages (1-1000) |

**Response:**
```json
{
  "messages": [
    {
      "message_id": "msg_uuid",
      "stored_at": "2025-01-15T12:00:00Z",
      "sender_iid": "SENDER_IID",
      "size": 1024,
      "envelope": "base64_encoded_puse_envelope"
    }
  ],
  "next_cursor": "MTAw",
  "has_more": true
}
```

---

### DELETE /messages

Delete messages from your mailbox.

**Headers:**
```
Authorization: Bearer <owner_mailbox_token>
Content-Type: application/json
```

**Request:**
```json
{
  "message_ids": ["msg_uuid_1", "msg_uuid_2"]
}
```

**Response:**
```json
{
  "deleted": 2
}
```

---

## Error Codes

All API errors follow this format:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error message",
    "details": null
  }
}
```

### Error Code Reference

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `INVALID_REQUEST` | 400 | Malformed request |
| `UNAUTHORIZED` | 401 | Missing or invalid authentication |
| `FORBIDDEN` | 403 | Insufficient permissions |
| `NOT_FOUND` | 404 | Resource not found |
| `CONFLICT` | 409 | Resource conflict (already exists, cannot delete, etc.) |
| `RATE_LIMITED` | 429 | Too many requests |
| `PAYLOAD_TOO_LARGE` | 413 | Request body too large |
| `VALIDATION_ERROR` | 422 | Invalid field values |
| `CSRF_INVALID` | 403 | Missing or invalid CSRF token |
| `FRESH_AUTH_REQUIRED` | 403 | Re-authentication required for sensitive operation |
| `INTERNAL_ERROR` | 500 | Server error |
| `SERVICE_UNAVAILABLE` | 503 | Service temporarily unavailable |
| `TIMEOUT` | 504 | Operation timed out |

---

## Rate Limits

The API implements rate limiting per IP address and per authenticated identity. Current limits:

| Endpoint Category | Limit |
|-------------------|-------|
| Authentication | 10 requests/minute |
| Read operations | 100 requests/minute |
| Write operations | 30 requests/minute |
| Mailbox storage | 100 messages/hour |

Exceeded limits return `429 Too Many Requests` with a `Retry-After` header.

---

## Versioning

The API uses URL versioning (`/admin/v1/`, `/api/v1/`). The `/api/v1/` prefix is an alias for `/admin/v1/` for backwards compatibility.

Breaking changes will be introduced in new versions (e.g., `/admin/v2/`), with deprecation notices provided in advance.

# Protocol Registry Specification

## Overview

The Protocol Registry is the **single source of truth** for all bridge methods, capabilities, schemas, and extension modules in Post-Urbit. It governs how methods are registered, versioned, authorized, discovered, and extended.

### Core Invariants

1. **Apps MUST NOT dynamically register or override methods** - Method dispatch determined solely by backend-controlled registry
2. **Registry changes MUST be auditable and integrity-protected** - All changes tracked via cryptographic hashes
3. **Introspection MUST NOT expose shell-only methods to apps** - Session-filtered views only
4. **Fail closed policy** - Unknown methods rejected; conflicts cause installation failure, not override
5. **Method identity is namespace-scoped** - Core platform owns reserved prefixes; extensions use reverse-DNS namespacing

### Related Documents

- [Domain 4: Secure Bridge Protocol](./04-SECURE_BRIDGE_PROTOCOL.md) - Registry provides MethodSpec for authorization
- [Domain 2: App Sandbox & Isolation](./02-APP_SANDBOX_ISOLATION.md) - Capability grants from registry
- [Domain 3: Resource Constraints](./03-RESOURCE_CONSTRAINTS.md) - Rate limit classes defined here

---

## Data Models

### ModuleSpec

```rust
use serde::{Deserialize, Serialize};

/// Module specification - logical grouping of related methods
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ModuleSpec {
    /// Unique module identifier (e.g., "storage", "x.com.example.calendar")
    pub module_id: String,

    /// Human-readable display name
    pub display_name: String,

    /// Module description
    pub description: String,

    /// Module type (Core or Extension)
    pub module_type: ModuleType,

    /// Version (semver)
    pub version: String,

    /// Namespace prefix this module owns
    pub namespace_prefix: String,

    /// Methods provided by this module
    pub methods: Vec<String>,

    /// Schemas registered by this module
    pub schemas: Vec<String>,

    /// Capabilities this module can mint
    pub capabilities: Vec<String>,

    /// Stability level
    pub stability: StabilityLevel,

    /// Activation state
    pub state: ModuleState,

    /// Cryptographic signature (extensions only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<ModuleSignature>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleType {
    Core,      // Built into backend
    Extension, // Installed separately
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleState {
    Pending,   // Installed but not validated
    Active,    // Validated and active
    Inactive,  // Temporarily disabled
    Updating,  // Being updated
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StabilityLevel {
    Experimental, // Subject to change
    Beta,         // API stabilizing
    Stable,       // Follows deprecation policy
    Deprecated,   // Scheduled for removal
}
```

### MethodSpec

```rust
/// Complete method specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MethodSpec {
    /// Full method name (e.g., "storage.v1.get")
    pub method: String,

    /// Module that owns this method
    pub module_id: String,

    /// Stability level
    pub stability: StabilityLevel,

    /// Required capabilities to invoke
    pub required_capabilities: Vec<String>,

    /// Permission tier
    pub permission_tier: PermissionTier,

    /// Whether method is idempotent (safe to retry)
    pub idempotent: bool,

    /// Timeout in milliseconds
    pub timeout_ms: u64,

    /// Rate limit class
    pub rate_limit_class: RateLimitClass,

    /// Max request payload bytes
    pub max_request_bytes: u32,

    /// Max response payload bytes
    pub max_response_bytes: u32,

    /// Schema ID for parameter validation
    pub params_schema_id: String,

    /// Schema ID for result validation
    pub result_schema_id: String,

    /// Description
    pub description: String,

    /// Deprecation info (if deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<DeprecationInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionTier {
    AlwaysGranted, // Automatically granted
    GrantOnce,     // Granted once, remembered
    PromptAlways,  // Prompt every time
    ShellOnly,     // Never granted to apps
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitClass {
    Unlimited,  // No rate limiting
    Standard,   // 50 rps sustained
    Expensive,  // 10 rps
    Restricted, // 1 rpm
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeprecationInfo {
    pub deprecated_since: String,
    pub sunset_after: String,
    pub replacement: Option<String>,
    pub migration_guide: Option<String>,
}
```

### SchemaSpec

```rust
/// Schema specification for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SchemaSpec {
    /// Unique schema ID
    pub schema_id: String,

    /// Owning module
    pub module_id: String,

    /// Version
    pub version: String,

    /// CDDL definition
    pub cddl: String,

    /// SHA-256 hash of CDDL
    pub cddl_hash: String,

    /// Whether unknown keys are rejected
    pub strict_mode: bool,

    /// Reserved extension key (if strict but extensions allowed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_key: Option<String>,
}
```

### CapabilitySpec

```rust
/// Capability specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CapabilitySpec {
    /// Full capability name (e.g., "storage:app")
    pub capability: String,

    /// Owning module
    pub module_id: String,

    /// Display name for prompts
    pub display_name: String,

    /// Description for prompts
    pub description: String,

    /// Default permission tier
    pub default_tier: PermissionTier,

    /// Risk level for UI
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,      // No user data access
    Medium,   // App-scoped data
    High,     // User data access
    Critical, // System modification
}
```

---

## Namespace Ownership Rules

### Reserved Platform Namespaces

| Prefix | Owner | Description |
|--------|-------|-------------|
| `bridge.*` | Platform | Bridge lifecycle |
| `events.*` | Platform | Event subscriptions |
| `system.*` | Platform | System information |
| `storage.*` | Platform | App-scoped storage |
| `resource.*` | Platform | Resource budgets |
| `permission.*` | Platform | Permission requests |
| `blob.*` | Platform | Chunked transfers |
| `shell.*` | Platform (Shell-Only) | Shell operations |
| `x.*` | Extensions | Third-party namespace |

### Extension Namespace Format

```
x.<reverse_dns>.<module>.vN.<method>

Examples:
x.com.example.calendar.v1.list_events
x.io.mycompany.analytics.v2.track_event
```

### Capability Namespace Rules

- **Core modules**: Can mint capabilities in their namespace (e.g., `storage` → `storage:app`)
- **Extensions**: Must use `x:` prefix with matching reverse-DNS (e.g., `x.com.example.mod` → `x:com.example.mod:read`)
- **Non-wildcard rule**: Apps must request specific capabilities, never `*`

---

## Registration Lifecycle

### Module Types

```
┌─────────────────────────────────────────────────────┐
│                   CORE MODULES                       │
│  - Built into backend binary                         │
│  - Registered at startup                             │
│  - Cannot be modified/removed at runtime             │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│                EXTENSION MODULES                     │
│                                                      │
│  Install → Validate → Activate ⟷ Deactivate         │
│     ↓                    ↓                           │
│  Reject              Update                          │
└─────────────────────────────────────────────────────┘
```

### Extension Installation

```rust
impl ProtocolRegistry {
    pub async fn install_extension(
        &mut self,
        manifest: &ExtensionManifest,
        signature: &ModuleSignature,
    ) -> Result<(), RegistryError> {
        // Phase 1: Validation
        self.validate_extension_manifest(manifest)?;
        self.verify_extension_signature(manifest, signature).await?;
        self.check_namespace_conflicts(manifest)?;
        self.validate_schema_hashes(manifest)?;

        // Phase 2: Registration
        let module = ModuleSpec::from_extension(manifest, signature.clone());
        self.modules.insert(manifest.module_id.clone(), module);

        // Phase 3: Activation
        self.activate_module(&manifest.module_id)?;

        // Update integrity hash
        self.compute_registry_hash()?;

        Ok(())
    }

    fn check_namespace_conflicts(&self, manifest: &ExtensionManifest) -> Result<(), RegistryError> {
        for method in &manifest.methods {
            if self.methods.contains_key(&method.method) {
                return Err(RegistryError::MethodConflict(method.method.clone()));
            }
            if !method.method.starts_with(&format!("{}.", manifest.module_id)) {
                return Err(RegistryError::NamespaceViolation);
            }
        }
        Ok(())
    }
}
```

---

## Version Negotiation

### Three Versioning Axes

| Axis | Location | Policy |
|------|----------|--------|
| **Envelope** | `bridge-request.v` | Major only, breaking changes |
| **Method** | Namespace segment | `storage.v1.get`, `storage.v2.get` |
| **Schema** | `params_schema_id` | Tied to method version |

### Version Coexistence

Multiple method versions coexist during transitions:

```rust
// Both v1 (deprecated) and v2 (stable) are registered
"storage.v1.get" → StabilityLevel::Deprecated
"storage.v2.get" → StabilityLevel::Stable
```

---

## Authorization Contract

### Authorization Flow

```
Request → Look up MethodSpec → Check permission tier → Check capabilities → Authorized
              ↓                      ↓                      ↓
         NOT_FOUND            ShellOnly+app=UNAUTHORIZED  PERMISSION_DENIED
```

### Implementation

```rust
impl ProtocolRegistry {
    pub fn authorize(
        &self,
        method: &str,
        session: &AppSession,
        is_shell: bool,
    ) -> Result<&MethodSpec, BridgeError> {
        // Look up method
        let spec = self.methods.get(method)
            .ok_or_else(|| BridgeError::invalid_request("Unknown method"))?;

        // Check permission tier
        if spec.permission_tier == PermissionTier::ShellOnly && !is_shell {
            return Err(BridgeError::unauthorized());
        }

        // Check capabilities
        for cap in &spec.required_capabilities {
            if !session.capabilities.contains(cap) {
                return Err(BridgeError::permission_denied(cap));
            }
        }

        Ok(spec)
    }
}
```

---

## Introspection APIs

### bridge.get_server_info

```rust
/// Returns server information and registry hash
/// Tier: AlwaysGranted
pub struct GetServerInfoResult {
    pub envelope_versions: Vec<u8>,
    pub preferred_envelope_version: u8,
    pub platform_version: String,
    pub registry_hash: String,
    pub active_modules: Vec<String>,
    pub server_time: u64,
}
```

### bridge.list_methods

```rust
/// Returns methods available to this session (filtered)
/// Tier: AlwaysGranted
/// SECURITY: Shell-only methods excluded for app sessions
pub struct ListMethodsParams {
    pub module_prefix: Option<String>,
    pub stability: Option<StabilityLevel>,
    pub include_deprecated: bool,
}

pub struct ListMethodsResult {
    pub methods: Vec<String>,
    pub registry_hash: String,
}

impl ProtocolRegistry {
    pub fn list_methods_for_session(
        &self,
        params: &ListMethodsParams,
        session: &AppSession,
        is_shell: bool,
    ) -> ListMethodsResult {
        let methods: Vec<String> = self.methods.values()
            .filter(|spec| {
                // SECURITY: Never expose shell-only to apps
                if !is_shell && spec.permission_tier == PermissionTier::ShellOnly {
                    return false;
                }
                // Apply filters...
                true
            })
            .map(|spec| spec.method.clone())
            .collect();

        ListMethodsResult {
            methods,
            registry_hash: self.registry_hash.clone(),
        }
    }
}
```

### bridge.get_method_spec

```rust
/// Returns specification for a single method
/// Tier: AlwaysGranted
/// SECURITY: Returns 404 for shell-only from app sessions
pub struct GetMethodSpecParams {
    pub method: String,
}

pub struct GetMethodSpecResult {
    pub spec: MethodSpec,
    pub params_schema: SchemaSpec,
    pub result_schema: SchemaSpec,
}
```

### Rate Limits

| Method | Sustained RPS | Burst |
|--------|--------------|-------|
| bridge.get_server_info | 10 | 50 |
| bridge.list_methods | 5 | 20 |
| bridge.get_method_spec | 10 | 50 |

---

## Extension Module System

### Extension Package Format

```
extension.postmod
├── manifest.json       # ExtensionManifest
├── SIGNATURE           # Ed25519 signature
├── schemas/
│   ├── params/*.cddl
│   └── results/*.cddl
└── handlers/           # Future: WASM handlers
    └── handler.wasm
```

### Extension Manifest

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub manifest_version: u8,
    pub module_id: String,          // x.com.example.module
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub min_platform_version: String,
    pub author_iid: String,
    pub methods: Vec<MethodSpec>,
    pub schemas: Vec<SchemaSpec>,
    pub capabilities: Vec<CapabilitySpec>,
    pub dependencies: Vec<ModuleDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDependency {
    pub module_id: String,
    pub min_version: String,
}
```

### Extension Validation

1. Verify manifest version
2. Validate module_id starts with `x.`
3. All methods under module namespace
4. All capabilities use `x:` prefix
5. All schema references exist
6. Dependencies satisfied
7. Signature verification
8. Schema hash verification

---

## Backward Compatibility Policy

### Deprecation Requirements

| Requirement | Value |
|-------------|-------|
| Minimum support window | 2 major releases OR 180 days |
| Deprecation announcement | At least 90 days before sunset |
| Replacement availability | Must be available before deprecation |
| Documentation | Migration guide required |

### Breaking Change Classification

| Change Type | Impact |
|-------------|--------|
| Add optional parameter | Non-breaking |
| Add new method | Non-breaking |
| Remove method | Major version bump |
| Change required parameter | Major version bump |
| Change return type | Major version bump |
| Add required capability | Major version bump |

### Deprecation Response

```rust
// Deprecated method responses include warning
if spec.stability == StabilityLevel::Deprecated {
    response.warnings.push(Warning {
        code: "DEPRECATED_METHOD",
        message: format!(
            "Method {} is deprecated. Use {} instead.",
            spec.method,
            spec.deprecation.replacement.unwrap_or("unknown")
        ),
    });
}
```

---

## Schema Validation

### Runtime Validation Flow

1. Parse CBOR request
2. Look up method's `params_schema_id`
3. Validate params against CDDL schema
4. Execute handler
5. Validate result against `result_schema_id`
6. Encode response

### Strict Mode

```rust
fn validate_strict(
    cddl: &str,
    cbor: &[u8],
    extension_key: &Option<String>,
) -> Result<(), ValidationError> {
    // Basic CDDL validation
    validate_cbor_from_slice(cddl, cbor)?;

    // Reject unknown keys (except extension_key)
    check_unknown_keys(&value, cddl, extension_key)?;

    Ok(())
}
```

---

## Registry Integrity

### Hash Computation

```rust
impl ProtocolRegistry {
    pub fn compute_registry_hash(&mut self) -> Result<(), RegistryError> {
        let mut hasher = Sha256::new();

        // Hash modules (sorted by ID)
        for module_id in self.modules.keys().sorted() {
            hasher.update(canonical_json(&self.modules[module_id]));
        }

        // Hash methods (sorted by name)
        for method_name in self.methods.keys().sorted() {
            hasher.update(canonical_json(&self.methods[method_name]));
        }

        // Hash schemas (sorted by ID)
        for schema_id in self.schemas.keys().sorted() {
            hasher.update(&self.schemas[schema_id].cddl_hash);
        }

        self.registry_hash = format!("sha256:{}", hex::encode(hasher.finalize()));
        Ok(())
    }
}
```

### Audit Log

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub change: RegistryChange,
    pub registry_hash_before: String,
    pub registry_hash_after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegistryChange {
    ModuleInstalled { module_id: String },
    ModuleActivated { module_id: String },
    ModuleDeactivated { module_id: String },
    ModuleUpdated { module_id: String, from: String, to: String },
    ModuleRemoved { module_id: String },
    MethodDeprecated { method: String },
}
```

---

## Core Method Registry

### Platform Namespaces

| Namespace | Methods |
|-----------|---------|
| `bridge` | ping, get_server_info, list_methods, get_method_spec |
| `storage` | v1.get, v1.set, v1.delete, v1.list |
| `events` | subscribe, poll, unsubscribe |
| `system` | get_time, get_identity |
| `resource` | get_budget, get_storage_usage, request_quota_increase |
| `permission` | prepare_action, execute_action, check, grant, revoke |
| `blob` | put_start, put_chunk, put_finish, get_chunk |
| `shell` | launch_app, close_app, install_app, uninstall_app, etc. |

---

## Acceptance Criteria

### Core Invariants

- [ ] Apps cannot call registry modification methods
- [ ] Apps cannot access shell-only methods via introspection
- [ ] Unknown methods return INVALID_REQUEST
- [ ] Extension namespace conflicts cause installation failure
- [ ] Registry hash changes on any modification
- [ ] All methods have valid schema references

### Registration

- [ ] Core modules registered at startup
- [ ] Extension installation validates namespace ownership
- [ ] Extension installation verifies signature
- [ ] Duplicate method registration fails closed

### Authorization

- [ ] ShellOnly methods rejected from app sessions
- [ ] Missing capability returns PERMISSION_DENIED
- [ ] AlwaysGranted methods require no prompt

### Introspection

- [ ] bridge.get_server_info returns registry_hash
- [ ] bridge.list_methods filters by session
- [ ] bridge.get_method_spec returns 404 for shell-only from app
- [ ] Rate limits enforced

---

## Test Cases

| Test | Setup | Expected Result |
|------|-------|-----------------|
| Core method available | Fresh registry | bridge.ping callable |
| Shell method hidden | App calls list_methods | shell.launch_app NOT in list |
| Shell method blocked | App calls shell.launch_app | UNAUTHORIZED |
| Unknown method | App calls foo.bar | INVALID_REQUEST |
| Extension install | Valid signed extension | Module activated |
| Extension conflict | Extension with existing method | RegistryError::MethodConflict |
| Extension namespace | Wrong namespace | RegistryError::NamespaceViolation |
| Capability denied | Missing required capability | PERMISSION_DENIED |
| Deprecated method | Call deprecated method | Success + warning |
| Schema validation | Invalid params | INVALID_REQUEST |
| Registry hash | Any change | Hash changes |

---

## Implementation Checklist

### Phase 1: Core Structures
- [ ] Define ModuleSpec, MethodSpec, SchemaSpec, CapabilitySpec
- [ ] Implement ProtocolRegistry with HashMap storage
- [ ] Implement registry hash computation

### Phase 2: Core Modules
- [ ] Register bridge namespace
- [ ] Register storage namespace
- [ ] Register events namespace
- [ ] Register system namespace
- [ ] Register resource namespace
- [ ] Register permission namespace
- [ ] Register shell namespace (ShellOnly)

### Phase 3: Authorization
- [ ] Implement authorize() method
- [ ] Integrate with session capabilities
- [ ] Implement ShellOnly filtering

### Phase 4: Introspection
- [ ] Implement bridge.get_server_info
- [ ] Implement bridge.list_methods
- [ ] Implement bridge.get_method_spec
- [ ] Add rate limiting

### Phase 5: Schema Validation
- [ ] Integrate CDDL validator
- [ ] Implement strict mode
- [ ] Wire into bridge handler

### Phase 6: Extension System
- [ ] Define extension manifest format
- [ ] Implement validation
- [ ] Implement signature verification
- [ ] Implement lifecycle management

### Phase 7: Compatibility
- [ ] Implement deprecation workflow
- [ ] Add deprecation warnings
- [ ] Document migration guides

### Phase 8: Integrity
- [ ] Implement audit log
- [ ] Verify hash on startup

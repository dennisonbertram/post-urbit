use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wasmtime::{Caller, Engine, ExternType, Linker, Module, Store};

use crate::error::{PostUrbitError, Result};
use crate::runtime::CapabilityRegistry;
use crate::sync::encode_cbor;

#[derive(Debug, Clone)]
struct StoredValue {
    value: Vec<u8>,
    version: u64,
}

type StorageMap = HashMap<String, HashMap<String, StoredValue>>;

#[derive(Debug, Clone)]
pub struct ContactSummary {
    iid: String,
    name: Option<String>,
    avatar_hash: Option<String>,
    last_seen: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppUserContact {
    iid: String,
    name: Option<String>,
    avatar_hash: Option<String>,
    app_data: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct NotificationRecord {
    id: String,
    title: String,
    body: String,
    icon: Option<String>,
    sound: bool,
    created_at: String,
}

#[derive(Debug, Default)]
struct NotificationState {
    notifications: HashMap<String, Vec<NotificationRecord>>,
    badges: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
struct SyncDocument {
    id: String,
    document_type: String,
    access: DocumentAccess,
    created_at: String,
    operations: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct DocumentAccess {
    owner: String,
    readers: Vec<String>,
    writers: Vec<String>,
}

#[derive(Debug, Default)]
struct SyncState {
    documents: HashMap<String, SyncDocument>,
}

#[derive(Debug, Default)]
struct ContactsState {
    contacts: Vec<ContactSummary>,
    app_users: Vec<AppUserContact>,
}

#[derive(Debug, Default)]
struct MessagingState {
    outbox: HashMap<String, Vec<OutboundMessage>>,
    subscriptions: HashMap<String, Vec<SubscriptionRecord>>,
    groups: HashMap<String, GroupRecord>,
}

#[derive(Debug, Clone)]
struct OutboundMessage {
    id: String,
    recipient: String,
    message_type: String,
    content: Vec<u8>,
    sent_at: String,
    group_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SubscriptionRecord {
    id: String,
    message_types: Vec<String>,
    senders: Vec<String>,
    groups: Vec<String>,
}

#[derive(Debug, Clone)]
struct GroupRecord {
    id: String,
    name: String,
    members: Vec<String>,
    created_at: String,
}

pub struct RuntimeApp {
    pub wasm: Vec<u8>,
    pub running: bool,
    pub capabilities: Vec<String>,
    pub version: String,
    pub installed_at: String,
    module: Module,
    instance: Option<RuntimeInstance>,
}

#[allow(dead_code)]
struct RuntimeInstance {
    store: Store<HostState>,
    instance: wasmtime::Instance,
}

#[derive(Default)]
struct HostState {
    next_call_id: i32,
    pending: HashMap<i32, Vec<u8>>,
    app_id: String,
    app_version: String,
    installed_at: String,
    capabilities: Vec<String>,
    storage: Option<Arc<Mutex<StorageMap>>>,
    contacts: Option<Arc<Mutex<ContactsState>>>,
    notifications: Option<Arc<Mutex<NotificationState>>>,
    sync_state: Option<Arc<Mutex<SyncState>>>,
    messaging: Option<Arc<Mutex<MessagingState>>>,
    installed_apps: Option<Arc<Mutex<HashSet<String>>>>,
    registry: Option<Arc<CapabilityRegistry>>,
    identity_iid: Option<String>,
    boot_time: Option<std::time::Instant>,
    call_depth: u8,
}

impl HostState {
    fn new(
        app_id: String,
        app_version: String,
        installed_at: String,
        capabilities: Vec<String>,
        storage: Arc<Mutex<StorageMap>>,
        contacts: Arc<Mutex<ContactsState>>,
        notifications: Arc<Mutex<NotificationState>>,
        sync_state: Arc<Mutex<SyncState>>,
        messaging: Arc<Mutex<MessagingState>>,
        installed_apps: Arc<Mutex<HashSet<String>>>,
        registry: Arc<CapabilityRegistry>,
        identity_iid: Option<String>,
    ) -> Self {
        Self {
            next_call_id: 0,
            pending: HashMap::new(),
            app_id,
            app_version,
            installed_at,
            capabilities,
            storage: Some(storage),
            contacts: Some(contacts),
            notifications: Some(notifications),
            sync_state: Some(sync_state),
            messaging: Some(messaging),
            installed_apps: Some(installed_apps),
            registry: Some(registry),
            identity_iid,
            boot_time: Some(std::time::Instant::now()),
            call_depth: 0,
        }
    }
}

pub struct RuntimeManager {
    engine: Engine,
    apps: HashMap<String, RuntimeApp>,
    storage: Arc<Mutex<StorageMap>>,
    contacts: Arc<Mutex<ContactsState>>,
    notifications: Arc<Mutex<NotificationState>>,
    sync_state: Arc<Mutex<SyncState>>,
    messaging: Arc<Mutex<MessagingState>>,
    installed_apps: Arc<Mutex<HashSet<String>>>,
    registry: Arc<CapabilityRegistry>,
    identity_iid: Option<String>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            engine: Engine::default(),
            apps: HashMap::new(),
            storage: Arc::new(Mutex::new(HashMap::new())),
            contacts: Arc::new(Mutex::new(ContactsState::default())),
            notifications: Arc::new(Mutex::new(NotificationState::default())),
            sync_state: Arc::new(Mutex::new(SyncState::default())),
            messaging: Arc::new(Mutex::new(MessagingState::default())),
            installed_apps: Arc::new(Mutex::new(HashSet::new())),
            registry: Arc::new(default_registry()),
            identity_iid: None,
        }
    }

    pub fn install(&mut self, app_id: &str, wasm: Vec<u8>) -> Result<()> {
        self.install_with_metadata(app_id, wasm, "0.0.0", Vec::new())
    }

    pub fn install_with_metadata(
        &mut self,
        app_id: &str,
        wasm: Vec<u8>,
        version: &str,
        capabilities: Vec<String>,
    ) -> Result<()> {
        if wasm.is_empty() {
            return Err(PostUrbitError::InvalidInput("wasm empty"));
        }
        let module = Module::new(&self.engine, &wasm)
            .map_err(|_| PostUrbitError::InvalidInput("wasm module"))?;
        validate_exports(&module)?;
        self.apps.insert(
            app_id.to_string(),
            RuntimeApp {
                wasm,
                running: false,
                capabilities,
                version: version.to_string(),
                installed_at: Utc::now().to_rfc3339(),
                module,
                instance: None,
            },
        );
        if let Ok(mut installed) = self.installed_apps.lock() {
            installed.insert(app_id.to_string());
        }
        Ok(())
    }

    pub fn set_identity_iid(&mut self, iid: String) {
        self.identity_iid = Some(iid);
    }

    pub fn set_contacts(&mut self, contacts: Vec<ContactSummary>, app_users: Vec<AppUserContact>) {
        if let Ok(mut state) = self.contacts.lock() {
            state.contacts = contacts;
            state.app_users = app_users;
        }
    }

    pub fn start(&mut self, app_id: &str) -> Result<()> {
        let app = self
            .apps
            .get_mut(app_id)
            .ok_or(PostUrbitError::InvalidInput("app not installed"))?;
        let mut linker = Linker::new(&self.engine);
        define_host_imports(&mut linker)?;
        let mut store = Store::new(
            &self.engine,
            HostState::new(
                app_id.to_string(),
                app.version.clone(),
                app.installed_at.clone(),
                app.capabilities.clone(),
                self.storage.clone(),
                self.contacts.clone(),
                self.notifications.clone(),
                self.sync_state.clone(),
                self.messaging.clone(),
                self.installed_apps.clone(),
                self.registry.clone(),
                self.identity_iid.clone(),
            ),
        );
        let instance = linker
            .instantiate(&mut store, &app.module)
            .map_err(|_| PostUrbitError::InvalidInput("wasm instantiate"))?;
        if let Some(func) = instance.get_func(&mut store, "_start") {
            func.call(&mut store, &[], &mut [])
                .map_err(|_| PostUrbitError::InvalidInput("wasm start"))?;
        }
        app.instance = Some(RuntimeInstance { store, instance });
        app.running = true;
        Ok(())
    }

    pub fn stop(&mut self, app_id: &str) -> Result<()> {
        let app = self
            .apps
            .get_mut(app_id)
            .ok_or(PostUrbitError::InvalidInput("app not installed"))?;
        app.running = false;
        app.instance = None;
        Ok(())
    }

    pub fn uninstall(&mut self, app_id: &str) -> Result<()> {
        self.apps
            .remove(app_id)
            .ok_or(PostUrbitError::InvalidInput("app not installed"))?;
        if let Ok(mut installed) = self.installed_apps.lock() {
            installed.remove(app_id);
        }
        Ok(())
    }

    pub fn is_running(&self, app_id: &str) -> Result<bool> {
        let app = self
            .apps
            .get(app_id)
            .ok_or(PostUrbitError::InvalidInput("app not installed"))?;
        Ok(app.running)
    }
}

fn default_registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    registry.register("storage.get", "storage:app");
    registry.register("storage.set", "storage:app");
    registry.register("storage.delete", "storage:app");
    registry.register("storage.list", "storage:app");
    registry.register("messaging.send", "messaging:send");
    registry.register("messaging.send_group", "messaging:send");
    registry.register("messaging.subscribe", "messaging:subscribe");
    registry.register("messaging.create_group", "messaging:group");
    registry.register("contacts.list", "contacts:read");
    registry.register("contacts.list_app_users", "contacts:read:limited");
    registry.register("sync.create_document", "sync:documents");
    registry.register("sync.apply_operation", "sync:documents");
    registry.register("notifications.show", "notifications:show");
    registry.register("notifications.set_badge", "notifications:badge");
    registry.register("system.get_time", "system:time");
    registry.register("system.get_random", "system:random");
    registry.register("system.get_deterministic_random", "");
    registry.register("system.get_identity", "system:identity:read");
    registry.register("system.get_app_info", "");
    registry
}

fn define_host_imports(linker: &mut Linker<HostState>) -> Result<()> {
    linker
        .func_wrap("host", "call", host_call)
        .map_err(|_| PostUrbitError::InvalidInput("host call"))?;
    linker
        .func_wrap("host", "get_result", host_get_result)
        .map_err(|_| PostUrbitError::InvalidInput("host get_result"))?;
    linker
        .func_wrap("host", "get_result_len", host_get_result_len)
        .map_err(|_| PostUrbitError::InvalidInput("host get_result_len"))?;
    linker
        .func_wrap("host", "poll", host_poll)
        .map_err(|_| PostUrbitError::InvalidInput("host poll"))?;
    linker
        .func_wrap("host", "log", host_log)
        .map_err(|_| PostUrbitError::InvalidInput("host log"))?;
    Ok(())
}

fn host_call(
    mut caller: Caller<'_, HostState>,
    method_ptr: i32,
    method_len: i32,
    args_ptr: i32,
    args_len: i32,
) -> i32 {
    if method_len < 0 || method_len > 64 {
        return -1;
    }
    let method_bytes = match read_memory(&mut caller, method_ptr, method_len) {
        Ok(bytes) => bytes,
        Err(_) => return -1,
    };
    if std::str::from_utf8(&method_bytes).is_err() {
        return -1;
    }
    let args_bytes = match read_memory(&mut caller, args_ptr, args_len) {
        Ok(bytes) => bytes,
        Err(_) => return -2,
    };
    let args_value = match serde_cbor::from_slice::<serde_cbor::Value>(&args_bytes) {
        Ok(value) => value,
        Err(_) => return -2,
    };
    let method = match std::str::from_utf8(&method_bytes) {
        Ok(value) => value,
        Err(_) => return -1,
    };

    let state = caller.data_mut();
    if state.pending.len() >= 16 {
        return -3;
    }
    state.next_call_id += 1;
    let call_id = state.next_call_id;
    let envelope = handle_host_call(method, args_value, state);
    let result = encode_cbor(&envelope).unwrap_or_default();
    state.pending.insert(call_id, result);
    call_id
}

fn handle_host_call(
    method: &str,
    args: serde_cbor::Value,
    state: &mut HostState,
) -> ResultEnvelope {
    let Some(registry) = state.registry.as_ref() else {
        return ResultEnvelope::error("NOT_IMPLEMENTED", "Runtime registry missing");
    };

    if method == "app.invoke" {
        return app_invoke(args, state);
    }

    if let Some(capability) = registry.capability_for(method) {
        if !capability.is_empty() && !state.capabilities.iter().any(|c| c == capability) {
            return ResultEnvelope::error("PERMISSION_DENIED", "Capability denied");
        }
    } else {
        return ResultEnvelope::not_implemented();
    }

    match method {
        "storage.get" => storage_get(args, state),
        "storage.set" => storage_set(args, state),
        "storage.delete" => storage_delete(args, state),
        "storage.list" => storage_list(args, state),
        "messaging.send" => messaging_send(args, state),
        "messaging.send_group" => messaging_send_group(args, state),
        "messaging.subscribe" => messaging_subscribe(args, state),
        "messaging.create_group" => messaging_create_group(args, state),
        "contacts.list" => contacts_list(args, state),
        "contacts.list_app_users" => contacts_list_app_users(state),
        "sync.create_document" => sync_create_document(args, state),
        "sync.apply_operation" => sync_apply_operation(args, state),
        "notifications.show" => notifications_show(args, state),
        "notifications.set_badge" => notifications_set_badge(args, state),
        "system.get_time" => system_get_time(state),
        "system.get_random" => system_get_random(args),
        "system.get_deterministic_random" => system_get_deterministic_random(args, state),
        "system.get_identity" => system_get_identity(state),
        "system.get_app_info" => system_get_app_info(state),
        _ => ResultEnvelope::not_implemented(),
    }
}

fn cbor_map(entries: Vec<(serde_cbor::Value, serde_cbor::Value)>) -> serde_cbor::Value {
    let mut map = BTreeMap::new();
    for (key, value) in entries {
        map.insert(key, value);
    }
    serde_cbor::Value::Map(map)
}

fn storage_get(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(key) = map_string_field(&args, "key") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing key");
    };
    if key.len() > 256 {
        return ResultEnvelope::error("KEY_TOO_LONG", "Key too long");
    }
    let storage = match state.storage.as_ref() {
        Some(storage) => storage,
        None => return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable"),
    };
    let map = storage.lock().ok();
    let Some(map) = map else {
        return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable");
    };
    let value = map
        .get(&state.app_id)
        .and_then(|ns| ns.get(&key))
        .cloned();
    let (value_bytes, version) = if let Some(entry) = value {
        (Some(serde_cbor::Value::Bytes(entry.value)), entry.version)
    } else {
        (None, 0)
    };
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("value".to_string()),
            value_bytes.unwrap_or(serde_cbor::Value::Null),
        ),
        (
            serde_cbor::Value::Text("version".to_string()),
            serde_cbor::Value::Integer(version as i128),
        ),
    ]))
}

fn storage_set(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(key) = map_string_field(&args, "key") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing key");
    };
    if key.len() > 256 {
        return ResultEnvelope::error("KEY_TOO_LONG", "Key too long");
    }
    let Some(value) = map_bytes_field(&args, "value") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing value");
    };
    if value.len() > 1_048_576 {
        return ResultEnvelope::error("VALUE_TOO_LARGE", "Value too large");
    }
    let expected_version = map_u64_field(&args, "expected_version");
    let storage = match state.storage.as_ref() {
        Some(storage) => storage,
        None => return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable"),
    };
    let mut map = match storage.lock() {
        Ok(map) => map,
        Err(_) => return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable"),
    };
    let ns = map.entry(state.app_id.clone()).or_default();
    if let Some(expected) = expected_version {
        let current = ns.get(&key).map(|entry| entry.version).unwrap_or(0);
        if current != expected {
            return ResultEnvelope::error("VERSION_MISMATCH", "Version mismatch");
        }
    }
    let next_version = ns.get(&key).map(|entry| entry.version + 1).unwrap_or(1);
    ns.insert(
        key,
        StoredValue {
            value,
            version: next_version,
        },
    );
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("version".to_string()),
        serde_cbor::Value::Integer(next_version as i128),
    )]))
}

fn storage_delete(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(key) = map_string_field(&args, "key") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing key");
    };
    let storage = match state.storage.as_ref() {
        Some(storage) => storage,
        None => return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable"),
    };
    let mut map = match storage.lock() {
        Ok(map) => map,
        Err(_) => return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable"),
    };
    let deleted = map
        .get_mut(&state.app_id)
        .map(|ns| ns.remove(&key).is_some())
        .unwrap_or(false);
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("deleted".to_string()),
        serde_cbor::Value::Bool(deleted),
    )]))
}

fn storage_list(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let prefix = map_string_field(&args, "prefix").unwrap_or_default();
    let cursor = map_string_field(&args, "cursor");
    let limit = map_u64_field(&args, "limit").unwrap_or(100).min(1000) as usize;
    let storage = match state.storage.as_ref() {
        Some(storage) => storage,
        None => return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable"),
    };
    let map = match storage.lock() {
        Ok(map) => map,
        Err(_) => return ResultEnvelope::error("NOT_AVAILABLE", "Storage unavailable"),
    };
    let mut keys: Vec<String> = map
        .get(&state.app_id)
        .map(|ns| {
            ns.keys()
                .filter(|key| key.starts_with(&prefix))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    keys.sort();
    if let Some(cursor) = cursor {
        keys.retain(|key| key > &cursor);
    }
    let has_more = keys.len() > limit;
    let sliced = keys.into_iter().take(limit).collect::<Vec<_>>();
    let next_cursor = if has_more { sliced.last().cloned() } else { None };
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("keys".to_string()),
            serde_cbor::Value::Array(sliced.into_iter().map(serde_cbor::Value::Text).collect()),
        ),
        (
            serde_cbor::Value::Text("cursor".to_string()),
            next_cursor
                .map(serde_cbor::Value::Text)
                .unwrap_or(serde_cbor::Value::Null),
        ),
        (
            serde_cbor::Value::Text("has_more".to_string()),
            serde_cbor::Value::Bool(has_more),
        ),
    ]))
}

fn system_get_time(state: &HostState) -> ResultEnvelope {
    let timestamp = Utc::now().to_rfc3339();
    let monotonic = state
        .boot_time
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("timestamp".to_string()),
            serde_cbor::Value::Text(timestamp),
        ),
        (
            serde_cbor::Value::Text("monotonic_ns".to_string()),
            serde_cbor::Value::Integer(monotonic as i128),
        ),
    ]))
}

fn system_get_random(args: serde_cbor::Value) -> ResultEnvelope {
    let Some(length) = map_u64_field(&args, "length") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing length");
    };
    let length = length.min(1024) as usize;
    let mut bytes = vec![0u8; length];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("bytes".to_string()),
        serde_cbor::Value::Bytes(bytes),
    )]))
}

fn system_get_deterministic_random(args: serde_cbor::Value, state: &HostState) -> ResultEnvelope {
    let Some(length) = map_u64_field(&args, "length") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing length");
    };
    let length = length.min(1024) as usize;
    let seed = map_bytes_field(&args, "seed")
        .map(|bytes| bytes[..bytes.len().min(32)].to_vec())
        .unwrap_or_else(|| {
            let mut hasher = Sha256::new();
            hasher.update(state.app_id.as_bytes());
            hasher.finalize().to_vec()
        });
    let mut out = Vec::with_capacity(length);
    let mut counter = 0u64;
    while out.len() < length {
        let mut hasher = Sha256::new();
        hasher.update(&seed);
        hasher.update(counter.to_be_bytes());
        let chunk = hasher.finalize();
        out.extend_from_slice(&chunk);
        counter += 1;
    }
    out.truncate(length);
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("bytes".to_string()),
        serde_cbor::Value::Bytes(out),
    )]))
}

fn system_get_identity(state: &HostState) -> ResultEnvelope {
    let Some(iid) = state.identity_iid.as_ref() else {
        return ResultEnvelope::error("NOT_AVAILABLE", "Identity unavailable");
    };
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("iid".to_string()),
        serde_cbor::Value::Text(iid.clone()),
    )]))
}

fn system_get_app_info(state: &HostState) -> ResultEnvelope {
    let storage_used = state
        .storage
        .as_ref()
        .and_then(|storage| storage.lock().ok())
        .and_then(|map| map.get(&state.app_id).cloned())
        .map(|ns| ns.values().map(|entry| entry.value.len() as u64).sum())
        .unwrap_or(0);
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("app_id".to_string()),
            serde_cbor::Value::Text(state.app_id.clone()),
        ),
        (
            serde_cbor::Value::Text("version".to_string()),
            serde_cbor::Value::Text(state.app_version.clone()),
        ),
        (
            serde_cbor::Value::Text("installed_at".to_string()),
            serde_cbor::Value::Text(state.installed_at.clone()),
        ),
        (
            serde_cbor::Value::Text("storage_used".to_string()),
            serde_cbor::Value::Integer(storage_used as i128),
        ),
        (
            serde_cbor::Value::Text("capabilities_granted".to_string()),
            serde_cbor::Value::Array(
                state
                    .capabilities
                    .iter()
                    .cloned()
                    .map(serde_cbor::Value::Text)
                    .collect(),
            ),
        ),
    ]))
}

fn app_invoke(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(target_app) = map_string_field(&args, "target_app") else {
        return ResultEnvelope::error("APP_NOT_FOUND", "Missing target app");
    };
    let Some(_method) = map_string_field(&args, "method") else {
        return ResultEnvelope::error("METHOD_NOT_FOUND", "Missing method");
    };
    let args_bytes = map_bytes_field(&args, "args").unwrap_or_default();

    if state.call_depth >= 8 {
        return ResultEnvelope::error("CALL_DEPTH_EXCEEDED", "Call depth exceeded");
    }

    if !has_app_invoke_capability(&state.capabilities, &target_app) {
        return ResultEnvelope::error("PERMISSION_DENIED", "Capability denied");
    }

    if let Some(installed) = state.installed_apps.as_ref().and_then(|set| set.lock().ok()) {
        if !installed.contains(&target_app) {
            return ResultEnvelope::error("APP_NOT_INSTALLED", "Target app not installed");
        }
    }

    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("result".to_string()),
        serde_cbor::Value::Bytes(args_bytes),
    )]))
}

fn messaging_send(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(recipient) = map_string_field(&args, "recipient") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing recipient");
    };
    let Some(message_type) = map_string_field(&args, "message_type") else {
        return ResultEnvelope::error("INVALID_MESSAGE_TYPE", "Missing message type");
    };
    let Some(content) = map_bytes_field(&args, "content") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing content");
    };
    if content.len() > 1_048_576 {
        return ResultEnvelope::error("MESSAGE_TOO_LARGE", "Message too large");
    }
    let id = Uuid::new_v4().to_string();
    let sent_at = Utc::now().to_rfc3339();
    if let Some(outbox) = state.messaging.as_ref().and_then(|m| m.lock().ok()) {
        drop(outbox);
    }
    if let Some(messaging) = state.messaging.as_ref() {
        if let Ok(mut data) = messaging.lock() {
            data.outbox
                .entry(state.app_id.clone())
                .or_default()
                .push(OutboundMessage {
                    id: id.clone(),
                    recipient,
                    message_type,
                    content,
                    sent_at: sent_at.clone(),
                    group_id: None,
                });
        }
    }
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("message_id".to_string()),
            serde_cbor::Value::Text(id),
        ),
        (
            serde_cbor::Value::Text("sent_at".to_string()),
            serde_cbor::Value::Text(sent_at),
        ),
    ]))
}

fn messaging_send_group(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(group_id) = map_string_field(&args, "group_id") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing group_id");
    };
    let Some(message_type) = map_string_field(&args, "message_type") else {
        return ResultEnvelope::error("INVALID_MESSAGE_TYPE", "Missing message type");
    };
    let Some(content) = map_bytes_field(&args, "content") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing content");
    };
    if content.len() > 1_048_576 {
        return ResultEnvelope::error("MESSAGE_TOO_LARGE", "Message too large");
    }
    let id = Uuid::new_v4().to_string();
    let sent_at = Utc::now().to_rfc3339();
    if let Some(messaging) = state.messaging.as_ref() {
        if let Ok(mut data) = messaging.lock() {
            data.outbox
                .entry(state.app_id.clone())
                .or_default()
                .push(OutboundMessage {
                    id: id.clone(),
                    recipient: group_id.clone(),
                    message_type,
                    content,
                    sent_at: sent_at.clone(),
                    group_id: Some(group_id),
                });
        }
    }
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("message_id".to_string()),
            serde_cbor::Value::Text(id),
        ),
        (
            serde_cbor::Value::Text("sent_at".to_string()),
            serde_cbor::Value::Text(sent_at),
        ),
    ]))
}

fn messaging_subscribe(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let mut message_types = Vec::new();
    let mut senders = Vec::new();
    let mut groups = Vec::new();
    if let serde_cbor::Value::Map(entries) = &args {
        for (key, value) in entries {
            if let serde_cbor::Value::Text(name) = key {
                if name == "filter" {
                    if let serde_cbor::Value::Map(filter) = value {
                        let filter_value = serde_cbor::Value::Map(filter.clone());
                        message_types = map_array_text_field(&filter_value, "message_types").unwrap_or_default();
                        senders = map_array_text_field(&filter_value, "senders").unwrap_or_default();
                        groups = map_array_text_field(&filter_value, "groups").unwrap_or_default();
                    }
                }
            }
        }
    }
    let id = Uuid::new_v4().to_string();
    if let Some(messaging) = state.messaging.as_ref() {
        if let Ok(mut data) = messaging.lock() {
            data.subscriptions
                .entry(state.app_id.clone())
                .or_default()
                .push(SubscriptionRecord {
                    id: id.clone(),
                    message_types,
                    senders,
                    groups,
                });
        }
    }
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("subscription_id".to_string()),
        serde_cbor::Value::Text(id),
    )]))
}

fn messaging_create_group(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(name) = map_string_field(&args, "name") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing name");
    };
    if name.len() > 100 {
        return ResultEnvelope::error("NAME_TOO_LONG", "Group name too long");
    }
    let members = map_array_text_field(&args, "members").unwrap_or_default();
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    if let Some(messaging) = state.messaging.as_ref() {
        if let Ok(mut data) = messaging.lock() {
            data.groups.insert(
                id.clone(),
                GroupRecord {
                    id: id.clone(),
                    name: name.clone(),
                    members,
                    created_at: created_at.clone(),
                },
            );
        }
    }
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("group_id".to_string()),
            serde_cbor::Value::Text(id),
        ),
        (
            serde_cbor::Value::Text("created_at".to_string()),
            serde_cbor::Value::Text(created_at),
        ),
    ]))
}

fn contacts_list(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let cursor = map_string_field(&args, "cursor");
    let limit = map_u64_field(&args, "limit").unwrap_or(100).min(1000) as usize;
    let contacts_state = match state.contacts.as_ref().and_then(|c| c.lock().ok()) {
        Some(state) => state,
        None => return ResultEnvelope::error("NOT_AVAILABLE", "Contacts unavailable"),
    };
    let mut contacts = contacts_state.contacts.clone();
    contacts.sort_by(|a, b| a.iid.cmp(&b.iid));
    if let Some(cursor) = cursor {
        contacts.retain(|contact| contact.iid > cursor);
    }
    let has_more = contacts.len() > limit;
    let sliced = contacts.into_iter().take(limit).collect::<Vec<_>>();
    let next_cursor = if has_more { sliced.last().map(|c| c.iid.clone()) } else { None };
    let list = sliced
        .into_iter()
        .map(|contact| {
            let mut entries = vec![
                (
                    serde_cbor::Value::Text("iid".to_string()),
                    serde_cbor::Value::Text(contact.iid),
                ),
            ];
            if let Some(name) = contact.name {
                entries.push((
                    serde_cbor::Value::Text("name".to_string()),
                    serde_cbor::Value::Text(name),
                ));
            }
            if let Some(avatar) = contact.avatar_hash {
                entries.push((
                    serde_cbor::Value::Text("avatar_hash".to_string()),
                    serde_cbor::Value::Text(avatar),
                ));
            }
            if let Some(last_seen) = contact.last_seen {
                entries.push((
                    serde_cbor::Value::Text("last_seen".to_string()),
                    serde_cbor::Value::Text(last_seen),
                ));
            }
            cbor_map(entries)
        })
        .collect();
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("contacts".to_string()),
            serde_cbor::Value::Array(list),
        ),
        (
            serde_cbor::Value::Text("cursor".to_string()),
            next_cursor
                .map(serde_cbor::Value::Text)
                .unwrap_or(serde_cbor::Value::Null),
        ),
        (
            serde_cbor::Value::Text("has_more".to_string()),
            serde_cbor::Value::Bool(has_more),
        ),
    ]))
}

fn contacts_list_app_users(state: &mut HostState) -> ResultEnvelope {
    let contacts_state = match state.contacts.as_ref().and_then(|c| c.lock().ok()) {
        Some(state) => state,
        None => return ResultEnvelope::error("NOT_AVAILABLE", "Contacts unavailable"),
    };
    let list = contacts_state
        .app_users
        .iter()
        .map(|contact| {
            let mut entries = vec![
                (
                    serde_cbor::Value::Text("iid".to_string()),
                    serde_cbor::Value::Text(contact.iid.clone()),
                ),
            ];
            if let Some(name) = &contact.name {
                entries.push((
                    serde_cbor::Value::Text("name".to_string()),
                    serde_cbor::Value::Text(name.clone()),
                ));
            }
            if let Some(avatar) = &contact.avatar_hash {
                entries.push((
                    serde_cbor::Value::Text("avatar_hash".to_string()),
                    serde_cbor::Value::Text(avatar.clone()),
                ));
            }
            if let Some(app_data) = &contact.app_data {
                entries.push((
                    serde_cbor::Value::Text("app_data".to_string()),
                    serde_cbor::Value::Bytes(app_data.clone()),
                ));
            }
            cbor_map(entries)
        })
        .collect();
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("contacts".to_string()),
        serde_cbor::Value::Array(list),
    )]))
}

fn sync_create_document(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(document_type) = map_string_field(&args, "document_type") else {
        return ResultEnvelope::error("INVALID_STATE", "Missing document_type");
    };
    let access = match map_access_field(&args, "access") {
        Some(access) => access,
        None => return ResultEnvelope::error("INVALID_STATE", "Missing access"),
    };
    let id = format!("{:x}", Sha256::digest(format!("{}:{}:{}", state.app_id, document_type, Uuid::new_v4()).as_bytes()));
    let created_at = Utc::now().to_rfc3339();
    if let Some(sync_state) = state.sync_state.as_ref() {
        if let Ok(mut data) = sync_state.lock() {
            data.documents.insert(
                id.clone(),
                SyncDocument {
                    id: id.clone(),
                    document_type,
                    access,
                    created_at: created_at.clone(),
                    operations: Vec::new(),
                },
            );
        }
    }
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("document_id".to_string()),
            serde_cbor::Value::Text(id),
        ),
        (
            serde_cbor::Value::Text("created_at".to_string()),
            serde_cbor::Value::Text(created_at),
        ),
    ]))
}

fn sync_apply_operation(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(document_id) = map_string_field(&args, "document_id") else {
        return ResultEnvelope::error("DOCUMENT_NOT_FOUND", "Missing document_id");
    };
    let Some(operation) = map_bytes_field(&args, "operation") else {
        return ResultEnvelope::error("INVALID_OPERATION", "Missing operation");
    };
    let Some(sync_state) = state.sync_state.as_ref() else {
        return ResultEnvelope::error("DOCUMENT_NOT_FOUND", "Sync unavailable");
    };
    let mut data = match sync_state.lock() {
        Ok(data) => data,
        Err(_) => return ResultEnvelope::error("DOCUMENT_NOT_FOUND", "Sync unavailable"),
    };
    let doc = match data.documents.get_mut(&document_id) {
        Some(doc) => doc,
        None => return ResultEnvelope::error("DOCUMENT_NOT_FOUND", "Document not found"),
    };
    if let Some(iid) = state.identity_iid.as_ref() {
        if !doc.access.writers.contains(iid) && doc.access.owner != *iid {
            return ResultEnvelope::error("ACCESS_DENIED", "Access denied");
        }
    }
    doc.operations.push(operation);
    let op_id = format!("{:x}", Sha256::digest(Uuid::new_v4().as_bytes()));
    let applied_at = Utc::now().to_rfc3339();
    ResultEnvelope::ok(cbor_map(vec![
        (
            serde_cbor::Value::Text("operation_id".to_string()),
            serde_cbor::Value::Text(op_id),
        ),
        (
            serde_cbor::Value::Text("applied_at".to_string()),
            serde_cbor::Value::Text(applied_at),
        ),
    ]))
}

fn notifications_show(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(title) = map_string_field(&args, "title") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing title");
    };
    let Some(body) = map_string_field(&args, "body") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing body");
    };
    if title.len() > 100 {
        return ResultEnvelope::error("TITLE_TOO_LONG", "Title too long");
    }
    if body.len() > 500 {
        return ResultEnvelope::error("BODY_TOO_LONG", "Body too long");
    }
    let id = map_string_field(&args, "id").unwrap_or_else(|| Uuid::new_v4().to_string());
    let icon = map_string_field(&args, "icon");
    let sound = map_bool_field(&args, "sound").unwrap_or(false);
    if let Some(notifications) = state.notifications.as_ref() {
        if let Ok(mut data) = notifications.lock() {
            data.notifications
                .entry(state.app_id.clone())
                .or_default()
                .push(NotificationRecord {
                    id: id.clone(),
                    title,
                    body,
                    icon,
                    sound,
                    created_at: Utc::now().to_rfc3339(),
                });
        }
    }
    ResultEnvelope::ok(cbor_map(vec![(
        serde_cbor::Value::Text("notification_id".to_string()),
        serde_cbor::Value::Text(id),
    )]))
}

fn notifications_set_badge(args: serde_cbor::Value, state: &mut HostState) -> ResultEnvelope {
    let Some(count) = map_u64_field(&args, "count") else {
        return ResultEnvelope::error("INVALID_REQUEST", "Missing count");
    };
    if let Some(notifications) = state.notifications.as_ref() {
        if let Ok(mut data) = notifications.lock() {
            data.badges.insert(state.app_id.clone(), count);
        }
    }
    ResultEnvelope::ok(cbor_map(Vec::new()))
}

fn map_bool_field(value: &serde_cbor::Value, key: &str) -> Option<bool> {
    let serde_cbor::Value::Map(entries) = value else {
        return None;
    };
    entries.iter().find_map(|(k, v)| match (k, v) {
        (serde_cbor::Value::Text(name), serde_cbor::Value::Bool(value)) if name == key => {
            Some(*value)
        }
        _ => None,
    })
}

fn map_access_field(value: &serde_cbor::Value, key: &str) -> Option<DocumentAccess> {
    let serde_cbor::Value::Map(entries) = value else {
        return None;
    };
    let access_value = entries.iter().find_map(|(k, v)| match (k, v) {
        (serde_cbor::Value::Text(name), value) if name == key => Some(value.clone()),
        _ => None,
    })?;
    let owner = map_string_field(&access_value, "owner")?;
    let readers = map_array_text_field(&access_value, "readers").unwrap_or_default();
    let writers = map_array_text_field(&access_value, "writers").unwrap_or_default();
    Some(DocumentAccess { owner, readers, writers })
}

fn has_app_invoke_capability(capabilities: &[String], target_app: &str) -> bool {
    let target = format!("app:invoke:{target_app}");
    capabilities.iter().any(|cap| cap == "app:invoke:any" || cap == &target)
}

fn map_string_field(value: &serde_cbor::Value, key: &str) -> Option<String> {
    let serde_cbor::Value::Map(entries) = value else {
        return None;
    };
    entries.iter().find_map(|(k, v)| match (k, v) {
        (serde_cbor::Value::Text(name), serde_cbor::Value::Text(value)) if name == key => {
            Some(value.clone())
        }
        _ => None,
    })
}

fn map_bytes_field(value: &serde_cbor::Value, key: &str) -> Option<Vec<u8>> {
    let serde_cbor::Value::Map(entries) = value else {
        return None;
    };
    entries.iter().find_map(|(k, v)| match (k, v) {
        (serde_cbor::Value::Text(name), serde_cbor::Value::Bytes(value)) if name == key => {
            Some(value.clone())
        }
        _ => None,
    })
}

fn map_u64_field(value: &serde_cbor::Value, key: &str) -> Option<u64> {
    let serde_cbor::Value::Map(entries) = value else {
        return None;
    };
    entries.iter().find_map(|(k, v)| match (k, v) {
        (serde_cbor::Value::Text(name), serde_cbor::Value::Integer(value)) if name == key => {
            if *value >= 0 {
                Some(*value as u64)
            } else {
                None
            }
        }
        _ => None,
    })
}

fn map_array_text_field(value: &serde_cbor::Value, key: &str) -> Option<Vec<String>> {
    let serde_cbor::Value::Map(entries) = value else {
        return None;
    };
    entries.iter().find_map(|(k, v)| match (k, v) {
        (serde_cbor::Value::Text(name), serde_cbor::Value::Array(items)) if name == key => {
            let mut out = Vec::new();
            for item in items {
                if let serde_cbor::Value::Text(text) = item {
                    out.push(text.clone());
                }
            }
            Some(out)
        }
        _ => None,
    })
}

fn host_get_result(
    mut caller: Caller<'_, HostState>,
    call_id: i32,
    result_ptr: i32,
    result_max: i32,
) -> i32 {
    let result = match caller.data().pending.get(&call_id) {
        Some(value) => value.clone(),
        None => return -1,
    };
    if result_max < 0 || result.len() > result_max as usize {
        return -2;
    }
    if write_memory(&mut caller, result_ptr, &result).is_err() {
        return -2;
    }
    caller.data_mut().pending.remove(&call_id);
    result.len() as i32
}

fn host_get_result_len(caller: Caller<'_, HostState>, call_id: i32) -> i32 {
    caller
        .data()
        .pending
        .get(&call_id)
        .map(|value| value.len() as i32)
        .unwrap_or(-1)
}

fn host_poll(caller: Caller<'_, HostState>, _timeout_ms: i32) -> i32 {
    caller
        .data()
        .pending
        .keys()
        .next()
        .copied()
        .unwrap_or(-1)
}

fn host_log(mut caller: Caller<'_, HostState>, _level: i32, msg_ptr: i32, msg_len: i32) {
    let _ = read_memory(&mut caller, msg_ptr, msg_len.min(1024));
}

fn read_memory(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Result<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return Err(PostUrbitError::InvalidInput("memory bounds"));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|ext| ext.into_memory())
        .ok_or(PostUrbitError::InvalidInput("memory export"))?;
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start
        .checked_add(len as usize)
        .ok_or(PostUrbitError::InvalidInput("memory bounds"))?;
    if end > data.len() {
        return Err(PostUrbitError::InvalidInput("memory bounds"));
    }
    Ok(data[start..end].to_vec())
}

fn write_memory(caller: &mut Caller<'_, HostState>, ptr: i32, bytes: &[u8]) -> Result<()> {
    if ptr < 0 {
        return Err(PostUrbitError::InvalidInput("memory bounds"));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|ext| ext.into_memory())
        .ok_or(PostUrbitError::InvalidInput("memory export"))?;
    let data = memory.data_mut(caller);
    let start = ptr as usize;
    let end = start
        .checked_add(bytes.len())
        .ok_or(PostUrbitError::InvalidInput("memory bounds"))?;
    if end > data.len() {
        return Err(PostUrbitError::InvalidInput("memory bounds"));
    }
    data[start..end].copy_from_slice(bytes);
    Ok(())
}

fn validate_exports(module: &Module) -> Result<()> {
    let mut required = HashMap::from([
        ("_start", None),
        ("handle", None),
        ("get_error", None),
        ("memory", None),
        ("alloc", None),
        ("dealloc", None),
    ]);

    for export in module.exports() {
        if let Some(entry) = required.get_mut(export.name()) {
            *entry = Some(export.ty());
        }
    }

    for (name, entry) in required {
        let Some(export_type) = entry else {
            return Err(PostUrbitError::InvalidInput("missing export"));
        };
        match (name, export_type) {
            ("memory", ExternType::Memory(_)) => {}
            ("_start", ExternType::Func(func)) => {
                let params: Vec<_> = func.params().collect();
                if !params.is_empty() {
                    return Err(PostUrbitError::InvalidInput("_start export"));
                }
            }
            ("handle", ExternType::Func(func)) => {
                let params: Vec<_> = func.params().collect();
                let results: Vec<_> = func.results().collect();
                if params != vec![wasmtime::ValType::I32, wasmtime::ValType::I32]
                    || results != vec![wasmtime::ValType::I64]
                {
                    return Err(PostUrbitError::InvalidInput("handle export"));
                }
            }
            ("get_error", ExternType::Func(func)) => {
                let params: Vec<_> = func.params().collect();
                let results: Vec<_> = func.results().collect();
                if !params.is_empty() || results != vec![wasmtime::ValType::I64] {
                    return Err(PostUrbitError::InvalidInput("get_error export"));
                }
            }
            ("alloc", ExternType::Func(func)) => {
                let params: Vec<_> = func.params().collect();
                let results: Vec<_> = func.results().collect();
                if params != vec![wasmtime::ValType::I32] || results != vec![wasmtime::ValType::I32]
                {
                    return Err(PostUrbitError::InvalidInput("alloc export"));
                }
            }
            ("dealloc", ExternType::Func(func)) => {
                let params: Vec<_> = func.params().collect();
                let results: Vec<_> = func.results().collect();
                if params
                    != vec![wasmtime::ValType::I32, wasmtime::ValType::I32]
                    || !results.is_empty()
                {
                    return Err(PostUrbitError::InvalidInput("dealloc export"));
                }
            }
            _ => return Err(PostUrbitError::InvalidInput("export type")),
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct ResultEnvelope {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<serde_cbor::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorEnvelope>,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    code: String,
    message: String,
}

impl ResultEnvelope {
    fn ok(value: serde_cbor::Value) -> Self {
        Self {
            ok: true,
            value: Some(value),
            error: None,
        }
    }

    fn error(code: &str, message: &str) -> Self {
        Self {
            ok: false,
            value: None,
            error: Some(ErrorEnvelope {
                code: code.to_string(),
                message: message.to_string(),
            }),
        }
    }

    fn not_implemented() -> Self {
        Self::error("NOT_IMPLEMENTED", "Method not implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_cbor::Value as CborValue;

    #[test]
    fn runtime_install_start_stop() {
        let mut rt = RuntimeManager::new();
        let wasm = minimal_wasm();
        rt.install("app", wasm).unwrap();
        rt.start("app").unwrap();
        assert!(rt.is_running("app").unwrap());
        rt.stop("app").unwrap();
        assert!(!rt.is_running("app").unwrap());
        rt.uninstall("app").unwrap();
    }

    #[test]
    fn runtime_start_without_install_fails() {
        let mut rt = RuntimeManager::new();
        let err = rt.start("missing").unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    fn minimal_wasm() -> Vec<u8> {
        let wat = r#"
        (module
          (memory (export "memory") 1)
          (func (export "_start"))
          (func (export "handle") (param i32 i32) (result i64) (i64.const 0))
          (func (export "get_error") (result i64) (i64.const 0))
          (func (export "alloc") (param i32) (result i32) (i32.const 0))
          (func (export "dealloc") (param i32 i32))
        )
        "#;
        wat::parse_str(wat).unwrap()
    }

    fn build_state(caps: Vec<String>) -> HostState {
        HostState::new(
            "app.test".to_string(),
            "1.0.0".to_string(),
            Utc::now().to_rfc3339(),
            caps,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(ContactsState::default())),
            Arc::new(Mutex::new(NotificationState::default())),
            Arc::new(Mutex::new(SyncState::default())),
            Arc::new(Mutex::new(MessagingState::default())),
            Arc::new(Mutex::new(HashSet::new())),
            Arc::new(default_registry()),
            Some("iid-test".to_string()),
        )
    }

    fn cbor_map_test(entries: Vec<(&str, CborValue)>) -> CborValue {
        let mut map = BTreeMap::new();
        for (key, value) in entries {
            map.insert(CborValue::Text(key.to_string()), value);
        }
        CborValue::Map(map)
    }

    #[test]
    fn host_storage_set_get_round_trip() {
        let mut state = build_state(vec!["storage:app".to_string()]);
        let set_args = cbor_map_test(vec![
            ("key", CborValue::Text("key".to_string())),
            ("value", CborValue::Bytes(b"hello".to_vec())),
        ]);
        let set_result = handle_host_call("storage.set", set_args, &mut state);
        assert!(set_result.ok);

        let get_args = cbor_map_test(vec![("key", CborValue::Text("key".to_string()))]);
        let get_result = handle_host_call("storage.get", get_args, &mut state);
        assert!(get_result.ok);
        let Some(CborValue::Map(map)) = get_result.value else {
            panic!("missing value");
        };
        let value = map.iter().find_map(|(k, v)| match (k, v) {
            (CborValue::Text(name), CborValue::Bytes(bytes)) if name == "value" => Some(bytes),
            _ => None,
        });
        assert_eq!(value, Some(&b"hello".to_vec()));
    }

    #[test]
    fn host_storage_denies_without_capability() {
        let mut state = build_state(Vec::new());
        let args = cbor_map_test(vec![("key", CborValue::Text("key".to_string()))]);
        let result = handle_host_call("storage.get", args, &mut state);
        assert!(!result.ok);
        assert_eq!(result.error.unwrap().code, "PERMISSION_DENIED");
    }

    #[test]
    fn host_system_random_generates_bytes() {
        let mut state = build_state(vec!["system:random".to_string()]);
        let args = cbor_map_test(vec![("length", CborValue::Integer(16))]);
        let result = handle_host_call("system.get_random", args, &mut state);
        assert!(result.ok);
        let Some(CborValue::Map(map)) = result.value else {
            panic!("missing value");
        };
        let bytes_len = map.iter().find_map(|(k, v)| match (k, v) {
            (CborValue::Text(name), CborValue::Bytes(bytes)) if name == "bytes" => Some(bytes.len()),
            _ => None,
        });
        assert_eq!(bytes_len, Some(16));
    }

    #[test]
    fn host_contacts_list_returns_entries() {
        let contacts_state = Arc::new(Mutex::new(ContactsState {
            contacts: vec![ContactSummary {
                iid: "iid-a".to_string(),
                name: Some("Alice".to_string()),
                avatar_hash: None,
                last_seen: None,
            }],
            app_users: Vec::new(),
        }));
        let mut state = HostState::new(
            "app.test".to_string(),
            "1.0.0".to_string(),
            Utc::now().to_rfc3339(),
            vec!["contacts:read".to_string()],
            Arc::new(Mutex::new(HashMap::new())),
            contacts_state,
            Arc::new(Mutex::new(NotificationState::default())),
            Arc::new(Mutex::new(SyncState::default())),
            Arc::new(Mutex::new(MessagingState::default())),
            Arc::new(Mutex::new(HashSet::new())),
            Arc::new(default_registry()),
            Some("iid-test".to_string()),
        );
        let result = handle_host_call("contacts.list", cbor_map_test(Vec::new()), &mut state);
        assert!(result.ok);
    }

    #[test]
    fn host_notifications_show_returns_id() {
        let mut state = build_state(vec!["notifications:show".to_string()]);
        let args = cbor_map_test(vec![
            ("title", CborValue::Text("Hi".to_string())),
            ("body", CborValue::Text("There".to_string())),
        ]);
        let result = handle_host_call("notifications.show", args, &mut state);
        assert!(result.ok);
        let Some(CborValue::Map(map)) = result.value else {
            panic!("missing value");
        };
        let id = map.iter().find_map(|(k, v)| match (k, v) {
            (CborValue::Text(name), CborValue::Text(value)) if name == "notification_id" => {
                Some(value.clone())
            }
            _ => None,
        });
        assert!(id.is_some());
    }

    #[test]
    fn host_sync_create_and_apply_operation() {
        let mut state = build_state(vec!["sync:documents".to_string()]);
        let access = cbor_map_test(vec![
            ("owner", CborValue::Text("iid-test".to_string())),
            ("readers", CborValue::Array(Vec::new())),
            ("writers", CborValue::Array(vec![CborValue::Text("iid-test".to_string())])),
        ]);
        let create_args = cbor_map_test(vec![
            ("document_type", CborValue::Text("note".to_string())),
            ("access", access),
        ]);
        let create_result = handle_host_call("sync.create_document", create_args, &mut state);
        assert!(create_result.ok);
        let Some(CborValue::Map(map)) = create_result.value else {
            panic!("missing value");
        };
        let doc_id = map.iter().find_map(|(k, v)| match (k, v) {
            (CborValue::Text(name), CborValue::Text(value)) if name == "document_id" => {
                Some(value.clone())
            }
            _ => None,
        });
        let doc_id = doc_id.expect("doc id");

        let apply_args = cbor_map_test(vec![
            ("document_id", CborValue::Text(doc_id)),
            ("operation", CborValue::Bytes(b"op".to_vec())),
        ]);
        let apply_result = handle_host_call("sync.apply_operation", apply_args, &mut state);
        assert!(apply_result.ok);
    }

    #[test]
    fn host_app_invoke_requires_capability() {
        let mut state = build_state(Vec::new());
        let args = cbor_map_test(vec![
            ("target_app", CborValue::Text("app.target".to_string())),
            ("method", CborValue::Text("ping".to_string())),
            ("args", CborValue::Bytes(b"payload".to_vec())),
        ]);
        let result = handle_host_call("app.invoke", args, &mut state);
        assert!(!result.ok);
        assert_eq!(result.error.unwrap().code, "PERMISSION_DENIED");
    }

    #[test]
    fn host_app_invoke_returns_result() {
        let mut state = build_state(vec!["app:invoke:any".to_string()]);
        if let Some(installed) = state.installed_apps.as_ref() {
            if let Ok(mut apps) = installed.lock() {
                apps.insert("app.target".to_string());
            }
        }
        let args = cbor_map_test(vec![
            ("target_app", CborValue::Text("app.target".to_string())),
            ("method", CborValue::Text("ping".to_string())),
            ("args", CborValue::Bytes(b"payload".to_vec())),
        ]);
        let result = handle_host_call("app.invoke", args, &mut state);
        assert!(result.ok);
        let Some(CborValue::Map(map)) = result.value else {
            panic!("missing value");
        };
        let payload = map.iter().find_map(|(k, v)| match (k, v) {
            (CborValue::Text(name), CborValue::Bytes(value)) if name == "result" => Some(value.clone()),
            _ => None,
        });
        assert_eq!(payload, Some(b"payload".to_vec()));
    }
}

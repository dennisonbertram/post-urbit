use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
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
    registry: Option<Arc<CapabilityRegistry>>,
    identity_iid: Option<String>,
    boot_time: Option<std::time::Instant>,
}

impl HostState {
    fn new(
        app_id: String,
        app_version: String,
        installed_at: String,
        capabilities: Vec<String>,
        storage: Arc<Mutex<StorageMap>>,
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
            registry: Some(registry),
            identity_iid,
            boot_time: Some(std::time::Instant::now()),
        }
    }
}

pub struct RuntimeManager {
    engine: Engine,
    apps: HashMap<String, RuntimeApp>,
    storage: Arc<Mutex<StorageMap>>,
    registry: Arc<CapabilityRegistry>,
    identity_iid: Option<String>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            engine: Engine::default(),
            apps: HashMap::new(),
            storage: Arc::new(Mutex::new(HashMap::new())),
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
        Ok(())
    }

    pub fn set_identity_iid(&mut self, iid: String) {
        self.identity_iid = Some(iid);
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
            Arc::new(default_registry()),
            Some("iid-test".to_string()),
        )
    }

    fn map(entries: Vec<(&str, CborValue)>) -> CborValue {
        let mut map = BTreeMap::new();
        for (key, value) in entries {
            map.insert(CborValue::Text(key.to_string()), value);
        }
        CborValue::Map(map)
    }

    #[test]
    fn host_storage_set_get_round_trip() {
        let mut state = build_state(vec!["storage:app".to_string()]);
        let set_args = map(vec![
            ("key", CborValue::Text("key".to_string())),
            ("value", CborValue::Bytes(b"hello".to_vec())),
        ]);
        let set_result = handle_host_call("storage.set", set_args, &mut state);
        assert!(set_result.ok);

        let get_args = map(vec![("key", CborValue::Text("key".to_string()))]);
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
        let args = map(vec![("key", CborValue::Text("key".to_string()))]);
        let result = handle_host_call("storage.get", args, &mut state);
        assert!(!result.ok);
        assert_eq!(result.error.unwrap().code, "PERMISSION_DENIED");
    }

    #[test]
    fn host_system_random_generates_bytes() {
        let mut state = build_state(vec!["system:random".to_string()]);
        let args = map(vec![("length", CborValue::Integer(16))]);
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
}

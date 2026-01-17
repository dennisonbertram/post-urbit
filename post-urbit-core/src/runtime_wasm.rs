use std::collections::HashMap;

use serde::Serialize;
use wasmtime::{Caller, Engine, ExternType, Linker, Module, Store};

use crate::error::{PostUrbitError, Result};
use crate::sync::encode_cbor;

pub struct RuntimeApp {
    pub wasm: Vec<u8>,
    pub running: bool,
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
}

pub struct RuntimeManager {
    engine: Engine,
    apps: HashMap<String, RuntimeApp>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            engine: Engine::default(),
            apps: HashMap::new(),
        }
    }

    pub fn install(&mut self, app_id: &str, wasm: Vec<u8>) -> Result<()> {
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
                module,
                instance: None,
            },
        );
        Ok(())
    }

    pub fn start(&mut self, app_id: &str) -> Result<()> {
        let app = self
            .apps
            .get_mut(app_id)
            .ok_or(PostUrbitError::InvalidInput("app not installed"))?;
        let mut linker = Linker::new(&self.engine);
        define_host_imports(&mut linker)?;
        let mut store = Store::new(&self.engine, HostState::default());
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
    if serde_cbor::from_slice::<serde_cbor::Value>(&args_bytes).is_err() {
        return -2;
    }

    let state = caller.data_mut();
    if state.pending.len() >= 16 {
        return -3;
    }
    state.next_call_id += 1;
    let call_id = state.next_call_id;
    let result = encode_cbor(&ResultEnvelope::not_implemented()).unwrap_or_default();
    state.pending.insert(call_id, result);
    call_id
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
struct ResultEnvelope<'a> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a serde_cbor::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorEnvelope<'a>>,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    code: &'a str,
    message: &'a str,
}

impl<'a> ResultEnvelope<'a> {
    fn not_implemented() -> Self {
        Self {
            ok: false,
            value: None,
            error: Some(ErrorEnvelope {
                code: "NOT_IMPLEMENTED",
                message: "Method not implemented",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

use std::collections::HashMap;

use crate::error::{PostUrbitError, Result};

#[derive(Debug, Clone)]
pub struct RuntimeApp {
    pub wasm: Vec<u8>,
    pub running: bool,
}

#[derive(Default)]
pub struct RuntimeManager {
    apps: HashMap<String, RuntimeApp>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self { apps: HashMap::new() }
    }

    pub fn install(&mut self, app_id: &str, wasm: Vec<u8>) -> Result<()> {
        if wasm.is_empty() {
            return Err(PostUrbitError::InvalidInput("wasm empty"));
        }
        self.apps.insert(
            app_id.to_string(),
            RuntimeApp { wasm, running: false },
        );
        Ok(())
    }

    pub fn start(&mut self, app_id: &str) -> Result<()> {
        let app = self
            .apps
            .get_mut(app_id)
            .ok_or(PostUrbitError::InvalidInput("app not installed"))?;
        app.running = true;
        Ok(())
    }

    pub fn stop(&mut self, app_id: &str) -> Result<()> {
        let app = self
            .apps
            .get_mut(app_id)
            .ok_or(PostUrbitError::InvalidInput("app not installed"))?;
        app.running = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_install_start_stop() {
        let mut rt = RuntimeManager::new();
        let wasm = vec![0x00, 0x61, 0x73, 0x6d];
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
}

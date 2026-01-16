use crate::error::Result;

pub trait NATDiscovery: Send + Sync {
    fn external_addr(&self) -> Result<Option<String>>;
}

#[derive(Default)]
pub struct NATStub {
    external: Option<String>,
}

impl NATStub {
    pub fn new() -> Self {
        Self { external: None }
    }

    pub fn set_external(&mut self, addr: Option<String>) {
        self.external = addr;
    }
}

impl NATDiscovery for NATStub {
    fn external_addr(&self) -> Result<Option<String>> {
        Ok(self.external.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nat_stub_unknown() {
        let stub = NATStub::new();
        assert!(stub.external_addr().unwrap().is_none());
    }

    #[test]
    fn nat_stub_state() {
        let mut stub = NATStub::new();
        stub.set_external(Some("1.2.3.4:1234".to_string()));
        assert_eq!(stub.external_addr().unwrap(), Some("1.2.3.4:1234".to_string()));
    }
}

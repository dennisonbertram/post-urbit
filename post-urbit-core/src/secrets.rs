use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use url::Url;

use crate::network::matches_domain_pattern;
use crate::runtime::{SecretDeclaration, SecretInjection};

#[derive(Debug, Default)]
pub struct SecretStore {
    declarations: HashMap<String, HashMap<String, SecretDeclaration>>,
    values: HashMap<String, HashMap<String, String>>,
}

impl SecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_declarations(&mut self, app_id: &str, secrets: HashMap<String, SecretDeclaration>) {
        self.declarations.insert(app_id.to_string(), secrets);
    }

    pub fn set_secret(&mut self, app_id: &str, name: &str, value: String) {
        self.values
            .entry(app_id.to_string())
            .or_default()
            .insert(name.to_string(), value);
    }

    pub fn remove_secret(&mut self, app_id: &str, name: &str) {
        if let Some(values) = self.values.get_mut(app_id) {
            values.remove(name);
        }
    }

    pub fn inject_for_domain(
        &self,
        app_id: &str,
        domain: &str,
        url: &mut Url,
        headers: &mut HashMap<String, String>,
    ) -> Result<(), SecretInjectionError> {
        let declarations = match self.declarations.get(app_id) {
            Some(decls) => decls,
            None => return Ok(()),
        };
        for (name, decl) in declarations {
            if !decl.inject.domains.iter().any(|pattern| matches_domain_pattern(pattern, domain)) {
                continue;
            }
            let value = self
                .values
                .get(app_id)
                .and_then(|map| map.get(name))
                .cloned();
            if value.is_none() && decl.required {
                return Err(SecretInjectionError::missing(name));
            }
            let Some(value) = value else {
                continue;
            };
            apply_injection(&decl.inject, value, url, headers)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct SecretInjectionError {
    pub code: &'static str,
    pub message: String,
}

impl SecretInjectionError {
    pub fn missing(name: &str) -> Self {
        Self {
            code: "SECRET_NOT_CONFIGURED",
            message: format!("Secret not configured: {name}"),
        }
    }
}

fn apply_injection(
    inject: &SecretInjection,
    value: String,
    url: &mut Url,
    headers: &mut HashMap<String, String>,
) -> Result<(), SecretInjectionError> {
    if let Some(header) = inject.header.as_ref() {
        let prefix = inject.header_prefix.as_deref().unwrap_or("");
        headers.insert(header.to_string(), format!("{prefix}{value}"));
        return Ok(());
    }
    if let Some(param) = inject.query_param.as_ref() {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair(param, &value);
        return Ok(());
    }
    if inject.basic_auth.unwrap_or(false) {
        let raw = format!(":{value}");
        let encoded = BASE64_STANDARD.encode(raw.as_bytes());
        headers.insert("authorization".to_string(), format!("Basic {encoded}"));
        return Ok(());
    }
    Err(SecretInjectionError {
        code: "SECRET_NOT_CONFIGURED",
        message: "Invalid injection".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_header_secret() {
        let mut store = SecretStore::new();
        let mut decls = HashMap::new();
        decls.insert(
            "api_key".to_string(),
            SecretDeclaration {
                description: "test".to_string(),
                required: true,
                inject: SecretInjection {
                    domains: vec!["api.example.com".to_string()],
                    header: Some("x-api-key".to_string()),
                    header_prefix: None,
                    query_param: None,
                    basic_auth: None,
                },
            },
        );
        store.set_declarations("app", decls);
        store.set_secret("app", "api_key", "secret".to_string());
        let mut url = Url::parse("https://api.example.com/v1").unwrap();
        let mut headers = HashMap::new();
        store
            .inject_for_domain("app", "api.example.com", &mut url, &mut headers)
            .unwrap();
        assert_eq!(headers.get("x-api-key"), Some(&"secret".to_string()));
    }

    #[test]
    fn injects_query_param_secret() {
        let mut store = SecretStore::new();
        let mut decls = HashMap::new();
        decls.insert(
            "key".to_string(),
            SecretDeclaration {
                description: "test".to_string(),
                required: false,
                inject: SecretInjection {
                    domains: vec!["api.example.com".to_string()],
                    header: None,
                    header_prefix: None,
                    query_param: Some("token".to_string()),
                    basic_auth: None,
                },
            },
        );
        store.set_declarations("app", decls);
        store.set_secret("app", "key", "secret".to_string());
        let mut url = Url::parse("https://api.example.com/v1").unwrap();
        let mut headers = HashMap::new();
        store
            .inject_for_domain("app", "api.example.com", &mut url, &mut headers)
            .unwrap();
        assert!(url.query().unwrap_or_default().contains("token=secret"));
        assert!(headers.is_empty());
    }
}

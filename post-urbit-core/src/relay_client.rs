use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::encoding::validate_crockford_base32_lower;
use crate::error::{PostUrbitError, Result};
use crate::relay::sign_relay_allocation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayAllocationRequest {
    pub iid: String,
    pub lifetime: u32,
    pub timestamp: String,
    pub nonce: String,
    pub identity_doc_sequence: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayAllocationResponse {
    pub allocation_id: String,
    pub relay_address: String,
    pub relay_port: u16,
    pub expires_at: String,
    pub token: String,
}

#[async_trait]
pub trait RelayHttp: Send + Sync {
    async fn post_json(&self, url: &str, body: Vec<u8>) -> Result<Vec<u8>>;
}

pub struct RelayClient {
    allocation_url: String,
    http: Arc<dyn RelayHttp>,
}

impl RelayClient {
    pub fn new(relay_host: &str) -> Result<Self> {
        let host = relay_host.trim().to_ascii_lowercase();
        if host.is_empty() {
            return Err(PostUrbitError::InvalidInput("relay host"));
        }
        let allocation_url = format!("https://{}/allocate", host);
        Ok(Self {
            allocation_url,
            http: Arc::new(ReqwestRelayHttp::new()?),
        })
    }

    pub fn with_http(allocation_url: &str, http: Arc<dyn RelayHttp>) -> Result<Self> {
        if !allocation_url.starts_with("https://") {
            return Err(PostUrbitError::InvalidInput("allocation url scheme"));
        }
        Ok(Self {
            allocation_url: allocation_url.to_string(),
            http,
        })
    }

    pub async fn allocate(
        &self,
        iid: &str,
        identity_doc_sequence: &str,
        lifetime: u32,
        signing_key: &SigningKey,
    ) -> Result<RelayAllocationResponse> {
        validate_crockford_base32_lower(iid)?;
        validate_sequence(identity_doc_sequence)?;
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let mut nonce = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let signature = sign_relay_allocation(signing_key, iid, lifetime, &timestamp, &nonce)?;

        let request = RelayAllocationRequest {
            iid: iid.to_string(),
            lifetime,
            timestamp,
            nonce: URL_SAFE_NO_PAD.encode(nonce),
            identity_doc_sequence: identity_doc_sequence.to_string(),
            signature,
        };
        let body = serde_json::to_vec(&request)
            .map_err(|_| PostUrbitError::InvalidInput("relay request json"))?;
        let response_body = self.http.post_json(&self.allocation_url, body).await?;
        let response: RelayAllocationResponse = serde_json::from_slice(&response_body)
            .map_err(|_| PostUrbitError::InvalidInput("relay response json"))?;
        validate_allocation_response(&response)?;
        Ok(response)
    }
}

fn validate_allocation_response(response: &RelayAllocationResponse) -> Result<()> {
    if response.allocation_id.is_empty()
        || response
            .allocation_id
            .chars()
            .any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'))
    {
        return Err(PostUrbitError::InvalidInput("allocation id"));
    }
    if response.relay_address.is_empty() {
        return Err(PostUrbitError::InvalidInput("relay address"));
    }
    if response.relay_port == 0 {
        return Err(PostUrbitError::InvalidInput("relay port"));
    }
    validate_timestamp(&response.expires_at)?;
    Ok(())
}

fn validate_sequence(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(PostUrbitError::InvalidInput("sequence empty"));
    }
    if value.starts_with('0') && value != "0" {
        return Err(PostUrbitError::InvalidInput("sequence leading zeros"));
    }
    if value.chars().any(|ch| !ch.is_ascii_digit()) {
        return Err(PostUrbitError::InvalidInput("sequence format"));
    }
    Ok(())
}

fn validate_timestamp(value: &str) -> Result<()> {
    if value.contains('.') {
        return Err(PostUrbitError::InvalidInput("timestamp fractional"));
    }
    if !value.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("timestamp utc"));
    }
    value
        .parse::<chrono::DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))?;
    Ok(())
}

pub struct ReqwestRelayHttp {
    client: reqwest::Client,
}

impl ReqwestRelayHttp {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("post-urbit/1")
            .build()
            .map_err(|_| PostUrbitError::InvalidInput("relay http client"))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl RelayHttp for ReqwestRelayHttp {
    async fn post_json(&self, url: &str, body: Vec<u8>) -> Result<Vec<u8>> {
        let resp = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|_| PostUrbitError::InvalidInput("relay http request"))?;
        if !resp.status().is_success() {
            return Err(PostUrbitError::InvalidInput("relay http status"));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|_| PostUrbitError::InvalidInput("relay http body"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::base64_decode;
    use crate::relay::verify_relay_allocation;
    use std::sync::Mutex;

    struct MockRelayHttp {
        captured: Arc<Mutex<Option<Vec<u8>>>>,
        response: Vec<u8>,
    }

    #[async_trait]
    impl RelayHttp for MockRelayHttp {
        async fn post_json(&self, _url: &str, body: Vec<u8>) -> Result<Vec<u8>> {
            *self.captured.lock().unwrap() = Some(body);
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn relay_client_allocate_builds_request() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let response = RelayAllocationResponse {
            allocation_id: "alloc-123".to_string(),
            relay_address: "relay.example.com".to_string(),
            relay_port: 4433,
            expires_at: "2025-01-15T00:10:00Z".to_string(),
            token: "token".to_string(),
        };
        let response_json = serde_json::to_vec(&response).unwrap();
        let captured = Arc::new(Mutex::new(None));
        let http = Arc::new(MockRelayHttp {
            captured: captured.clone(),
            response: response_json,
        });
        let client = RelayClient::with_http("https://relay.example.com/allocate", http).unwrap();

        let _ = client
            .allocate(
                "b1n7cfscgashm32xx7eaxw0y09gy0y2v",
                "1",
                3600,
                &signing_key,
            )
            .await
            .unwrap();

        let body = captured.lock().unwrap().clone().unwrap();
        let request: RelayAllocationRequest = serde_json::from_slice(&body).unwrap();
        let nonce = URL_SAFE_NO_PAD.decode(request.nonce.as_bytes()).unwrap();
        let nonce: [u8; 16] = nonce.as_slice().try_into().unwrap();
        let key_b64 = crate::encoding::base64_encode(signing_key.verifying_key().as_bytes());
        verify_relay_allocation(
            &request.signature,
            &key_b64,
            &request.iid,
            request.lifetime,
            &request.timestamp,
            &nonce,
        )
        .unwrap();
        let _ = base64_decode(&request.signature).unwrap();
    }
}

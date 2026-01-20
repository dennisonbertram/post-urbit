use std::convert::Infallible;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use hyper::{Body, Method, Request, Response, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::dht::Dht;
use crate::encoding::base64_encode;
use crate::error::{PostUrbitError, Result};
use crate::identity::{fetch_identity, IdentityDocument};
use crate::mailbox::{
    canonicalize_mailbox_url, verify_mailbox_token_with_identity, MailboxToken,
    MailboxBearerTokenGenerator, TokenRequest, TokenResponse,
};
use crate::mailbox_store::{MailboxStore, StoredMessage};
use crate::messaging::decode_puse_envelope;

#[derive(Debug, Clone)]
pub struct MailboxHttpConfig {
    pub public_url: String,
    pub retention_days: i64,
    /// Secret key for bearer token generation (32 bytes)
    /// If None, bearer token endpoint is disabled
    pub bearer_token_secret: Option<[u8; 32]>,
}

impl MailboxHttpConfig {
    pub fn canonical_url(&self) -> Result<String> {
        canonicalize_mailbox_url(&self.public_url)
    }
}

/// Authenticated bearer token context
#[derive(Debug, Clone)]
pub struct BearerTokenContext {
    pub sender_iid: String,
    pub recipient_iid: String,
    pub expires_at: String,
}

pub struct MailboxHttpServer {
    cfg: MailboxHttpConfig,
    dht: Arc<dyn Dht + Send + Sync>,
    store: Arc<Mutex<MailboxStore>>,
    bearer_token_generator: Option<MailboxBearerTokenGenerator>,
}

impl MailboxHttpServer {
    pub fn new(
        cfg: MailboxHttpConfig,
        dht: Arc<dyn Dht + Send + Sync>,
        store: Arc<Mutex<MailboxStore>>,
    ) -> Self {
        let bearer_token_generator = cfg.bearer_token_secret.map(MailboxBearerTokenGenerator::new);
        Self { cfg, dht, store, bearer_token_generator }
    }

    pub async fn run(self: Arc<Self>, addr: std::net::SocketAddr) -> Result<()> {
        let make = make_service_fn(move |_conn| {
            let server = self.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let server = server.clone();
                    async move { Ok::<_, Infallible>(server.handle_request(req).await) }
                }))
            }
        });

        hyper::Server::bind(&addr)
            .serve(make)
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        Ok(())
    }

    async fn handle_request(self: Arc<Self>, req: Request<Body>) -> Response<Body> {
        match (req.method(), req.uri().path()) {
            // Bearer token request endpoint (REQ-MSG-086-089)
            (&Method::POST, path) if path.starts_with("/mailbox/token/") => {
                self.handle_token_request(req).await
            }
            (&Method::POST, path) if path.starts_with("/messages/") => {
                self.handle_store(req).await
            }
            (&Method::GET, "/messages") => self.handle_retrieve(req).await,
            (&Method::DELETE, "/messages") => self.handle_delete(req).await,
            _ => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap(),
        }
    }

    /// Handle POST /mailbox/token/{recipient_iid} - Request a bearer token
    ///
    /// Request body: { "sender_iid": "...", "validity_hours": 24 }
    /// Response: { "token": "...", "expires_at": "...", "recipient_iid": "...", "sender_iid": "..." }
    ///
    /// This endpoint allows a sender to request a bearer token that allows them
    /// to store messages in a recipient's mailbox. The sender must first authenticate
    /// using their identity document token.
    async fn handle_token_request(self: Arc<Self>, req: Request<Body>) -> Response<Body> {
        // Check if bearer token generation is enabled
        let Some(ref generator) = self.bearer_token_generator else {
            return error_response(
                StatusCode::NOT_IMPLEMENTED,
                "not_implemented",
                "bearer token endpoint disabled",
            );
        };

        // Extract recipient IID from path
        let Some(recipient_iid) = req.uri().path().strip_prefix("/mailbox/token/") else {
            return error_response(StatusCode::BAD_REQUEST, "bad_path", "missing recipient iid");
        };
        if recipient_iid.is_empty() {
            return error_response(StatusCode::BAD_REQUEST, "bad_path", "missing recipient iid");
        }
        let recipient_iid = recipient_iid.to_string();

        // Authenticate the requester using their identity document token
        let token = match self.authenticate(&req).await {
            Ok(token) => token,
            Err(resp) => return resp,
        };

        // Parse request body
        let body = match hyper::body::to_bytes(req.into_body()).await {
            Ok(bytes) => bytes,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "bad_body", "invalid body"),
        };

        let request: TokenRequest = match serde_json::from_slice(&body) {
            Ok(req) => req,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "bad_body", "invalid json"),
        };

        // Validate sender_iid in request matches the authenticated token's iid
        if request.sender_iid != token.iid {
            return error_response(
                StatusCode::FORBIDDEN,
                "sender_mismatch",
                "sender_iid must match authenticated identity",
            );
        }

        // Generate bearer token
        let (bearer_token, expires_at) = match generator.generate_token(
            &recipient_iid,
            &request.sender_iid,
            request.validity_hours,
        ) {
            Ok(result) => result,
            Err(PostUrbitError::InvalidInput(msg)) => {
                return error_response(StatusCode::BAD_REQUEST, "invalid_request", msg);
            }
            Err(PostUrbitError::InvalidEncoding(msg)) => {
                return error_response(StatusCode::BAD_REQUEST, "invalid_encoding", msg);
            }
            Err(_) => {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "token_generation_failed",
                    "failed to generate token",
                );
            }
        };

        let response = TokenResponse {
            token: bearer_token,
            expires_at,
            recipient_iid,
            sender_iid: request.sender_iid,
        };
        json_response(StatusCode::OK, &response)
    }

    async fn handle_store(self: Arc<Self>, req: Request<Body>) -> Response<Body> {
        let Some(inbox_owner_iid) = req.uri().path().strip_prefix("/messages/") else {
            return error_response(StatusCode::BAD_REQUEST, "bad_path", "missing inbox iid");
        };
        if inbox_owner_iid.is_empty() {
            return error_response(StatusCode::BAD_REQUEST, "bad_path", "missing inbox iid");
        }
        let inbox_owner_iid = inbox_owner_iid.to_string();

        let token = match self.authenticate(&req).await {
            Ok(token) => token,
            Err(resp) => return resp,
        };

        let body = match hyper::body::to_bytes(req.into_body()).await {
            Ok(bytes) => bytes,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "bad_body", "invalid body"),
        };

        if body.len() > 1_048_576 {
            return error_response(StatusCode::PAYLOAD_TOO_LARGE, "too_large", "envelope too large");
        }

        if decode_puse_envelope(&body).is_err() {
            return error_response(StatusCode::BAD_REQUEST, "bad_envelope", "invalid envelope");
        }

        let mut store = self.store.lock().await;
        match store.store(&inbox_owner_iid, &token.iid, &body) {
            Ok(stored) => store_response(&stored, self.cfg.retention_days),
            Err(PostUrbitError::InvalidInput(msg)) if msg == "sender iid mismatch" => {
                error_response(StatusCode::FORBIDDEN, "sender_mismatch", "sender mismatch")
            }
            Err(PostUrbitError::InvalidInput(msg)) if msg == "envelope too large" => {
                error_response(StatusCode::PAYLOAD_TOO_LARGE, "too_large", "envelope too large")
            }
            Err(_) => error_response(StatusCode::BAD_REQUEST, "store_failed", "store failed"),
        }
    }

    async fn handle_retrieve(self: Arc<Self>, req: Request<Body>) -> Response<Body> {
        let token = match self.authenticate(&req).await {
            Ok(token) => token,
            Err(resp) => return resp,
        };

        let query = req.uri().query().unwrap_or("");
        let mut cursor: Option<u64> = None;
        let mut limit: u64 = 100;
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            if key == "cursor" {
                cursor = match decode_cursor(&value) {
                    Ok(offset) => Some(offset),
                    Err(_) => {
                        return error_response(StatusCode::BAD_REQUEST, "bad_cursor", "invalid cursor")
                    }
                };
            } else if key == "limit" {
                if let Ok(parsed) = value.parse::<u64>() {
                    if parsed > 0 {
                        limit = parsed.min(1000);
                    }
                }
            }
        }

        let store = self.store.lock().await;
        let messages = match store.retrieve(&token.iid) {
            Ok(messages) => messages,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "retrieve_failed", "retrieve failed"),
        };
        let total = messages.len();
        let offset = cursor.unwrap_or(0) as usize;
        if offset > total {
            return error_response(StatusCode::BAD_REQUEST, "bad_cursor", "invalid cursor");
        }

        let limit = limit as usize;
        let page = messages
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let next_offset = offset + page.len();
        let has_more = next_offset < total;
        let next_cursor = if has_more {
            Some(encode_cursor(next_offset as u64))
        } else {
            None
        };

        let response = MailboxRetrieveResponse {
            messages: page.into_iter().map(StoredMessageView::from).collect(),
            next_cursor,
            has_more,
        };
        json_response(StatusCode::OK, &response)
    }

    async fn handle_delete(self: Arc<Self>, req: Request<Body>) -> Response<Body> {
        let token = match self.authenticate(&req).await {
            Ok(token) => token,
            Err(resp) => return resp,
        };

        let body = match hyper::body::to_bytes(req.into_body()).await {
            Ok(bytes) => bytes,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "bad_body", "invalid body"),
        };

        let payload: DeleteRequest = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "bad_body", "invalid json"),
        };

        let mut store = self.store.lock().await;
        let deleted = match store.delete(&token.iid, &payload.message_ids) {
            Ok(count) => count,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "delete_failed", "delete failed"),
        };
        let response = DeleteResponse { deleted };
        json_response(StatusCode::OK, &response)
    }

    async fn authenticate(&self, req: &Request<Body>) -> std::result::Result<MailboxToken, Response<Body>> {
        let Some(auth) = req.headers().get(hyper::header::AUTHORIZATION) else {
            return Err(error_response(StatusCode::UNAUTHORIZED, "unauthorized", "missing token"));
        };
        let auth = auth.to_str().map_err(|_| {
            error_response(StatusCode::UNAUTHORIZED, "unauthorized", "invalid token")
        })?;
        let Some(token_b64) = auth.strip_prefix("Bearer ") else {
            return Err(error_response(StatusCode::UNAUTHORIZED, "unauthorized", "invalid token"));
        };

        let identity = match self.fetch_identity(token_b64).await {
            Ok(identity) => identity,
            Err(_) => {
                return Err(error_response(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "invalid token",
                ))
            }
        };
        let token = verify_mailbox_token_with_identity(token_b64, &identity, Utc::now())
            .map_err(|_| {
                error_response(StatusCode::UNAUTHORIZED, "unauthorized", "invalid token")
            })?;

        let canonical = self.cfg.canonical_url().map_err(|_| {
            error_response(StatusCode::UNAUTHORIZED, "unauthorized", "invalid token")
        })?;
        if token.mailbox_url != canonical {
            return Err(error_response(StatusCode::UNAUTHORIZED, "unauthorized", "invalid token"));
        }

        Ok(token)
    }

    async fn fetch_identity(&self, token_b64: &str) -> Result<IdentityDocument> {
        let token_bytes = URL_SAFE_NO_PAD
            .decode(token_b64.as_bytes())
            .map_err(|_| PostUrbitError::InvalidEncoding("token base64url"))?;
        let token: MailboxToken = serde_json::from_slice(&token_bytes)
            .map_err(|_| PostUrbitError::InvalidInput("token json"))?;
        let doc = fetch_identity(self.dht.as_ref(), &token.iid)
            .await?
            .ok_or(PostUrbitError::InvalidInput("identity not found"))?;
        Ok(doc)
    }
}

#[derive(Serialize, Deserialize)]
struct StoreResponse {
    message_id: String,
    stored_at: String,
    expires_at: String,
}

#[derive(Serialize, Deserialize)]
struct MailboxRetrieveResponse {
    messages: Vec<StoredMessageView>,
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredMessageView {
    message_id: String,
    stored_at: String,
    sender_iid: String,
    size: u64,
    envelope: String,
}

impl From<StoredMessage> for StoredMessageView {
    fn from(msg: StoredMessage) -> Self {
        Self {
            message_id: msg.message_id,
            stored_at: msg.stored_at,
            sender_iid: msg.sender_iid,
            size: msg.size,
            envelope: base64_encode(&msg.envelope),
        }
    }
}

#[derive(Deserialize)]
struct DeleteRequest {
    message_ids: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct DeleteResponse {
    deleted: u64,
}

fn store_response(stored: &StoredMessage, retention_days: i64) -> Response<Body> {
    let stored_at = stored.stored_at.clone();
    let expires_at = stored
        .stored_at
        .parse::<DateTime<Utc>>()
        .unwrap_or_else(|_| Utc::now())
        + Duration::days(retention_days);
    let expires_at = expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let response = StoreResponse {
        message_id: stored.message_id.clone(),
        stored_at,
        expires_at,
    };
    json_response(StatusCode::CREATED, &response)
}

fn encode_cursor(offset: u64) -> String {
    URL_SAFE_NO_PAD.encode(offset.to_string().as_bytes())
}

fn decode_cursor(value: &str) -> Result<u64> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .map_err(|_| PostUrbitError::InvalidInput("cursor decode"))?;
    let text = std::str::from_utf8(&decoded)
        .map_err(|_| PostUrbitError::InvalidInput("cursor utf8"))?;
    text.parse::<u64>()
        .map_err(|_| PostUrbitError::InvalidInput("cursor parse"))
}

fn json_response<T: Serialize>(status: StatusCode, payload: &T) -> Response<Body> {
    match serde_json::to_vec(payload) {
        Ok(body) => Response::builder()
            .status(status)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap(),
    }
}

fn error_response(status: StatusCode, code: &str, message: &str) -> Response<Body> {
    let body = serde_json::json!({
        "error": code,
        "message": message,
    });
    json_response(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::{base64_encode, crockford_base32_decode};
    use crate::identity::{derive_iid, publish_identity, sign_idoc, Claims, EncryptionKeys, Keys, Recovery, Signatures, SigningKeys};
    use crate::mailbox::create_mailbox_token;
    use crate::messaging::{build_puse_envelope, PUSEHeader};
    use crate::dht::MemoryDht;
    use ed25519_dalek::SigningKey;
    use x25519_dalek::{PublicKey, StaticSecret};

    fn identity_doc(signing_key: &SigningKey, enc_key: &StaticSecret) -> IdentityDocument {
        let verifying_key = signing_key.verifying_key();
        let iid = derive_iid(&verifying_key);
        let enc_pub = PublicKey::from(enc_key);
        let mut doc = IdentityDocument {
            version: 1,
            iid,
            sequence: "0".to_string(),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            keys: Keys {
                signing: SigningKeys {
                    genesis: base64_encode(verifying_key.as_bytes()),
                    current: base64_encode(verifying_key.as_bytes()),
                    previous: None,
                    history: Vec::new(),
                },
                encryption: EncryptionKeys {
                    current: base64_encode(enc_pub.as_bytes()),
                    previous: Vec::new(),
                },
            },
            endpoints: Vec::new(),
            claims: Claims::default(),
            recovery: Recovery {
                method: "none".to_string(),
                config: serde_json::Value::Object(serde_json::Map::new()),
            },
            extensions: serde_json::Value::Object(serde_json::Map::new()),
            recovery_proof: None,
            signatures: Signatures {
                current: String::new(),
                previous: None,
            },
        };
        doc.signatures.current = sign_idoc(&doc, signing_key).unwrap();
        doc
    }

    async fn setup_server() -> (Arc<MailboxHttpServer>, String, IdentityDocument, SigningKey, IdentityDocument, SigningKey) {
        let dht = Arc::new(MemoryDht::new());
        let store = Arc::new(Mutex::new(MailboxStore::new()));
        let cfg = MailboxHttpConfig {
            public_url: "https://mailbox.example.com/".to_string(),
            retention_days: 30,
            bearer_token_secret: Some([42u8; 32]),
        };

        let sender_signing = SigningKey::generate(&mut rand::rngs::OsRng);
        let sender_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let sender_doc = identity_doc(&sender_signing, &sender_enc);

        let recipient_signing = SigningKey::generate(&mut rand::rngs::OsRng);
        let recipient_enc = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let recipient_doc = identity_doc(&recipient_signing, &recipient_enc);

        publish_identity(&*dht, &sender_doc).await.unwrap();
        publish_identity(&*dht, &recipient_doc).await.unwrap();

        let server = Arc::new(MailboxHttpServer::new(cfg, dht, store));
        (
            server,
            "https://mailbox.example.com/".to_string(),
            sender_doc,
            sender_signing,
            recipient_doc,
            recipient_signing,
        )
    }

    #[tokio::test]
    async fn mailbox_http_store_retrieve_delete() {
        let (server, mailbox_url, sender_doc, sender_signing, recipient_doc, recipient_signing) =
            setup_server().await;

        let sender_iid_raw = crockford_base32_decode(&sender_doc.iid).unwrap();
        let sender_iid_raw: [u8; 20] = sender_iid_raw.as_slice().try_into().unwrap();
        let recipient_iid_raw = crockford_base32_decode(&recipient_doc.iid).unwrap();
        let recipient_iid_raw: [u8; 20] = recipient_iid_raw.as_slice().try_into().unwrap();

        let header = PUSEHeader {
            flags: 0,
            sender_iid: sender_iid_raw,
            recipient_iid: recipient_iid_raw,
            message_id: [9u8; 16],
            header_extension: vec![0x00; 33],
            nonce: [1u8; 12],
            ciphertext_length: 0,
        };
        let message_key = [7u8; 32];
        let envelope = build_puse_envelope(&sender_signing, header, &message_key, b"hi")
            .unwrap();

        let expires_at = Utc::now() + Duration::hours(2);
        let token = create_mailbox_token(
            &sender_doc.iid,
            &mailbox_url,
            expires_at,
            [3u8; 16],
            &sender_signing,
        )
        .unwrap();

        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/messages/{}", recipient_doc.iid))
            .header(hyper::header::AUTHORIZATION, format!("Bearer {token}"))
            .header(hyper::header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(envelope))
            .unwrap();
        let resp = server.clone().handle_request(req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let recipient_token = create_mailbox_token(
            &recipient_doc.iid,
            &mailbox_url,
            expires_at,
            [4u8; 16],
            &recipient_signing,
        )
        .unwrap();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/messages?limit=10")
            .header(hyper::header::AUTHORIZATION, format!("Bearer {recipient_token}"))
            .body(Body::empty())
            .unwrap();
        let resp = server.clone().handle_request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let parsed: MailboxRetrieveResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        let message_id = parsed.messages[0].message_id.clone();

        let delete_body = serde_json::json!({ "message_ids": [message_id] });
        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/messages")
            .header(hyper::header::AUTHORIZATION, format!("Bearer {recipient_token}"))
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::from(delete_body.to_string()))
            .unwrap();
        let resp = server.clone().handle_request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

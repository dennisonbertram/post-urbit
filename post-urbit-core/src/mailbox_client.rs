use hyper::{Body, Client, Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::encoding::base64_decode;
use crate::error::{PostUrbitError, Result};

#[derive(Debug, Clone)]
pub struct MailboxClient {
    base_url: String,
    token: String,
    client: Client<hyper::client::HttpConnector>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StoreResponse {
    pub message_id: String,
    pub stored_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetrieveMessage {
    pub message_id: String,
    pub stored_at: String,
    pub sender_iid: String,
    pub size: u64,
    pub envelope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetrieveResponse {
    pub messages: Vec<RetrieveMessage>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DeleteRequest {
    message_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeleteResponse {
    pub deleted: u64,
}

impl MailboxClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client: Client::new(),
        })
    }

    pub async fn store_message(&self, inbox_owner_iid: &str, envelope: &[u8]) -> Result<StoreResponse> {
        let url = format!("{}/messages/{}", self.base_url, inbox_owner_iid);
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header(hyper::header::AUTHORIZATION, format!("Bearer {}", self.token))
            .header(hyper::header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(envelope.to_vec()))
            .map_err(|_| PostUrbitError::InvalidInput("store request"))?;

        let resp = self.client.request(req).await
            .map_err(|_| PostUrbitError::InvalidInput("store response"))?;
        if resp.status() != StatusCode::CREATED {
            return Err(PostUrbitError::InvalidInput("store failed"));
        }
        let body = hyper::body::to_bytes(resp.into_body()).await
            .map_err(|_| PostUrbitError::InvalidInput("store body"))?;
        serde_json::from_slice(&body)
            .map_err(|_| PostUrbitError::InvalidInput("store json"))
    }

    pub async fn retrieve_messages(&self, cursor: Option<&str>, limit: Option<u16>) -> Result<Vec<Vec<u8>>> {
        let mut url = Url::parse(&format!("{}/messages", self.base_url))
            .map_err(|_| PostUrbitError::InvalidInput("retrieve url"))?;
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
        if let Some(limit) = limit {
            url.query_pairs_mut().append_pair("limit", &limit.to_string());
        }

        let req = Request::builder()
            .method(Method::GET)
            .uri(url.as_str())
            .header(hyper::header::AUTHORIZATION, format!("Bearer {}", self.token))
            .body(Body::empty())
            .map_err(|_| PostUrbitError::InvalidInput("retrieve request"))?;
        let resp = self.client.request(req).await
            .map_err(|_| PostUrbitError::InvalidInput("retrieve response"))?;
        if resp.status() != StatusCode::OK {
            return Err(PostUrbitError::InvalidInput("retrieve failed"));
        }
        let body = hyper::body::to_bytes(resp.into_body()).await
            .map_err(|_| PostUrbitError::InvalidInput("retrieve body"))?;
        let parsed: RetrieveResponse = serde_json::from_slice(&body)
            .map_err(|_| PostUrbitError::InvalidInput("retrieve json"))?;
        let mut envelopes = Vec::new();
        for msg in parsed.messages {
            let bytes = base64_decode(&msg.envelope)?;
            envelopes.push(bytes);
        }
        Ok(envelopes)
    }

    pub async fn delete_messages(&self, ids: Vec<String>) -> Result<DeleteResponse> {
        let url = format!("{}/messages", self.base_url);
        let body = serde_json::to_vec(&DeleteRequest { message_ids: ids })
            .map_err(|_| PostUrbitError::InvalidInput("delete json"))?;
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(&url)
            .header(hyper::header::AUTHORIZATION, format!("Bearer {}", self.token))
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .map_err(|_| PostUrbitError::InvalidInput("delete request"))?;
        let resp = self.client.request(req).await
            .map_err(|_| PostUrbitError::InvalidInput("delete response"))?;
        if resp.status() != StatusCode::OK {
            return Err(PostUrbitError::InvalidInput("delete failed"));
        }
        let body = hyper::body::to_bytes(resp.into_body()).await
            .map_err(|_| PostUrbitError::InvalidInput("delete body"))?;
        serde_json::from_slice(&body)
            .map_err(|_| PostUrbitError::InvalidInput("delete json"))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::net::{SocketAddr, TcpListener};
    use hyper::service::{make_service_fn, service_fn};
    use hyper::Response;
    use super::*;

    #[tokio::test]
    async fn mailbox_client_flow() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(addr).unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = hyper::Server::from_tcp(listener).unwrap();
        let make = make_service_fn(|_conn| async {
            Ok::<_, Infallible>(service_fn(|req| async move {
                let path = req.uri().path().to_string();
                match (req.method(), path.as_str()) {
                    (&Method::POST, _) => {
                        let body = hyper::body::to_bytes(req.into_body()).await.unwrap();
                        assert!(!body.is_empty());
                        let response = StoreResponse {
                            message_id: "00000000-0000-4000-8000-000000000000".to_string(),
                            stored_at: "2025-01-15T00:00:00Z".to_string(),
                            expires_at: "2025-01-16T00:00:00Z".to_string(),
                        };
                        let json = serde_json::to_vec(&response).unwrap();
                        Ok::<_, Infallible>(Response::builder()
                            .status(StatusCode::CREATED)
                            .body(Body::from(json))
                            .unwrap())
                    }
                    (&Method::GET, "/messages") => {
                        let envelope = crate::encoding::base64_encode(b"hello");
                        let response = RetrieveResponse {
                            messages: vec![RetrieveMessage {
                                message_id: "00000000-0000-4000-8000-000000000000".to_string(),
                                stored_at: "2025-01-15T00:00:00Z".to_string(),
                                sender_iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v".to_string(),
                                size: 5,
                                envelope,
                            }],
                            next_cursor: None,
                            has_more: false,
                        };
                        let json = serde_json::to_vec(&response).unwrap();
                        Ok::<_, Infallible>(Response::new(Body::from(json)))
                    }
                    (&Method::DELETE, "/messages") => {
                        let response = DeleteResponse { deleted: 1 };
                        let json = serde_json::to_vec(&response).unwrap();
                        Ok::<_, Infallible>(Response::new(Body::from(json)))
                    }
                    _ => Ok::<_, Infallible>(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap()),
                }
            }))
        });
        let server_handle = tokio::spawn(async move { server.serve(make).await.unwrap() });

        let client = MailboxClient::new(&format!("http://{}", addr), "token").unwrap();
        let _ = client
            .store_message("b1n7cfscgashm32xx7eaxw0y09gy0y2v", b"data")
            .await
            .unwrap();
        let messages = client.retrieve_messages(None, None).await.unwrap();
        assert_eq!(messages[0], b"hello".to_vec());
        let deleted = client.delete_messages(vec!["id".to_string()]).await.unwrap();
        assert_eq!(deleted.deleted, 1);

        server_handle.abort();
    }
}

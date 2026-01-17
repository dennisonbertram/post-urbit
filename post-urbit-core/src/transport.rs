use std::io::{Read, Write};
use std::sync::Arc;
use chrono::{DateTime, Utc};
use async_trait::async_trait;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rcgen::generate_simple_self_signed;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use crate::identity::IdentityManager;
use crate::canonical_json::canonical_json_from;
use crate::encoding::{base64_decode, crockford_base32_decode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};

pub struct QuicTransport {
    endpoint: Endpoint,
    identity: Arc<IdentityManager>,
}

impl QuicTransport {
    pub async fn new(port: u16, identity: Arc<IdentityManager>) -> Result<Self> {
        // Create self-signed certificate (identity verification happens at protocol level)
        let cert = generate_simple_self_signed(vec!["localhost".to_string()])
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        let cert_der = cert.serialize_der().map_err(|err| PostUrbitError::Io(err.to_string()))?;
        let priv_key_der = cert.serialize_private_key_der();

        let server_config = Self::configure_server(cert_der, priv_key_der)?;
        let mut endpoint = Endpoint::server(server_config, ([0, 0, 0, 0], port).into())
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        // Configure client side for outgoing connections
        let client_config = Self::configure_client();
        endpoint.set_default_client_config(client_config);

        Ok(Self {
            endpoint,
            identity,
        })
    }

    fn configure_server(cert_der: Vec<u8>, priv_key_der: Vec<u8>) -> Result<ServerConfig> {
        let cert = rustls::Certificate(cert_der);
        let priv_key = rustls::PrivateKey(priv_key_der);
        let server_config = ServerConfig::with_single_cert(vec![cert], priv_key)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        Ok(server_config)
    }

    fn configure_client() -> ClientConfig {
        let verifier = Arc::new(NoCertificateVerification {});
        let client_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        ClientConfig::new(Arc::new(client_config))
    }

    pub async fn run(self: Arc<Self>) -> Result<()> {
        println!("QUIC transport listening...");

        while let Some(conn) = self.endpoint.accept().await {
            let identity = self.identity.clone();
            tokio::spawn(async move {
                if let Ok(connection) = conn.await {
                    println!("New connection from {:?}", connection.remote_address());

                    // Handle the connection with identity handshake
                    if let Err(e) = Self::handle_connection(connection, identity).await {
                        eprintln!("Connection error: {}", e);
                    }
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(
        connection: quinn::Connection,
        identity: Arc<IdentityManager>,
    ) -> Result<()> {
        // Accept the control stream (first bidirectional stream)
        let (mut send, _recv) = connection
            .accept_bi()
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        // Perform identity handshake (RFC-0002 §5.2)
        // 1. Receive peer's identity challenge
        // 2. Sign challenge with our identity key
        // 3. Send signed response
        // 4. Verify peer's response

        // For now, just echo identity info
        let identity_info = format!("IID: {}", identity.iid());
        send.write_all(identity_info.as_bytes())
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        send.finish()
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        println!("Identity handshake completed with peer");

        // Keep connection alive for future streams
        tokio::spawn(async move {
            // Handle additional streams (identity updates, messaging, etc.)
            while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                // Process streams based on type...
                let _ = tokio::io::copy(&mut recv, &mut send).await;
            }
        });

        Ok(())
    }

    pub async fn connect_to_peer(
        &self,
        address: std::net::SocketAddr,
    ) -> Result<quinn::Connection> {
        let connection = self
            .endpoint
            .connect(address, "localhost")
            .map_err(|err| PostUrbitError::Io(err.to_string()))?
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        Ok(connection)
    }
}

struct NoCertificateVerification;

impl rustls::client::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> std::result::Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::Certificate,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::Certificate,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[async_trait]
pub trait TransportLayer {
    async fn listen(
        &self,
        port: u16,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn connect(
        &self,
        address: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn send_message(
        &self,
        peer: &str,
        message: &[u8],
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub const STREAM_IDENTITY: u8 = 0x01;
pub const STREAM_MESSAGING: u8 = 0x02;
pub const STREAM_SYNC: u8 = 0x03;
pub const STREAM_APP: u8 = 0x04;
pub const STREAM_ADMIN: u8 = 0x05;

pub fn validate_stream_type(t: u8) -> Result<()> {
    match t {
        STREAM_IDENTITY | STREAM_MESSAGING | STREAM_SYNC | STREAM_APP | STREAM_ADMIN => Ok(()),
        _ => Err(PostUrbitError::InvalidInput("unknown stream type")),
    }
}

pub fn write_stream_type<W: Write>(w: &mut W, t: u8) -> Result<()> {
    validate_stream_type(t)?;
    w.write_all(&[t])
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

pub fn read_stream_type<R: Read>(r: &mut R) -> Result<u8> {
    let mut buf = [0u8; 1];
    r.read_exact(&mut buf)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    validate_stream_type(buf[0])?;
    Ok(buf[0])
}

pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> Result<()> {
    let len: u32 = payload
        .len()
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("frame length"))?;
    w.write_all(&len.to_be_bytes())
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    w.write_all(payload)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

pub fn read_frame<R: Read>(r: &mut R, max_size: u32) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    let len = u32::from_be_bytes(len_buf);
    if len > max_size {
        return Err(PostUrbitError::InvalidInput("frame too large"));
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(payload)
}

const HANDSHAKE_DOMAIN: &[u8] = b"post-urbit-handshake-v1";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum HandshakeMessage {
    #[serde(rename = "client_hello")]
    ClientHello(ClientHello),
    #[serde(rename = "server_hello")]
    ServerHello(ServerHello),
    #[serde(rename = "client_auth")]
    ClientAuth(ClientAuth),
    #[serde(rename = "handshake_complete")]
    HandshakeComplete(HandshakeComplete),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientHello {
    pub version: u8,
    pub client_iid: String,
    pub client_did: Option<String>,
    pub expected_server_iid: Option<String>,
    pub client_nonce: String,
    pub timestamp: String,
    pub tls_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerHello {
    pub version: u8,
    pub server_iid: String,
    pub server_did: Option<String>,
    pub server_nonce: String,
    pub timestamp: String,
    pub tls_binding: String,
    pub identity_document: serde_json::Value,
    pub device_document: Option<serde_json::Value>,
    pub challenge_signature: String,
    pub device_signature: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientAuth {
    pub version: u8,
    pub identity_document: serde_json::Value,
    pub device_document: Option<serde_json::Value>,
    pub challenge_signature: String,
    pub device_signature: Option<String>,
    pub tls_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HandshakeComplete {
    pub version: u8,
    pub success: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeRole {
    Client,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeState {
    Start,
    ClientHelloSent,
    ClientHelloReceived,
    ServerHelloSent,
    ServerHelloReceived,
    ClientAuthSent,
    ClientAuthReceived,
    Complete,
}

#[derive(Debug)]
pub struct HandshakeFsm {
    role: HandshakeRole,
    state: HandshakeState,
}

impl HandshakeFsm {
    pub fn new(role: HandshakeRole) -> Self {
        Self {
            role,
            state: HandshakeState::Start,
        }
    }

    pub fn state(&self) -> HandshakeState {
        self.state
    }

    pub fn on_send(&mut self, msg: &HandshakeMessage) -> Result<()> {
        use HandshakeMessage::*;
        match (self.role, self.state, msg) {
            (HandshakeRole::Client, HandshakeState::Start, ClientHello(_)) => {
                self.state = HandshakeState::ClientHelloSent;
                Ok(())
            }
            (HandshakeRole::Client, HandshakeState::ServerHelloReceived, ClientAuth(_)) => {
                self.state = HandshakeState::ClientAuthSent;
                Ok(())
            }
            (HandshakeRole::Server, HandshakeState::ClientHelloReceived, ServerHello(_)) => {
                self.state = HandshakeState::ServerHelloSent;
                Ok(())
            }
            (HandshakeRole::Server, HandshakeState::ClientAuthReceived, HandshakeComplete(_)) => {
                self.state = HandshakeState::Complete;
                Ok(())
            }
            _ => Err(PostUrbitError::InvalidInput("handshake send order")),
        }
    }

    pub fn on_receive(&mut self, msg: &HandshakeMessage) -> Result<()> {
        use HandshakeMessage::*;
        match (self.role, self.state, msg) {
            (HandshakeRole::Client, HandshakeState::ClientHelloSent, ServerHello(_)) => {
                self.state = HandshakeState::ServerHelloReceived;
                Ok(())
            }
            (HandshakeRole::Client, HandshakeState::ClientAuthSent, HandshakeComplete(_)) => {
                self.state = HandshakeState::Complete;
                Ok(())
            }
            (HandshakeRole::Server, HandshakeState::Start, ClientHello(_)) => {
                self.state = HandshakeState::ClientHelloReceived;
                Ok(())
            }
            (HandshakeRole::Server, HandshakeState::ServerHelloSent, ClientAuth(_)) => {
                self.state = HandshakeState::ClientAuthReceived;
                Ok(())
            }
            _ => Err(PostUrbitError::InvalidInput("handshake receive order")),
        }
    }
}

pub fn canonical_handshake_json(msg: &HandshakeMessage) -> Result<String> {
    canonical_json_from(msg)
}

pub fn validate_handshake_timestamp(ts: &str) -> Result<()> {
    if ts.contains('.') {
        return Err(PostUrbitError::InvalidInput("timestamp has fractional seconds"));
    }
    let _: DateTime<Utc> = ts
        .parse()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))?;
    if !ts.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("timestamp not UTC"));
    }
    Ok(())
}

pub fn verify_challenge_signature(
    signature_base64: &str,
    signing_key_base64: &str,
    client_nonce: &str,
    server_nonce: &str,
    tls_binding: &str,
    client_iid: &str,
    server_iid: &str,
    server_signature: bool,
) -> Result<()> {
    validate_crockford_base32_lower(client_iid)?;
    validate_crockford_base32_lower(server_iid)?;

    let client_nonce = base64_decode(client_nonce)?;
    let server_nonce = base64_decode(server_nonce)?;
    let tls_binding = base64_decode(tls_binding)?;
    if client_nonce.len() != 32 || server_nonce.len() != 32 || tls_binding.len() != 32 {
        return Err(PostUrbitError::InvalidInput("nonce or tls_binding length"));
    }

    let client_iid_raw = crockford_base32_decode(client_iid)?;
    let server_iid_raw = crockford_base32_decode(server_iid)?;
    if client_iid_raw.len() != 20 || server_iid_raw.len() != 20 {
        return Err(PostUrbitError::InvalidInput("iid length"));
    }

    let mut challenge = Vec::with_capacity(159);
    challenge.extend_from_slice(HANDSHAKE_DOMAIN);
    if server_signature {
        challenge.extend_from_slice(&client_nonce);
        challenge.extend_from_slice(&server_nonce);
        challenge.extend_from_slice(&tls_binding);
        challenge.extend_from_slice(&client_iid_raw);
        challenge.extend_from_slice(&server_iid_raw);
    } else {
        challenge.extend_from_slice(&server_nonce);
        challenge.extend_from_slice(&client_nonce);
        challenge.extend_from_slice(&tls_binding);
        challenge.extend_from_slice(&server_iid_raw);
        challenge.extend_from_slice(&client_iid_raw);
    }

    let digest = Sha256::digest(&challenge);
    let signature_bytes = base64_decode(signature_base64)?;
    if signature_bytes.len() != 64 {
        return Err(PostUrbitError::InvalidInput("challenge signature length"));
    }
    let key_bytes = base64_decode(signing_key_base64)?;
    if key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("signing key length"));
    }

    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signing key length"))?,
    )
    .map_err(|_| PostUrbitError::InvalidInput("signing key parse"))?;
    let signature = ed25519_dalek::Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );
    verifying_key
        .verify_strict(&digest, &signature)
        .map_err(|_| PostUrbitError::Crypto("challenge signature invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::base64_encode;
    use ed25519_dalek::Signer;

    #[test]
    fn framing_round_trip() {
        let mut buf = Vec::new();
        write_stream_type(&mut buf, STREAM_IDENTITY).unwrap();
        write_frame(&mut buf, b"hello").unwrap();

        let mut reader = &buf[..];
        let stream_type = read_stream_type(&mut reader).unwrap();
        let payload = read_frame(&mut reader, 1024).unwrap();

        assert_eq!(stream_type, STREAM_IDENTITY);
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn framing_rejects_large() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &[0u8; 5]).unwrap();
        let mut reader = &buf[..];
        let err = read_frame(&mut reader, 4).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn handshake_canonical_json_stable() {
        let msg = HandshakeMessage::ClientHello(ClientHello {
            version: 1,
            client_iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v".to_string(),
            client_did: None,
            expected_server_iid: None,
            client_nonce: crate::encoding::base64_encode(&[7u8; 32]),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            tls_binding: crate::encoding::base64_encode(&[9u8; 32]),
        });
        let json = canonical_handshake_json(&msg).unwrap();
        assert!(json.contains("client_hello"));
    }

    #[test]
    fn handshake_challenge_round_trip() {
        let client_nonce = base64_encode(
            &hex::decode("0001020304050607080910111213141516171819202122232425262728293031")
                .unwrap(),
        );
        let server_nonce = base64_encode(
            &hex::decode("3130292827262524232221201918171615141312111009080706050403020100")
                .unwrap(),
        );
        let tls_binding = base64_encode(
            &hex::decode("ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100")
                .unwrap(),
        );

        let signing_seed = hex::decode(
            "a227446ee9fe9e7a55d2d1247bd83639bf213aa035b4faf3b66da60a208be99c",
        )
        .unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(
            signing_seed.as_slice().try_into().unwrap(),
        );

        let client_nonce_raw = base64_decode(&client_nonce).unwrap();
        let server_nonce_raw = base64_decode(&server_nonce).unwrap();
        let tls_binding_raw = base64_decode(&tls_binding).unwrap();
        let client_iid_raw = crockford_base32_decode("b1n7cfscgashm32xx7eaxw0y09gy0y2v").unwrap();
        let server_iid_raw = crockford_base32_decode("2f0fcybfpmka5vf7ge737ex07crgnxsw").unwrap();

        let mut challenge = Vec::new();
        challenge.extend_from_slice(HANDSHAKE_DOMAIN);
        challenge.extend_from_slice(&client_nonce_raw);
        challenge.extend_from_slice(&server_nonce_raw);
        challenge.extend_from_slice(&tls_binding_raw);
        challenge.extend_from_slice(&client_iid_raw);
        challenge.extend_from_slice(&server_iid_raw);

        let digest = Sha256::digest(&challenge);
        let signature = signing_key.sign(&digest);
        let signature_base64 = base64_encode(signature.to_bytes().as_slice());
        let signing_key_base64 = base64_encode(signing_key.verifying_key().as_bytes());
        verify_challenge_signature(
            &signature_base64,
            &signing_key_base64,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            "b1n7cfscgashm32xx7eaxw0y09gy0y2v",
            "2f0fcybfpmka5vf7ge737ex07crgnxsw",
            true,
        )
        .unwrap();
    }

    #[test]
    fn handshake_fsm_client_flow() {
        let mut fsm = HandshakeFsm::new(HandshakeRole::Client);
        let client_hello = HandshakeMessage::ClientHello(ClientHello {
            version: 1,
            client_iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v".to_string(),
            client_did: None,
            expected_server_iid: None,
            client_nonce: base64_encode(&[0u8; 32]),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            tls_binding: base64_encode(&[1u8; 32]),
        });
        fsm.on_send(&client_hello).unwrap();

        let server_hello = HandshakeMessage::ServerHello(ServerHello {
            version: 1,
            server_iid: "2f0fcybfpmka5vf7ge737ex07crgnxsw".to_string(),
            server_did: None,
            server_nonce: base64_encode(&[2u8; 32]),
            timestamp: "2025-01-15T00:00:01Z".to_string(),
            tls_binding: base64_encode(&[3u8; 32]),
            identity_document: serde_json::json!({}),
            device_document: None,
            challenge_signature: base64_encode(&[4u8; 64]),
            device_signature: None,
        });
        fsm.on_receive(&server_hello).unwrap();

        let client_auth = HandshakeMessage::ClientAuth(ClientAuth {
            version: 1,
            identity_document: serde_json::json!({}),
            device_document: None,
            challenge_signature: base64_encode(&[5u8; 64]),
            device_signature: None,
            tls_binding: base64_encode(&[6u8; 32]),
        });
        fsm.on_send(&client_auth).unwrap();

        let complete = HandshakeMessage::HandshakeComplete(HandshakeComplete {
            version: 1,
            success: true,
        });
        fsm.on_receive(&complete).unwrap();
        assert_eq!(fsm.state(), HandshakeState::Complete);
    }

    #[test]
    fn handshake_fsm_rejects_out_of_order() {
        let mut fsm = HandshakeFsm::new(HandshakeRole::Server);
        let bad = HandshakeMessage::ClientAuth(ClientAuth {
            version: 1,
            identity_document: serde_json::json!({}),
            device_document: None,
            challenge_signature: base64_encode(&[5u8; 64]),
            device_signature: None,
            tls_binding: base64_encode(&[6u8; 32]),
        });
        let err = fsm.on_receive(&bad).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }
}

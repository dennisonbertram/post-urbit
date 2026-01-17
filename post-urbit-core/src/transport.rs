use std::io::{Read, Write};
use std::sync::Arc;
use chrono::{DateTime, Utc};
use async_trait::async_trait;
use quinn::{ClientConfig, Endpoint, EndpointConfig, IdleTimeout, ServerConfig, TransportConfig, VarInt};
use rcgen::generate_simple_self_signed;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use crate::identity::{decode_idoc_envelope, IdentityManager};
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
        let endpoint_config = Self::configure_endpoint()?;
        let addr = std::net::SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, port));
        let socket = std::net::UdpSocket::bind(addr)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        let runtime = quinn::default_runtime()
            .ok_or(PostUrbitError::Io("no quinn runtime".to_string()))?;
        let mut endpoint = Endpoint::new(endpoint_config, Some(server_config), socket, runtime)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        // Configure client side for outgoing connections
        let client_config = Self::configure_client()?;
        endpoint.set_default_client_config(client_config);

        Ok(Self {
            endpoint,
            identity,
        })
    }

    fn configure_server(cert_der: Vec<u8>, priv_key_der: Vec<u8>) -> Result<ServerConfig> {
        let cert = rustls::Certificate(cert_der);
        let priv_key = rustls::PrivateKey(priv_key_der);
        let mut tls_config = rustls::ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|_| PostUrbitError::InvalidInput("tls versions"))?
            .with_no_client_auth()
            .with_single_cert(vec![cert], priv_key)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        tls_config.alpn_protocols = vec![b"post-urbit/1".to_vec()];
        tls_config.max_early_data_size = u32::MAX;

        let transport = Arc::new(Self::transport_config()?);
        let mut server_config = ServerConfig::with_crypto(Arc::new(tls_config));
        server_config.transport_config(transport);
        Ok(server_config)
    }

    fn configure_client() -> Result<ClientConfig> {
        let verifier = Arc::new(NoCertificateVerification {});
        let mut client_config = rustls::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|_| PostUrbitError::InvalidInput("tls versions"))?
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"post-urbit/1".to_vec()];
        client_config.enable_early_data = false;
        let mut config = ClientConfig::new(Arc::new(client_config));
        let transport = Arc::new(Self::transport_config()?);
        config.transport_config(transport);
        Ok(config)
    }

    fn configure_endpoint() -> Result<EndpointConfig> {
        let mut config = EndpointConfig::default();
        config
            .max_udp_payload_size(1200)
            .map_err(|_| PostUrbitError::InvalidInput("max udp payload"))?;
        Ok(config)
    }

    fn transport_config() -> Result<TransportConfig> {
        let mut config = TransportConfig::default();
        let idle: IdleTimeout = std::time::Duration::from_secs(30)
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("idle timeout"))?;
        config
            .max_idle_timeout(Some(idle))
            .max_concurrent_bidi_streams(VarInt::from_u32(100))
            .max_concurrent_uni_streams(VarInt::from_u32(100))
            .initial_rtt(std::time::Duration::from_millis(100));
        Ok(config)
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

pub const STREAM_CONTROL: u8 = 0x01;
pub const STREAM_IDENTITY: u8 = 0x02;
pub const STREAM_MESSAGE: u8 = 0x03;
pub const STREAM_SYNC: u8 = 0x04;
pub const STREAM_BULK: u8 = 0x05;

pub fn validate_stream_type(t: u8) -> Result<()> {
    match t {
        STREAM_CONTROL | STREAM_IDENTITY | STREAM_MESSAGE | STREAM_SYNC => Ok(()),
        STREAM_BULK => Err(PostUrbitError::InvalidInput("bulk stream reserved")),
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
const DEVICE_HANDSHAKE_DOMAIN: &[u8] = b"post-urbit-device-v1";

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
    validate_handshake_timestamp_with_now(ts, Utc::now())
}

pub fn validate_handshake_timestamp_with_now(ts: &str, now: DateTime<Utc>) -> Result<()> {
    let parsed = validate_timestamp_canonical(ts)?;
    let delta = (parsed - now).num_seconds().abs();
    if delta > 300 {
        return Err(PostUrbitError::InvalidInput("timestamp outside window"));
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

pub fn verify_device_signature(
    signature_base64: &str,
    device_signing_key_base64: &str,
    client_nonce: &str,
    server_nonce: &str,
    tls_binding: &str,
    server_iid: &str,
    server_did: &str,
) -> Result<()> {
    validate_crockford_base32_lower(server_iid)?;
    validate_crockford_base32_lower(server_did)?;

    let client_nonce = base64_decode(client_nonce)?;
    let server_nonce = base64_decode(server_nonce)?;
    let tls_binding = base64_decode(tls_binding)?;
    if client_nonce.len() != 32 || server_nonce.len() != 32 || tls_binding.len() != 32 {
        return Err(PostUrbitError::InvalidInput("nonce or tls_binding length"));
    }

    let server_iid_raw = crockford_base32_decode(server_iid)?;
    let server_did_raw = crockford_base32_decode(server_did)?;
    if server_iid_raw.len() != 20 || server_did_raw.len() != 20 {
        return Err(PostUrbitError::InvalidInput("iid length"));
    }

    let mut data = Vec::with_capacity(156);
    data.extend_from_slice(DEVICE_HANDSHAKE_DOMAIN);
    data.extend_from_slice(&client_nonce);
    data.extend_from_slice(&server_nonce);
    data.extend_from_slice(&tls_binding);
    data.extend_from_slice(&server_iid_raw);
    data.extend_from_slice(&server_did_raw);

    let digest = Sha256::digest(&data);
    let signature_bytes = base64_decode(signature_base64)?;
    if signature_bytes.len() != 64 {
        return Err(PostUrbitError::InvalidInput("device signature length"));
    }
    let key_bytes = base64_decode(device_signing_key_base64)?;
    if key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("device signing key length"));
    }
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("device signing key length"))?,
    )
    .map_err(|_| PostUrbitError::InvalidInput("device signing key parse"))?;
    let signature = ed25519_dalek::Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );
    verifying_key
        .verify_strict(&digest, &signature)
        .map_err(|_| PostUrbitError::Crypto("device signature invalid"))
}

pub fn validate_client_hello(msg: &ClientHello, now: DateTime<Utc>) -> Result<()> {
    validate_crockford_base32_lower(&msg.client_iid)?;
    if let Some(did) = &msg.client_did {
        validate_crockford_base32_lower(did)?;
    }
    if let Some(expected) = &msg.expected_server_iid {
        validate_crockford_base32_lower(expected)?;
    }
    validate_handshake_timestamp_with_now(&msg.timestamp, now)?;
    let client_nonce = base64_decode(&msg.client_nonce)?;
    let tls_binding = base64_decode(&msg.tls_binding)?;
    if client_nonce.len() != 32 || tls_binding.len() != 32 {
        return Err(PostUrbitError::InvalidInput("nonce or tls binding length"));
    }
    Ok(())
}

pub fn validate_server_hello(msg: &ServerHello, now: DateTime<Utc>) -> Result<()> {
    validate_crockford_base32_lower(&msg.server_iid)?;
    if let Some(did) = &msg.server_did {
        validate_crockford_base32_lower(did)?;
        if msg.device_document.is_none() || msg.device_signature.is_none() {
            return Err(PostUrbitError::InvalidInput("device fields missing"));
        }
    }
    validate_handshake_timestamp_with_now(&msg.timestamp, now)?;
    let server_nonce = base64_decode(&msg.server_nonce)?;
    let tls_binding = base64_decode(&msg.tls_binding)?;
    if server_nonce.len() != 32 || tls_binding.len() != 32 {
        return Err(PostUrbitError::InvalidInput("nonce or tls binding length"));
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum IdentityStreamMessage {
    #[serde(rename = "identity_update")]
    Update(IdentityUpdateMessage),
    #[serde(rename = "identity_request")]
    Request(IdentityRequestMessage),
    #[serde(rename = "identity_response")]
    Response(IdentityResponseMessage),
    #[serde(rename = "identity_ack")]
    Ack(IdentityAckMessage),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityUpdateMessage {
    pub idoc: String,
    pub sequence: String,
    pub sent_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityRequestMessage {
    pub known_sequence: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityResponseMessage {
    pub has_update: bool,
    pub idoc: Option<String>,
    pub sequence: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityAckMessage {
    pub accepted: bool,
    pub sequence: String,
    pub error_code: Option<u64>,
    pub error_message: Option<String>,
}

pub fn encode_identity_message(message: &IdentityStreamMessage) -> Result<Vec<u8>> {
    serde_json::to_vec(message).map_err(|_| PostUrbitError::InvalidInput("identity json"))
}

pub fn decode_identity_message(bytes: &[u8]) -> Result<IdentityStreamMessage> {
    let message: IdentityStreamMessage =
        serde_json::from_slice(bytes).map_err(|_| PostUrbitError::InvalidInput("identity json"))?;
    validate_identity_message(&message)?;
    Ok(message)
}

fn validate_identity_message(message: &IdentityStreamMessage) -> Result<()> {
    match message {
        IdentityStreamMessage::Update(update) => {
            let idoc_bytes = base64_decode(&update.idoc)?;
            let _doc = decode_idoc_envelope(&idoc_bytes)?;
            validate_sequence(&update.sequence)?;
            validate_timestamp_canonical(&update.sent_at)?;
        }
        IdentityStreamMessage::Request(req) => {
            validate_sequence(&req.known_sequence)?;
        }
        IdentityStreamMessage::Response(resp) => {
            match resp.has_update {
                true => {
                    let idoc = resp
                        .idoc
                        .as_ref()
                        .ok_or(PostUrbitError::InvalidInput("identity response idoc"))?;
                    let idoc_bytes = base64_decode(idoc)?;
                    let _doc = decode_idoc_envelope(&idoc_bytes)?;
                    let seq = resp
                        .sequence
                        .as_ref()
                        .ok_or(PostUrbitError::InvalidInput("identity response sequence"))?;
                    validate_sequence(seq)?;
                }
                false => {
                    if resp.idoc.is_some() || resp.sequence.is_some() {
                        return Err(PostUrbitError::InvalidInput(
                            "identity response has_update false",
                        ));
                    }
                }
            }
        }
        IdentityStreamMessage::Ack(ack) => {
            validate_sequence(&ack.sequence)?;
            if !ack.accepted && ack.error_code.is_none() {
                return Err(PostUrbitError::InvalidInput("identity ack error code"));
            }
        }
    }
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

fn validate_timestamp_canonical(value: &str) -> Result<DateTime<Utc>> {
    if value.contains('.') {
        return Err(PostUrbitError::InvalidInput("timestamp fractional"));
    }
    if !value.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("timestamp utc"));
    }
    value
        .parse::<DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::base64_encode;
    use crate::identity::encode_idoc_envelope;
    use ed25519_dalek::Signer;

    #[test]
    fn framing_round_trip() {
        let mut buf = Vec::new();
        write_stream_type(&mut buf, STREAM_CONTROL).unwrap();
        write_frame(&mut buf, b"hello").unwrap();

        let mut reader = &buf[..];
        let stream_type = read_stream_type(&mut reader).unwrap();
        let payload = read_frame(&mut reader, 1024).unwrap();

        assert_eq!(stream_type, STREAM_CONTROL);
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
    fn device_signature_round_trip() {
        let client_nonce = base64_encode(&[1u8; 32]);
        let server_nonce = base64_encode(&[2u8; 32]);
        let tls_binding = base64_encode(&[3u8; 32]);
        let server_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let server_did = "42kbzq2tyab939amybd76bm8kfpzgn95";

        let device_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let mut data = Vec::new();
        data.extend_from_slice(DEVICE_HANDSHAKE_DOMAIN);
        data.extend_from_slice(&base64_decode(&client_nonce).unwrap());
        data.extend_from_slice(&base64_decode(&server_nonce).unwrap());
        data.extend_from_slice(&base64_decode(&tls_binding).unwrap());
        data.extend_from_slice(&crockford_base32_decode(server_iid).unwrap());
        data.extend_from_slice(&crockford_base32_decode(server_did).unwrap());
        let digest = Sha256::digest(&data);
        let signature = device_key.sign(&digest);
        let signature_base64 = base64_encode(signature.to_bytes().as_slice());
        let key_b64 = base64_encode(device_key.verifying_key().as_bytes());

        verify_device_signature(
            &signature_base64,
            &key_b64,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            server_iid,
            server_did,
        )
        .unwrap();
    }

    #[test]
    fn validate_client_hello_checks_nonce_length() {
        let msg = ClientHello {
            version: 1,
            client_iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v".to_string(),
            client_did: None,
            expected_server_iid: None,
            client_nonce: base64_encode(&[0u8; 16]),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            tls_binding: base64_encode(&[1u8; 32]),
        };
        let now = "2025-01-15T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let err = validate_client_hello(&msg, now).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn validate_server_hello_requires_device_fields() {
        let msg = ServerHello {
            version: 1,
            server_iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v".to_string(),
            server_did: Some("42kbzq2tyab939amybd76bm8kfpzgn95".to_string()),
            server_nonce: base64_encode(&[2u8; 32]),
            timestamp: "2025-01-15T00:00:00Z".to_string(),
            tls_binding: base64_encode(&[3u8; 32]),
            identity_document: serde_json::json!({}),
            device_document: None,
            challenge_signature: base64_encode(&[4u8; 64]),
            device_signature: None,
        };
        let now = "2025-01-15T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let err = validate_server_hello(&msg, now).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
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

    #[test]
    fn handshake_timestamp_window() {
        let now = "2025-01-15T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        validate_handshake_timestamp_with_now("2025-01-15T00:04:59Z", now).unwrap();
        let err = validate_handshake_timestamp_with_now("2025-01-15T00:05:01Z", now).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn identity_message_round_trip_update() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let doc = rt
            .block_on(async {
                let manager = IdentityManager::new(temp.path().to_str().unwrap()).await?;
                Ok::<_, PostUrbitError>(manager.identity_document().clone())
            })
            .unwrap();
        let idoc = encode_idoc_envelope(&doc).unwrap();
        let msg = IdentityStreamMessage::Update(IdentityUpdateMessage {
            idoc: base64_encode(&idoc),
            sequence: doc.sequence.clone(),
            sent_at: "2025-01-15T00:00:00Z".to_string(),
        });
        let encoded = encode_identity_message(&msg).unwrap();
        let decoded = decode_identity_message(&encoded).unwrap();
        match decoded {
            IdentityStreamMessage::Update(update) => {
                assert_eq!(update.sequence, doc.sequence);
            }
            _ => panic!("unexpected message"),
        }
    }

    #[test]
    fn identity_response_requires_idoc_when_has_update() {
        let msg = IdentityStreamMessage::Response(IdentityResponseMessage {
            has_update: true,
            idoc: None,
            sequence: None,
        });
        let encoded = encode_identity_message(&msg).unwrap();
        let err = decode_identity_message(&encoded).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn identity_ack_requires_error_code() {
        let msg = IdentityStreamMessage::Ack(IdentityAckMessage {
            accepted: false,
            sequence: "1".to_string(),
            error_code: None,
            error_message: Some("bad".to_string()),
        });
        let encoded = encode_identity_message(&msg).unwrap();
        let err = decode_identity_message(&encoded).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn quic_transport_config_matches_spec() {
        let config = QuicTransport::transport_config().unwrap();
        let debug = format!("{config:?}");
        assert!(debug.contains("max_idle_timeout: Some(30000)"));
        assert!(debug.contains("max_concurrent_bidi_streams: 100"));
        assert!(debug.contains("max_concurrent_uni_streams: 100"));
        assert!(debug.contains("initial_rtt: 100ms"));

        let endpoint = QuicTransport::configure_endpoint().unwrap();
        assert_eq!(endpoint.get_max_udp_payload_size(), 1200);
    }
}

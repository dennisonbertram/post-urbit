use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use async_trait::async_trait;
use ed25519_dalek::Signer;
use quinn::{ClientConfig, Endpoint, EndpointConfig, IdleTimeout, ServerConfig, TransportConfig, VarInt};
use rcgen::generate_simple_self_signed;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::time::timeout;
use crate::identity::{decode_idoc_envelope, IdentityDocument, IdentityManager};
use crate::canonical_json::canonical_json_from;
use crate::encoding::{base64_decode, base64_encode, crockford_base32_decode, validate_crockford_base32_lower};
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

                    // Handle the connection with secure identity handshake
                    match Self::handle_connection(connection.clone(), identity).await {
                        Ok(handshake_result) => {
                            println!(
                                "Authenticated peer {} from {:?}",
                                handshake_result.peer_iid,
                                connection.remote_address()
                            );
                            // Connection is now authenticated - keep alive for future streams
                            tokio::spawn(async move {
                                while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                                    // Process additional streams (identity updates, messaging, etc.)
                                    let _ = tokio::io::copy(&mut recv, &mut send).await;
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("Connection handshake error: {}", e);
                            // Connection is not authenticated - it will be dropped
                        }
                    }
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(
        connection: quinn::Connection,
        identity: Arc<IdentityManager>,
    ) -> Result<HandshakeResult> {
        // Extract TLS binding BEFORE accepting streams - this binds identity to this TLS session
        // and prevents MITM attacks where handshake messages could be transplanted
        let tls_binding = extract_tls_binding(&connection)?;

        // Accept the control stream (first bidirectional stream)
        let (mut send, mut recv) = connection
            .accept_bi()
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        // Perform secure identity handshake (RFC-0002 §5.2)
        // This exchanges identity documents, verifies signatures, and binds to TLS session
        let handshake_result = execute_server_handshake(
            &mut send,
            &mut recv,
            &identity,
            tls_binding,
        ).await?;

        println!(
            "Identity handshake completed with peer: {}",
            handshake_result.peer_iid
        );

        // Return the handshake result - caller can use peer_iid for authorization
        // and spawn connection handler if needed
        Ok(handshake_result)
    }

    /// Connect to a peer and perform the identity handshake.
    ///
    /// This establishes a QUIC connection, extracts the TLS binding, and performs
    /// the full identity handshake protocol per RFC-0002 §5.
    ///
    /// # Arguments
    /// * `address` - The socket address of the peer
    /// * `expected_server_iid` - Optional expected server IID for verification
    ///
    /// # Returns
    /// A tuple of (connection, handshake_result) on success
    pub async fn connect_to_peer(
        &self,
        address: std::net::SocketAddr,
    ) -> Result<quinn::Connection> {
        // For backward compatibility, call the new secure method without expected IID
        let (connection, _result) = self.connect_to_peer_secure(address, None).await?;
        Ok(connection)
    }

    /// Connect to a peer with full identity verification.
    ///
    /// This is the secure connection method that performs the complete identity
    /// handshake with TLS binding verification.
    ///
    /// # Arguments
    /// * `address` - The socket address of the peer
    /// * `expected_server_iid` - Optional expected server IID for verification.
    ///   If provided, the connection will fail if the server's IID doesn't match.
    ///
    /// # Returns
    /// A tuple of (connection, handshake_result) containing the authenticated
    /// connection and the verified peer identity information.
    pub async fn connect_to_peer_secure(
        &self,
        address: std::net::SocketAddr,
        expected_server_iid: Option<&str>,
    ) -> Result<(quinn::Connection, HandshakeResult)> {
        // Establish QUIC connection
        let connection = self
            .endpoint
            .connect(address, "localhost")
            .map_err(|err| PostUrbitError::Io(err.to_string()))?
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        // Extract TLS binding - this cryptographically binds our identity handshake
        // to this specific TLS session, preventing MITM attacks
        let tls_binding = extract_tls_binding(&connection)?;

        // Open the control stream for handshake
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        // Perform the identity handshake as client
        let handshake_result = execute_client_handshake(
            &mut send,
            &mut recv,
            &self.identity,
            expected_server_iid,
            tls_binding,
        ).await?;

        println!(
            "Connected and authenticated with peer: {}",
            handshake_result.peer_iid
        );

        Ok((connection, handshake_result))
    }

    /// Get the local identity manager
    pub fn identity(&self) -> &Arc<IdentityManager> {
        &self.identity
    }
}

/// TLS exporter label for channel binding per RFC-0002 §4.4
const TLS_EXPORTER_LABEL: &[u8] = b"post-urbit handshake binding";

/// Extract TLS exporter value for channel binding
/// RFC 8446 §7.5 exporter with label "post-urbit handshake binding" and empty context
/// Returns 32-byte binding value used in identity handshake.
///
/// This binds the identity handshake to the specific TLS session, preventing
/// man-in-the-middle attacks where handshake messages could be transplanted
/// to a different connection.
pub fn extract_tls_binding(connection: &quinn::Connection) -> Result<[u8; 32]> {
    let mut output = [0u8; 32];
    connection
        .export_keying_material(&mut output, TLS_EXPORTER_LABEL, &[])
        .map_err(|_| PostUrbitError::Crypto("TLS exporter failed"))?;
    Ok(output)
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

pub const HANDSHAKE_DOMAIN: &[u8] = b"post-urbit-handshake-v1";
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<HandshakeError>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HandshakeError {
    pub code: String,
    pub message: String,
}

/// Result of a successful handshake
#[derive(Debug, Clone)]
pub struct HandshakeResult {
    /// The authenticated peer's Identity Identifier
    pub peer_iid: String,
    /// The authenticated peer's Device Identifier (if provided)
    pub peer_did: Option<String>,
    /// The peer's identity document
    pub peer_identity_document: IdentityDocument,
}

/// Timeout for awaiting ClientHello/ServerHello (REQ-TRANS-503-509)
pub const HANDSHAKE_HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for awaiting ClientAuth/HandshakeComplete (REQ-TRANS-503-509)
pub const HANDSHAKE_AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Total handshake timeout (REQ-TRANS-503-509)
pub const HANDSHAKE_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum handshake message size (64 KB per RFC-0002 §6.4)
pub const HANDSHAKE_MAX_MESSAGE_SIZE: u32 = 65536;

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

// ===========================================================================
// Handshake Protocol Implementation
// ===========================================================================

/// Read a length-prefixed frame from an async stream
async fn read_frame_async(
    recv: &mut quinn::RecvStream,
    max_size: u32,
) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    let len = u32::from_be_bytes(len_buf);
    if len > max_size {
        return Err(PostUrbitError::InvalidInput("frame too large"));
    }
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(payload)
}

/// Write a length-prefixed frame to an async stream
async fn write_frame_async(
    send: &mut quinn::SendStream,
    payload: &[u8],
) -> Result<()> {
    let len: u32 = payload
        .len()
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("frame length"))?;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    send.write_all(payload)
        .await
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

/// Generate a 32-byte random nonce for handshake
fn generate_nonce() -> [u8; 32] {
    let mut nonce = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
    nonce
}

/// Generate canonical timestamp for handshake
fn generate_timestamp() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Create challenge signature for handshake (server side signs first)
fn create_challenge_signature(
    signing_key: &ed25519_dalek::SigningKey,
    client_nonce: &[u8],
    server_nonce: &[u8],
    tls_binding: &[u8],
    client_iid: &str,
    server_iid: &str,
    is_server: bool,
) -> Result<String> {
    let client_iid_raw = crockford_base32_decode(client_iid)?;
    let server_iid_raw = crockford_base32_decode(server_iid)?;

    let mut challenge = Vec::with_capacity(159);
    challenge.extend_from_slice(HANDSHAKE_DOMAIN);
    if is_server {
        // Server signs: client_nonce first, then server_nonce
        challenge.extend_from_slice(client_nonce);
        challenge.extend_from_slice(server_nonce);
        challenge.extend_from_slice(tls_binding);
        challenge.extend_from_slice(&client_iid_raw);
        challenge.extend_from_slice(&server_iid_raw);
    } else {
        // Client signs: server_nonce first, then client_nonce (swapped)
        challenge.extend_from_slice(server_nonce);
        challenge.extend_from_slice(client_nonce);
        challenge.extend_from_slice(tls_binding);
        challenge.extend_from_slice(&server_iid_raw);
        challenge.extend_from_slice(&client_iid_raw);
    }

    let digest = Sha256::digest(&challenge);
    let signature = signing_key.sign(&digest);
    Ok(base64_encode(signature.to_bytes().as_slice()))
}

/// Verify a peer's identity document and derive IID
fn verify_and_extract_iid(identity_document: &serde_json::Value) -> Result<(String, IdentityDocument)> {
    let doc_str = serde_json::to_string(identity_document)
        .map_err(|_| PostUrbitError::InvalidInput("identity document json"))?;
    let doc: IdentityDocument = serde_json::from_str(&doc_str)
        .map_err(|_| PostUrbitError::InvalidInput("identity document parse"))?;

    // Verify the document signature
    IdentityManager::verify_document(&doc)?;

    // Validate IID format
    validate_crockford_base32_lower(&doc.iid)?;

    // Verify the genesis key derives to the claimed IID
    let genesis_key_bytes = base64_decode(&doc.keys.signing.genesis)?;
    if genesis_key_bytes.len() != 32 {
        return Err(PostUrbitError::InvalidInput("genesis key length"));
    }
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(
        genesis_key_bytes.as_slice().try_into()
            .map_err(|_| PostUrbitError::InvalidInput("genesis key bytes"))?
    ).map_err(|_| PostUrbitError::InvalidInput("genesis key invalid"))?;

    let derived_iid = crate::identity::derive_iid(&verifying_key);
    if derived_iid != doc.iid {
        return Err(PostUrbitError::InvalidInput("iid mismatch with genesis key"));
    }

    Ok((doc.iid.clone(), doc))
}

/// Execute client-side identity handshake
///
/// This implements the full client handshake flow per RFC-0002 §5:
/// 1. Send ClientHello with client identity and nonce
/// 2. Receive ServerHello with server's identity and challenge signature
/// 3. Verify server's challenge signature and identity document
/// 4. Send ClientAuth with our identity document and challenge signature
/// 5. Receive HandshakeComplete confirming success
///
/// Returns the authenticated peer's identity information on success.
pub async fn execute_client_handshake(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    client_identity: &IdentityManager,
    expected_server_iid: Option<&str>,
    tls_binding: [u8; 32],
) -> Result<HandshakeResult> {
    // Wrap entire handshake with total timeout
    timeout(HANDSHAKE_TOTAL_TIMEOUT, async {
        let client_iid = client_identity.iid().await;
        let client_nonce = generate_nonce();
        let timestamp = generate_timestamp();
        let tls_binding_b64 = base64_encode(&tls_binding);

        // Step 1: Send ClientHello
        let client_hello = HandshakeMessage::ClientHello(ClientHello {
            version: 1,
            client_iid: client_iid.clone(),
            client_did: None, // DID support not implemented yet
            expected_server_iid: expected_server_iid.map(|s| s.to_string()),
            client_nonce: base64_encode(&client_nonce),
            timestamp,
            tls_binding: tls_binding_b64.clone(),
        });

        let client_hello_json = canonical_handshake_json(&client_hello)?;
        write_frame_async(send, client_hello_json.as_bytes()).await?;

        // Step 2: Receive ServerHello with hello timeout
        let server_hello_bytes = timeout(
            HANDSHAKE_HELLO_TIMEOUT,
            read_frame_async(recv, HANDSHAKE_MAX_MESSAGE_SIZE)
        )
        .await
        .map_err(|_| PostUrbitError::Io("ServerHello timeout".to_string()))??;

        let server_hello_msg: HandshakeMessage = serde_json::from_slice(&server_hello_bytes)
            .map_err(|_| PostUrbitError::InvalidInput("ServerHello json"))?;

        let server_hello = match server_hello_msg {
            HandshakeMessage::ServerHello(sh) => sh,
            _ => return Err(PostUrbitError::InvalidInput("expected ServerHello")),
        };

        // Validate server hello
        let now = Utc::now();
        validate_server_hello(&server_hello, now)?;

        // Verify TLS binding matches
        if server_hello.tls_binding != tls_binding_b64 {
            return Err(PostUrbitError::InvalidInput("TLS binding mismatch"));
        }

        // If we expected a specific server IID, verify it matches
        if let Some(expected) = expected_server_iid {
            if server_hello.server_iid != expected {
                return Err(PostUrbitError::InvalidInput("server IID mismatch"));
            }
        }

        // Verify server's identity document and extract IID
        let (server_iid, server_identity_doc) = verify_and_extract_iid(&server_hello.identity_document)?;
        if server_iid != server_hello.server_iid {
            return Err(PostUrbitError::InvalidInput("server IID mismatch with document"));
        }

        // Verify server's challenge signature
        let server_nonce = base64_decode(&server_hello.server_nonce)?;
        verify_challenge_signature(
            &server_hello.challenge_signature,
            &server_identity_doc.keys.signing.current,
            &base64_encode(&client_nonce),
            &server_hello.server_nonce,
            &tls_binding_b64,
            &client_iid,
            &server_iid,
            true, // server signature
        )?;

        // Step 3: Send ClientAuth
        let identity_doc = client_identity.identity_document().await;
        let challenge_sig = create_challenge_signature_with_manager(
            client_identity,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            &client_iid,
            &server_iid,
            false, // client signature
        ).await?;

        let identity_doc_value = serde_json::to_value(&identity_doc)
            .map_err(|_| PostUrbitError::InvalidInput("identity document serialize"))?;

        let client_auth = HandshakeMessage::ClientAuth(ClientAuth {
            version: 1,
            identity_document: identity_doc_value,
            device_document: None,
            challenge_signature: challenge_sig,
            device_signature: None,
            tls_binding: tls_binding_b64,
        });

        let client_auth_json = canonical_handshake_json(&client_auth)?;
        write_frame_async(send, client_auth_json.as_bytes()).await?;

        // Step 4: Receive HandshakeComplete with auth timeout
        let complete_bytes = timeout(
            HANDSHAKE_AUTH_TIMEOUT,
            read_frame_async(recv, HANDSHAKE_MAX_MESSAGE_SIZE)
        )
        .await
        .map_err(|_| PostUrbitError::Io("HandshakeComplete timeout".to_string()))??;

        let complete_msg: HandshakeMessage = serde_json::from_slice(&complete_bytes)
            .map_err(|_| PostUrbitError::InvalidInput("HandshakeComplete json"))?;

        let complete = match complete_msg {
            HandshakeMessage::HandshakeComplete(hc) => hc,
            _ => return Err(PostUrbitError::InvalidInput("expected HandshakeComplete")),
        };

        if !complete.success {
            let error_msg = complete.error
                .map(|e| format!("{}: {}", e.code, e.message))
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(PostUrbitError::Io(format!("handshake failed: {}", error_msg)));
        }

        Ok(HandshakeResult {
            peer_iid: server_iid,
            peer_did: server_hello.server_did,
            peer_identity_document: server_identity_doc,
        })
    })
    .await
    .map_err(|_| PostUrbitError::Io("handshake total timeout".to_string()))?
}

/// Execute server-side identity handshake
///
/// This implements the full server handshake flow per RFC-0002 §5:
/// 1. Receive ClientHello with client identity and nonce
/// 2. Validate ClientHello and verify TLS binding
/// 3. Send ServerHello with our identity and challenge signature
/// 4. Receive ClientAuth with client's identity document and signature
/// 5. Verify client's challenge signature and identity document
/// 6. Send HandshakeComplete confirming success
///
/// Returns the authenticated peer's identity information on success.
pub async fn execute_server_handshake(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    server_identity: &IdentityManager,
    tls_binding: [u8; 32],
) -> Result<HandshakeResult> {
    // Wrap entire handshake with total timeout
    timeout(HANDSHAKE_TOTAL_TIMEOUT, async {
        let server_iid = server_identity.iid().await;
        let server_nonce = generate_nonce();
        let timestamp = generate_timestamp();
        let tls_binding_b64 = base64_encode(&tls_binding);

        // Step 1: Receive ClientHello with hello timeout
        let client_hello_bytes = timeout(
            HANDSHAKE_HELLO_TIMEOUT,
            read_frame_async(recv, HANDSHAKE_MAX_MESSAGE_SIZE)
        )
        .await
        .map_err(|_| PostUrbitError::Io("ClientHello timeout".to_string()))??;

        let client_hello_msg: HandshakeMessage = serde_json::from_slice(&client_hello_bytes)
            .map_err(|_| PostUrbitError::InvalidInput("ClientHello json"))?;

        let client_hello = match client_hello_msg {
            HandshakeMessage::ClientHello(ch) => ch,
            _ => return Err(PostUrbitError::InvalidInput("expected ClientHello")),
        };

        // Validate client hello
        let now = Utc::now();
        validate_client_hello(&client_hello, now)?;

        // Verify TLS binding matches
        if client_hello.tls_binding != tls_binding_b64 {
            return Err(PostUrbitError::InvalidInput("TLS binding mismatch"));
        }

        // If client expects a specific server IID, verify we match
        if let Some(expected) = &client_hello.expected_server_iid {
            if *expected != server_iid {
                // Send failure response before closing
                let complete = HandshakeMessage::HandshakeComplete(HandshakeComplete {
                    version: 1,
                    success: false,
                    error: Some(HandshakeError {
                        code: "IDENTITY_MISMATCH".to_string(),
                        message: "Server IID does not match expected".to_string(),
                    }),
                });
                let complete_json = canonical_handshake_json(&complete)?;
                let _ = write_frame_async(send, complete_json.as_bytes()).await;
                return Err(PostUrbitError::InvalidInput("server IID mismatch"));
            }
        }

        let client_iid = client_hello.client_iid.clone();
        let client_nonce = base64_decode(&client_hello.client_nonce)?;

        // Step 2: Send ServerHello
        let identity_doc = server_identity.identity_document().await;
        let challenge_sig = create_challenge_signature_with_manager(
            server_identity,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            &client_iid,
            &server_iid,
            true, // server signature
        ).await?;

        let identity_doc_value = serde_json::to_value(&identity_doc)
            .map_err(|_| PostUrbitError::InvalidInput("identity document serialize"))?;

        let server_hello = HandshakeMessage::ServerHello(ServerHello {
            version: 1,
            server_iid: server_iid.clone(),
            server_did: None,
            server_nonce: base64_encode(&server_nonce),
            timestamp,
            tls_binding: tls_binding_b64.clone(),
            identity_document: identity_doc_value,
            device_document: None,
            challenge_signature: challenge_sig,
            device_signature: None,
        });

        let server_hello_json = canonical_handshake_json(&server_hello)?;
        write_frame_async(send, server_hello_json.as_bytes()).await?;

        // Step 3: Receive ClientAuth with auth timeout
        let client_auth_bytes = timeout(
            HANDSHAKE_AUTH_TIMEOUT,
            read_frame_async(recv, HANDSHAKE_MAX_MESSAGE_SIZE)
        )
        .await
        .map_err(|_| PostUrbitError::Io("ClientAuth timeout".to_string()))??;

        let client_auth_msg: HandshakeMessage = serde_json::from_slice(&client_auth_bytes)
            .map_err(|_| PostUrbitError::InvalidInput("ClientAuth json"))?;

        let client_auth = match client_auth_msg {
            HandshakeMessage::ClientAuth(ca) => ca,
            _ => return Err(PostUrbitError::InvalidInput("expected ClientAuth")),
        };

        // Verify TLS binding in ClientAuth
        if client_auth.tls_binding != tls_binding_b64 {
            let complete = HandshakeMessage::HandshakeComplete(HandshakeComplete {
                version: 1,
                success: false,
                error: Some(HandshakeError {
                    code: "TLS_BINDING_MISMATCH".to_string(),
                    message: "TLS binding mismatch in ClientAuth".to_string(),
                }),
            });
            let complete_json = canonical_handshake_json(&complete)?;
            let _ = write_frame_async(send, complete_json.as_bytes()).await;
            return Err(PostUrbitError::InvalidInput("TLS binding mismatch in ClientAuth"));
        }

        // Verify client's identity document
        let (verified_client_iid, client_identity_doc) = verify_and_extract_iid(&client_auth.identity_document)?;
        if verified_client_iid != client_iid {
            let complete = HandshakeMessage::HandshakeComplete(HandshakeComplete {
                version: 1,
                success: false,
                error: Some(HandshakeError {
                    code: "IDENTITY_MISMATCH".to_string(),
                    message: "Client IID mismatch with document".to_string(),
                }),
            });
            let complete_json = canonical_handshake_json(&complete)?;
            let _ = write_frame_async(send, complete_json.as_bytes()).await;
            return Err(PostUrbitError::InvalidInput("client IID mismatch with document"));
        }

        // Verify client's challenge signature
        verify_challenge_signature(
            &client_auth.challenge_signature,
            &client_identity_doc.keys.signing.current,
            &client_hello.client_nonce,
            &base64_encode(&server_nonce),
            &tls_binding_b64,
            &client_iid,
            &server_iid,
            false, // client signature
        ).map_err(|_| {
            // Don't block on sending error response
            PostUrbitError::Crypto("client challenge signature invalid")
        })?;

        // Step 4: Send HandshakeComplete
        let complete = HandshakeMessage::HandshakeComplete(HandshakeComplete {
            version: 1,
            success: true,
            error: None,
        });
        let complete_json = canonical_handshake_json(&complete)?;
        write_frame_async(send, complete_json.as_bytes()).await?;

        Ok(HandshakeResult {
            peer_iid: client_iid,
            peer_did: client_hello.client_did,
            peer_identity_document: client_identity_doc,
        })
    })
    .await
    .map_err(|_| PostUrbitError::Io("handshake total timeout".to_string()))?
}

/// Create challenge signature using IdentityManager's sign_data method.
/// This constructs the challenge data and signs it directly using the identity manager.
async fn create_challenge_signature_with_manager(
    identity: &IdentityManager,
    client_nonce: &[u8],
    server_nonce: &[u8],
    tls_binding: &[u8],
    client_iid: &str,
    server_iid: &str,
    is_server: bool,
) -> Result<String> {
    let client_iid_raw = crockford_base32_decode(client_iid)?;
    let server_iid_raw = crockford_base32_decode(server_iid)?;

    let mut challenge = Vec::with_capacity(159);
    challenge.extend_from_slice(HANDSHAKE_DOMAIN);
    if is_server {
        // Server signs: client_nonce first, then server_nonce
        challenge.extend_from_slice(client_nonce);
        challenge.extend_from_slice(server_nonce);
        challenge.extend_from_slice(tls_binding);
        challenge.extend_from_slice(&client_iid_raw);
        challenge.extend_from_slice(&server_iid_raw);
    } else {
        // Client signs: server_nonce first, then client_nonce (swapped)
        challenge.extend_from_slice(server_nonce);
        challenge.extend_from_slice(client_nonce);
        challenge.extend_from_slice(tls_binding);
        challenge.extend_from_slice(&server_iid_raw);
        challenge.extend_from_slice(&client_iid_raw);
    }

    let digest = Sha256::digest(&challenge);
    let signature = identity.sign_data(&digest).await;
    Ok(base64_encode(&signature))
}

// ===========================================================================
// Glare Resolution (Connection Deduplication)
// ===========================================================================

/// Information about an active connection
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Who initiated the connection (true = we initiated, false = peer initiated)
    pub we_initiated: bool,
    /// When the connection was established
    pub established_at: std::time::Instant,
}

/// Track active connections to detect and resolve glare
///
/// Glare occurs when two peers attempt to connect to each other simultaneously.
/// This tracker implements REQ-TRANS-152-154 for deterministic resolution.
#[derive(Debug, Default)]
pub struct ConnectionTracker {
    /// Map of (remote_iid, remote_did) -> ConnectionInfo
    active: HashMap<(String, Option<String>), ConnectionInfo>,
}

impl ConnectionTracker {
    /// Create a new connection tracker
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    /// Register a new connection
    ///
    /// Returns true if the connection was registered, false if a connection
    /// to this peer already exists (glare detected).
    pub fn register(
        &mut self,
        remote_iid: &str,
        remote_did: Option<&str>,
        we_initiated: bool,
    ) -> bool {
        let key = (remote_iid.to_string(), remote_did.map(|s| s.to_string()));
        if self.active.contains_key(&key) {
            return false; // Glare detected
        }
        self.active.insert(key, ConnectionInfo {
            we_initiated,
            established_at: std::time::Instant::now(),
        });
        true
    }

    /// Remove a connection from tracking
    pub fn remove(&mut self, remote_iid: &str, remote_did: Option<&str>) {
        let key = (remote_iid.to_string(), remote_did.map(|s| s.to_string()));
        self.active.remove(&key);
    }

    /// Check if we have an active connection to this peer
    pub fn has_connection(&self, remote_iid: &str, remote_did: Option<&str>) -> bool {
        let key = (remote_iid.to_string(), remote_did.map(|s| s.to_string()));
        self.active.contains_key(&key)
    }

    /// Get information about an active connection
    pub fn get_connection(&self, remote_iid: &str, remote_did: Option<&str>) -> Option<&ConnectionInfo> {
        let key = (remote_iid.to_string(), remote_did.map(|s| s.to_string()));
        self.active.get(&key)
    }

    /// Check if connection is duplicate and resolve glare
    ///
    /// Returns true if this connection should be kept, false if it should close.
    ///
    /// Resolution rule (REQ-TRANS-152-154):
    /// Compare (iid, did) tuples lexicographically.
    /// The connection initiated by the lexicographically smaller tuple survives.
    ///
    /// # Arguments
    /// * `local_iid` - Our Identity Identifier
    /// * `local_did` - Our Device Identifier (if any)
    /// * `remote_iid` - Remote peer's Identity Identifier
    /// * `remote_did` - Remote peer's Device Identifier (if any)
    ///
    /// # Returns
    /// * `true` if this connection should be kept
    /// * `false` if this connection should be closed (the other one survives)
    pub fn resolve_glare(
        &mut self,
        local_iid: &str,
        local_did: Option<&str>,
        remote_iid: &str,
        remote_did: Option<&str>,
    ) -> bool {
        let key = (remote_iid.to_string(), remote_did.map(|s| s.to_string()));

        // Check if we already have a connection to this peer
        let Some(existing) = self.active.get(&key) else {
            // No existing connection, keep this one
            return true;
        };

        // Glare detected! Resolve using lexicographic ordering of initiator tuples
        // The connection initiated by the smaller (iid, did) tuple survives

        let local_tuple = (local_iid, local_did);
        let remote_tuple = (remote_iid, remote_did);

        // Determine which tuple is smaller using lexicographic comparison
        let local_is_smaller = tuple_less_than(local_tuple, remote_tuple);

        // If we initiated the existing connection:
        // - local_is_smaller means we should keep our connection (return false for new one)
        // - !local_is_smaller means peer's connection wins (close our existing, keep new)
        //
        // If peer initiated the existing connection:
        // - local_is_smaller means we initiated the new one, and our tuple is smaller, so new wins
        // - !local_is_smaller means peer's tuple is smaller, keep existing (return false)

        if existing.we_initiated {
            // We initiated the existing connection
            // The existing connection was initiated by local tuple
            // Keep the one initiated by smaller tuple
            if local_is_smaller {
                // Our tuple is smaller, keep existing, close new
                false
            } else {
                // Remote tuple is smaller, but remote initiated new connection
                // Wait, if we_initiated for existing, then new connection is from peer
                // Peer's tuple is remote_tuple. If remote > local, then local_is_smaller is true
                // This case: !local_is_smaller means remote < local
                // Peer initiated new connection (their tuple), and peer's tuple is smaller
                // So keep new connection (return true), remove existing
                self.active.remove(&key);
                true
            }
        } else {
            // Peer initiated the existing connection
            // New connection: who initiated it? We did (since we're checking glare)
            // Our tuple initiated new, peer's tuple initiated existing
            if local_is_smaller {
                // Our tuple is smaller, new connection wins
                self.active.remove(&key);
                true
            } else {
                // Peer's tuple is smaller, existing wins
                false
            }
        }
    }
}

/// Compare (iid, did) tuples lexicographically per RFC-0002 §8.5
///
/// - iid: 32-char Crockford Base32 string
/// - did: 32-char Crockford Base32 string or None
/// - None sorts before any defined value
fn tuple_less_than(
    a: (&str, Option<&str>),
    b: (&str, Option<&str>),
) -> bool {
    // First compare IIDs
    if a.0 != b.0 {
        return a.0 < b.0;
    }

    // IIDs equal, compare DIDs
    match (a.1, b.1) {
        (None, None) => false,      // Equal
        (None, Some(_)) => true,    // None < any defined value
        (Some(_), None) => false,   // Any defined value > None
        (Some(a_did), Some(b_did)) => a_did < b_did,
    }
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
            error: None,
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
                Ok::<_, PostUrbitError>(manager.identity_document().await)
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

    // ===========================================================================
    // Tests for TLS Binding Extraction (REQ-TRANS-088, 089)
    // ===========================================================================

    #[test]
    fn tls_exporter_label_is_correct() {
        // Verify the label matches RFC-0002 §4.4
        assert_eq!(TLS_EXPORTER_LABEL, b"post-urbit handshake binding");
    }

    // Note: Full extract_tls_binding test requires an actual QUIC connection
    // which would be an integration test. The function is tested implicitly
    // through the handshake integration tests.

    // ===========================================================================
    // Tests for Handshake Timeouts (REQ-TRANS-503-509)
    // ===========================================================================

    #[test]
    fn handshake_timeout_constants_match_spec() {
        // REQ-TRANS-503-509: Verify timeout values match RFC specification
        assert_eq!(HANDSHAKE_HELLO_TIMEOUT, Duration::from_secs(10));
        assert_eq!(HANDSHAKE_AUTH_TIMEOUT, Duration::from_secs(10));
        assert_eq!(HANDSHAKE_TOTAL_TIMEOUT, Duration::from_secs(30));
        assert_eq!(HANDSHAKE_MAX_MESSAGE_SIZE, 65536);
    }

    // ===========================================================================
    // Tests for Glare Resolution (REQ-TRANS-152-154)
    // ===========================================================================

    #[test]
    fn tuple_less_than_compares_iid_first() {
        // Different IIDs
        let a = ("a0000000000000000000000000000000", None);
        let b = ("b0000000000000000000000000000000", None);
        assert!(tuple_less_than(a, b));
        assert!(!tuple_less_than(b, a));
    }

    #[test]
    fn tuple_less_than_compares_did_when_iid_equal() {
        let iid = "a0000000000000000000000000000000";
        let a = (iid, Some("d0000000000000000000000000000000"));
        let b = (iid, Some("e0000000000000000000000000000000"));
        assert!(tuple_less_than(a, b));
        assert!(!tuple_less_than(b, a));
    }

    #[test]
    fn tuple_less_than_none_did_sorts_first() {
        let iid = "a0000000000000000000000000000000";
        let a = (iid, None);
        let b = (iid, Some("d0000000000000000000000000000000"));
        assert!(tuple_less_than(a, b));
        assert!(!tuple_less_than(b, a));
    }

    #[test]
    fn tuple_less_than_equal_tuples() {
        let a = ("a0000000000000000000000000000000", Some("d0000000000000000000000000000000"));
        assert!(!tuple_less_than(a, a));
    }

    #[test]
    fn connection_tracker_register_new_connection() {
        let mut tracker = ConnectionTracker::new();
        let iid = "a0000000000000000000000000000000";

        // First connection should succeed
        assert!(tracker.register(iid, None, true));
        assert!(tracker.has_connection(iid, None));

        // Second connection to same peer should fail (glare)
        assert!(!tracker.register(iid, None, false));
    }

    #[test]
    fn connection_tracker_remove_connection() {
        let mut tracker = ConnectionTracker::new();
        let iid = "a0000000000000000000000000000000";

        tracker.register(iid, None, true);
        assert!(tracker.has_connection(iid, None));

        tracker.remove(iid, None);
        assert!(!tracker.has_connection(iid, None));
    }

    #[test]
    fn connection_tracker_separate_did_connections() {
        let mut tracker = ConnectionTracker::new();
        let iid = "a0000000000000000000000000000000";
        let did1 = "d0000000000000000000000000000000";
        let did2 = "e0000000000000000000000000000000";

        // Different DIDs should be separate connections
        assert!(tracker.register(iid, Some(did1), true));
        assert!(tracker.register(iid, Some(did2), true));
        assert!(tracker.has_connection(iid, Some(did1)));
        assert!(tracker.has_connection(iid, Some(did2)));
    }

    #[test]
    fn glare_resolution_smaller_tuple_wins() {
        // Per RFC-0002 §8.5: The connection initiated by the smaller (iid, did) tuple survives

        let mut tracker = ConnectionTracker::new();
        let local_iid = "a0000000000000000000000000000000";   // smaller
        let remote_iid = "b0000000000000000000000000000000";  // larger

        // We initiated an existing connection to remote
        tracker.register(remote_iid, None, true);

        // Glare: resolve with local (smaller) having initiated
        // Since we initiated and our tuple is smaller, we keep existing
        let keep = tracker.resolve_glare(local_iid, None, remote_iid, None);
        assert!(!keep); // Don't keep new connection, existing one (ours) survives
    }

    #[test]
    fn glare_resolution_larger_tuple_closes() {
        let mut tracker = ConnectionTracker::new();
        let local_iid = "b0000000000000000000000000000000";   // larger
        let remote_iid = "a0000000000000000000000000000000";  // smaller

        // We initiated an existing connection to remote
        tracker.register(remote_iid, None, true);

        // Glare: resolve with local (larger) having initiated existing
        // Remote is smaller, so remote's connection should win
        // The new connection (from remote) should be kept
        let keep = tracker.resolve_glare(local_iid, None, remote_iid, None);
        assert!(keep); // Keep new connection (remote initiated, remote tuple is smaller)

        // Existing connection should be removed
        assert!(!tracker.has_connection(remote_iid, None));
    }

    #[test]
    fn glare_resolution_no_existing_connection() {
        let mut tracker = ConnectionTracker::new();
        let local_iid = "a0000000000000000000000000000000";
        let remote_iid = "b0000000000000000000000000000000";

        // No existing connection - should keep new one
        let keep = tracker.resolve_glare(local_iid, None, remote_iid, None);
        assert!(keep);
    }

    #[test]
    fn glare_resolution_with_did() {
        let mut tracker = ConnectionTracker::new();
        let local_iid = "a0000000000000000000000000000000";
        let local_did = Some("c0000000000000000000000000000000");
        let remote_iid = "a0000000000000000000000000000000";  // same IID
        let remote_did = Some("d0000000000000000000000000000000");  // larger DID

        // Register existing connection (we initiated to remote)
        tracker.register(remote_iid, remote_did, true);

        // Glare: local (same iid, smaller did) vs remote (same iid, larger did)
        // Local tuple is smaller, so our existing connection should survive
        let keep = tracker.resolve_glare(local_iid, local_did, remote_iid, remote_did);
        assert!(!keep);
    }

    // ===========================================================================
    // Tests for Challenge Signature Creation
    // ===========================================================================

    #[test]
    fn create_challenge_signature_server() {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let client_nonce = [1u8; 32];
        let server_nonce = [2u8; 32];
        let tls_binding = [3u8; 32];
        let client_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let server_iid = "2f0fcybfpmka5vf7ge737ex07crgnxsw";

        let sig = create_challenge_signature(
            &signing_key,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            client_iid,
            server_iid,
            true,  // server
        ).unwrap();

        // Verify we got a valid base64 signature
        let sig_bytes = base64_decode(&sig).unwrap();
        assert_eq!(sig_bytes.len(), 64);

        // Verify the signature is valid
        let signing_key_b64 = base64_encode(signing_key.verifying_key().as_bytes());
        verify_challenge_signature(
            &sig,
            &signing_key_b64,
            &base64_encode(&client_nonce),
            &base64_encode(&server_nonce),
            &base64_encode(&tls_binding),
            client_iid,
            server_iid,
            true,
        ).unwrap();
    }

    #[test]
    fn create_challenge_signature_client() {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let client_nonce = [1u8; 32];
        let server_nonce = [2u8; 32];
        let tls_binding = [3u8; 32];
        let client_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let server_iid = "2f0fcybfpmka5vf7ge737ex07crgnxsw";

        let sig = create_challenge_signature(
            &signing_key,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            client_iid,
            server_iid,
            false,  // client
        ).unwrap();

        // Verify we got a valid base64 signature
        let sig_bytes = base64_decode(&sig).unwrap();
        assert_eq!(sig_bytes.len(), 64);

        // Verify the signature is valid
        let signing_key_b64 = base64_encode(signing_key.verifying_key().as_bytes());
        verify_challenge_signature(
            &sig,
            &signing_key_b64,
            &base64_encode(&client_nonce),
            &base64_encode(&server_nonce),
            &base64_encode(&tls_binding),
            client_iid,
            server_iid,
            false,
        ).unwrap();
    }

    #[test]
    fn server_and_client_signatures_differ() {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let client_nonce = [1u8; 32];
        let server_nonce = [2u8; 32];
        let tls_binding = [3u8; 32];
        let client_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let server_iid = "2f0fcybfpmka5vf7ge737ex07crgnxsw";

        let server_sig = create_challenge_signature(
            &signing_key,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            client_iid,
            server_iid,
            true,
        ).unwrap();

        let client_sig = create_challenge_signature(
            &signing_key,
            &client_nonce,
            &server_nonce,
            &tls_binding,
            client_iid,
            server_iid,
            false,
        ).unwrap();

        // Server and client signatures should be different (different challenge data order)
        assert_ne!(server_sig, client_sig);
    }

    // ===========================================================================
    // Tests for verify_and_extract_iid
    // ===========================================================================

    #[test]
    fn verify_and_extract_iid_valid_document() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp = tempfile::tempdir().unwrap();

        let (iid, doc) = rt.block_on(async {
            let manager = IdentityManager::new(temp.path().to_str().unwrap()).await.unwrap();
            let doc = manager.identity_document().await;
            (manager.iid().await, doc)
        });

        let doc_value = serde_json::to_value(&doc).unwrap();
        let (extracted_iid, extracted_doc) = verify_and_extract_iid(&doc_value).unwrap();

        assert_eq!(extracted_iid, iid);
        assert_eq!(extracted_doc.iid, iid);
    }

    #[test]
    fn verify_and_extract_iid_invalid_signature() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp = tempfile::tempdir().unwrap();

        let mut doc = rt.block_on(async {
            let manager = IdentityManager::new(temp.path().to_str().unwrap()).await.unwrap();
            manager.identity_document().await
        });

        // Corrupt the signature
        doc.signatures.current = base64_encode(&[0u8; 64]);

        let doc_value = serde_json::to_value(&doc).unwrap();
        let err = verify_and_extract_iid(&doc_value).unwrap_err();
        assert!(matches!(err, PostUrbitError::Crypto(_)));
    }

    // ===========================================================================
    // Tests for HandshakeError and HandshakeComplete
    // ===========================================================================

    #[test]
    fn handshake_complete_success_serialization() {
        let msg = HandshakeComplete {
            version: 1,
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(!json.contains("error")); // error should be skipped when None
    }

    #[test]
    fn handshake_complete_failure_serialization() {
        let msg = HandshakeComplete {
            version: 1,
            success: false,
            error: Some(HandshakeError {
                code: "IDENTITY_MISMATCH".to_string(),
                message: "Server IID does not match expected".to_string(),
            }),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("IDENTITY_MISMATCH"));
        assert!(json.contains("Server IID does not match expected"));
    }

    #[test]
    fn handshake_complete_round_trip() {
        let original = HandshakeComplete {
            version: 1,
            success: false,
            error: Some(HandshakeError {
                code: "TLS_BINDING_MISMATCH".to_string(),
                message: "TLS binding mismatch".to_string(),
            }),
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: HandshakeComplete = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.version, 1);
        assert!(!decoded.success);
        assert!(decoded.error.is_some());
        let err = decoded.error.unwrap();
        assert_eq!(err.code, "TLS_BINDING_MISMATCH");
    }

    // ===========================================================================
    // Tests for async frame reading/writing
    // ===========================================================================

    #[test]
    fn generate_nonce_is_32_bytes() {
        let nonce = generate_nonce();
        assert_eq!(nonce.len(), 32);

        // Verify randomness (two nonces should be different)
        let nonce2 = generate_nonce();
        assert_ne!(nonce, nonce2);
    }

    #[test]
    fn generate_timestamp_is_canonical() {
        let ts = generate_timestamp();

        // Should be in canonical format: YYYY-MM-DDTHH:MM:SSZ
        assert!(ts.ends_with('Z'));
        assert!(!ts.contains('.'));  // No fractional seconds
        assert_eq!(ts.len(), 20);    // Fixed length

        // Should parse successfully
        let parsed = ts.parse::<DateTime<Utc>>();
        assert!(parsed.is_ok());
    }

    // ===========================================================================
    // Integration Tests for Secure Handshake (QUIC Transport)
    // ===========================================================================

    /// Test that QuicTransport can be created successfully
    #[tokio::test]
    async fn quic_transport_creation() {
        let temp = tempfile::tempdir().unwrap();
        let identity = IdentityManager::new(temp.path().to_str().unwrap())
            .await
            .unwrap();
        let transport = QuicTransport::new(0, Arc::new(identity)).await;
        assert!(transport.is_ok());
    }

    /// Test full client-server handshake over QUIC
    /// This is the main integration test that verifies the security fix works
    #[tokio::test]
    async fn secure_handshake_client_server_integration() {
        // Create two separate identities (server and client)
        let server_temp = tempfile::tempdir().unwrap();
        let client_temp = tempfile::tempdir().unwrap();

        let server_identity = Arc::new(
            IdentityManager::new(server_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );
        let client_identity = Arc::new(
            IdentityManager::new(client_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let server_iid = server_identity.iid().await;
        let client_iid = client_identity.iid().await;

        // They should have different IIDs
        assert_ne!(server_iid, client_iid);

        // Create server transport on random port
        let server_transport = Arc::new(
            QuicTransport::new(0, server_identity.clone())
                .await
                .unwrap()
        );

        // Get the actual bound port and use localhost (127.0.0.1) for connection
        let local_addr = server_transport.endpoint.local_addr().unwrap();
        let server_addr = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            local_addr.port()
        );

        // Create client transport
        let client_transport = QuicTransport::new(0, client_identity.clone())
            .await
            .unwrap();

        // Channel to get server result back
        let (server_tx, mut server_rx) = tokio::sync::mpsc::channel::<Result<HandshakeResult>>(1);

        // Spawn server acceptor - keeps connection alive via holding the connection object
        let server_identity_clone = server_identity.clone();
        tokio::spawn(async move {
            let conn = server_transport.endpoint.accept().await.unwrap();
            let connection = conn.await.unwrap();

            // Perform server-side handshake
            let result = QuicTransport::handle_connection(connection.clone(), server_identity_clone).await;
            let _ = server_tx.send(result).await;

            // Keep connection alive for a bit to let client finish
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            drop(connection);
        });

        // Give server a moment to be ready
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Client connects and performs handshake
        let (_connection, client_result) = client_transport
            .connect_to_peer_secure(server_addr, None)
            .await
            .unwrap();

        // Verify client got server's identity
        assert_eq!(client_result.peer_iid, server_iid);

        // Wait for server to complete
        let server_result = server_rx.recv().await.unwrap().unwrap();

        // Verify server got client's identity
        assert_eq!(server_result.peer_iid, client_iid);
    }

    /// Test that handshake fails when expected server IID doesn't match
    #[tokio::test]
    async fn secure_handshake_rejects_unexpected_server_iid() {
        let server_temp = tempfile::tempdir().unwrap();
        let client_temp = tempfile::tempdir().unwrap();

        let server_identity = Arc::new(
            IdentityManager::new(server_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );
        let client_identity = Arc::new(
            IdentityManager::new(client_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let server_transport = Arc::new(
            QuicTransport::new(0, server_identity.clone())
                .await
                .unwrap()
        );
        let local_addr = server_transport.endpoint.local_addr().unwrap();
        let server_addr = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            local_addr.port()
        );

        let client_transport = QuicTransport::new(0, client_identity)
            .await
            .unwrap();

        // Spawn server
        let server_identity_clone = server_identity.clone();
        tokio::spawn(async move {
            if let Some(conn) = server_transport.endpoint.accept().await {
                if let Ok(connection) = conn.await {
                    let _ = QuicTransport::handle_connection(connection, server_identity_clone).await;
                }
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Client expects a different server IID - should fail
        let wrong_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v"; // Some random valid IID
        let result = client_transport
            .connect_to_peer_secure(server_addr, Some(wrong_iid))
            .await;

        // Should fail due to IID mismatch
        assert!(result.is_err());
    }

    /// Test that TLS binding is properly extracted and used
    #[tokio::test]
    async fn tls_binding_extraction_works() {
        let server_temp = tempfile::tempdir().unwrap();
        let client_temp = tempfile::tempdir().unwrap();

        let server_identity = Arc::new(
            IdentityManager::new(server_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );
        let client_identity = Arc::new(
            IdentityManager::new(client_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let server_transport = Arc::new(
            QuicTransport::new(0, server_identity.clone())
                .await
                .unwrap()
        );
        let local_addr = server_transport.endpoint.local_addr().unwrap();
        let server_addr = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            local_addr.port()
        );

        let client_transport = QuicTransport::new(0, client_identity)
            .await
            .unwrap();

        // Channel to send TLS binding from server
        let (tx, mut rx) = tokio::sync::mpsc::channel::<[u8; 32]>(1);

        let server_identity_clone = server_identity.clone();
        tokio::spawn(async move {
            let conn = server_transport.endpoint.accept().await.unwrap();
            let connection = conn.await.unwrap();

            // Extract TLS binding on server side
            let server_binding = extract_tls_binding(&connection).unwrap();
            tx.send(server_binding).await.unwrap();

            let _ = QuicTransport::handle_connection(connection, server_identity_clone).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Client connects
        let client_connection = client_transport
            .endpoint
            .connect(server_addr, "localhost")
            .unwrap()
            .await
            .unwrap();

        // Extract TLS binding on client side
        let client_binding = extract_tls_binding(&client_connection).unwrap();

        // Get server's binding
        let server_binding = rx.recv().await.unwrap();

        // Both sides should have the same TLS binding for the same session
        assert_eq!(client_binding, server_binding);

        // Binding should not be all zeros
        assert_ne!(client_binding, [0u8; 32]);
    }

    /// Test that different connections have different TLS bindings
    #[tokio::test]
    async fn different_connections_have_different_tls_bindings() {
        let server_temp = tempfile::tempdir().unwrap();
        let client_temp = tempfile::tempdir().unwrap();

        let server_identity = Arc::new(
            IdentityManager::new(server_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );
        let client_identity = Arc::new(
            IdentityManager::new(client_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let server_transport = Arc::new(
            QuicTransport::new(0, server_identity.clone())
                .await
                .unwrap()
        );
        let local_addr = server_transport.endpoint.local_addr().unwrap();
        let server_addr = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            local_addr.port()
        );

        let client_transport = QuicTransport::new(0, client_identity)
            .await
            .unwrap();

        // Make first connection
        let server_transport_clone = server_transport.clone();
        let server_identity_clone = server_identity.clone();
        tokio::spawn(async move {
            let conn = server_transport_clone.endpoint.accept().await.unwrap();
            let connection = conn.await.unwrap();
            let _ = QuicTransport::handle_connection(connection.clone(), server_identity_clone).await;
            // Keep connection alive
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (connection1, _) = client_transport
            .connect_to_peer_secure(server_addr, None)
            .await
            .unwrap();
        let binding1 = extract_tls_binding(&connection1).unwrap();

        // Make second connection
        let server_identity_clone2 = server_identity.clone();
        tokio::spawn(async move {
            let conn = server_transport.endpoint.accept().await.unwrap();
            let connection = conn.await.unwrap();
            let _ = QuicTransport::handle_connection(connection.clone(), server_identity_clone2).await;
            // Keep connection alive
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (connection2, _) = client_transport
            .connect_to_peer_secure(server_addr, None)
            .await
            .unwrap();
        let binding2 = extract_tls_binding(&connection2).unwrap();

        // Different connections should have different TLS bindings
        // This proves that replay/transplant attacks would fail
        assert_ne!(binding1, binding2);
    }

    /// Test backward compatibility - connect_to_peer still works
    #[tokio::test]
    async fn connect_to_peer_backward_compatible() {
        let server_temp = tempfile::tempdir().unwrap();
        let client_temp = tempfile::tempdir().unwrap();

        let server_identity = Arc::new(
            IdentityManager::new(server_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );
        let client_identity = Arc::new(
            IdentityManager::new(client_temp.path().to_str().unwrap())
                .await
                .unwrap()
        );

        let server_transport = Arc::new(
            QuicTransport::new(0, server_identity.clone())
                .await
                .unwrap()
        );
        let local_addr = server_transport.endpoint.local_addr().unwrap();
        let server_addr = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            local_addr.port()
        );

        let client_transport = QuicTransport::new(0, client_identity)
            .await
            .unwrap();

        let server_identity_clone = server_identity.clone();
        tokio::spawn(async move {
            let conn = server_transport.endpoint.accept().await.unwrap();
            let connection = conn.await.unwrap();
            let _ = QuicTransport::handle_connection(connection.clone(), server_identity_clone).await;
            // Keep connection alive
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Old API should still work (but now performs handshake internally)
        let connection = client_transport
            .connect_to_peer(server_addr)
            .await
            .unwrap();

        // Connection should be established and authenticated
        assert!(!connection.close_reason().is_some());
    }
}

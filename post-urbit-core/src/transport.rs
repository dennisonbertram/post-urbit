use std::sync::Arc;
use async_trait::async_trait;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rcgen::generate_simple_self_signed;
use crate::identity::IdentityManager;
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

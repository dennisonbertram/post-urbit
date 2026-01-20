//! NAT discovery and traversal using STUN protocol (RFC 5389)
//!
//! This module provides NAT type detection and external address discovery
//! using the STUN (Session Traversal Utilities for NAT) protocol.

use crate::error::{PostUrbitError, Result};
use async_trait::async_trait;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

/// STUN message types
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_RESPONSE: u16 = 0x0101;

/// STUN magic cookie (RFC 5389)
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;

/// STUN attribute types
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const ATTR_MAPPED_ADDRESS: u16 = 0x0001;

/// Address family constants
const ADDR_FAMILY_IPV4: u8 = 0x01;
const ADDR_FAMILY_IPV6: u8 = 0x02;

/// NAT type classification based on behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NATType {
    /// No NAT detected (public IP)
    None,
    /// Full cone NAT (most permissive)
    FullCone,
    /// Restricted cone NAT
    RestrictedCone,
    /// Port-restricted cone NAT
    PortRestricted,
    /// Symmetric NAT (most restrictive, needs relay)
    Symmetric,
    /// Unknown/couldn't determine
    Unknown,
}

impl std::fmt::Display for NATType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NATType::None => write!(f, "No NAT"),
            NATType::FullCone => write!(f, "Full Cone"),
            NATType::RestrictedCone => write!(f, "Restricted Cone"),
            NATType::PortRestricted => write!(f, "Port Restricted"),
            NATType::Symmetric => write!(f, "Symmetric"),
            NATType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Trait for NAT discovery implementations
#[async_trait]
pub trait NATDiscovery: Send + Sync {
    /// Get external address (may be cached)
    fn external_addr(&self) -> Result<Option<String>>;

    /// Async version of external address discovery
    async fn discover_external_address(&self) -> Result<Option<SocketAddr>>;

    /// Get the detected NAT type
    fn nat_type(&self) -> NATType;
}

/// Cached external address with timestamp
#[derive(Debug, Clone)]
struct CachedAddress {
    address: SocketAddr,
    discovered_at: Instant,
}

/// STUN-based NAT discovery implementation
pub struct StunNatDiscovery {
    stun_servers: Vec<String>,
    local_port: u16,
    cached_address: Arc<RwLock<Option<CachedAddress>>>,
    cache_duration: Duration,
    detected_nat_type: Arc<RwLock<NATType>>,
    timeout: Duration,
}

impl StunNatDiscovery {
    /// Create with default STUN servers
    pub fn new(local_port: u16) -> Self {
        Self {
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
                "stun1.l.google.com:19302".to_string(),
                "stun.cloudflare.com:3478".to_string(),
            ],
            local_port,
            cached_address: Arc::new(RwLock::new(None)),
            cache_duration: Duration::from_secs(300), // 5 min cache
            detected_nat_type: Arc::new(RwLock::new(NATType::Unknown)),
            timeout: Duration::from_secs(5),
        }
    }

    /// Create with custom STUN servers
    pub fn with_servers(servers: Vec<String>, local_port: u16) -> Self {
        Self {
            stun_servers: servers,
            local_port,
            cached_address: Arc::new(RwLock::new(None)),
            cache_duration: Duration::from_secs(300),
            detected_nat_type: Arc::new(RwLock::new(NATType::Unknown)),
            timeout: Duration::from_secs(5),
        }
    }

    /// Set cache duration
    pub fn with_cache_duration(mut self, duration: Duration) -> Self {
        self.cache_duration = duration;
        self
    }

    /// Set timeout for STUN requests
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Check if cached address is still valid
    async fn get_cached(&self) -> Option<SocketAddr> {
        let cache = self.cached_address.read().await;
        if let Some(ref cached) = *cache {
            if cached.discovered_at.elapsed() < self.cache_duration {
                return Some(cached.address);
            }
        }
        None
    }

    /// Store address in cache
    async fn set_cached(&self, addr: SocketAddr) {
        let mut cache = self.cached_address.write().await;
        *cache = Some(CachedAddress {
            address: addr,
            discovered_at: Instant::now(),
        });
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cached_address.write().await;
        *cache = None;
    }

    /// Discover external address using STUN
    async fn discover_via_stun(&self) -> Result<SocketAddr> {
        // Try each STUN server until one succeeds
        let mut last_error = None;

        for server in &self.stun_servers {
            match self.query_stun_server(server).await {
                Ok(addr) => {
                    self.set_cached(addr).await;
                    return Ok(addr);
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            PostUrbitError::Io("No STUN servers available".to_string())
        }))
    }

    /// Query a single STUN server
    async fn query_stun_server(&self, server: &str) -> Result<SocketAddr> {
        // Resolve server address
        let server_addr: SocketAddr = tokio::net::lookup_host(server)
            .await
            .map_err(|e| PostUrbitError::Io(format!("DNS lookup failed for {}: {}", server, e)))?
            .next()
            .ok_or_else(|| PostUrbitError::Io(format!("No address found for {}", server)))?;

        // Create UDP socket bound to local port
        let bind_addr = format!("0.0.0.0:{}", self.local_port);
        let socket = UdpSocket::bind(&bind_addr)
            .await
            .or_else(|_| {
                // If specific port fails, try ephemeral
                futures::executor::block_on(UdpSocket::bind("0.0.0.0:0"))
            })
            .map_err(|e| PostUrbitError::Io(format!("Failed to bind UDP socket: {}", e)))?;

        // Build and send STUN request
        let request = Self::build_stun_request();
        socket
            .send_to(&request, server_addr)
            .await
            .map_err(|e| PostUrbitError::Io(format!("Failed to send STUN request: {}", e)))?;

        // Receive response with timeout
        let mut buf = [0u8; 1024];
        let recv_future = socket.recv_from(&mut buf);
        let result = tokio::time::timeout(self.timeout, recv_future).await;

        match result {
            Ok(Ok((len, _from))) => Self::parse_stun_response(&buf[..len]),
            Ok(Err(e)) => Err(PostUrbitError::Io(format!(
                "Failed to receive STUN response: {}",
                e
            ))),
            Err(_) => Err(PostUrbitError::Io("STUN request timed out".to_string())),
        }
    }

    /// Build a STUN binding request (RFC 5389)
    pub fn build_stun_request() -> Vec<u8> {
        let mut request = Vec::with_capacity(20);

        // Message Type: Binding Request (0x0001)
        request.extend_from_slice(&STUN_BINDING_REQUEST.to_be_bytes());

        // Message Length: 0 (no attributes)
        request.extend_from_slice(&0u16.to_be_bytes());

        // Magic Cookie
        request.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());

        // Transaction ID (96 bits = 12 bytes, random)
        let transaction_id: [u8; 12] = rand::random();
        request.extend_from_slice(&transaction_id);

        request
    }

    /// Parse STUN binding response (RFC 5389)
    pub fn parse_stun_response(response: &[u8]) -> Result<SocketAddr> {
        if response.len() < 20 {
            return Err(PostUrbitError::InvalidInput("STUN response too short"));
        }

        // Check message type
        let msg_type = u16::from_be_bytes([response[0], response[1]]);
        if msg_type != STUN_BINDING_RESPONSE {
            return Err(PostUrbitError::InvalidInput("Not a STUN binding response"));
        }

        // Get message length
        let msg_len = u16::from_be_bytes([response[2], response[3]]) as usize;
        if response.len() < 20 + msg_len {
            return Err(PostUrbitError::InvalidInput("STUN response truncated"));
        }

        // Verify magic cookie
        let magic = u32::from_be_bytes([response[4], response[5], response[6], response[7]]);
        if magic != STUN_MAGIC_COOKIE {
            return Err(PostUrbitError::InvalidInput("Invalid STUN magic cookie"));
        }

        // Parse attributes (starting at byte 20)
        let mut offset = 20;
        let end = 20 + msg_len;

        while offset + 4 <= end {
            let attr_type = u16::from_be_bytes([response[offset], response[offset + 1]]);
            let attr_len =
                u16::from_be_bytes([response[offset + 2], response[offset + 3]]) as usize;

            offset += 4;

            if offset + attr_len > end {
                break;
            }

            // Try XOR-MAPPED-ADDRESS first (preferred), then MAPPED-ADDRESS
            if attr_type == ATTR_XOR_MAPPED_ADDRESS {
                return Self::parse_xor_mapped_address(&response[offset..offset + attr_len]);
            } else if attr_type == ATTR_MAPPED_ADDRESS {
                return Self::parse_mapped_address(&response[offset..offset + attr_len]);
            }

            // Attributes are padded to 4-byte boundary
            offset += (attr_len + 3) & !3;
        }

        Err(PostUrbitError::InvalidInput(
            "No mapped address in STUN response",
        ))
    }

    /// Parse XOR-MAPPED-ADDRESS attribute (RFC 5389)
    fn parse_xor_mapped_address(data: &[u8]) -> Result<SocketAddr> {
        if data.len() < 8 {
            return Err(PostUrbitError::InvalidInput(
                "XOR-MAPPED-ADDRESS too short",
            ));
        }

        let family = data[1];
        let xport = u16::from_be_bytes([data[2], data[3]]);

        // XOR port with upper 16 bits of magic cookie
        let port = xport ^ ((STUN_MAGIC_COOKIE >> 16) as u16);

        match family {
            ADDR_FAMILY_IPV4 => {
                if data.len() < 8 {
                    return Err(PostUrbitError::InvalidInput(
                        "XOR-MAPPED-ADDRESS IPv4 too short",
                    ));
                }
                let xaddr = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                // XOR address with magic cookie
                let addr = xaddr ^ STUN_MAGIC_COOKIE;
                let ip = Ipv4Addr::from(addr);
                Ok(SocketAddr::new(IpAddr::V4(ip), port))
            }
            ADDR_FAMILY_IPV6 => {
                // IPv6 is XORed with magic cookie + transaction ID
                // For simplicity, return error for now (most NAT scenarios are IPv4)
                Err(PostUrbitError::InvalidInput(
                    "IPv6 XOR-MAPPED-ADDRESS not yet supported",
                ))
            }
            _ => Err(PostUrbitError::InvalidInput(
                "Unknown address family in XOR-MAPPED-ADDRESS",
            )),
        }
    }

    /// Parse MAPPED-ADDRESS attribute (legacy, RFC 3489)
    fn parse_mapped_address(data: &[u8]) -> Result<SocketAddr> {
        if data.len() < 8 {
            return Err(PostUrbitError::InvalidInput("MAPPED-ADDRESS too short"));
        }

        let family = data[1];
        let port = u16::from_be_bytes([data[2], data[3]]);

        match family {
            ADDR_FAMILY_IPV4 => {
                let addr = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                let ip = Ipv4Addr::from(addr);
                Ok(SocketAddr::new(IpAddr::V4(ip), port))
            }
            ADDR_FAMILY_IPV6 => Err(PostUrbitError::InvalidInput(
                "IPv6 MAPPED-ADDRESS not yet supported",
            )),
            _ => Err(PostUrbitError::InvalidInput(
                "Unknown address family in MAPPED-ADDRESS",
            )),
        }
    }

    /// Detect NAT type by querying multiple STUN servers
    pub async fn detect_nat_type(&self) -> Result<NATType> {
        // Get local address
        let local_ip = self.get_local_ip().await?;

        // Query first STUN server
        let first_result = if !self.stun_servers.is_empty() {
            self.query_stun_server(&self.stun_servers[0]).await.ok()
        } else {
            return Ok(NATType::Unknown);
        };

        let first_addr = match first_result {
            Some(addr) => addr,
            None => return Ok(NATType::Unknown),
        };

        // Check if external IP matches local IP (no NAT)
        if first_addr.ip() == local_ip {
            let mut nat_type = self.detected_nat_type.write().await;
            *nat_type = NATType::None;
            return Ok(NATType::None);
        }

        // Query second STUN server if available
        if self.stun_servers.len() < 2 {
            // Can't fully determine NAT type with single server
            let mut nat_type = self.detected_nat_type.write().await;
            *nat_type = NATType::Unknown;
            return Ok(NATType::Unknown);
        }

        let second_result = self.query_stun_server(&self.stun_servers[1]).await.ok();

        let detected = match second_result {
            Some(second_addr) => {
                // Compare ports from different STUN servers
                if first_addr.port() == second_addr.port() {
                    // Same external port - likely cone NAT
                    // Note: Full cone vs restricted requires additional tests
                    NATType::FullCone
                } else {
                    // Different external ports - symmetric NAT
                    NATType::Symmetric
                }
            }
            None => NATType::Unknown,
        };

        let mut nat_type = self.detected_nat_type.write().await;
        *nat_type = detected;
        Ok(detected)
    }

    /// Get local IP address
    async fn get_local_ip(&self) -> Result<IpAddr> {
        // Connect to a public address to determine local interface
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| PostUrbitError::Io(e.to_string()))?;

        socket
            .connect("8.8.8.8:80")
            .await
            .map_err(|e| PostUrbitError::Io(e.to_string()))?;

        let local_addr = socket
            .local_addr()
            .map_err(|e| PostUrbitError::Io(e.to_string()))?;

        Ok(local_addr.ip())
    }
}

#[async_trait]
impl NATDiscovery for StunNatDiscovery {
    fn external_addr(&self) -> Result<Option<String>> {
        // Synchronous version - check cache only
        // Use tokio runtime to access async cache
        let cache = futures::executor::block_on(self.cached_address.read());
        if let Some(ref cached) = *cache {
            if cached.discovered_at.elapsed() < self.cache_duration {
                return Ok(Some(cached.address.to_string()));
            }
        }
        Ok(None)
    }

    async fn discover_external_address(&self) -> Result<Option<SocketAddr>> {
        // Check cache first
        if let Some(cached) = self.get_cached().await {
            return Ok(Some(cached));
        }

        // Discover via STUN
        match self.discover_via_stun().await {
            Ok(addr) => Ok(Some(addr)),
            Err(_) => Ok(None),
        }
    }

    fn nat_type(&self) -> NATType {
        futures::executor::block_on(async { *self.detected_nat_type.read().await })
    }
}

/// Simple NAT stub for testing (original implementation)
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

#[async_trait]
impl NATDiscovery for NATStub {
    fn external_addr(&self) -> Result<Option<String>> {
        Ok(self.external.clone())
    }

    async fn discover_external_address(&self) -> Result<Option<SocketAddr>> {
        match &self.external {
            Some(addr_str) => {
                let addr: SocketAddr = addr_str
                    .parse()
                    .map_err(|_| PostUrbitError::InvalidInput("Invalid address format"))?;
                Ok(Some(addr))
            }
            None => Ok(None),
        }
    }

    fn nat_type(&self) -> NATType {
        NATType::Unknown
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
        assert_eq!(
            stub.external_addr().unwrap(),
            Some("1.2.3.4:1234".to_string())
        );
    }

    #[test]
    fn stun_request_format() {
        let request = StunNatDiscovery::build_stun_request();

        // Should be exactly 20 bytes
        assert_eq!(request.len(), 20);

        // Check message type (Binding Request = 0x0001)
        assert_eq!(request[0], 0x00);
        assert_eq!(request[1], 0x01);

        // Check message length (0 attributes)
        assert_eq!(request[2], 0x00);
        assert_eq!(request[3], 0x00);

        // Check magic cookie (0x2112A442)
        assert_eq!(request[4], 0x21);
        assert_eq!(request[5], 0x12);
        assert_eq!(request[6], 0xA4);
        assert_eq!(request[7], 0x42);

        // Transaction ID should be 12 bytes (not verifiable for randomness)
        assert_eq!(request.len() - 8, 12);
    }

    #[test]
    fn stun_request_unique_transaction_ids() {
        let req1 = StunNatDiscovery::build_stun_request();
        let req2 = StunNatDiscovery::build_stun_request();

        // Transaction IDs should be different
        assert_ne!(&req1[8..20], &req2[8..20]);
    }

    #[test]
    fn parse_xor_mapped_address_response() {
        // Construct a valid STUN Binding Response with XOR-MAPPED-ADDRESS
        // External address: 203.0.113.5:12345
        // XOR with magic cookie:
        // - Port: 12345 XOR 0x2112 = 0x3039 XOR 0x2112 = 0x112B
        // - IP: 203.0.113.5 = 0xCB007105 XOR 0x2112A442 = 0xEA12D547

        let mut response = Vec::new();

        // Header
        response.extend_from_slice(&[0x01, 0x01]); // Binding Response
        response.extend_from_slice(&[0x00, 0x0C]); // Length: 12 bytes
        response.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]); // Magic cookie
        response.extend_from_slice(&[0; 12]); // Transaction ID

        // XOR-MAPPED-ADDRESS attribute
        response.extend_from_slice(&[0x00, 0x20]); // Type: XOR-MAPPED-ADDRESS
        response.extend_from_slice(&[0x00, 0x08]); // Length: 8 bytes
        response.push(0x00); // Reserved
        response.push(0x01); // Family: IPv4
        response.extend_from_slice(&[0x11, 0x2B]); // XORed port: 12345 XOR 0x2112
        response.extend_from_slice(&[0xEA, 0x12, 0xD5, 0x47]); // XORed IP

        let result = StunNatDiscovery::parse_stun_response(&response);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let addr = result.unwrap();
        assert_eq!(addr.ip().to_string(), "203.0.113.5");
        assert_eq!(addr.port(), 12345);
    }

    #[test]
    fn parse_mapped_address_response() {
        // Construct a valid STUN Binding Response with MAPPED-ADDRESS (legacy)
        // External address: 192.168.1.100:54321

        let mut response = Vec::new();

        // Header
        response.extend_from_slice(&[0x01, 0x01]); // Binding Response
        response.extend_from_slice(&[0x00, 0x0C]); // Length: 12 bytes
        response.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]); // Magic cookie
        response.extend_from_slice(&[0; 12]); // Transaction ID

        // MAPPED-ADDRESS attribute
        response.extend_from_slice(&[0x00, 0x01]); // Type: MAPPED-ADDRESS
        response.extend_from_slice(&[0x00, 0x08]); // Length: 8 bytes
        response.push(0x00); // Reserved
        response.push(0x01); // Family: IPv4
        response.extend_from_slice(&0xD431u16.to_be_bytes()); // Port: 54321
        response.extend_from_slice(&[192, 168, 1, 100]); // IP: 192.168.1.100

        let result = StunNatDiscovery::parse_stun_response(&response);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let addr = result.unwrap();
        assert_eq!(addr.ip().to_string(), "192.168.1.100");
        assert_eq!(addr.port(), 54321);
    }

    #[test]
    fn parse_stun_response_too_short() {
        let response = [0u8; 10]; // Too short
        let result = StunNatDiscovery::parse_stun_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn parse_stun_response_wrong_type() {
        let mut response = vec![0u8; 20];
        response[0] = 0x00;
        response[1] = 0x01; // Binding Request, not Response
        response[4] = 0x21;
        response[5] = 0x12;
        response[6] = 0xA4;
        response[7] = 0x42;

        let result = StunNatDiscovery::parse_stun_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn parse_stun_response_invalid_magic() {
        let mut response = vec![0u8; 20];
        response[0] = 0x01;
        response[1] = 0x01; // Binding Response
        // Wrong magic cookie
        response[4] = 0x00;
        response[5] = 0x00;
        response[6] = 0x00;
        response[7] = 0x00;

        let result = StunNatDiscovery::parse_stun_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn parse_stun_response_no_address() {
        let mut response = Vec::new();

        // Header with no attributes
        response.extend_from_slice(&[0x01, 0x01]); // Binding Response
        response.extend_from_slice(&[0x00, 0x00]); // Length: 0 bytes
        response.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]); // Magic cookie
        response.extend_from_slice(&[0; 12]); // Transaction ID

        let result = StunNatDiscovery::parse_stun_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn nat_type_display() {
        assert_eq!(format!("{}", NATType::None), "No NAT");
        assert_eq!(format!("{}", NATType::FullCone), "Full Cone");
        assert_eq!(format!("{}", NATType::RestrictedCone), "Restricted Cone");
        assert_eq!(format!("{}", NATType::PortRestricted), "Port Restricted");
        assert_eq!(format!("{}", NATType::Symmetric), "Symmetric");
        assert_eq!(format!("{}", NATType::Unknown), "Unknown");
    }

    #[test]
    fn stun_discovery_creation() {
        let discovery = StunNatDiscovery::new(4433);
        assert_eq!(discovery.local_port, 4433);
        assert_eq!(discovery.stun_servers.len(), 3);
        assert_eq!(discovery.cache_duration, Duration::from_secs(300));
    }

    #[test]
    fn stun_discovery_with_custom_servers() {
        let servers = vec!["custom.stun.server:3478".to_string()];
        let discovery = StunNatDiscovery::with_servers(servers.clone(), 5000);
        assert_eq!(discovery.stun_servers, servers);
        assert_eq!(discovery.local_port, 5000);
    }

    #[test]
    fn stun_discovery_builder_pattern() {
        let discovery = StunNatDiscovery::new(4433)
            .with_cache_duration(Duration::from_secs(600))
            .with_timeout(Duration::from_secs(10));

        assert_eq!(discovery.cache_duration, Duration::from_secs(600));
        assert_eq!(discovery.timeout, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn stun_discovery_cache_behavior() {
        let discovery =
            StunNatDiscovery::new(0).with_cache_duration(Duration::from_millis(100));

        // Initially no cache
        assert!(discovery.get_cached().await.is_none());

        // Set cache
        let addr: SocketAddr = "1.2.3.4:5678".parse().unwrap();
        discovery.set_cached(addr).await;

        // Should be cached
        assert_eq!(discovery.get_cached().await, Some(addr));

        // Wait for cache to expire
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Cache should be expired
        assert!(discovery.get_cached().await.is_none());
    }

    #[tokio::test]
    async fn stun_discovery_clear_cache() {
        let discovery = StunNatDiscovery::new(0);

        let addr: SocketAddr = "1.2.3.4:5678".parse().unwrap();
        discovery.set_cached(addr).await;

        assert!(discovery.get_cached().await.is_some());

        discovery.clear_cache().await;

        assert!(discovery.get_cached().await.is_none());
    }

    #[test]
    fn nat_type_equality() {
        assert_eq!(NATType::None, NATType::None);
        assert_eq!(NATType::Symmetric, NATType::Symmetric);
        assert_ne!(NATType::None, NATType::Symmetric);
        assert_ne!(NATType::FullCone, NATType::PortRestricted);
    }

    // Test vectors for STUN XOR encoding
    #[test]
    fn xor_encoding_test_vector() {
        // RFC 5389 example-like test
        // Magic cookie: 0x2112A442
        // Port 12345 (0x3039) XOR 0x2112 = 0x112B
        let port: u16 = 12345;
        let xor_port = port ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
        assert_eq!(xor_port, 0x112B);

        // IP 203.0.113.5 (0xCB007105) XOR 0x2112A442 = 0xEA12D547
        let ip: u32 = u32::from_be_bytes([203, 0, 113, 5]);
        let xor_ip = ip ^ STUN_MAGIC_COOKIE;
        assert_eq!(xor_ip, 0xEA12D547);
    }

    #[tokio::test]
    async fn nat_stub_async_discovery() {
        let mut stub = NATStub::new();
        stub.set_external(Some("10.0.0.1:8080".to_string()));

        let result = stub.discover_external_address().await;
        assert!(result.is_ok());

        let addr = result.unwrap();
        assert!(addr.is_some());
        assert_eq!(addr.unwrap().to_string(), "10.0.0.1:8080");
    }

    #[tokio::test]
    async fn nat_stub_async_discovery_none() {
        let stub = NATStub::new();

        let result = stub.discover_external_address().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn nat_stub_nat_type() {
        let stub = NATStub::new();
        assert_eq!(stub.nat_type(), NATType::Unknown);
    }

    // Note: Live STUN server tests are commented out as they require network access
    // Uncomment for integration testing
    /*
    #[tokio::test]
    async fn live_stun_discovery() {
        let discovery = StunNatDiscovery::new(0);
        let result = discovery.discover_external_address().await;

        match result {
            Ok(Some(addr)) => {
                println!("Discovered external address: {}", addr);
                assert!(!addr.ip().is_loopback());
            }
            Ok(None) => {
                println!("No external address discovered (may be behind restrictive firewall)");
            }
            Err(e) => {
                println!("STUN discovery failed (expected in some environments): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn live_nat_type_detection() {
        let discovery = StunNatDiscovery::new(0);
        let nat_type = discovery.detect_nat_type().await;

        match nat_type {
            Ok(t) => println!("Detected NAT type: {}", t),
            Err(e) => println!("NAT type detection failed: {}", e),
        }
    }
    */
}

use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;
use reqwest::tls::Version as TlsVersion;
use url::Url;

type NetworkResult<T> = std::result::Result<T, NetworkError>;

const DEFAULT_REQUESTS_PER_MINUTE: u32 = 100;
const DEFAULT_REQUESTS_PER_DAY: u32 = 10_000;
const DEFAULT_BYTES_PER_DAY: u64 = 100 * 1024 * 1024;

pub const MAX_REQUEST_BYTES: usize = 50 * 1024 * 1024;
pub const DEFAULT_REQUEST_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 50 * 1024 * 1024;
pub const DEFAULT_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const MAX_REDIRECTS: usize = 10;

#[derive(Debug, Clone)]
pub struct NetworkRequest {
    pub url: Url,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
    pub max_response_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct NetworkResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct NetworkLimits {
    pub requests_per_minute: u32,
    pub requests_per_day: u32,
    pub bytes_per_day: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkLimitsOverride {
    pub requests_per_minute: Option<u32>,
    pub requests_per_day: Option<u32>,
}

#[derive(Debug)]
pub struct NetworkManager {
    limiter: std::sync::Mutex<RateLimiter>,
    overrides: std::sync::Mutex<HashMap<String, HashMap<String, NetworkLimitsOverride>>>,
}

#[derive(Debug)]
struct RateLimiter {
    buckets: HashMap<(String, String), RateLimitBucket>,
}

#[derive(Debug, Clone)]
struct RateLimitBucket {
    minute_start: Instant,
    day_start: Instant,
    minute_count: u32,
    day_count: u32,
    day_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct NetworkError {
    pub code: &'static str,
    pub message: String,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            limiter: std::sync::Mutex::new(RateLimiter {
                buckets: HashMap::new(),
            }),
            overrides: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn set_app_limits(
        &self,
        app_id: &str,
        limits: HashMap<String, NetworkLimitsOverride>,
    ) {
        if let Ok(mut map) = self.overrides.lock() {
            map.insert(app_id.to_string(), limits);
        }
    }

    pub fn check_request(
        &self,
        app_id: &str,
        domain: &str,
        request_bytes: u64,
    ) -> NetworkResult<()> {
        let limits = self.effective_limits(app_id, domain);
        let mut limiter = self
            .limiter
            .lock()
            .map_err(|_| NetworkError::internal("rate limiter"))?;
        limiter.check(app_id, domain, request_bytes, limits)
    }

    pub fn record_response(
        &self,
        app_id: &str,
        domain: &str,
        response_bytes: u64,
    ) -> NetworkResult<()> {
        let limits = self.effective_limits(app_id, domain);
        let mut limiter = self
            .limiter
            .lock()
            .map_err(|_| NetworkError::internal("rate limiter"))?;
        limiter.record_response(app_id, domain, response_bytes, limits)
    }

    fn effective_limits(&self, app_id: &str, domain: &str) -> NetworkLimits {
        let default = NetworkLimits {
            requests_per_minute: DEFAULT_REQUESTS_PER_MINUTE,
            requests_per_day: DEFAULT_REQUESTS_PER_DAY,
            bytes_per_day: DEFAULT_BYTES_PER_DAY,
        };
        let overrides = self
            .overrides
            .lock()
            .ok()
            .and_then(|map| map.get(app_id).cloned())
            .unwrap_or_default();
        let override_entry = overrides.get(domain);
        let mut limits = default.clone();
        if let Some(override_entry) = override_entry {
            if let Some(value) = override_entry.requests_per_minute {
                limits.requests_per_minute = limits.requests_per_minute.min(value);
            }
            if let Some(value) = override_entry.requests_per_day {
                limits.requests_per_day = limits.requests_per_day.min(value);
            }
        }
        limits
    }
}

impl RateLimiter {
    fn check(
        &mut self,
        app_id: &str,
        domain: &str,
        request_bytes: u64,
        limits: NetworkLimits,
    ) -> NetworkResult<()> {
        let bucket = self
            .buckets
            .entry((app_id.to_string(), domain.to_string()))
            .or_insert_with(|| RateLimitBucket::new());
        bucket.rollover();
        if bucket.minute_count >= limits.requests_per_minute
            || bucket.day_count >= limits.requests_per_day
            || bucket.day_bytes.saturating_add(request_bytes) > limits.bytes_per_day
        {
            return Err(NetworkError::new("RATE_LIMITED", "Rate limit exceeded"));
        }
        bucket.minute_count += 1;
        bucket.day_count += 1;
        bucket.day_bytes = bucket.day_bytes.saturating_add(request_bytes);
        Ok(())
    }

    fn record_response(
        &mut self,
        app_id: &str,
        domain: &str,
        response_bytes: u64,
        limits: NetworkLimits,
    ) -> NetworkResult<()> {
        let bucket = self
            .buckets
            .entry((app_id.to_string(), domain.to_string()))
            .or_insert_with(|| RateLimitBucket::new());
        bucket.rollover();
        if bucket.day_bytes.saturating_add(response_bytes) > limits.bytes_per_day {
            return Err(NetworkError::new("RATE_LIMITED", "Rate limit exceeded"));
        }
        bucket.day_bytes = bucket.day_bytes.saturating_add(response_bytes);
        Ok(())
    }
}

impl RateLimitBucket {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            minute_start: now,
            day_start: now,
            minute_count: 0,
            day_count: 0,
            day_bytes: 0,
        }
    }

    fn rollover(&mut self) {
        if self.minute_start.elapsed() >= Duration::from_secs(60) {
            self.minute_start = Instant::now();
            self.minute_count = 0;
        }
        if self.day_start.elapsed() >= Duration::from_secs(60 * 60 * 24) {
            self.day_start = Instant::now();
            self.day_count = 0;
            self.day_bytes = 0;
        }
    }
}

impl NetworkError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("NETWORK_ERROR", message)
    }
}

pub fn execute_request(request: &NetworkRequest) -> NetworkResult<NetworkResponse> {
    let mut client_builder = Client::builder()
        .redirect(Policy::none())
        .timeout(request.timeout)
        .min_tls_version(TlsVersion::TLS_1_2);
    let host = request
        .url
        .host_str()
        .ok_or_else(|| NetworkError::new("INVALID_URL", "Missing host"))?;
    let port = request.url.port_or_known_default().unwrap_or(443);
    let addr = resolve_allowed_ip(host, port)?;
    client_builder = client_builder.resolve(host, addr);
    let client = client_builder
        .build()
        .map_err(|_| NetworkError::internal("HTTP client"))?;
    let method = request
        .method
        .parse::<reqwest::Method>()
        .map_err(|_| NetworkError::new("INVALID_REQUEST", "Invalid method"))?;
    let mut req = client.request(method, request.url.clone());
    let headers = build_headers(&request.headers)?;
    req = req.headers(headers);
    if let Some(body) = request.body.as_ref() {
        req = req.body(body.clone());
    }
    let response = req
        .send()
        .map_err(|err| map_reqwest_error(err))?;
    read_response(response, request.max_response_bytes)
}

pub fn is_blocked_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    if host == "localhost" {
        return true;
    }
    if host == "metadata.google.internal"
        || host == "metadata.azure.com"
        || host == "instance-data.ec2.internal"
    {
        return true;
    }
    false
}

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => is_blocked_ipv4(addr),
        IpAddr::V6(addr) => is_blocked_ipv6(addr),
    }
}

pub fn matches_domain_pattern(pattern: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();
    if let Some(rest) = pattern.strip_prefix("*.") {
        if host == rest {
            return false;
        }
        return host.ends_with(&format!(".{rest}"));
    }
    host == pattern
}

pub fn capability_allows(grants: &[String], scheme: &str, host: &str) -> bool {
    grants.iter().any(|cap| matches_capability(cap, scheme, host))
}

fn matches_capability(cap: &str, scheme: &str, host: &str) -> bool {
    if !cap.starts_with("network:") {
        return false;
    }
    let mut parts = cap.splitn(3, ':');
    let _ = parts.next();
    let protocol = parts.next().unwrap_or_default();
    let pattern = parts.next().unwrap_or_default();
    let scheme_allowed = match protocol {
        "https" => scheme == "https",
        "http" => scheme == "http",
        "http+https" => scheme == "http" || scheme == "https",
        _ => false,
    };
    if !scheme_allowed {
        return false;
    }
    matches_domain_pattern(pattern, host)
}

fn resolve_allowed_ip(host: &str, port: u16) -> NetworkResult<SocketAddr> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|_| NetworkError::new("NETWORK_ERROR", "DNS resolution failed"))?;
    for addr in addrs {
        if is_blocked_ip(addr.ip()) {
            continue;
        }
        return Ok(addr);
    }
    Err(NetworkError::new(
        "BLOCKED_DOMAIN",
        "Blocked destination",
    ))
}

fn build_headers(headers: &HashMap<String, String>) -> NetworkResult<HeaderMap> {
    let mut map = HeaderMap::new();
    for (key, value) in headers {
        if is_disallowed_header(key) {
            return Err(NetworkError::new(
                "INVALID_REQUEST",
                "Disallowed header",
            ));
        }
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| NetworkError::new("INVALID_REQUEST", "Invalid header"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| NetworkError::new("INVALID_REQUEST", "Invalid header"))?;
        map.insert(name, value);
    }
    Ok(map)
}

fn read_response(response: Response, max_bytes: usize) -> NetworkResult<NetworkResponse> {
    let status = response.status();
    let status_text = status
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let url = response.url().to_string();
    let mut headers = HashMap::new();
    for (name, value) in response.headers().iter() {
        if let Ok(value) = value.to_str() {
            headers.insert(name.as_str().to_string(), value.to_string());
        }
    }
    let mut body = Vec::new();
    let mut reader = response;
    let mut buf = [0u8; 8192];
    let mut total = 0usize;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|_| NetworkError::new("NETWORK_ERROR", "Response read failed"))?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n);
        if total > max_bytes {
            return Err(NetworkError::new(
                "RESPONSE_TOO_LARGE",
                "Response too large",
            ));
        }
        body.extend_from_slice(&buf[..n]);
    }
    Ok(NetworkResponse {
        status: status.as_u16(),
        status_text,
        headers,
        body,
        url,
    })
}

fn map_reqwest_error(err: reqwest::Error) -> NetworkError {
    if err.is_timeout() {
        return NetworkError::new("TIMEOUT", "Request timeout");
    }
    if err.is_builder() || err.is_request() {
        return NetworkError::new("NETWORK_ERROR", "Network error");
    }
    if err.is_connect() {
        return NetworkError::new("NETWORK_ERROR", "Connection failed");
    }
    if err.is_status() {
        return NetworkError::new("NETWORK_ERROR", "HTTP status error");
    }
    if err.to_string().to_lowercase().contains("tls") {
        return NetworkError::new("TLS_ERROR", "TLS error");
    }
    NetworkError::new("NETWORK_ERROR", "Network error")
}

fn is_blocked_ipv4(addr: Ipv4Addr) -> bool {
    let octets = addr.octets();
    if octets[0] == 127 {
        return true;
    }
    if octets[0] == 10 {
        return true;
    }
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return true;
    }
    if octets[0] == 169 && octets[1] == 254 {
        return true;
    }
    if octets[0] == 100 && octets[1] == 100 && octets[2] == 100 && octets[3] == 200 {
        return true;
    }
    addr == Ipv4Addr::new(169, 254, 169, 254)
}

fn is_blocked_ipv6(addr: Ipv6Addr) -> bool {
    if addr.is_loopback() {
        return true;
    }
    let segments = addr.segments();
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    false
}

pub fn validate_network_scheme(url: &Url) -> NetworkResult<()> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(NetworkError::new("INVALID_URL", "Invalid scheme"));
    }
    Ok(())
}

pub fn validate_host_not_blocked(host: &str) -> NetworkResult<()> {
    if is_blocked_host(host) {
        return Err(NetworkError::new("BLOCKED_DOMAIN", "Blocked destination"));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(NetworkError::new("BLOCKED_DOMAIN", "Blocked destination"));
        }
    }
    Ok(())
}

fn is_disallowed_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "proxy-connection"
            | "keep-alive"
            | "upgrade"
            | "te"
            | "trailer"
    )
}

pub fn normalize_request_timeout(timeout_ms: Option<u64>) -> Duration {
    let timeout = timeout_ms
        .map(|value| Duration::from_millis(value))
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT_SECS));
    timeout.min(Duration::from_secs(MAX_TIMEOUT_SECS))
}

pub fn normalize_max_response_bytes(limit: Option<u64>) -> usize {
    let limit = limit.unwrap_or(DEFAULT_RESPONSE_BYTES as u64);
    (limit.min(MAX_RESPONSE_BYTES as u64)) as usize
}

pub fn normalize_method(method: Option<String>) -> String {
    method.unwrap_or_else(|| "GET".to_string())
}

pub fn normalize_request_body(body: Option<Vec<u8>>) -> NetworkResult<Option<Vec<u8>>> {
    if let Some(body) = body {
        if body.len() > MAX_REQUEST_BYTES {
            return Err(NetworkError::new(
                "INVALID_REQUEST",
                "Request body too large",
            ));
        }
        return Ok(Some(body));
    }
    Ok(None)
}

pub fn validate_request_body_size(body: Option<&[u8]>, max_bytes: usize) -> NetworkResult<()> {
    if let Some(body) = body {
        if body.len() > max_bytes {
            return Err(NetworkError::new(
                "INVALID_REQUEST",
                "Request body too large",
            ));
        }
    }
    Ok(())
}

pub fn cbor_json_body_to_bytes(value: serde_cbor::Value) -> NetworkResult<Vec<u8>> {
    let json_value: serde_json::Value = serde_cbor::value::from_value(value)
        .map_err(|_| NetworkError::new("INVALID_REQUEST", "Invalid JSON body"))?;
    serde_json::to_vec(&json_value)
        .map_err(|_| NetworkError::new("INVALID_REQUEST", "JSON encode failed"))
}

pub fn to_cbor_value(value: serde_json::Value) -> NetworkResult<serde_cbor::Value> {
    serde_cbor::value::to_value(value)
        .map_err(|_| NetworkError::new("JSON_PARSE_ERROR", "Invalid JSON response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_pattern_matching() {
        assert!(matches_domain_pattern("api.example.com", "api.example.com"));
        assert!(!matches_domain_pattern("api.example.com", "www.example.com"));
        assert!(matches_domain_pattern("*.example.com", "api.example.com"));
        assert!(!matches_domain_pattern("*.example.com", "example.com"));
    }

    #[test]
    fn capability_matching() {
        let caps = vec![
            "network:https:api.example.com".to_string(),
            "network:http+https:*.example.org".to_string(),
        ];
        assert!(capability_allows(&caps, "https", "api.example.com"));
        assert!(!capability_allows(&caps, "http", "api.example.com"));
        assert!(capability_allows(&caps, "http", "foo.example.org"));
        assert!(capability_allows(&caps, "https", "foo.example.org"));
    }

    #[test]
    fn blocked_ipv4_ranges() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 10, 10))));
        assert!(!is_blocked_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }
}

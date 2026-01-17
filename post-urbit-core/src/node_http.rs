use std::borrow::Cow;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use futures::{SinkExt, StreamExt};
use hyper::{Body, Method, Request, Response, StatusCode};
use hyper::header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, SET_COOKIE};
use hyper::service::{make_service_fn, service_fn};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Value, json};

use crate::admin_auth::{
    AuthConfig, create_session_cookie, generate_token_hex, hash_token, verify_password,
    verify_session_cookie,
};
use crate::admin_state::{AdminState, ApiKeyRecord, CachedRepository, SessionRecord};
use crate::admin_types::{
    api_error, AddContactRequest, ApiErrorCode, ApiKey, BackupListEntry, BackupResult, Contact,
    ContactUpdate, CreateApiKeyRequest, CreateApiKeyResponse, Device, DeviceAddResult, IdentityInfo,
    InstalledApp, InstallRequest, InstallResult, LogEntry, LoginRequest, LoginResponse,
    LogsResponse, NodeStatus, PaginatedResult, Permission, PermissionPatch, PublicProfile,
    RestoreResult, Session, UpdateResult,
};
use crate::error::{PostUrbitError, Result};
use crate::app_store::{fetch_repository, install_package, parse_postapp, verify_package_with_dht, verify_repository, RepositoryManifest};
use crate::dht::Dht;
use crate::event_bus::EventBus;
use crate::identity::{IdentityManager, Recovery};
use crate::node_backup::{create_backup, restore_backup};
use crate::node_config::default_node_settings;
use sha2::Digest;

#[derive(Debug, Clone)]
pub struct HttpServerConfig {
    pub metrics_enabled: bool,
    pub max_request_body_bytes: usize,
    pub session_cookie_secure: bool,
}

#[derive(Clone)]
pub struct HealthState {
    ready: Arc<AtomicBool>,
    shutting_down: Arc<AtomicBool>,
    details: Arc<tokio::sync::RwLock<ReadinessDetails>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ReadinessDetails {
    pub identity: String,
    pub transport: String,
    pub messaging: String,
    pub apps: String,
}

impl Default for ReadinessDetails {
    fn default() -> Self {
        Self {
            identity: "loaded".to_string(),
            transport: "starting".to_string(),
            messaging: "waiting".to_string(),
            apps: "waiting".to_string(),
        }
    }
}

impl HealthState {
    pub fn new() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(true)),
            shutting_down: Arc::new(AtomicBool::new(false)),
            details: Arc::new(tokio::sync::RwLock::new(ReadinessDetails::default())),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
    }

    pub fn set_shutting_down(&self, shutting_down: bool) {
        self.shutting_down.store(shutting_down, Ordering::SeqCst);
    }

    pub async fn readiness_details(&self) -> ReadinessDetails {
        self.details.read().await.clone()
    }

    pub async fn set_readiness_details(&self, details: ReadinessDetails) {
        *self.details.write().await = details;
    }
}

#[derive(Clone)]
pub struct HttpServerState {
    pub admin: AdminState,
    pub auth: AuthConfig,
    pub identity: Arc<IdentityManager>,
    pub dht: Arc<dyn Dht + Send + Sync>,
    pub event_bus: Arc<EventBus>,
    pub started_at: Instant,
    pub config: HttpServerConfig,
    pub health: HealthState,
    pub apps_dir: PathBuf,
}

pub async fn run_http_server(addr: SocketAddr, state: HttpServerState) -> Result<()> {
    let state = Arc::new(state);
    let make = make_service_fn(move |_conn| {
        let state = state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let state = state.clone();
                async move { Ok::<_, Infallible>(handle_request(req, state).await) }
            }))
        }
    });

    hyper::Server::bind(&addr)
        .serve(make)
        .await
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

async fn handle_request(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let path = req.uri().path().to_string();

    if path.starts_with("/apps/") && path.contains("/api/") {
        return handle_app_api(req, state).await;
    }

    if let Some(resp) = handle_public(&req, &path, &state).await {
        return resp;
    }

    let admin_path = normalize_admin_path(&path);
    if admin_path.starts_with("/admin/v1/") {
        return handle_admin(req, admin_path.as_ref(), state).await;
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

fn normalize_admin_path(path: &str) -> Cow<'_, str> {
    if let Some(stripped) = path.strip_prefix("/api/v1/") {
        return Cow::Owned(format!("/admin/v1/{}", stripped));
    }
    if path == "/api/v1" {
        return Cow::Owned("/admin/v1".to_string());
    }
    Cow::Borrowed(path)
}

async fn handle_public(req: &Request<Body>, path: &str, state: &HttpServerState) -> Option<Response<Body>> {
    match (req.method(), path) {
        (&Method::GET, "/health/live") => Some(handle_health_live(state).await),
        (&Method::GET, "/health/ready") => Some(handle_health_ready(state).await),
        (&Method::GET, "/health") => Some(handle_health(state).await),
        (&Method::GET, "/metrics") => {
            if state.config.metrics_enabled {
                let payload = render_metrics(state).await;
                Some(Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/plain; version=0.0.4")
                    .body(Body::from(payload))
                    .unwrap())
            } else {
                Some(Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap())
            }
        }
        (&Method::GET, path) if path.starts_with("/apps/") => Some(serve_app(path, state)),
        (&Method::POST, path) if path.starts_with("/apps/") => Some(Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap()),
        (&Method::PUT, path) if path.starts_with("/apps/") => Some(Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap()),
        (&Method::PATCH, path) if path.starts_with("/apps/") => Some(Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap()),
        (&Method::DELETE, path) if path.starts_with("/apps/") => Some(Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap()),
        _ => None,
    }
}

async fn handle_admin(req: Request<Body>, path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let method = req.method().clone();

    match (method, path) {
        (Method::POST, "/admin/v1/auth/login") => handle_login(req, state).await,
        (Method::POST, "/admin/v1/auth/logout") => handle_logout(req, state).await,
        (Method::POST, "/admin/v1/auth/refresh") => handle_refresh(req, state).await,
        (Method::POST, "/admin/v1/auth/reauth") => handle_reauth(req, state).await,
        (Method::GET, "/admin/v1/events") => handle_events(req, state).await,
        _ => handle_admin_authed(req, path, state).await,
    }
}

async fn handle_admin_authed(req: Request<Body>, path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let auth = match authenticate(&req, &state).await {
        Ok(auth) => auth,
        Err(resp) => return resp,
    };

    if is_state_change(req.method()) {
        if auth.requires_csrf && !csrf_ok(&req) {
            return api_error_response(ApiErrorCode::CsrfInvalid, "CSRF token missing or invalid", StatusCode::FORBIDDEN);
        }
    }

    match (req.method(), path) {
        (&Method::GET, "/admin/v1/identity") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadIdentity) { return resp; }
            handle_identity(state).await
        }
        (&Method::PUT, "/admin/v1/identity/profile") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteIdentity) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_identity_profile(req, state).await
        }
        (&Method::POST, "/admin/v1/identity/rotate/signing") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteIdentity) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_rotate_signing(state).await
        }
        (&Method::POST, "/admin/v1/identity/rotate/encryption") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteIdentity) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_rotate_encryption(state).await
        }
        (&Method::GET, "/admin/v1/identity/recovery") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadIdentity) { return resp; }
            handle_recovery(state).await
        }
        (&Method::PUT, "/admin/v1/identity/recovery") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteIdentity) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_update_recovery(req, state).await
        }
        (&Method::GET, "/admin/v1/identity/export") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadIdentity) { return resp; }
            handle_identity_export(state).await
        }
        (&Method::GET, "/admin/v1/devices") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadIdentity) { return resp; }
            handle_devices(state).await
        }
        (&Method::POST, "/admin/v1/devices") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteIdentity) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_add_device(req, state).await
        }
        (&Method::DELETE, path) if path.starts_with("/admin/v1/devices/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteIdentity) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_remove_device(path, state).await
        }
        (&Method::GET, "/admin/v1/contacts") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadContacts) { return resp; }
            handle_list_contacts(req, state).await
        }
        (&Method::POST, "/admin/v1/contacts") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteContacts) { return resp; }
            handle_add_contact(req, state).await
        }
        (&Method::GET, path) if path.starts_with("/admin/v1/contacts/") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadContacts) { return resp; }
            handle_get_contact(path, state).await
        }
        (&Method::PUT, path) if path.starts_with("/admin/v1/contacts/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteContacts) { return resp; }
            handle_update_contact(req, path, state).await
        }
        (&Method::DELETE, path) if path.starts_with("/admin/v1/contacts/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteContacts) { return resp; }
            handle_delete_contact(path, state).await
        }
        (&Method::POST, path) if path.ends_with("/block") && path.starts_with("/admin/v1/contacts/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteContacts) { return resp; }
            handle_block_contact(path, state, true).await
        }
        (&Method::DELETE, path) if path.ends_with("/block") && path.starts_with("/admin/v1/contacts/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteContacts) { return resp; }
            handle_block_contact(path, state, false).await
        }
        (&Method::GET, "/admin/v1/apps") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadApps) { return resp; }
            handle_list_apps(state).await
        }
        (&Method::POST, "/admin/v1/apps/install") => {
            if let Err(resp) = require_permission(&auth, Permission::ManageApps) { return resp; }
            handle_install_app(req, state).await
        }
        (&Method::POST, "/admin/v1/apps/install/upload") => {
            if let Err(resp) = require_permission(&auth, Permission::ManageApps) { return resp; }
            handle_install_upload(req, state).await
        }
        (&Method::POST, path) if path.ends_with("/update") && path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ManageApps) { return resp; }
            handle_update_app(path, state).await
        }
        (&Method::DELETE, path) if path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ManageApps) { return resp; }
            handle_delete_app(req, path, state).await
        }
        (&Method::GET, path) if path.ends_with("/permissions") && path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadApps) { return resp; }
            handle_get_app_permissions(path, state).await
        }
        (&Method::GET, path) if path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadApps) { return resp; }
            handle_get_app(path, state).await
        }
        (&Method::PATCH, path) if path.ends_with("/permissions") && path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ManageApps) { return resp; }
            handle_patch_app_permissions(req, path, state).await
        }
        (&Method::GET, path) if path.ends_with("/settings") && path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadApps) { return resp; }
            handle_get_app_settings(path, state).await
        }
        (&Method::PUT, path) if path.ends_with("/settings") && path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ManageApps) { return resp; }
            handle_put_app_settings(req, path, state).await
        }
        (&Method::POST, path) if path.ends_with("/clear-data") && path.starts_with("/admin/v1/apps/") => {
            if let Err(resp) = require_permission(&auth, Permission::ManageApps) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_clear_app_data(path, state).await
        }
        (&Method::GET, "/admin/v1/settings") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadSettings) { return resp; }
            handle_settings(state).await
        }
        (&Method::GET, path) if path.starts_with("/admin/v1/settings/") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadSettings) { return resp; }
            handle_settings_section(path, state).await
        }
        (&Method::PATCH, "/admin/v1/settings") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_patch_settings(req, state).await
        }
        (&Method::POST, "/admin/v1/settings/reset") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_reset_settings(req, state).await
        }
        (&Method::GET, "/admin/v1/backups") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadSettings) { return resp; }
            handle_list_backups(state).await
        }
        (&Method::POST, "/admin/v1/backups") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            handle_create_backup(req, state).await
        }
        (&Method::POST, "/admin/v1/backups/upload") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            handle_upload_backup(req, state).await
        }
        (&Method::GET, path) if path.starts_with("/admin/v1/backups/") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadSettings) { return resp; }
            handle_download_backup(path, state).await
        }
        (&Method::POST, path) if path.ends_with("/restore") && path.starts_with("/admin/v1/backups/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_restore_backup(req, path, state).await
        }
        (&Method::DELETE, path) if path.starts_with("/admin/v1/backups/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            handle_delete_backup(path, state).await
        }
        (&Method::GET, "/admin/v1/api-keys") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadSettings) { return resp; }
            handle_list_api_keys(state).await
        }
        (&Method::POST, "/admin/v1/api-keys") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            if let Err(resp) = require_fresh_auth(&auth) { return resp; }
            handle_create_api_key(req, state).await
        }
        (&Method::DELETE, path) if path.starts_with("/admin/v1/api-keys/") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            handle_delete_api_key(path, state).await
        }
        (&Method::GET, "/admin/v1/status") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadSettings) { return resp; }
            handle_status(state).await
        }
        (&Method::GET, "/admin/v1/logs") => {
            if let Err(resp) = require_permission(&auth, Permission::ReadSettings) { return resp; }
            handle_logs(req, state).await
        }
        (&Method::POST, "/admin/v1/restart") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            handle_restart(&state).await
        }
        (&Method::POST, "/admin/v1/shutdown") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            handle_shutdown(&state).await
        }
        _ => Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap(),
    }
}

struct AuthContext {
    requires_csrf: bool,
    permissions: Vec<Permission>,
    session_id: Option<String>,
    fresh_auth_at: Option<DateTime<Utc>>,
}

async fn authenticate(req: &Request<Body>, state: &HttpServerState) -> std::result::Result<AuthContext, Response<Body>> {
    if let Some(auth_header) = req.headers().get(hyper::header::AUTHORIZATION) {
        if let Ok(value) = auth_header.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                let token_hash = hash_token(token);
                if let Some(admin_hash) = state.auth.admin_token_hash.as_ref() {
                    if constant_time_eq(token_hash.as_bytes(), admin_hash.as_bytes()) {
                        return Ok(AuthContext {
                            requires_csrf: false,
                            permissions: vec![Permission::AdminFull],
                            session_id: None,
                            fresh_auth_at: Some(Utc::now()),
                        });
                    }
                }
                let mut data = state.admin.data.lock().await;
                if let Some(record) = data.api_keys.iter_mut().find(|record| record.key_hash == token_hash) {
                    record.key.last_used = Some(Utc::now().to_rfc3339());
                    let permissions = record.key.permissions.clone();
                    drop(data);
                    let _ = state.admin.persist().await;
                    return Ok(AuthContext {
                        requires_csrf: false,
                        permissions,
                        session_id: None,
                        fresh_auth_at: Some(Utc::now()),
                    });
                }
            }
        }
    }

    if let Some(cookie) = req.headers().get(COOKIE) {
        if let Ok(value) = cookie.to_str() {
            if let Some(session_value) = extract_cookie(value, "postnode_session") {
                if let Ok(session_id) = verify_session_cookie(session_value, &state.auth.session_secret) {
                    let mut data = state.admin.data.lock().await;
                    if let Some(session) = data.sessions.get_mut(&session_id) {
                        if !session_expired(session) {
                            session.last_activity = Utc::now().to_rfc3339();
                            let fresh = session
                                .fresh_auth_at
                                .as_ref()
                                .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                                .map(|ts| ts.with_timezone(&Utc));
                            return Ok(AuthContext {
                                requires_csrf: true,
                                permissions: vec![Permission::AdminFull],
                                session_id: Some(session_id),
                                fresh_auth_at: fresh,
                            });
                        }
                    }
                }
            }
        }
    }

    Err(api_error_response(ApiErrorCode::Unauthorized, "unauthorized", StatusCode::UNAUTHORIZED))
}

fn session_expired(session: &SessionRecord) -> bool {
    DateTime::parse_from_rfc3339(&session.expires_at)
        .map(|ts| ts.with_timezone(&Utc) < Utc::now())
        .unwrap_or(true)
}

fn is_state_change(method: &Method) -> bool {
    matches!(method, &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE)
}

fn csrf_ok(req: &Request<Body>) -> bool {
    let csrf_cookie = req
        .headers()
        .get(COOKIE)
        .and_then(|header| header.to_str().ok())
        .and_then(|value| extract_cookie(value, "postnode_csrf"));
    let csrf_header = req
        .headers()
        .get("x-csrf-token")
        .and_then(|header| header.to_str().ok());
    match (csrf_cookie, csrf_header) {
        (Some(cookie), Some(header)) => constant_time_eq(cookie.as_bytes(), header.as_bytes()),
        _ => false,
    }
}

fn require_permission(auth: &AuthContext, permission: Permission) -> std::result::Result<(), Response<Body>> {
    if auth.permissions.contains(&Permission::AdminFull) || auth.permissions.contains(&permission) {
        Ok(())
    } else {
        Err(api_error_response(ApiErrorCode::Forbidden, "forbidden", StatusCode::FORBIDDEN))
    }
}

fn require_fresh_auth(auth: &AuthContext) -> std::result::Result<(), Response<Body>> {
    if let Some(fresh) = auth.fresh_auth_at {
        if Utc::now().signed_duration_since(fresh) <= Duration::minutes(5) {
            return Ok(());
        }
    }
    Err(api_error_response(
        ApiErrorCode::FreshAuthRequired,
        "fresh authentication required",
        StatusCode::FORBIDDEN,
    ))
}

async fn handle_login(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let user_agent = req
        .headers()
        .get(hyper::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let ip_address = req
        .extensions()
        .get::<SocketAddr>()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let body = match read_json::<LoginRequest>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };

    let Some(hash) = state.auth.password_hash.as_ref() else {
        return api_error_response(ApiErrorCode::Unauthorized, "password authentication disabled", StatusCode::UNAUTHORIZED);
    };
    if verify_password(hash, &body.password).is_err() {
        log_entry(
            &state,
            "warn",
            "postnode::admin",
            "admin login failed",
            Some(json!({"ip_address": ip_address, "user_agent": user_agent})),
        )
        .await;
        return api_error_response(ApiErrorCode::Unauthorized, "invalid credentials", StatusCode::UNAUTHORIZED);
    }

    let session_id = generate_token_hex(16);
    let csrf_token = generate_token_hex(32);
    let now = Utc::now();
    let expires = now + Duration::hours(state.auth.session_timeout_hours as i64);

    let session_record = SessionRecord {
        id: session_id.clone(),
        created_at: now.to_rfc3339(),
        expires_at: expires.to_rfc3339(),
        last_activity: now.to_rfc3339(),
        user_agent: user_agent.clone(),
        ip_address: ip_address.clone(),
        device_id: if body.remember_device.unwrap_or(false) {
            Some(generate_token_hex(8))
        } else {
            None
        },
        csrf_token: csrf_token.clone(),
        fresh_auth_at: Some(now.to_rfc3339()),
    };

    {
        let mut data = state.admin.data.lock().await;
        data.sessions.insert(session_id.clone(), session_record.clone());
    }
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::admin",
        "admin login",
        Some(json!({"device_id": session_record.device_id})),
    )
    .await;

    let session_cookie = match create_session_cookie(&session_id, &state.auth.session_secret) {
        Ok(cookie) => cookie,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "session error", StatusCode::INTERNAL_SERVER_ERROR),
    };

    let session = Session {
        id: session_id.clone(),
        created_at: session_record.created_at.clone(),
        expires_at: session_record.expires_at.clone(),
        last_activity: session_record.last_activity.clone(),
        user_agent,
        ip_address,
        device_id: session_record.device_id.clone(),
        requires_fresh_auth: false,
    };

    let response = LoginResponse {
        session,
        csrf_token: csrf_token.clone(),
    };

    let mut resp = json_response(response);
    set_cookie_header(&mut resp, &format!("postnode_session={}; Path=/admin; HttpOnly; SameSite=Strict{}", session_cookie, secure_flag(state.config.session_cookie_secure)));
    set_cookie_header(&mut resp, &format!("postnode_csrf={}; Path=/admin; SameSite=Strict{}", csrf_token, secure_flag(state.config.session_cookie_secure)));
    resp
}

async fn handle_logout(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    if let Some(cookie) = req.headers().get(COOKIE) {
        if let Ok(value) = cookie.to_str() {
            if let Some(session_value) = extract_cookie(value, "postnode_session") {
                if let Ok(session_id) = verify_session_cookie(session_value, &state.auth.session_secret) {
                    state.admin.remove_session(&session_id).await;
                    log_entry(
                        &state,
                        "info",
                        "postnode::admin",
                        "admin logout",
                        Some(json!({"session_id": session_id})),
                    )
                    .await;
                }
            }
        }
    }
    let _ = state.admin.persist().await;
    let mut resp = Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap();
    set_cookie_header(&mut resp, &format!("postnode_session=deleted; Path=/admin; Max-Age=0; HttpOnly; SameSite=Strict{}", secure_flag(state.config.session_cookie_secure)));
    set_cookie_header(&mut resp, &format!("postnode_csrf=deleted; Path=/admin; Max-Age=0; SameSite=Strict{}", secure_flag(state.config.session_cookie_secure)));
    resp
}

async fn handle_refresh(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let auth = match authenticate(&req, &state).await {
        Ok(auth) => auth,
        Err(resp) => return resp,
    };
    let Some(session_id) = auth.session_id else {
        return api_error_response(ApiErrorCode::Forbidden, "session required", StatusCode::FORBIDDEN);
    };

    let mut data = state.admin.data.lock().await;
    if let Some(session) = data.sessions.get_mut(&session_id) {
        let now = Utc::now();
        let expires = now + Duration::hours(state.auth.session_timeout_hours as i64);
        session.expires_at = expires.to_rfc3339();
        session.last_activity = now.to_rfc3339();
        let response = Session {
            id: session.id.clone(),
            created_at: session.created_at.clone(),
            expires_at: session.expires_at.clone(),
            last_activity: session.last_activity.clone(),
            user_agent: session.user_agent.clone(),
            ip_address: session.ip_address.clone(),
            device_id: session.device_id.clone(),
            requires_fresh_auth: false,
        };
        drop(data);
        let _ = state.admin.persist().await;
        return json_response(response);
    }

    api_error_response(ApiErrorCode::Unauthorized, "session expired", StatusCode::UNAUTHORIZED)
}

async fn handle_reauth(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let auth = match authenticate(&req, &state).await {
        Ok(auth) => auth,
        Err(resp) => return resp,
    };
    let Some(session_id) = auth.session_id.clone() else {
        return api_error_response(ApiErrorCode::Forbidden, "session required", StatusCode::FORBIDDEN);
    };
    let body = match read_json::<Value>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("");

    let Some(hash) = state.auth.password_hash.as_ref() else {
        return api_error_response(ApiErrorCode::Unauthorized, "password authentication disabled", StatusCode::UNAUTHORIZED);
    };
    if verify_password(hash, password).is_err() {
        return api_error_response(ApiErrorCode::Unauthorized, "invalid credentials", StatusCode::UNAUTHORIZED);
    }

    state.admin.set_fresh_auth(&session_id).await;
    let data = state.admin.data.lock().await;
    if let Some(session) = data.sessions.get(&session_id) {
        let response = Session {
            id: session.id.clone(),
            created_at: session.created_at.clone(),
            expires_at: session.expires_at.clone(),
            last_activity: session.last_activity.clone(),
            user_agent: session.user_agent.clone(),
            ip_address: session.ip_address.clone(),
            device_id: session.device_id.clone(),
            requires_fresh_auth: false,
        };
        return json_response(response);
    }

    api_error_response(ApiErrorCode::Unauthorized, "session expired", StatusCode::UNAUTHORIZED)
}

async fn handle_identity(state: Arc<HttpServerState>) -> Response<Body> {
    let idoc = state.identity.identity_document().await;
    let created_at = idoc.timestamp.clone();
    let last_rotation = state.admin.data.lock().await.last_key_rotation.clone();
    let profile = if idoc.claims.name.is_some() || idoc.claims.avatar.is_some() || idoc.claims.bio.is_some() {
        Some(PublicProfile {
            display_name: idoc.claims.name.clone(),
            avatar: idoc.claims.avatar.clone(),
            bio: idoc.claims.bio.clone(),
        })
    } else {
        None
    };

    let info = IdentityInfo {
        iid: idoc.iid.clone(),
        genesis_key_fingerprint: key_fingerprint(&idoc.keys.signing.genesis),
        current_signing_key_fingerprint: key_fingerprint(&idoc.keys.signing.current),
        current_encryption_key_fingerprint: key_fingerprint(&idoc.keys.encryption.current),
        created_at,
        last_key_rotation: last_rotation,
        recovery_method: idoc.recovery.method.clone(),
        endpoints: idoc.endpoints.clone(),
        profile,
    };
    json_response(info)
}

async fn handle_identity_profile(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let profile = match read_json::<PublicProfile>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let updated = match state.identity.update_claims(profile.display_name, profile.avatar, profile.bio).await {
        Ok(doc) => doc,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "identity update failed", StatusCode::INTERNAL_SERVER_ERROR),
    };

    let info = IdentityInfo {
        iid: updated.iid.clone(),
        genesis_key_fingerprint: key_fingerprint(&updated.keys.signing.genesis),
        current_signing_key_fingerprint: key_fingerprint(&updated.keys.signing.current),
        current_encryption_key_fingerprint: key_fingerprint(&updated.keys.encryption.current),
        created_at: updated.timestamp.clone(),
        last_key_rotation: state.admin.data.lock().await.last_key_rotation.clone(),
        recovery_method: updated.recovery.method.clone(),
        endpoints: updated.endpoints.clone(),
        profile: Some(PublicProfile {
            display_name: updated.claims.name,
            avatar: updated.claims.avatar,
            bio: updated.claims.bio,
        }),
    };
    let _ = state.admin.persist().await;
    json_response(info)
}

async fn handle_rotate_signing(state: Arc<HttpServerState>) -> Response<Body> {
    let result = match state.identity.rotate_signing_key().await {
        Ok(result) => result,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "rotation failed", StatusCode::INTERNAL_SERVER_ERROR),
    };
    state.admin.data.lock().await.last_key_rotation = Some(result.rotated_at.clone());
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::identity",
        "signing key rotated",
        Some(json!({"rotated_at": result.rotated_at})),
    )
    .await;
    json_response(result)
}

async fn handle_rotate_encryption(state: Arc<HttpServerState>) -> Response<Body> {
    let result = match state.identity.rotate_encryption_key().await {
        Ok(result) => result,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "rotation failed", StatusCode::INTERNAL_SERVER_ERROR),
    };
    state.admin.data.lock().await.last_key_rotation = Some(result.rotated_at.clone());
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::identity",
        "encryption key rotated",
        Some(json!({"rotated_at": result.rotated_at})),
    )
    .await;
    json_response(result)
}

async fn handle_recovery(state: Arc<HttpServerState>) -> Response<Body> {
    let recovery = state.identity.identity_document().await.recovery.clone();
    json_response(recovery)
}

async fn handle_update_recovery(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let recovery = match read_json::<Recovery>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    if state.identity.update_recovery(recovery.clone()).await.is_err() {
        return api_error_response(ApiErrorCode::InternalError, "recovery update failed", StatusCode::INTERNAL_SERVER_ERROR);
    }
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::identity",
        "recovery updated",
        Some(json!({"method": recovery.method})),
    )
    .await;
    json_response(recovery)
}

async fn handle_identity_export(state: Arc<HttpServerState>) -> Response<Body> {
    let identity_dir = state.admin.data_dir.join("identity");
    let data = match create_backup(&identity_dir, "") {
        Ok(data) => data,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "backup failed", StatusCode::INTERNAL_SERVER_ERROR),
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(data))
        .unwrap()
}

async fn handle_devices(state: Arc<HttpServerState>) -> Response<Body> {
    let data = state.admin.data.lock().await;
    json_response(data.devices.clone())
}

async fn handle_add_device(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let body = match read_json::<Value>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.trim().is_empty() {
        return api_error_response(ApiErrorCode::ValidationError, "device name required", StatusCode::UNPROCESSABLE_ENTITY);
    }

    let did = generate_token_hex(10);
    let activation_code = generate_token_hex(12);
    let expires_at = (Utc::now() + Duration::hours(24)).to_rfc3339();
    let device = Device {
        did: did.clone(),
        name: name.to_string(),
        created_at: Utc::now().to_rfc3339(),
        last_seen: Utc::now().to_rfc3339(),
        is_current: false,
        platform: None,
    };

    {
        let mut data = state.admin.data.lock().await;
        data.devices.push(device.clone());
    }
    let _ = state.admin.persist().await;

    let result = DeviceAddResult {
        did,
        name: name.to_string(),
        activation_code,
        expires_at,
    };
    log_entry(
        &state,
        "info",
        "postnode::identity",
        "device added",
        Some(json!({"device_name": name})),
    )
    .await;
    json_response(result)
}

async fn handle_remove_device(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let did = path.trim_start_matches("/admin/v1/devices/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.devices.iter().position(|device| device.did == did) {
        if data.devices[pos].is_current {
            return api_error_response(ApiErrorCode::Conflict, "cannot remove current device", StatusCode::CONFLICT);
        }
        let device = data.devices.remove(pos);
        drop(data);
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::identity",
            "device removed",
            Some(json!({"device_id": device.did})),
        )
        .await;
        return Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap();
    }
    api_error_response(ApiErrorCode::NotFound, "device not found", StatusCode::NOT_FOUND)
}

async fn handle_list_contacts(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let query = req.uri().query().unwrap_or("");
    let params = parse_query(query);
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50usize);
    let offset = params.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0usize);
    let sort_by = params.get("sort_by").map(String::as_str).unwrap_or("display_name");
    let sort_order = params.get("sort_order").map(String::as_str).unwrap_or("asc");

    let data = state.admin.data.lock().await;
    let mut contacts = data.contacts.clone();
    contacts.sort_by(|a, b| {
        let ordering = match sort_by {
            "added_at" => a.added_at.cmp(&b.added_at),
            "last_seen" => a.last_seen.cmp(&b.last_seen),
            "trust_level" => format!("{:?}", a.trust_level).cmp(&format!("{:?}", b.trust_level)),
            _ => a.display_name.cmp(&b.display_name),
        };
        if sort_order == "desc" { ordering.reverse() } else { ordering }
    });
    let total = contacts.len();
    let items = contacts.into_iter().skip(offset).take(limit).collect();
    let response = PaginatedResult { items, total, limit, offset };
    json_response(response)
}

async fn handle_add_contact(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let body = match read_json::<AddContactRequest>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let mut data = state.admin.data.lock().await;
    if data.contacts.iter().any(|c| c.iid == body.iid) {
        return api_error_response(ApiErrorCode::Conflict, "contact exists", StatusCode::CONFLICT);
    }
    let contact = Contact {
        iid: body.iid.clone(),
        display_name: body.display_name.clone(),
        avatar: None,
        trust_level: body.trust_level.unwrap_or(crate::admin_types::TrustLevel::Unverified),
        is_blocked: false,
        is_online: false,
        last_seen: None,
        added_at: Utc::now().to_rfc3339(),
        added_by: "manual".to_string(),
        notes: None,
        tags: Vec::new(),
        shared_groups: Vec::new(),
    };
    data.contacts.push(contact.clone());
    drop(data);
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::contacts",
        "contact added",
        Some(json!({"iid": contact.iid})),
    )
    .await;
    json_response(contact)
}

async fn handle_get_contact(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let iid = path.trim_start_matches("/admin/v1/contacts/");
    let data = state.admin.data.lock().await;
    if let Some(contact) = data.contacts.iter().find(|c| c.iid == iid) {
        return json_response(contact.clone());
    }
    api_error_response(ApiErrorCode::NotFound, "contact not found", StatusCode::NOT_FOUND)
}

async fn handle_update_contact(req: Request<Body>, path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let iid = path.trim_start_matches("/admin/v1/contacts/");
    let update = match read_json::<ContactUpdate>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let mut data = state.admin.data.lock().await;
    if let Some(contact) = data.contacts.iter_mut().find(|c| c.iid == iid) {
        if let Some(name) = update.display_name { contact.display_name = Some(name); }
        if let Some(notes) = update.notes { contact.notes = Some(notes); }
        if let Some(tags) = update.tags { contact.tags = tags; }
        if let Some(level) = update.trust_level { contact.trust_level = level; }
        let updated = contact.clone();
        drop(data);
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::contacts",
            "contact updated",
            Some(json!({"iid": iid})),
        )
        .await;
        return json_response(updated);
    }
    api_error_response(ApiErrorCode::NotFound, "contact not found", StatusCode::NOT_FOUND)
}

async fn handle_delete_contact(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let iid = path.trim_start_matches("/admin/v1/contacts/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.contacts.iter().position(|c| c.iid == iid) {
        let contact = data.contacts.remove(pos);
        drop(data);
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::contacts",
            "contact removed",
            Some(json!({"iid": contact.iid})),
        )
        .await;
        return Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap();
    }
    api_error_response(ApiErrorCode::NotFound, "contact not found", StatusCode::NOT_FOUND)
}

async fn handle_block_contact(path: &str, state: Arc<HttpServerState>, block: bool) -> Response<Body> {
    let iid = path.trim_start_matches("/admin/v1/contacts/").trim_end_matches("/block");
    let mut data = state.admin.data.lock().await;
    if let Some(contact) = data.contacts.iter_mut().find(|c| c.iid == iid) {
        contact.is_blocked = block;
        drop(data);
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::contacts",
            if block { "contact blocked" } else { "contact unblocked" },
            Some(json!({"iid": iid})),
        )
        .await;
        return Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap();
    }
    api_error_response(ApiErrorCode::NotFound, "contact not found", StatusCode::NOT_FOUND)
}

async fn handle_list_apps(state: Arc<HttpServerState>) -> Response<Body> {
    let data = state.admin.data.lock().await;
    json_response(data.apps.clone())
}

async fn handle_get_app(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/");
    let data = state.admin.data.lock().await;
    if let Some(app) = data.apps.iter().find(|app| app.id == app_id) {
        return json_response(app.clone());
    }
    api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND)
}

async fn handle_install_app(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let request = match read_json::<InstallRequest>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };

    let bytes = match request.source.r#type.as_str() {
        "url" => match fetch_bytes(&request.source.value).await {
            Ok(bytes) => bytes,
            Err(resp) => return resp,
        },
        "repository" => {
            let (repo_id, app_id) = match parse_repository_reference(&request.source.value) {
                Some(parts) => parts,
                None => return api_error_response(ApiErrorCode::ValidationError, "invalid repository reference", StatusCode::UNPROCESSABLE_ENTITY),
            };
            let manifest = match fetch_trusted_repository(&repo_id, &state).await {
                Ok(manifest) => manifest,
                Err(resp) => return resp,
            };
            let app = match manifest.apps.iter().find(|app| app.id == app_id) {
                Some(app) => app,
                None => return api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND),
            };
            match fetch_bytes(&app.download_url).await {
                Ok(bytes) => bytes,
                Err(resp) => return resp,
            }
        }
        _ => return api_error_response(ApiErrorCode::ValidationError, "unsupported source type", StatusCode::UNPROCESSABLE_ENTITY),
    };

    let package = match parse_postapp(&bytes) {
        Ok(package) => package,
        Err(_) => return api_error_response(ApiErrorCode::ValidationError, "invalid package", StatusCode::UNPROCESSABLE_ENTITY),
    };
    if verify_package_with_dht(state.dht.as_ref(), &package).await.is_err() {
        return api_error_response(ApiErrorCode::Forbidden, "package verification failed", StatusCode::FORBIDDEN);
    }
    if std::fs::create_dir_all(&state.apps_dir).is_err() {
        return api_error_response(ApiErrorCode::InternalError, "app directory error", StatusCode::INTERNAL_SERVER_ERROR);
    }
    if install_package(&package, &state.apps_dir).is_err() {
        return api_error_response(ApiErrorCode::Conflict, "app already installed", StatusCode::CONFLICT);
    }

    let required = package.manifest.capabilities.required.clone();
    let optional = package.manifest.capabilities.optional.clone().unwrap_or_default();
    let app = InstalledApp {
        id: package.manifest.app.id.clone(),
        name: package.manifest.app.name.clone(),
        version: package.manifest.app.version.clone(),
        author_iid: package.signature.author_iid.clone(),
        author_name: Some(package.manifest.app.author.name.clone()),
        description: package.manifest.app.description.clone(),
        icon: None,
        installed_at: Utc::now().to_rfc3339(),
        last_opened: None,
        update_available: None,
        status: crate::admin_types::AppStatus::Installed,
        permissions: crate::admin_types::AppPermissions {
            granted: required.clone(),
            denied: Vec::new(),
            pending: optional.clone(),
        },
        storage_used: package.manifest.files.total_size,
        storage_quota: 0,
    };
    {
        let mut data = state.admin.data.lock().await;
        data.apps.push(app.clone());
        data.app_sources.insert(app.id.clone(), request.source.clone());
    }
    let _ = state.admin.persist().await;
    state
        .event_bus
        .emit(
            "app_installed",
            json!({ "app_id": app.id, "version": app.version }),
        )
        .await;
    log_entry(
        &state,
        "info",
        "postnode::apps",
        "app installed",
        Some(json!({"app_id": app.id, "version": app.version})),
    )
    .await;
    let mut permissions_requested = required.clone();
    permissions_requested.extend(optional.clone());
    let result = InstallResult {
        app,
        permissions_requested,
        permissions_granted: required,
    };
    json_response(result)
}

async fn handle_install_upload(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let bytes = match read_multipart_file(req, state.config.max_request_body_bytes).await {
        Ok(bytes) => bytes,
        Err(resp) => return resp,
    };
    let package = match parse_postapp(&bytes) {
        Ok(package) => package,
        Err(_) => return api_error_response(ApiErrorCode::ValidationError, "invalid package", StatusCode::UNPROCESSABLE_ENTITY),
    };
    if verify_package_with_dht(state.dht.as_ref(), &package).await.is_err() {
        return api_error_response(ApiErrorCode::Forbidden, "package verification failed", StatusCode::FORBIDDEN);
    }
    if std::fs::create_dir_all(&state.apps_dir).is_err() {
        return api_error_response(ApiErrorCode::InternalError, "app directory error", StatusCode::INTERNAL_SERVER_ERROR);
    }
    if install_package(&package, &state.apps_dir).is_err() {
        return api_error_response(ApiErrorCode::Conflict, "app already installed", StatusCode::CONFLICT);
    }

    let required = package.manifest.capabilities.required.clone();
    let optional = package.manifest.capabilities.optional.clone().unwrap_or_default();
    let app = InstalledApp {
        id: package.manifest.app.id.clone(),
        name: package.manifest.app.name.clone(),
        version: package.manifest.app.version.clone(),
        author_iid: package.signature.author_iid.clone(),
        author_name: Some(package.manifest.app.author.name.clone()),
        description: package.manifest.app.description.clone(),
        icon: None,
        installed_at: Utc::now().to_rfc3339(),
        last_opened: None,
        update_available: None,
        status: crate::admin_types::AppStatus::Installed,
        permissions: crate::admin_types::AppPermissions {
            granted: required.clone(),
            denied: Vec::new(),
            pending: optional.clone(),
        },
        storage_used: package.manifest.files.total_size,
        storage_quota: 0,
    };
    {
        let mut data = state.admin.data.lock().await;
        data.apps.push(app.clone());
        data.app_sources.insert(app.id.clone(), crate::admin_types::AppSource {
            r#type: "file".to_string(),
            value: "upload".to_string(),
        });
    }
    let _ = state.admin.persist().await;
    state
        .event_bus
        .emit(
            "app_installed",
            json!({ "app_id": app.id, "version": app.version }),
        )
        .await;
    log_entry(
        &state,
        "info",
        "postnode::apps",
        "app installed",
        Some(json!({"app_id": app.id, "version": app.version})),
    )
    .await;
    let mut permissions_requested = required.clone();
    permissions_requested.extend(optional);
    let result = InstallResult {
        app,
        permissions_requested,
        permissions_granted: required,
    };
    json_response(result)
}

async fn handle_update_app(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/").trim_end_matches("/update");
    let source = {
        let data = state.admin.data.lock().await;
        if data.apps.iter().all(|app| app.id != app_id) {
            return api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND);
        }
        data.app_sources.get(app_id).cloned()
    };

    let Some(source) = source else {
        return api_error_response(ApiErrorCode::Conflict, "missing app source", StatusCode::CONFLICT);
    };
    if source.r#type == "file" {
        return api_error_response(ApiErrorCode::Conflict, "app source not updatable", StatusCode::CONFLICT);
    }

    let bytes = match source.r#type.as_str() {
        "url" => match fetch_bytes(&source.value).await {
            Ok(bytes) => bytes,
            Err(resp) => return resp,
        },
        "repository" => {
            let (repo_id, app_id_ref) = match parse_repository_reference(&source.value) {
                Some(parts) => parts,
                None => return api_error_response(ApiErrorCode::ValidationError, "invalid repository reference", StatusCode::UNPROCESSABLE_ENTITY),
            };
            let manifest = match fetch_trusted_repository(&repo_id, &state).await {
                Ok(manifest) => manifest,
                Err(resp) => return resp,
            };
            let app = match manifest.apps.iter().find(|app| app.id == app_id_ref) {
                Some(app) => app,
                None => return api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND),
            };
            match fetch_bytes(&app.download_url).await {
                Ok(bytes) => bytes,
                Err(resp) => return resp,
            }
        }
        _ => return api_error_response(ApiErrorCode::ValidationError, "unsupported source type", StatusCode::UNPROCESSABLE_ENTITY),
    };

    let package = match parse_postapp(&bytes) {
        Ok(package) => package,
        Err(_) => return api_error_response(ApiErrorCode::ValidationError, "invalid package", StatusCode::UNPROCESSABLE_ENTITY),
    };
    if verify_package_with_dht(state.dht.as_ref(), &package).await.is_err() {
        return api_error_response(ApiErrorCode::Forbidden, "package verification failed", StatusCode::FORBIDDEN);
    }

    if std::fs::create_dir_all(&state.apps_dir).is_err() {
        return api_error_response(ApiErrorCode::InternalError, "app directory error", StatusCode::INTERNAL_SERVER_ERROR);
    }
    let app_dir = state.apps_dir.join(&package.manifest.app.id);
    let _ = std::fs::remove_dir_all(&app_dir);
    if install_package(&package, &state.apps_dir).is_err() {
        return api_error_response(ApiErrorCode::InternalError, "app update failed", StatusCode::INTERNAL_SERVER_ERROR);
    }

    let required = package.manifest.capabilities.required.clone();
    let optional = package.manifest.capabilities.optional.clone().unwrap_or_default();

    let mut data = state.admin.data.lock().await;
    let app = data.apps.iter_mut().find(|app| app.id == app_id);
    if let Some(app) = app {
        let previous = app.version.clone();
        app.name = package.manifest.app.name.clone();
        app.version = package.manifest.app.version.clone();
        app.description = package.manifest.app.description.clone();
        app.author_iid = package.signature.author_iid.clone();
        app.author_name = Some(package.manifest.app.author.name.clone());
        app.permissions.granted = required.clone();
        app.permissions.pending = optional.clone();
        app.storage_used = package.manifest.files.total_size;
        let updated = app.clone();
        drop(data);
        let _ = state.admin.persist().await;
        let result = UpdateResult {
            app: updated,
            previous_version: previous,
            new_permissions: optional,
        };
        state
            .event_bus
            .emit(
                "app_updated",
                json!({ "app_id": result.app.id, "version": result.app.version }),
            )
            .await;
        log_entry(
            &state,
            "info",
            "postnode::apps",
            "app updated",
            Some(json!({"app_id": result.app.id, "version": result.app.version})),
        )
        .await;
        return json_response(result);
    }

    api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND)
}

async fn handle_delete_app(req: Request<Body>, path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/");
    let keep_data = req
        .uri()
        .query()
        .map(parse_query)
        .and_then(|params| params.get("keepData").cloned())
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false);
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.apps.iter().position(|app| app.id == app_id) {
        let app = data.apps.remove(pos);
        data.app_settings.remove(app_id);
        data.app_sources.remove(app_id);
        drop(data);
        let app_dir = state.apps_dir.join(app_id);
        let _ = std::fs::remove_dir_all(app_dir);
        if !keep_data {
            let storage_dir = state.admin.data_dir.join("apps").join("storage").join(app_id);
            let _ = std::fs::remove_dir_all(storage_dir);
        }
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::apps",
            "app uninstalled",
            Some(json!({"app_id": app.id, "keep_data": keep_data})),
        )
        .await;
        return Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap();
    }
    api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND)
}

async fn handle_get_app_permissions(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/").trim_end_matches("/permissions");
    let data = state.admin.data.lock().await;
    if let Some(app) = data.apps.iter().find(|app| app.id == app_id) {
        return json_response(app.permissions.clone());
    }
    api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND)
}

async fn handle_patch_app_permissions(req: Request<Body>, path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/").trim_end_matches("/permissions");
    let patch = match read_json::<PermissionPatch>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let mut data = state.admin.data.lock().await;
    if let Some(app) = data.apps.iter_mut().find(|app| app.id == app_id) {
        if let Some(grant) = patch.grant.as_ref() {
            app.permissions.granted.extend(grant.iter().cloned());
        }
        if let Some(deny) = patch.deny.as_ref() {
            app.permissions.denied.extend(deny.iter().cloned());
        }
        if let Some(reset) = patch.reset.as_ref() {
            app.permissions.granted.retain(|cap| !reset.contains(cap));
            app.permissions.denied.retain(|cap| !reset.contains(cap));
        }
        let updated = app.permissions.clone();
        drop(data);
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::apps",
            "app permissions updated",
            Some(json!({"app_id": app_id, "grant": patch.grant, "deny": patch.deny, "reset": patch.reset})),
        )
        .await;
        return json_response(updated);
    }
    api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND)
}

async fn handle_clear_app_data(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/").trim_end_matches("/clear-data");
    let storage_dir = state.admin.data_dir.join("apps").join("storage").join(app_id);
    let _ = std::fs::remove_dir_all(storage_dir);
    {
        let mut data = state.admin.data.lock().await;
        let Some(app) = data.apps.iter_mut().find(|app| app.id == app_id) else {
            return api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND);
        };
        app.storage_used = 0;
    }
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::apps",
        "app data cleared",
        Some(json!({"app_id": app_id})),
    )
    .await;
    Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap()
}

async fn handle_get_app_settings(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/").trim_end_matches("/settings");
    let data = state.admin.data.lock().await;
    if !data.apps.iter().any(|app| app.id == app_id) {
        return api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND);
    }
    let value = data.app_settings.get(app_id).cloned().unwrap_or_else(|| Value::Object(Default::default()));
    json_response(value)
}

async fn handle_put_app_settings(req: Request<Body>, path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/").trim_end_matches("/settings");
    let value = match read_json::<Value>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let mut data = state.admin.data.lock().await;
    if !data.apps.iter().any(|app| app.id == app_id) {
        return api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND);
    }
    data.app_settings.insert(app_id.to_string(), value.clone());
    drop(data);
    let _ = state.admin.persist().await;
    json_response(value)
}

async fn handle_settings(state: Arc<HttpServerState>) -> Response<Body> {
    let data = state.admin.data.lock().await;
    json_response(data.settings.clone())
}

async fn handle_settings_section(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let section = path.trim_start_matches("/admin/v1/settings/");
    let data = state.admin.data.lock().await;
    let value = match section {
        "network" => serde_json::to_value(&data.settings.network).ok(),
        "admin" => serde_json::to_value(&data.settings.admin).ok(),
        "apps" => serde_json::to_value(&data.settings.apps).ok(),
        "privacy" => serde_json::to_value(&data.settings.privacy).ok(),
        "storage" => serde_json::to_value(&data.settings.storage).ok(),
        "notifications" => serde_json::to_value(&data.settings.notifications).ok(),
        _ => None,
    };
    if let Some(value) = value {
        return json_response(value);
    }
    api_error_response(ApiErrorCode::NotFound, "settings section not found", StatusCode::NOT_FOUND)
}

async fn handle_patch_settings(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let patch = match read_json::<Value>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let changed_sections: Vec<String> = patch
        .as_object()
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();
    let mut data = state.admin.data.lock().await;
    let mut settings = data.settings.clone();
    merge_settings(&mut settings, &patch);
    data.settings = settings.clone();
    drop(data);
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::admin",
        "settings updated",
        Some(json!({"sections": changed_sections})),
    )
    .await;
    json_response(settings)
}

async fn handle_reset_settings(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let body = match read_json::<Value>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let section = body.get("section").and_then(|v| v.as_str());
    let mut data = state.admin.data.lock().await;
    let defaults = default_node_settings(&data.settings.storage.data_dir, &data.settings.storage.log_dir);
    match section {
        Some("network") => data.settings.network = defaults.network,
        Some("admin") => data.settings.admin = defaults.admin,
        Some("apps") => data.settings.apps = defaults.apps,
        Some("privacy") => data.settings.privacy = defaults.privacy,
        Some("storage") => data.settings.storage = defaults.storage,
        Some("notifications") => data.settings.notifications = defaults.notifications,
        None => data.settings = defaults,
        _ => {
            return api_error_response(ApiErrorCode::NotFound, "settings section not found", StatusCode::NOT_FOUND);
        }
    }
    let response = data.settings.clone();
    drop(data);
    let _ = state.admin.persist().await;
    json_response(response)
}

async fn handle_list_backups(state: Arc<HttpServerState>) -> Response<Body> {
    let data = state.admin.data.lock().await;
    json_response(data.backups.clone())
}

async fn handle_create_backup(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let backups_dir = state.admin.data_dir.join("backups");
    if tokio::fs::create_dir_all(&backups_dir).await.is_err() {
        return api_error_response(ApiErrorCode::InternalError, "backup directory error", StatusCode::INTERNAL_SERVER_ERROR);
    }
    let bytes = match hyper::body::to_bytes(req.into_body()).await {
        Ok(bytes) => bytes,
        Err(_) => return api_error_response(ApiErrorCode::InvalidRequest, "invalid body", StatusCode::BAD_REQUEST),
    };
    let backup_type = if bytes.is_empty() {
        "full".to_string()
    } else {
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return api_error_response(ApiErrorCode::InvalidRequest, "invalid json", StatusCode::BAD_REQUEST),
        };
        match value.get("type").and_then(|value| value.as_str()) {
            Some(value @ ("full" | "identity" | "data")) => value.to_string(),
            None => "full".to_string(),
            _ => return api_error_response(ApiErrorCode::ValidationError, "invalid backup type", StatusCode::UNPROCESSABLE_ENTITY),
        }
    };
    let backup_id = generate_token_hex(8);
    let path = backups_dir.join(format!("{}.pusb", backup_id));
    let data = match create_backup(&state.admin.data_dir, "") {
        Ok(data) => data,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "backup failed", StatusCode::INTERNAL_SERVER_ERROR),
    };
    if tokio::fs::write(&path, &data).await.is_err() {
        return api_error_response(ApiErrorCode::InternalError, "backup write failed", StatusCode::INTERNAL_SERVER_ERROR);
    }
    let entry = BackupListEntry {
        id: backup_id.clone(),
        created_at: Utc::now().to_rfc3339(),
        size: data.len() as u64,
        path: path.to_string_lossy().to_string(),
        r#type: backup_type.clone(),
    };
    {
        let mut data = state.admin.data.lock().await;
        data.backups.push(entry.clone());
    }
    let _ = state.admin.persist().await;
    let result = BackupResult {
        id: entry.id,
        created_at: entry.created_at,
        size: entry.size,
        path: entry.path,
        encrypted: true,
    };
    log_entry(
        &state,
        "info",
        "postnode::admin",
        "backup created",
        Some(json!({"backup_id": result.id, "size": result.size, "type": backup_type})),
    )
    .await;
    json_response(result)
}

async fn handle_upload_backup(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let backups_dir = state.admin.data_dir.join("backups");
    if tokio::fs::create_dir_all(&backups_dir).await.is_err() {
        return api_error_response(ApiErrorCode::InternalError, "backup directory error", StatusCode::INTERNAL_SERVER_ERROR);
    }

    let bytes = match read_multipart_file(req, state.config.max_request_body_bytes).await {
        Ok(bytes) => bytes,
        Err(resp) => return resp,
    };

    let backup_id = generate_token_hex(8);
    let path = backups_dir.join(format!("{}.pusb", backup_id));
    if tokio::fs::write(&path, &bytes).await.is_err() {
        return api_error_response(ApiErrorCode::InternalError, "backup write failed", StatusCode::INTERNAL_SERVER_ERROR);
    }

    let entry = BackupListEntry {
        id: backup_id.clone(),
        created_at: Utc::now().to_rfc3339(),
        size: bytes.len() as u64,
        path: path.to_string_lossy().to_string(),
        r#type: "full".to_string(),
    };
    {
        let mut data = state.admin.data.lock().await;
        data.backups.push(entry.clone());
    }
    let _ = state.admin.persist().await;
    log_entry(
        &state,
        "info",
        "postnode::admin",
        "backup uploaded",
        Some(json!({"backup_id": entry.id, "size": entry.size})),
    )
    .await;
    json_response(entry)
}

async fn handle_download_backup(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let id = path.trim_start_matches("/admin/v1/backups/");
    let data = state.admin.data.lock().await;
    if let Some(entry) = data.backups.iter().find(|entry| entry.id == id) {
        if let Ok(bytes) = tokio::fs::read(&entry.path).await {
            return Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(bytes))
                .unwrap();
        }
        return api_error_response(ApiErrorCode::InternalError, "backup read failed", StatusCode::INTERNAL_SERVER_ERROR);
    }
    api_error_response(ApiErrorCode::NotFound, "backup not found", StatusCode::NOT_FOUND)
}

async fn handle_restore_backup(req: Request<Body>, path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let id = path.trim_start_matches("/admin/v1/backups/").trim_end_matches("/restore");
    let body = match read_json::<Value>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("");

    let data = state.admin.data.lock().await;
    let Some(entry) = data.backups.iter().find(|entry| entry.id == id) else {
        return api_error_response(ApiErrorCode::NotFound, "backup not found", StatusCode::NOT_FOUND);
    };
    let backup_bytes = match tokio::fs::read(&entry.path).await {
        Ok(bytes) => bytes,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "backup read failed", StatusCode::INTERNAL_SERVER_ERROR),
    };
    if restore_backup(&backup_bytes, password, &state.admin.data_dir).is_err() {
        return api_error_response(ApiErrorCode::InternalError, "restore failed", StatusCode::INTERNAL_SERVER_ERROR);
    }

    let result = RestoreResult {
        success: true,
        restored_at: Utc::now().to_rfc3339(),
        identity: state.identity.iid().await,
        contacts_restored: 0,
        messages_restored: 0,
        apps_restored: 0,
        warnings: Vec::new(),
    };
    log_entry(
        &state,
        "info",
        "postnode::admin",
        "backup restored",
        Some(json!({"backup_id": id})),
    )
    .await;
    json_response(result)
}

async fn handle_delete_backup(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let id = path.trim_start_matches("/admin/v1/backups/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.backups.iter().position(|entry| entry.id == id) {
        let entry = data.backups.remove(pos);
        drop(data);
        let _ = tokio::fs::remove_file(entry.path).await;
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::admin",
            "backup deleted",
            Some(json!({"backup_id": entry.id})),
        )
        .await;
        return Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap();
    }
    api_error_response(ApiErrorCode::NotFound, "backup not found", StatusCode::NOT_FOUND)
}

async fn handle_list_api_keys(state: Arc<HttpServerState>) -> Response<Body> {
    let data = state.admin.data.lock().await;
    let keys: Vec<ApiKey> = data.api_keys.iter().map(|record| record.key.clone()).collect();
    json_response(keys)
}

async fn handle_create_api_key(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let body = match read_json::<CreateApiKeyRequest>(req, state.config.max_request_body_bytes).await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let secret = generate_token_hex(32);
    let now = Utc::now();
    let expires_at = body.expires_in_days.map(|days| (now + Duration::days(days as i64)).to_rfc3339());
    let key = ApiKey {
        id: generate_token_hex(6),
        name: body.name,
        permissions: body.permissions,
        created_at: now.to_rfc3339(),
        expires_at,
        last_used: None,
    };
    let record = ApiKeyRecord {
        key: key.clone(),
        key_hash: hash_token(&secret),
    };
    {
        let mut data = state.admin.data.lock().await;
        data.api_keys.push(record);
    }
    let _ = state.admin.persist().await;

    let response = CreateApiKeyResponse { key, secret };
    log_entry(
        &state,
        "info",
        "postnode::admin",
        "api key created",
        Some(json!({"api_key_id": response.key.id})),
    )
    .await;
    json_response(response)
}

async fn handle_delete_api_key(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let id = path.trim_start_matches("/admin/v1/api-keys/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.api_keys.iter().position(|record| record.key.id == id) {
        let record = data.api_keys.remove(pos);
        drop(data);
        let _ = state.admin.persist().await;
        log_entry(
            &state,
            "info",
            "postnode::admin",
            "api key revoked",
            Some(json!({"api_key_id": record.key.id})),
        )
        .await;
        return Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap();
    }
    api_error_response(ApiErrorCode::NotFound, "api key not found", StatusCode::NOT_FOUND)
}

async fn handle_status(state: Arc<HttpServerState>) -> Response<Body> {
    let uptime = state.started_at.elapsed().as_secs();
    let data = state.admin.data.lock().await;
    let status = NodeStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
        status: "healthy".to_string(),
        identity: crate::admin_types::IdentityStatus {
            iid: state.identity.iid().await,
            last_published: None,
            device_count: data.devices.len() as u32,
        },
        network: crate::admin_types::NetworkStatus {
            connections_active: 0,
            connections_direct: 0,
            connections_relay: 0,
            relays_connected: 0,
            bytes_sent: 0,
            bytes_received: 0,
            external_addr_detected: None,
        },
        storage: crate::admin_types::StorageStatus {
            data_used_bytes: directory_size(&state.admin.data_dir),
            data_free_bytes: fs2::available_space(&state.admin.data_dir).unwrap_or(0),
            messages_count: 0,
            documents_count: 0,
        },
        apps: crate::admin_types::AppsStatus {
            installed: data.apps.len() as u32,
            running: 0,
            total_storage_used: data.apps.iter().map(|app| app.storage_used).sum(),
        },
    };
    json_response(status)
}

async fn handle_logs(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let query = req.uri().query().unwrap_or("");
    let params = parse_query(query);
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100usize);
    let cursor = params.get("cursor").cloned();
    let level_filter = params.get("level").cloned();
    let target_filter = params.get("target").cloned();
    let search_filter = params.get("search").cloned();
    let since = params
        .get("since")
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|ts| ts.with_timezone(&Utc));
    let until = params
        .get("until")
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|ts| ts.with_timezone(&Utc));

    let data = state.admin.data.lock().await;
    let mut filtered: Vec<&LogEntry> = data
        .logs
        .iter()
        .filter(|entry| match &level_filter {
            Some(level) => entry.level.eq_ignore_ascii_case(level),
            None => true,
        })
        .filter(|entry| match &target_filter {
            Some(target) => entry.target == *target,
            None => true,
        })
        .filter(|entry| match &search_filter {
            Some(search) => entry.message.contains(search),
            None => true,
        })
        .filter(|entry| match since {
            Some(since) => DateTime::parse_from_rfc3339(&entry.timestamp)
                .map(|ts| ts.with_timezone(&Utc) >= since)
                .unwrap_or(false),
            None => true,
        })
        .filter(|entry| match until {
            Some(until) => DateTime::parse_from_rfc3339(&entry.timestamp)
                .map(|ts| ts.with_timezone(&Utc) <= until)
                .unwrap_or(false),
            None => true,
        })
        .collect();

    filtered.sort_by_key(|entry| entry.timestamp.clone());

    let start_index = cursor
        .as_ref()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let total = filtered.len();
    let end_index = std::cmp::min(start_index + limit, total);
    let entries: Vec<LogEntry> = filtered[start_index..end_index].iter().map(|entry| (*entry).clone()).collect();
    let next_cursor = if end_index < total { Some(end_index.to_string()) } else { None };

    json_response(LogsResponse {
        entries,
        cursor: next_cursor,
        has_more: end_index < total,
    })
}

async fn log_entry(
    state: &HttpServerState,
    level: &str,
    target: &str,
    message: &str,
    fields: Option<Value>,
) {
    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        level: level.to_string(),
        target: target.to_string(),
        message: message.to_string(),
        fields,
    };
    state.admin.append_log(entry.clone(), 1000).await;
    let _ = state
        .event_bus
        .emit(
            "log_entry",
            json!({
                "timestamp": entry.timestamp,
                "level": entry.level,
                "target": entry.target,
                "message": entry.message,
                "fields": entry.fields,
            }),
        )
        .await;
}

async fn handle_health(state: &HttpServerState) -> Response<Body> {
    let identity_doc = state.identity.identity_document().await;
    let iid = state.identity.iid().await;
    let apps_installed = {
        let data = state.admin.data.lock().await;
        data.apps.len() as u64
    };
    let disk_used_bytes = directory_size(&state.admin.data_dir);
    let disk_free_bytes = fs2::available_space(&state.admin.data_dir).unwrap_or(0);
    let payload = json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.started_at.elapsed().as_secs(),
        "checks": {
            "identity": {
                "status": "healthy",
                "iid": iid,
                "last_published": identity_doc.timestamp,
            },
            "transport": {
                "status": "healthy",
                "connections": 0,
                "relays_connected": 0,
            },
            "messaging": {
                "status": "healthy",
                "queue_depth": 0,
                "sessions_active": 0,
            },
            "storage": {
                "status": "healthy",
                "disk_used_bytes": disk_used_bytes,
                "disk_free_bytes": disk_free_bytes,
            },
            "apps": {
                "status": "healthy",
                "installed": apps_installed,
                "running": 0,
            },
        }
    });
    json_response(payload)
}

async fn render_metrics(state: &HttpServerState) -> String {
    use std::fmt::Write;

    let iid = state.identity.iid().await;
    let uptime_seconds = state.started_at.elapsed().as_secs();
    let apps_installed = {
        let data = state.admin.data.lock().await;
        data.apps.len() as u64
    };
    let apps_running = 0u64;
    let identity_bytes = directory_size(&state.admin.data_dir.join("identity"));
    let messages_bytes = directory_size(&state.admin.data_dir.join("messages"));
    let sync_bytes = directory_size(&state.admin.data_dir.join("sync"));
    let apps_bytes = directory_size(&state.admin.data_dir.join("apps"));
    let runtime_bytes = directory_size(&state.admin.data_dir.join("runtime"));

    let mut out = String::new();
    let _ = writeln!(out, "postnode_uptime_seconds {}", uptime_seconds);
    let _ = writeln!(
        out,
        "postnode_info{{version=\"{}\", iid=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION"),
        iid
    );
    let _ = writeln!(out, "postnode_memory_bytes{{type=\"heap\"}} 0");
    let _ = writeln!(out, "postnode_memory_bytes{{type=\"resident\"}} 0");
    let _ = writeln!(out, "postnode_cpu_seconds_total 0");
    let _ = writeln!(out, "postnode_open_file_descriptors 0");
    let _ = writeln!(out, "postnode_connections_total{{type=\"direct\"}} 0");
    let _ = writeln!(out, "postnode_connections_total{{type=\"relay\"}} 0");
    let _ = writeln!(out, "postnode_connections_active 0");
    let _ = writeln!(out, "postnode_connection_events_total{{event=\"opened\"}} 0");
    let _ = writeln!(out, "postnode_connection_events_total{{event=\"closed\"}} 0");
    let _ = writeln!(out, "postnode_connection_events_total{{event=\"failed\"}} 0");
    let _ = writeln!(out, "postnode_bytes_sent_total 0");
    let _ = writeln!(out, "postnode_bytes_received_total 0");
    let _ = writeln!(out, "postnode_messages_sent_total{{type=\"direct\"}} 0");
    let _ = writeln!(out, "postnode_messages_sent_total{{type=\"group\"}} 0");
    let _ = writeln!(out, "postnode_messages_received_total{{type=\"direct\"}} 0");
    let _ = writeln!(out, "postnode_messages_received_total{{type=\"group\"}} 0");
    let _ = writeln!(out, "postnode_message_queue_depth{{queue=\"outgoing\"}} 0");
    let _ = writeln!(out, "postnode_message_queue_depth{{queue=\"incoming\"}} 0");
    let _ = writeln!(out, "postnode_apps_installed_total {}", apps_installed);
    let _ = writeln!(out, "postnode_apps_running {}", apps_running);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"identity\"}} {}", identity_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"messages\"}} {}", messages_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"sync\"}} {}", sync_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"apps\"}} {}", apps_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"runtime\"}} {}", runtime_bytes);
    out
}

async fn handle_restart(state: &HttpServerState) -> Response<Body> {
    state.health.set_ready(false);
    state.health.set_shutting_down(true);
    Response::builder().status(StatusCode::ACCEPTED).body(Body::empty()).unwrap()
}

async fn handle_shutdown(state: &HttpServerState) -> Response<Body> {
    state.health.set_ready(false);
    state.health.set_shutting_down(true);
    Response::builder().status(StatusCode::ACCEPTED).body(Body::empty()).unwrap()
}

async fn handle_health_live(state: &HttpServerState) -> Response<Body> {
    if state.health.is_shutting_down() {
        return json_response_with_status(
            json!({"status": "dead", "reason": "shutting_down"}),
            StatusCode::SERVICE_UNAVAILABLE,
        );
    }
    json_response(json!({"status": "alive"}))
}

async fn handle_health_ready(state: &HttpServerState) -> Response<Body> {
    if state.health.is_ready() {
        return json_response(json!({"status": "ready"}));
    }
    let details = state.health.readiness_details().await;
    json_response_with_status(
        json!({
            "status": "not_ready",
            "reason": "initializing",
            "details": details,
        }),
        StatusCode::SERVICE_UNAVAILABLE,
    )
}

async fn handle_events(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    if !hyper_tungstenite::is_upgrade_request(&req) {
        return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::empty()).unwrap();
    }

    if !authenticate_events(&req, &state).await {
        return api_error_response(ApiErrorCode::Unauthorized, "unauthorized", StatusCode::UNAUTHORIZED);
    }

    let params = parse_query(req.uri().query().unwrap_or(""));
    let last_id = params.get("lastEventId").and_then(|value| value.parse::<u64>().ok());

    let (response, websocket) = match hyper_tungstenite::upgrade(req, None) {
        Ok(parts) => parts,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "websocket upgrade failed", StatusCode::INTERNAL_SERVER_ERROR),
    };

    let bus = state.event_bus.clone();
    tokio::spawn(async move {
        if let Ok(stream) = websocket.await {
            handle_event_socket(stream, bus, last_id).await;
        }
    });

    response
}

async fn authenticate_events(req: &Request<Body>, state: &HttpServerState) -> bool {
    let params = parse_query(req.uri().query().unwrap_or(""));
    if let Some(token) = params.get("token") {
        return token_matches(token, state).await;
    }
    authenticate(req, state).await.is_ok()
}

async fn token_matches(token: &str, state: &HttpServerState) -> bool {
    let token_hash = hash_token(token);
    if let Some(admin_hash) = state.auth.admin_token_hash.as_ref() {
        if constant_time_eq(token_hash.as_bytes(), admin_hash.as_bytes()) {
            return true;
        }
    }
    let data = state.admin.data.lock().await;
    data.api_keys.iter().any(|record| record.key_hash == token_hash)
}

async fn handle_event_socket(
    mut ws: hyper_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>,
    bus: Arc<EventBus>,
    last_id: Option<u64>,
) {
    let mut subscriptions = default_subscriptions();
    let mut receiver = bus.subscribe();

    for msg in bus.replay_since(last_id).await {
        if subscriptions.contains(&msg.r#type) {
            if let Ok(payload) = serde_json::to_string(&msg) {
                let _ = ws.send(hyper_tungstenite::tungstenite::Message::Text(payload)).await;
            }
        }
    }

    loop {
        tokio::select! {
            inbound = ws.next() => {
                match inbound {
                    Some(Ok(message)) => {
                        if message.is_text() {
                            if let Ok(value) = serde_json::from_str::<Value>(message.to_text().unwrap_or("")) {
                                apply_subscription_update(&mut subscriptions, &value);
                            }
                        } else if message.is_close() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            event = receiver.recv() => {
                if let Ok(msg) = event {
                    if subscriptions.contains(&msg.r#type) {
                        if let Ok(payload) = serde_json::to_string(&msg) {
                            let _ = ws.send(hyper_tungstenite::tungstenite::Message::Text(payload)).await;
                        }
                    }
                }
            }
        }
    }
}

fn default_subscriptions() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for event in [
        "status_change",
        "contact_online",
        "message_received",
        "app_installed",
        "app_updated",
        "app_error",
        "sync_progress",
        "error",
    ] {
        set.insert(event.to_string());
    }
    set
}

fn apply_subscription_update(subscriptions: &mut std::collections::HashSet<String>, value: &Value) {
    let Some(kind) = value.get("type").and_then(|v| v.as_str()) else { return; };
    let Some(events) = value.get("events").and_then(|v| v.as_array()) else { return; };
    let items: Vec<String> = events
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    match kind {
        "subscribe" => {
            for item in items {
                subscriptions.insert(item);
            }
        }
        "unsubscribe" => {
            for item in items {
                subscriptions.remove(&item);
            }
        }
        _ => {}
    }
}

async fn fetch_bytes(url: &str) -> std::result::Result<Vec<u8>, Response<Body>> {
    let response = reqwest::get(url).await
        .map_err(|_| api_error_response(ApiErrorCode::InvalidRequest, "download failed", StatusCode::BAD_REQUEST))?;
    if !response.status().is_success() {
        return Err(api_error_response(ApiErrorCode::InvalidRequest, "download failed", StatusCode::BAD_REQUEST));
    }
    let bytes = response.bytes().await
        .map_err(|_| api_error_response(ApiErrorCode::InvalidRequest, "download failed", StatusCode::BAD_REQUEST))?;
    Ok(bytes.to_vec())
}

fn parse_repository_reference(value: &str) -> Option<(String, String)> {
    let mut parts = value.splitn(2, ':');
    let repo = parts.next()?.to_string();
    let app = parts.next()?.to_string();
    if repo.is_empty() || app.is_empty() {
        return None;
    }
    Some((repo, app))
}

async fn fetch_trusted_repository(repo_id: &str, state: &HttpServerState) -> std::result::Result<RepositoryManifest, Response<Body>> {
    let data = state.admin.data.lock().await;
    let repo = data
        .settings
        .apps
        .trusted_repositories
        .iter()
        .find(|repo| repo.id == repo_id)
        .cloned();

    let Some(repo) = repo else {
        return Err(api_error_response(ApiErrorCode::NotFound, "repository not found", StatusCode::NOT_FOUND));
    };
    if repo.trust_level == "disabled" {
        return Err(api_error_response(ApiErrorCode::Forbidden, "repository disabled", StatusCode::FORBIDDEN));
    }

    let now = Utc::now();
    if let Some(cached) = data.repo_cache.get(repo_id) {
        if let Ok(fetched_at) = DateTime::parse_from_rfc3339(&cached.fetched_at) {
            if fetched_at.with_timezone(&Utc) + Duration::hours(1) > now {
                let manifest: RepositoryManifest = serde_json::from_value(cached.manifest.clone())
                    .map_err(|_| api_error_response(ApiErrorCode::InternalError, "repository cache invalid", StatusCode::INTERNAL_SERVER_ERROR))?;
                return Ok(manifest);
            }
        }
    }
    drop(data);

    let url = if repo.url.ends_with("repository.json") {
        repo.url
    } else {
        format!("{}/repository.json", repo.url.trim_end_matches('/'))
    };
    let manifest = fetch_repository(&url).await
        .map_err(|_| api_error_response(ApiErrorCode::InvalidRequest, "repository fetch failed", StatusCode::BAD_REQUEST))?;
    if manifest.signature.operator_iid != repo.operator_iid {
        return Err(api_error_response(ApiErrorCode::Forbidden, "repository operator mismatch", StatusCode::FORBIDDEN));
    }
    let verified_key = verify_repository(state.dht.as_ref(), &manifest).await
        .map_err(|_| api_error_response(ApiErrorCode::Forbidden, "repository verification failed", StatusCode::FORBIDDEN))?;

    if let Some(expected) = repo.operator_key_fingerprint.as_ref() {
        let actual = fingerprint_key_bytes(&verified_key);
        if actual != *expected {
            return Err(api_error_response(ApiErrorCode::Forbidden, "repository key mismatch", StatusCode::FORBIDDEN));
        }
    }

    let mut data = state.admin.data.lock().await;
    let cached = CachedRepository {
        fetched_at: now.to_rfc3339(),
        manifest: serde_json::to_value(&manifest).map_err(|_| api_error_response(ApiErrorCode::InternalError, "repository cache failed", StatusCode::INTERNAL_SERVER_ERROR))?,
    };
    data.repo_cache.insert(repo_id.to_string(), cached);
    drop(data);
    let _ = state.admin.persist().await;

    Ok(manifest)
}

async fn read_multipart_file(req: Request<Body>, max_bytes: usize) -> std::result::Result<Vec<u8>, Response<Body>> {
    let content_type = req.headers().get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let boundary = content_type.split("boundary=").nth(1).unwrap_or("").to_string();
    if boundary.is_empty() {
        return Err(api_error_response(ApiErrorCode::InvalidRequest, "missing boundary", StatusCode::BAD_REQUEST));
    }

    let bytes = match hyper::body::to_bytes(req.into_body()).await {
        Ok(bytes) => bytes,
        Err(_) => return Err(api_error_response(ApiErrorCode::InvalidRequest, "invalid body", StatusCode::BAD_REQUEST)),
    };
    if bytes.len() > max_bytes {
        return Err(api_error_response(ApiErrorCode::PayloadTooLarge, "payload too large", StatusCode::PAYLOAD_TOO_LARGE));
    }

    let boundary_marker = format!("--{boundary}").into_bytes();
    let mut cursor = 0;
    while let Some(start) = find_subslice(&bytes[cursor..], &boundary_marker) {
        let part_start = cursor + start + boundary_marker.len();
        if bytes.get(part_start..part_start + 2) == Some(b"--") {
            break;
        }
        let mut idx = part_start;
        if bytes.get(idx..idx + 2) == Some(b"\r\n") {
            idx += 2;
        }
        let header_end = find_subslice(&bytes[idx..], b"\r\n\r\n")
            .ok_or_else(|| api_error_response(ApiErrorCode::InvalidRequest, "invalid multipart", StatusCode::BAD_REQUEST))?;
        let headers = &bytes[idx..idx + header_end];
        let headers_str = String::from_utf8_lossy(headers);
        let content_start = idx + header_end + 4;
        let next_boundary = find_subslice(&bytes[content_start..], &boundary_marker)
            .ok_or_else(|| api_error_response(ApiErrorCode::InvalidRequest, "invalid multipart", StatusCode::BAD_REQUEST))?;
        let mut content_end = content_start + next_boundary;
        if bytes.get(content_end - 2..content_end) == Some(b"\r\n") {
            content_end -= 2;
        }
        if headers_str.contains("name=\"file\"") {
            return Ok(bytes[content_start..content_end].to_vec());
        }
        cursor = content_start + next_boundary;
    }

    Err(api_error_response(ApiErrorCode::InvalidRequest, "missing file", StatusCode::BAD_REQUEST))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn serve_app(path: &str, state: &HttpServerState) -> Response<Body> {
    let mut parts = path.trim_start_matches("/apps/").splitn(2, '/');
    let app_id = match parts.next() {
        Some(id) if !id.is_empty() => id,
        _ => return Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap(),
    };
    let rest = parts.next().unwrap_or("");
    if rest.starts_with("api/") {
        return Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap();
    }

    let relative = if rest.is_empty() { "ui/index.html".to_string() } else if rest.starts_with("assets/") {
        let asset_path = format!("ui/{}", rest);
        if state.apps_dir.join(app_id).join(&asset_path).exists() {
            asset_path
        } else {
            rest.to_string()
        }
    } else if rest.ends_with('/') {
        format!("ui/{}index.html", rest)
    } else {
        format!("ui/{}", rest)
    };

    let app_root = state.apps_dir.join(app_id);
    let Some(full) = safe_join(&app_root, &relative) else {
        return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::empty()).unwrap();
    };
    if let Ok(bytes) = std::fs::read(&full) {
        return Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type_for_path(&full))
            .body(Body::from(bytes))
            .unwrap();
    }

    Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap()
}

async fn handle_app_api(req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let path = req.uri().path();
    let mut parts = path.trim_start_matches("/apps/").splitn(2, '/');
    let app_id = match parts.next() {
        Some(id) if !id.is_empty() => id,
        _ => return Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap(),
    };
    let rest = parts.next().unwrap_or("");
    let api_suffix = rest.strip_prefix("api/").unwrap_or("");

    let Some(base_url) = app_api_base_url(&state, app_id).await else {
        return Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap();
    };
    let mut full = base_url.trim_end_matches('/').to_string();
    if !api_suffix.is_empty() {
        full.push('/');
        full.push_str(api_suffix);
    }
    if let Some(query) = req.uri().query() {
        full.push('?');
        full.push_str(query);
    }

    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);
    let mut builder = reqwest::Client::new().request(method, full);
    for (name, value) in req.headers() {
        if name == hyper::header::HOST || name == CONTENT_LENGTH {
            continue;
        }
        builder = builder.header(name, value);
    }
    let body = match hyper::body::to_bytes(req.into_body()).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap(),
    };
    if !body.is_empty() {
        builder = builder.body(body.to_vec());
    }
    let response = match builder.send().await {
        Ok(resp) => resp,
        Err(_) => return Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap(),
    };

    let mut out = Response::builder().status(response.status());
    for (name, value) in response.headers() {
        if name == hyper::header::CONNECTION || name == hyper::header::TRANSFER_ENCODING {
            continue;
        }
        out = out.header(name, value);
    }
    match response.bytes().await {
        Ok(bytes) => out.body(Body::from(bytes)).unwrap(),
        Err(_) => Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::empty()).unwrap(),
    }
}

async fn app_api_base_url(state: &HttpServerState, app_id: &str) -> Option<String> {
    let data = state.admin.data.lock().await;
    let settings = data.app_settings.get(app_id)?;
    let custom = settings.get("customConfig").and_then(|value| value.as_object())?;
    let direct = custom
        .get("api_base_url")
        .and_then(|value| value.as_str())
        .or_else(|| custom.get("apiBaseUrl").and_then(|value| value.as_str()))?;
    if direct.trim().is_empty() {
        None
    } else {
        Some(direct.to_string())
    }
}

fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    let candidate = root.join(relative);
    if candidate.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        return None;
    }
    Some(candidate)
}

fn directory_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else { continue; };
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            total = total.saturating_add(directory_size(&entry.path()));
        }
    }
    total
}

fn content_type_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn json_response<T: serde::Serialize>(payload: T) -> Response<Body> {
    let body = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn json_response_with_status<T: serde::Serialize>(payload: T, status: StatusCode) -> Response<Body> {
    let body = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn api_error_response(code: ApiErrorCode, message: &str, status: StatusCode) -> Response<Body> {
    let body = api_error(code, message);
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap()
}

fn set_cookie_header(resp: &mut Response<Body>, value: &str) {
    let header = HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static(""));
    resp.headers_mut().append(SET_COOKIE, header);
}

fn extract_cookie<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    for part in cookies.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix(name) {
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value);
            }
        }
    }
    None
}

fn secure_flag(enabled: bool) -> &'static str {
    if enabled { "; Secure" } else { "" }
}

async fn read_json<T: DeserializeOwned>(req: Request<Body>, max_bytes: usize) -> std::result::Result<T, Response<Body>> {
    if let Some(len) = req.headers().get(CONTENT_LENGTH) {
        if let Ok(value) = len.to_str() {
            if let Ok(size) = value.parse::<usize>() {
                if size > max_bytes {
                    return Err(api_error_response(ApiErrorCode::PayloadTooLarge, "payload too large", StatusCode::PAYLOAD_TOO_LARGE));
                }
            }
        }
    }

    let bytes = match hyper::body::to_bytes(req.into_body()).await {
        Ok(bytes) => bytes,
        Err(_) => return Err(api_error_response(ApiErrorCode::InvalidRequest, "invalid body", StatusCode::BAD_REQUEST)),
    };
    if bytes.len() > max_bytes {
        return Err(api_error_response(ApiErrorCode::PayloadTooLarge, "payload too large", StatusCode::PAYLOAD_TOO_LARGE));
    }

    serde_json::from_slice(&bytes)
        .map_err(|_| api_error_response(ApiErrorCode::InvalidRequest, "invalid json", StatusCode::BAD_REQUEST))
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        if let Some(key) = parts.next() {
            let value = parts.next().unwrap_or("");
            out.insert(key.to_string(), value.to_string());
        }
    }
    out
}

fn merge_settings(current: &mut crate::admin_types::NodeSettings, patch: &Value) {
    if let Some(network) = patch.get("network") {
        if let Ok(value) = serde_json::from_value(network.clone()) {
            current.network = value;
        }
    }
    if let Some(admin) = patch.get("admin") {
        if let Ok(value) = serde_json::from_value(admin.clone()) {
            current.admin = value;
        }
    }
    if let Some(apps) = patch.get("apps") {
        if let Ok(value) = serde_json::from_value(apps.clone()) {
            current.apps = value;
        }
    }
    if let Some(privacy) = patch.get("privacy") {
        if let Ok(value) = serde_json::from_value(privacy.clone()) {
            current.privacy = value;
        }
    }
    if let Some(storage) = patch.get("storage") {
        if let Ok(value) = serde_json::from_value(storage.clone()) {
            current.storage = value;
        }
    }
    if let Some(notifications) = patch.get("notifications") {
        if let Ok(value) = serde_json::from_value(notifications.clone()) {
            current.notifications = value;
        }
    }
}

fn key_fingerprint(base64_key: &str) -> String {
    let raw = crate::encoding::base64_decode(base64_key).unwrap_or_default();
    let hash = sha2::Sha256::digest(raw.as_slice());
    format!("sha256:{}", hex::encode(hash))
}

fn fingerprint_key_bytes(raw: &[u8]) -> String {
    let hash = sha2::Sha256::digest(raw);
    format!("sha256:{}", hex::encode(hash))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut out = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        out |= x ^ y;
    }
    out == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin_auth::hash_token;
    use crate::dht::MemoryDht;

    async fn test_state() -> HttpServerState {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let log_dir = dir.path().join("logs");
        let apps_dir = data_dir.join("apps").join("installed");
        let _ = std::fs::create_dir_all(&apps_dir);
        let settings = default_node_settings(
            data_dir.to_string_lossy().as_ref(),
            log_dir.to_string_lossy().as_ref(),
        );
        let admin = AdminState::load(&data_dir, settings).await.unwrap();
        let identity = Arc::new(IdentityManager::new(&data_dir.join("identity").to_string_lossy()).await.unwrap());
        HttpServerState {
            admin,
            auth: AuthConfig {
                password_hash: None,
                admin_token_hash: None,
                session_secret: vec![1u8; 32],
                session_timeout_hours: 24,
            },
            identity,
            dht: Arc::new(MemoryDht::new()),
            event_bus: Arc::new(EventBus::new()),
            started_at: Instant::now(),
            config: HttpServerConfig {
                metrics_enabled: true,
                max_request_body_bytes: 1024 * 1024,
                session_cookie_secure: false,
            },
            health: HealthState::new(),
            apps_dir,
        }
    }

    #[tokio::test]
    async fn health_ok() {
        let state = Arc::new(test_state().await);
        let req = Request::builder().method(Method::GET).uri("/health").body(Body::empty()).unwrap();
        let resp = handle_request(req, state).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value.get("status").and_then(|v| v.as_str()), Some("healthy"));
    }

    #[tokio::test]
    async fn metrics_disabled() {
        let mut state = test_state().await;
        state.config.metrics_enabled = false;
        let state = Arc::new(state);
        let req = Request::builder().method(Method::GET).uri("/metrics").body(Body::empty()).unwrap();
        let resp = handle_request(req, state).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn metrics_enabled_payload() {
        let state = Arc::new(test_state().await);
        let req = Request::builder().method(Method::GET).uri("/metrics").body(Body::empty()).unwrap();
        let resp = handle_request(req, state).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains("postnode_uptime_seconds"));
        assert!(body.contains("postnode_info"));
    }

    #[tokio::test]
    async fn health_live_ready_ok() {
        let state = Arc::new(test_state().await);
        let live_req = Request::builder().method(Method::GET).uri("/health/live").body(Body::empty()).unwrap();
        let live_resp = handle_request(live_req, state.clone()).await;
        assert_eq!(live_resp.status(), StatusCode::OK);
        let live_bytes = hyper::body::to_bytes(live_resp.into_body()).await.unwrap();
        let live_value: Value = serde_json::from_slice(&live_bytes).unwrap();
        assert_eq!(live_value.get("status").and_then(|v| v.as_str()), Some("alive"));

        let ready_req = Request::builder().method(Method::GET).uri("/health/ready").body(Body::empty()).unwrap();
        let ready_resp = handle_request(ready_req, state).await;
        assert_eq!(ready_resp.status(), StatusCode::OK);
        let ready_bytes = hyper::body::to_bytes(ready_resp.into_body()).await.unwrap();
        let ready_value: Value = serde_json::from_slice(&ready_bytes).unwrap();
        assert_eq!(ready_value.get("status").and_then(|v| v.as_str()), Some("ready"));
    }

    #[tokio::test]
    async fn health_ready_not_ready_returns_503() {
        let state = test_state().await;
        state.health.set_ready(false);
        let state = Arc::new(state);
        let ready_req = Request::builder().method(Method::GET).uri("/health/ready").body(Body::empty()).unwrap();
        let ready_resp = handle_request(ready_req, state).await;
        assert_eq!(ready_resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let ready_bytes = hyper::body::to_bytes(ready_resp.into_body()).await.unwrap();
        let ready_value: Value = serde_json::from_slice(&ready_bytes).unwrap();
        assert_eq!(ready_value.get("status").and_then(|v| v.as_str()), Some("not_ready"));
        assert!(ready_value.get("details").is_some());
    }

    #[tokio::test]
    async fn health_live_shutting_down_returns_503() {
        let state = test_state().await;
        state.health.set_shutting_down(true);
        let state = Arc::new(state);
        let live_req = Request::builder().method(Method::GET).uri("/health/live").body(Body::empty()).unwrap();
        let live_resp = handle_request(live_req, state).await;
        assert_eq!(live_resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let live_bytes = hyper::body::to_bytes(live_resp.into_body()).await.unwrap();
        let live_value: Value = serde_json::from_slice(&live_bytes).unwrap();
        assert_eq!(live_value.get("status").and_then(|v| v.as_str()), Some("dead"));
    }

    #[tokio::test]
    async fn backup_upload_round_trip() {
        let mut state = test_state().await;
        state.auth.admin_token_hash = Some(hash_token("token"));
        let state = Arc::new(state);

        let boundary = "----boundary";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"file\"; filename=\"backup.pusb\"\r\n");
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(b"backup");
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let req = Request::builder()
            .method(Method::POST)
            .uri("/admin/v1/backups/upload")
            .header(CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .header(hyper::header::AUTHORIZATION, "Bearer token")
            .body(Body::from(body))
            .unwrap();
        let resp = handle_request(req, state).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let entry: BackupListEntry = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(entry.size, 6);
        assert_eq!(entry.r#type, "full");
        assert!(std::fs::metadata(entry.path).is_ok());
    }

    #[tokio::test]
    async fn logs_query_returns_entries() {
        let mut state = test_state().await;
        state.auth.admin_token_hash = Some(hash_token("token"));
        let state = Arc::new(state);
        log_entry(
            &state,
            "info",
            "postnode::admin",
            "test log entry",
            Some(json!({"detail": "ok"})),
        )
        .await;

        let req = Request::builder()
            .method(Method::GET)
            .uri("/admin/v1/logs?limit=10")
            .header(hyper::header::AUTHORIZATION, "Bearer token")
            .body(Body::empty())
            .unwrap();
        let resp = handle_request(req, state).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let payload: LogsResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(!payload.entries.is_empty());
        assert_eq!(payload.entries[0].message, "test log entry");
    }

    #[tokio::test]
    async fn patch_settings_writes_audit_log() {
        let mut state = test_state().await;
        state.auth.admin_token_hash = Some(hash_token("token"));
        let state = Arc::new(state);
        let network = {
            let data = state.admin.data.lock().await;
            data.settings.network.clone()
        };
        let patch = json!({ "network": network });
        let req = Request::builder()
            .method(Method::PATCH)
            .uri("/admin/v1/settings")
            .header(hyper::header::AUTHORIZATION, "Bearer token")
            .body(Body::from(patch.to_string()))
            .unwrap();
        let resp = handle_request(req, state.clone()).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let data = state.admin.data.lock().await;
        let entry = data.logs.last().expect("log entry");
        assert_eq!(entry.message, "settings updated");
    }

    #[tokio::test]
    async fn patch_app_permissions_writes_audit_log() {
        let mut state = test_state().await;
        state.auth.admin_token_hash = Some(hash_token("token"));
        {
            let mut data = state.admin.data.lock().await;
            data.apps.push(InstalledApp {
                id: "com.example.app".to_string(),
                name: "Example".to_string(),
                version: "1.0.0".to_string(),
                author_iid: "k5xq7z4m".to_string(),
                author_name: None,
                description: "Example app".to_string(),
                icon: None,
                installed_at: Utc::now().to_rfc3339(),
                last_opened: None,
                update_available: None,
                status: crate::admin_types::AppStatus::Installed,
                permissions: crate::admin_types::AppPermissions {
                    granted: Vec::new(),
                    denied: Vec::new(),
                    pending: Vec::new(),
                },
                storage_used: 0,
                storage_quota: 1024,
            });
        }
        let state = Arc::new(state);
        let patch = json!({
            "grant": ["messaging:send"],
            "deny": ["storage:app"],
            "reset": ["contacts:read"]
        });
        let req = Request::builder()
            .method(Method::PATCH)
            .uri("/admin/v1/apps/com.example.app/permissions")
            .header(hyper::header::AUTHORIZATION, "Bearer token")
            .body(Body::from(patch.to_string()))
            .unwrap();
        let resp = handle_request(req, state.clone()).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let data = state.admin.data.lock().await;
        let entry = data.logs.last().expect("log entry");
        assert_eq!(entry.message, "app permissions updated");
    }

    #[tokio::test]
    async fn api_v1_aliases_admin_routes() {
        let state = test_state().await;
        let key = ApiKey {
            id: "key-1".to_string(),
            name: "api".to_string(),
            permissions: vec![Permission::ReadIdentity],
            created_at: Utc::now().to_rfc3339(),
            expires_at: None,
            last_used: None,
        };
        let record = ApiKeyRecord {
            key: key.clone(),
            key_hash: hash_token("secret"),
        };
        {
            let mut data = state.admin.data.lock().await;
            data.api_keys.push(record);
        }
        let state = Arc::new(state);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/identity")
            .header(hyper::header::AUTHORIZATION, "Bearer secret")
            .body(Body::empty())
            .unwrap();
        let resp = handle_request(req, state.clone()).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let payload: IdentityInfo = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload.iid, state.identity.iid().await);
    }

    #[tokio::test]
    async fn api_key_updates_last_used() {
        let state = test_state().await;
        let key = ApiKey {
            id: "key-1".to_string(),
            name: "api".to_string(),
            permissions: vec![Permission::ReadIdentity],
            created_at: Utc::now().to_rfc3339(),
            expires_at: None,
            last_used: None,
        };
        let record = ApiKeyRecord {
            key: key.clone(),
            key_hash: hash_token("secret"),
        };
        {
            let mut data = state.admin.data.lock().await;
            data.api_keys.push(record);
        }
        let state = Arc::new(state);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/admin/v1/identity")
            .header(hyper::header::AUTHORIZATION, "Bearer secret")
            .body(Body::empty())
            .unwrap();
        let resp = handle_request(req, state.clone()).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let data = state.admin.data.lock().await;
        let updated = data.api_keys.iter().find(|record| record.key.id == "key-1").unwrap();
        assert!(updated.key.last_used.is_some());
    }

    #[tokio::test]
    async fn app_api_missing_config_returns_bad_gateway() {
        let state = Arc::new(test_state().await);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/apps/com.example.app/api/test")
            .body(Body::empty())
            .unwrap();
        let resp = handle_request(req, state).await;
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}

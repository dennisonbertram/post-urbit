use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use hyper::{Body, Method, Request, Response, StatusCode};
use hyper::header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, SET_COOKIE};
use hyper::service::{make_service_fn, service_fn};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::admin_auth::{
    AuthConfig, create_session_cookie, generate_token_hex, hash_token, verify_password,
    verify_session_cookie,
};
use crate::admin_state::{AdminState, ApiKeyRecord, SessionRecord};
use crate::admin_types::{
    api_error, AddContactRequest, ApiErrorCode, ApiKey, BackupListEntry, BackupResult, Contact,
    ContactUpdate, CreateApiKeyRequest, CreateApiKeyResponse, Device, DeviceAddResult, IdentityInfo,
    InstalledApp, InstallRequest, InstallResult, LoginRequest, LoginResponse, LogsResponse,
    NodeStatus, PaginatedResult, Permission, PermissionPatch, PublicProfile, RestoreResult, Session,
    UpdateResult,
};
use crate::error::{PostUrbitError, Result};
use crate::app_store::{fetch_repository, install_package, parse_postapp, verify_package_with_dht, verify_repository, RepositoryManifest};
use crate::dht::Dht;
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
pub struct HttpServerState {
    pub admin: AdminState,
    pub auth: AuthConfig,
    pub identity: Arc<IdentityManager>,
    pub dht: Arc<dyn Dht + Send + Sync>,
    pub started_at: Instant,
    pub config: HttpServerConfig,
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

    if let Some(resp) = handle_public(&req, &path, &state).await {
        return resp;
    }

    if path.starts_with("/admin/v1/") {
        return handle_admin(req, &path, state).await;
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

async fn handle_public(req: &Request<Body>, path: &str, state: &HttpServerState) -> Option<Response<Body>> {
    match (req.method(), path) {
        (&Method::GET, "/health/live") => Some(json_response(Value::String("alive".to_string()))),
        (&Method::GET, "/health/ready") => Some(json_response(Value::String("ready".to_string()))),
        (&Method::GET, "/health") => {
            let status = Value::String("ok".to_string());
            Some(json_response(status))
        }
        (&Method::GET, "/metrics") => {
            if state.config.metrics_enabled {
                Some(Response::new(Body::from("post_urbit_up 1\n")))
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
            handle_delete_app(path, state).await
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
            handle_restart().await
        }
        (&Method::POST, "/admin/v1/shutdown") => {
            if let Err(resp) = require_permission(&auth, Permission::WriteSettings) { return resp; }
            handle_shutdown().await
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
                let data = state.admin.data.lock().await;
                if let Some(record) = data.api_keys.iter().find(|record| record.key_hash == token_hash) {
                    return Ok(AuthContext {
                        requires_csrf: false,
                        permissions: record.key.permissions.clone(),
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
    json_response(result)
}

async fn handle_rotate_encryption(state: Arc<HttpServerState>) -> Response<Body> {
    let result = match state.identity.rotate_encryption_key().await {
        Ok(result) => result,
        Err(_) => return api_error_response(ApiErrorCode::InternalError, "rotation failed", StatusCode::INTERNAL_SERVER_ERROR),
    };
    state.admin.data.lock().await.last_key_rotation = Some(result.rotated_at.clone());
    let _ = state.admin.persist().await;
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
    json_response(result)
}

async fn handle_remove_device(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let did = path.trim_start_matches("/admin/v1/devices/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.devices.iter().position(|device| device.did == did) {
        if data.devices[pos].is_current {
            return api_error_response(ApiErrorCode::Conflict, "cannot remove current device", StatusCode::CONFLICT);
        }
        data.devices.remove(pos);
        drop(data);
        let _ = state.admin.persist().await;
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
        return json_response(updated);
    }
    api_error_response(ApiErrorCode::NotFound, "contact not found", StatusCode::NOT_FOUND)
}

async fn handle_delete_contact(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let iid = path.trim_start_matches("/admin/v1/contacts/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.contacts.iter().position(|c| c.iid == iid) {
        data.contacts.remove(pos);
        drop(data);
        let _ = state.admin.persist().await;
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
    }
    let _ = state.admin.persist().await;
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
    }
    let _ = state.admin.persist().await;
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
    let mut data = state.admin.data.lock().await;
    if let Some(app) = data.apps.iter_mut().find(|app| app.id == app_id) {
        let previous = app.version.clone();
        app.version = app.version.clone();
        let updated = app.clone();
        drop(data);
        let _ = state.admin.persist().await;
        let result = UpdateResult {
            app: updated,
            previous_version: previous,
            new_permissions: Vec::new(),
        };
        return json_response(result);
    }
    api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND)
}

async fn handle_delete_app(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let app_id = path.trim_start_matches("/admin/v1/apps/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.apps.iter().position(|app| app.id == app_id) {
        data.apps.remove(pos);
        data.app_settings.remove(app_id);
        drop(data);
        let app_dir = state.apps_dir.join(app_id);
        let _ = std::fs::remove_dir_all(app_dir);
        let _ = state.admin.persist().await;
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
        if let Some(grant) = patch.grant { app.permissions.granted.extend(grant); }
        if let Some(deny) = patch.deny { app.permissions.denied.extend(deny); }
        if let Some(reset) = patch.reset {
            app.permissions.granted.retain(|cap| !reset.contains(cap));
            app.permissions.denied.retain(|cap| !reset.contains(cap));
        }
        let updated = app.permissions.clone();
        drop(data);
        let _ = state.admin.persist().await;
        return json_response(updated);
    }
    api_error_response(ApiErrorCode::NotFound, "app not found", StatusCode::NOT_FOUND)
}

async fn handle_clear_app_data(_path: &str, _state: Arc<HttpServerState>) -> Response<Body> {
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
    let mut data = state.admin.data.lock().await;
    let mut settings = data.settings.clone();
    merge_settings(&mut settings, &patch);
    data.settings = settings.clone();
    drop(data);
    let _ = state.admin.persist().await;
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

async fn handle_create_backup(_req: Request<Body>, state: Arc<HttpServerState>) -> Response<Body> {
    let backups_dir = state.admin.data_dir.join("backups");
    if tokio::fs::create_dir_all(&backups_dir).await.is_err() {
        return api_error_response(ApiErrorCode::InternalError, "backup directory error", StatusCode::INTERNAL_SERVER_ERROR);
    }
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
        r#type: "full".to_string(),
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
    json_response(result)
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
    json_response(response)
}

async fn handle_delete_api_key(path: &str, state: Arc<HttpServerState>) -> Response<Body> {
    let id = path.trim_start_matches("/admin/v1/api-keys/");
    let mut data = state.admin.data.lock().await;
    if let Some(pos) = data.api_keys.iter().position(|record| record.key.id == id) {
        data.api_keys.remove(pos);
        drop(data);
        let _ = state.admin.persist().await;
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
            data_used_bytes: 0,
            data_free_bytes: 0,
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

async fn handle_logs(req: Request<Body>, _state: Arc<HttpServerState>) -> Response<Body> {
    let query = req.uri().query().unwrap_or("");
    let params = parse_query(query);
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100usize);
    let entries = Vec::new();
    let response = LogsResponse {
        entries: entries.into_iter().take(limit).collect(),
        cursor: None,
        has_more: false,
    };
    json_response(response)
}

async fn handle_restart() -> Response<Body> {
    Response::builder().status(StatusCode::ACCEPTED).body(Body::empty()).unwrap()
}

async fn handle_shutdown() -> Response<Body> {
    Response::builder().status(StatusCode::ACCEPTED).body(Body::empty()).unwrap()
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
    drop(data);

    let Some(repo) = repo else {
        return Err(api_error_response(ApiErrorCode::NotFound, "repository not found", StatusCode::NOT_FOUND));
    };
    if repo.trust_level == "disabled" {
        return Err(api_error_response(ApiErrorCode::Forbidden, "repository disabled", StatusCode::FORBIDDEN));
    }
    let url = if repo.url.ends_with("repository.json") {
        repo.url
    } else {
        format!("{}/repository.json", repo.url.trim_end_matches('/'))
    };
    let manifest = fetch_repository(&url).await
        .map_err(|_| api_error_response(ApiErrorCode::InvalidRequest, "repository fetch failed", StatusCode::BAD_REQUEST))?;
    verify_repository(state.dht.as_ref(), &manifest).await
        .map_err(|_| api_error_response(ApiErrorCode::Forbidden, "repository verification failed", StatusCode::FORBIDDEN))?;
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
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(bytes))
            .unwrap();
    }

    Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap()
}

fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    let candidate = root.join(relative);
    if candidate.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        return None;
    }
    Some(candidate)
}

fn json_response<T: serde::Serialize>(payload: T) -> Response<Body> {
    let body = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(StatusCode::OK)
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
            started_at: Instant::now(),
            config: HttpServerConfig {
                metrics_enabled: true,
                max_request_body_bytes: 1024 * 1024,
                session_cookie_secure: false,
            },
            apps_dir,
        }
    }

    #[tokio::test]
    async fn health_ok() {
        let state = Arc::new(test_state().await);
        let req = Request::builder().method(Method::GET).uri("/health").body(Body::empty()).unwrap();
        let resp = handle_request(req, state).await;
        assert_eq!(resp.status(), StatusCode::OK);
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
}

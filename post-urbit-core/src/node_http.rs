use std::convert::Infallible;
use std::sync::Arc;

use hyper::{Body, Method, Request, Response, StatusCode};
use hyper::service::{make_service_fn, service_fn};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub metrics_enabled: bool,
    pub admin_token: Option<String>,
    pub csrf_token: Option<String>,
}

pub async fn run_http_server(addr: std::net::SocketAddr, cfg: HttpConfig) -> Result<()> {
    let cfg = Arc::new(cfg);
    let make = make_service_fn(move |_conn| {
        let cfg = cfg.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let cfg = cfg.clone();
                async move { Ok::<_, Infallible>(handle_request(req, cfg).await) }
            }))
        }
    });

    hyper::Server::bind(&addr)
        .serve(make)
        .await
        .map_err(|err| crate::error::PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

async fn handle_request(req: Request<Body>, cfg: Arc<HttpConfig>) -> Response<Body> {
    let path = req.uri().path();
    match (req.method(), path) {
        (&Method::GET, "/health") => Response::new(Body::from("ok")),
        (&Method::GET, "/metrics") => {
            if cfg.metrics_enabled {
                Response::new(Body::from("post_urbit_up 1\n"))
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::empty())
                    .unwrap()
            }
        }
        (&Method::GET, "/admin/identity") => {
            if !authorize(&req, &cfg) {
                return unauthorized();
            }
            Response::new(Body::from("{}"))
        }
        (&Method::GET, "/admin/apps") => {
            if !authorize(&req, &cfg) {
                return unauthorized();
            }
            Response::new(Body::from("[]"))
        }
        (&Method::POST, "/admin/apps/install") => {
            if !authorize(&req, &cfg) {
                return unauthorized();
            }
            if !csrf_ok(&req, &cfg) {
                return Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Body::empty())
                    .unwrap();
            }
            Response::new(Body::from("{}"))
        }
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    }
}

fn authorize(req: &Request<Body>, cfg: &HttpConfig) -> bool {
    let Some(token) = cfg.admin_token.as_ref() else {
        return true;
    };
    if let Some(auth) = req.headers().get(hyper::header::AUTHORIZATION) {
        if let Ok(value) = auth.to_str() {
            if value == format!("Bearer {token}") {
                return true;
            }
        }
    }
    if let Some(cookie) = req.headers().get(hyper::header::COOKIE) {
        if let Ok(value) = cookie.to_str() {
            for part in value.split(';') {
                let trimmed = part.trim();
                if let Some(rest) = trimmed.strip_prefix("session=") {
                    if rest == token {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn csrf_ok(req: &Request<Body>, cfg: &HttpConfig) -> bool {
    let Some(expected) = cfg.csrf_token.as_ref() else {
        return true;
    };
    req.headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == expected)
        .unwrap_or(false)
}

fn unauthorized() -> Response<Body> {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(Body::empty())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_ok() {
        let cfg = Arc::new(HttpConfig {
            metrics_enabled: true,
            admin_token: None,
            csrf_token: None,
        });
        let req = Request::builder().method(Method::GET).uri("/health").body(Body::empty()).unwrap();
        let resp = handle_request(req, cfg).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_disabled() {
        let cfg = Arc::new(HttpConfig {
            metrics_enabled: false,
            admin_token: None,
            csrf_token: None,
        });
        let req = Request::builder().method(Method::GET).uri("/metrics").body(Body::empty()).unwrap();
        let resp = handle_request(req, cfg).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn admin_requires_auth() {
        let cfg = Arc::new(HttpConfig {
            metrics_enabled: true,
            admin_token: Some("token".to_string()),
            csrf_token: None,
        });
        let req = Request::builder().method(Method::GET).uri("/admin/identity").body(Body::empty()).unwrap();
        let resp = handle_request(req, cfg).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

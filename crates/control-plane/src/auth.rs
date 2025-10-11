use axum::{extract::{Request, State}, http::{StatusCode, header}, middleware::Next, response::{Response, IntoResponse}};
use crate::error::ApiError;
 

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role { Admin, User }

#[derive(Clone, Debug)]
pub struct Identity { pub role: Role, pub subject: String }

fn is_auth_enabled() -> bool { std::env::var("AETHER_AUTH_ENABLED").unwrap_or_default() == "1" }

fn extract_bearer(req: &Request) -> Option<String> {
    let header = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len()==2 && parts[0].eq_ignore_ascii_case("Bearer") { Some(parts[1].trim().to_string()) } else { None }
}

// Constant-time equality
fn ct_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() { return false; }
    let mut diff: u8 = 0;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes()) { diff |= x ^ y; }
    diff == 0
}

async fn validate_env_token(token: &str) -> Option<Identity> {
    if let Ok(admin) = std::env::var("AETHER_ADMIN_TOKEN") {
        eprintln!("[auth] compare admin env len={} vs token len={}", admin.len(), token.len());
        if !admin.is_empty() && ct_equal(&admin, token) { return Some(Identity { role: Role::Admin, subject: "admin_env".into() }); }
    }
    if let Ok(user) = std::env::var("AETHER_USER_TOKEN") {
        eprintln!("[auth] compare user env '{}' vs token '{}'", &user, token);
        if !user.is_empty() && ct_equal(&user, token) { return Some(Identity { role: Role::User, subject: "user_env".into() }); }
    }
    None
}

async fn validate_db_token(db: &sqlx::Pool<sqlx::Postgres>, token: &str) -> Option<Identity> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let hex_hash = hex::encode(hasher.finalize());
    let row = sqlx::query_as::<_, (String,)>("SELECT role FROM users WHERE token_hash=$1").bind(&hex_hash).fetch_optional(db).await.ok()?;
    match row { Some((role_str,)) => {
        let role = if role_str.eq_ignore_ascii_case("admin") { Role::Admin } else { Role::User };
        Some(Identity { role, subject: "db_user".into() })
    }, None => None }
}

pub async fn auth_layer(State(db): State<sqlx::Pool<sqlx::Postgres>>, mut req: Request, next: Next) -> Result<Response, Response> {
    if !is_auth_enabled() {
        return Ok(next.run(req).await);
    }
    // Allow-list public endpoints regardless of auth
    let path = req.uri().path();
    if matches!(path, "/health" | "/readyz" | "/startupz" | "/metrics" | "/openapi.json" | "/swagger") {
        return Ok(next.run(req).await);
    }
    let hdr = req.headers().get(header::AUTHORIZATION).cloned();
    if let Some(h) = &hdr { eprintln!("[auth] authorization header present: {}", h.to_str().unwrap_or("<bad>")); } else { eprintln!("[auth] no authorization header"); }
    let Some(token) = extract_bearer(&req) else {
    tracing::debug!(%path, "auth_missing_bearer");
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "missing bearer token").into_response());
    };
    let mode = std::env::var("AETHER_AUTH_MODE").unwrap_or_else(|_| "env".into());
    if mode == "db" {
        tracing::debug!("auth_mode_db");
    } else {
        let a = std::env::var("AETHER_ADMIN_TOKEN").ok();
        let u = std::env::var("AETHER_USER_TOKEN").ok();
        eprintln!("[auth] env mode: admin? {} user? {}", a.is_some(), u.is_some());
        tracing::debug!(admin_token_set=%a.is_some(), user_token_set=%u.is_some(), "auth_mode_env");
    }
    eprintln!("[auth] extracted bearer len={} (starts {})", token.len(), &token.chars().take(5).collect::<String>());
    let ident = if mode == "db" { validate_db_token(&db, &token).await } else { validate_env_token(&token).await };
    let Some(identity) = ident else {
    tracing::debug!(%path, "auth_invalid_token");
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "invalid token").into_response());
    };
    req.extensions_mut().insert(identity);
    Ok(next.run(req).await)
}

// RBAC guard helper: enforce admin for mutating ops; GETs allowed for user
pub fn require_admin(identity: Option<&Identity>) -> Result<(), ApiError> {
    match identity { Some(id) => match id.role { Role::Admin => Ok(()), Role::User => Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden", "admin required")) }, None => Err(ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "missing identity")) }
}

// Middleware to enforce admin at route-level
pub async fn require_admin_mw(req: Request, next: Next) -> Result<Response, Response> {
    if !is_auth_enabled() {
        return Ok(next.run(req).await);
    }
    let identity = req.extensions().get::<Identity>();
    match require_admin(identity) {
        Ok(()) => Ok(next.run(req).await),
        Err(e) => Err(e.into_response()),
    }
}

use axum::{extract::Request, http::StatusCode, middleware::Next};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{warn, info};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
	Admin,
	Reader,
}

impl Role {
	pub fn from_str(s: &str) -> Option<Self> {
		match s {
			"admin" => Some(Role::Admin),
			"reader" => Some(Role::Reader),
			_ => None,
		}
	}
	pub fn as_str(&self) -> &'static str {
		match self { Role::Admin => "admin", Role::Reader => "reader" }
	}
	pub fn allows(&self, required: Role) -> bool {
		match (self, required) {
			(Role::Admin, _) => true,
			(Role::Reader, Role::Reader) => true,
			(Role::Reader, Role::Admin) => false,
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserContext {
	pub user_id: Uuid,
	pub role: Role,
	pub name: Option<String>,
	pub token_hash_hex: String, // for debugging prefix logging only
}

#[derive(Clone, Debug)]
pub struct UserInfo {
	pub role: Role,
	pub name: Option<String>,
	pub token_hash: [u8; 32],
	pub token_hash_hex: String,
}

#[derive(Clone, Debug)]
pub struct AuthStore {
	// sha256(token) -> UserInfo
	by_hash: HashMap<[u8; 32], UserInfo>,
	pub auth_required: bool,
}

impl AuthStore {
	pub fn empty() -> Self { Self { by_hash: HashMap::new(), auth_required: false } }
	pub fn from_env() -> Self {
	let tokens_env = std::env::var("AETHER_API_TOKENS").unwrap_or_default();
	// Only enable when explicitly requested to avoid surprising existing tests
	let required = std::env::var("AETHER_AUTH_REQUIRED").ok().map(|v| v == "1").unwrap_or(false);
		let mut by_hash = HashMap::new();
		for part in tokens_env.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
			// token:role[:name]
			let mut segs = part.split(':');
			let token = segs.next().unwrap_or("");
			let role_s = segs.next().unwrap_or("");
			let name = segs.next().map(|s| s.to_string());
			if token.is_empty() || role_s.is_empty() { continue; }
			let Some(role) = Role::from_str(role_s) else { continue; };
			let mut hasher = Sha256::new();
			hasher.update(token.as_bytes());
			let hash = hasher.finalize();
			let mut arr = [0u8; 32];
			arr.copy_from_slice(&hash);
			let hex_hash = hex::encode(arr);
			let info = UserInfo { role, name, token_hash: arr, token_hash_hex: hex_hash.clone() };
			by_hash.insert(arr, info);
		}
		Self { by_hash, auth_required: required }
	}
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
	if a.len() != b.len() { return false; }
	let mut diff: u8 = 0;
	for i in 0..a.len() { diff |= a[i] ^ b[i]; }
	diff == 0
}

pub fn is_auth_enabled(cfg: &AuthStore) -> bool {
	cfg.auth_required && !cfg.by_hash.is_empty()
}

pub async fn auth_middleware(mut req: Request, next: Next, store: Arc<AuthStore>) -> Result<axum::response::Response, axum::response::Response> {
	// Allow pass-through if not enabled
	if !is_auth_enabled(&store) { return Ok(next.run(req).await); }

	// Expect Authorization: Bearer <token>
	static UNAUTH_COUNT: AtomicUsize = AtomicUsize::new(0);
	let Some(val) = req.headers().get(axum::http::header::AUTHORIZATION) else {
		let c = UNAUTH_COUNT.fetch_add(1, Ordering::Relaxed);
		if c % 10 == 0 { warn!("auth.unauthorized.missing_header"); }
		return Err(axum::response::Response::builder().status(StatusCode::UNAUTHORIZED).body(axum::body::Body::empty()).unwrap());
	};
	let Ok(hdr) = val.to_str() else {
		let c = UNAUTH_COUNT.fetch_add(1, Ordering::Relaxed);
		if c % 10 == 0 { warn!("auth.unauthorized.bad_header"); }
		return Err(axum::response::Response::builder().status(StatusCode::UNAUTHORIZED).body(axum::body::Body::empty()).unwrap());
	};
	let prefix = "Bearer ";
	if !hdr.starts_with(prefix) {
		let c = UNAUTH_COUNT.fetch_add(1, Ordering::Relaxed);
		if c % 10 == 0 { warn!("auth.unauthorized.bad_schema"); }
		return Err(axum::response::Response::builder().status(StatusCode::UNAUTHORIZED).body(axum::body::Body::empty()).unwrap());
	}
	let token = &hdr[prefix.len()..];
	// Hash the token and lookup
	let mut hasher = Sha256::new();
	hasher.update(token.as_bytes());
	let hash = hasher.finalize();
	let mut arr = [0u8; 32];
	arr.copy_from_slice(&hash);
	if let Some(info) = store.by_hash.get(&arr) {
		// Constant-time confirmation (redundant as hash-length fixed, but good practice)
		if !ct_eq(&arr, &info.token_hash) {
			return Err(axum::response::Response::builder().status(StatusCode::UNAUTHORIZED).body(axum::body::Body::empty()).unwrap());
		}
	// Create stable user_id from sha256(token) first 16 bytes
	let hash = Sha256::digest(token.as_bytes());
	let mut b16 = [0u8; 16]; b16.copy_from_slice(&hash[..16]);
	let user_id = Uuid::from_bytes(b16);
		let ctx = UserContext { user_id, role: info.role, name: info.name.clone(), token_hash_hex: info.token_hash_hex.clone() };
		// Minimal logging without token
		let log_prefix = &ctx.token_hash_hex[..6.min(ctx.token_hash_hex.len())];
		tracing::debug!(role=%ctx.role.as_str(), hash_prefix=%log_prefix, "auth.ok");
	// Emit event with auth context (fields can be picked by subscriber)
		info!(user_role=%ctx.role.as_str(), user_name=%ctx.name.as_deref().unwrap_or("-"), auth_result="ok", "auth.context");
		req.extensions_mut().insert(ctx);
		Ok(next.run(req).await)
	} else {
		let short = &hex::encode(arr)[..6];
		warn!(hash_prefix=%short, "auth.fail.unknown_token");
		Err(axum::response::Response::builder().status(StatusCode::UNAUTHORIZED).body(axum::body::Body::empty()).unwrap())
	}
}

// Route-level RBAC guard; min_role enforced if auth is enabled; otherwise pass-through
pub async fn require_role(mut req: Request, next: Next, store: Arc<AuthStore>, min_role: Role) -> Result<axum::response::Response, axum::response::Response> {
	if !is_auth_enabled(&store) { return Ok(next.run(req).await); }
	if let Some(ctx) = req.extensions().get::<UserContext>() {
		if ctx.role.allows(min_role) { return Ok(next.run(req).await); }
		info!(user_role=%ctx.role.as_str(), user_name=%ctx.name.as_deref().unwrap_or("-"), auth_result="forbidden", "auth.rbac");
		return Err(axum::response::Response::builder().status(StatusCode::FORBIDDEN).body(axum::body::Body::empty()).unwrap());
	}
	warn!("auth.unauthorized.missing_context");
	Err(axum::response::Response::builder().status(StatusCode::UNAUTHORIZED).body(axum::body::Body::empty()).unwrap())
}

// Note: layer builders are created inline via axum::middleware::from_fn_with_state in lib.rs


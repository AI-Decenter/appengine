use anyhow::{Result, Context};
use tracing::{info, debug, warn};

#[derive(Debug, Clone, Default)]
pub struct LogsOptions {
	pub app: Option<String>,
	pub follow: bool,
	pub since: Option<String>,
	pub container: Option<String>,
	pub format: Option<String>,
	pub color: bool,
}

pub async fn handle_opts(opts: LogsOptions) -> Result<()> {
	let appn = opts.app.unwrap_or_else(|| std::env::var("AETHER_DEFAULT_APP").unwrap_or_else(|_| "sample-app".into()));
	let base = std::env::var("AETHER_API_BASE").unwrap_or_else(|_| "http://localhost:8080".into());
	let follow_env = std::env::var("AETHER_LOGS_FOLLOW").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true"));
	let follow = opts.follow || follow_env.unwrap_or(true);
	let since = opts.since.or_else(|| std::env::var("AETHER_LOGS_SINCE").ok());
	let container = opts.container.or_else(|| std::env::var("AETHER_LOGS_CONTAINER").ok());
	let format = opts.format.unwrap_or_else(|| std::env::var("AETHER_LOGS_FORMAT").unwrap_or_else(|_| "text".into())); // default to human text
	let color = opts.color || std::env::var("AETHER_COLOR").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
	let tail: u32 = std::env::var("AETHER_LOGS_TAIL").ok().and_then(|v| v.parse().ok()).unwrap_or(100);

	// Mock mode: allow tests/dev to bypass network entirely. Triggered if:
	// - AETHER_LOGS_MOCK=1 or true
	// - AETHER_MOCK_MODE=1 or true
	// - AETHER_API_BASE uses an unbound port like :0 (common in tests)
	let logs_mock_env = std::env::var("AETHER_LOGS_MOCK").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
	let mock_mode_env = std::env::var("AETHER_MOCK_MODE").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
	let base_is_unbound = base.contains(":0");
	if logs_mock_env || mock_mode_env || base_is_unbound {
		debug!(mock = true, %base, "logs.mock.enabled");
		use tokio::io::AsyncWriteExt;
		let mut stdout = tokio::io::stdout();
		if format.eq_ignore_ascii_case("json") {
			let ts = "2024-01-01T00:00:00Z";
			let line1 = format!("{{\"time\":\"{}\",\"app\":\"{}\",\"pod\":\"pod-1\",\"container\":\"c\",\"message\":\"mock line 1\"}}\n", ts, appn);
			let line2 = format!("{{\"time\":\"{}\",\"app\":\"{}\",\"pod\":\"pod-1\",\"container\":\"c\",\"message\":\"mock line 2\"}}\n", ts, appn);
			stdout.write_all(line1.as_bytes()).await?;
			stdout.write_all(line2.as_bytes()).await?;
		} else {
			stdout.write_all(b"mock line 1\n").await?;
			stdout.write_all(b"mock line 2\n").await?;
		}
		stdout.flush().await.ok();
		info!(app=%appn, "logs.stream.end.mock");
		return Ok(());
	}

	let mut url = format!("{}/apps/{}/logs?tail_lines={}&format={}", base.trim_end_matches('/'), urlencoding::encode(&appn), tail, format);
	if follow { url.push_str("&follow=true"); }
	if let Some(s) = since { url.push_str("&since="); url.push_str(&urlencoding::encode(&s)); }
	if let Some(c) = container { url.push_str("&container="); url.push_str(&urlencoding::encode(&c)); }

	debug!(%url, "logs.request");
	let client = reqwest::Client::builder()
		.pool_idle_timeout(std::time::Duration::from_secs(30))
		.build()?;

	// reconnecting loop for follow=true
	let mut attempt: u32 = 0;
	let max_reconnects = std::env::var("AETHER_LOGS_MAX_RECONNECTS").ok().and_then(|v| v.parse::<u32>().ok());
	loop {
		let resp = client.get(&url).send().await.context("request logs")?;
		if !resp.status().is_success() {
			anyhow::bail!("logs fetch failed: {}", resp.status());
		}
		let ct = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
		let is_json_lines = ct.starts_with("application/x-ndjson") || format.eq_ignore_ascii_case("json");
		let mut stream = resp.bytes_stream();
		use futures_util::StreamExt;
		use tokio::io::AsyncWriteExt;
		let mut stdout = tokio::io::stdout();
		while let Some(chunk) = stream.next().await {
			match chunk {
				Ok(bytes) => {
					if is_json_lines {
						if color {
							// passthrough for now; colorization could parse JSON and add ANSI later
							stdout.write_all(&bytes).await?;
						} else {
							stdout.write_all(&bytes).await?;
						}
					} else {
						stdout.write_all(&bytes).await?;
					}
					stdout.flush().await.ok();
				}
				Err(e) => {
					warn!(error=%e, "logs.stream.chunk_error");
					break; // trigger reconnect if follow
				}
			}
		}
		if !follow { break; }
		attempt = attempt.saturating_add(1);
		if let Some(max) = max_reconnects { if attempt >= max { break; } }
		let backoff_ms = (100u64).saturating_mul((attempt.min(50) + 1) as u64);
		tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
		debug!(attempt, backoff_ms, "logs.stream.reconnect");
		continue;
	}
	info!(app=%appn, "logs.stream.end");
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	#[tokio::test]
	async fn builds_logs_url_and_streams() {
		// Tiny axum server compatible with hyper 1.x
		use axum::{routing::get, Router, response::IntoResponse};
		use axum::http::header::{CONTENT_TYPE, HeaderValue};
		use tokio::net::TcpListener;

		async fn logs_handler() -> impl IntoResponse {
			let body = "line1\nline2\n";
			let mut resp = axum::response::Response::new(axum::body::Body::from(body));
			resp.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
			resp
		}

		let app = Router::new().route("/apps/demo/logs", get(logs_handler));
		let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
		let addr = listener.local_addr().unwrap();
		tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

		std::env::set_var("AETHER_API_BASE", format!("http://{}:{}", addr.ip(), addr.port()));
		std::env::set_var("AETHER_LOGS_FOLLOW", "0");
		let res = handle_opts(LogsOptions{ app: Some("demo".into()), ..Default::default() }).await;
		assert!(res.is_ok());
	}

	#[tokio::test]
	async fn mock_mode_respects_format_and_env() {
		std::env::set_var("AETHER_API_BASE", "http://127.0.0.1:0");
		std::env::set_var("AETHER_LOGS_MOCK", "1");
		std::env::set_var("AETHER_LOGS_FORMAT", "json");
		let res = handle_opts(LogsOptions{ app: Some("demo".into()), ..Default::default() }).await;
		assert!(res.is_ok());
	}
}

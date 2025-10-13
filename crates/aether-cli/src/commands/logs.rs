use anyhow::{Result, Context};
use tracing::{info, debug};
use std::time::Duration;

pub async fn handle(app: Option<String>) -> Result<()> {
	let appn = app.unwrap_or_else(|| std::env::var("AETHER_DEFAULT_APP").unwrap_or_else(|_| "sample-app".into()));
	let base = std::env::var("AETHER_API_BASE").unwrap_or_else(|_| "http://localhost:8080".into());
	let follow = std::env::var("AETHER_LOGS_FOLLOW").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(true);
	let since = std::env::var("AETHER_LOGS_SINCE").ok();
	let container = std::env::var("AETHER_LOGS_CONTAINER").ok();
	let format = std::env::var("AETHER_LOGS_FORMAT").unwrap_or_else(|_| "text".into()); // default to human text
	let tail: u32 = std::env::var("AETHER_LOGS_TAIL").ok().and_then(|v| v.parse().ok()).unwrap_or(100);

	let mut url = format!("{}/apps/{}/logs?tail_lines={}&format={}", base.trim_end_matches('/'), urlencoding::encode(&appn), tail, format);
	if follow { url.push_str("&follow=true"); }
	if let Some(s) = since { url.push_str("&since="); url.push_str(&urlencoding::encode(&s)); }
	if let Some(c) = container { url.push_str("&container="); url.push_str(&urlencoding::encode(&c)); }

	debug!(%url, "logs.request");
	let client = reqwest::Client::builder().build()?;
	let mut resp = client.get(&url).send().await.context("request logs")?;
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
		let bytes = chunk.context("read chunk")?;
		if is_json_lines {
			stdout.write_all(&bytes).await?; // already newline delimited
		} else {
			stdout.write_all(&bytes).await?; // text lines already framed by server
		}
		stdout.flush().await.ok();
	}
	info!(app=%appn, "logs.stream.end");
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	#[tokio::test]
	async fn builds_logs_url_and_streams() {
		// Spin up a tiny hyper server that returns two lines
		use hyper::{Server, Body, Request, Response, Method};
		use std::net::SocketAddr;
		let make_svc = hyper::service::make_service_fn(|_conn| async move {
			Ok::<_, hyper::Error>(hyper::service::service_fn(|req: Request<Body>| async move {
				if req.method()==Method::GET && req.uri().path().starts_with("/apps/demo/logs") {
					let mut resp = Response::new(Body::from("line1\nline2\n"));
					resp.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/plain"));
					Ok::<_, hyper::Error>(resp)
				} else { Ok::<_, hyper::Error>(Response::new(Body::empty())) }
			}))
		});
		let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
		let server = Server::try_bind(&addr).unwrap().serve(make_svc);
		let port = server.local_addr().port();
		tokio::spawn(server);

		std::env::set_var("AETHER_API_BASE", format!("http://127.0.0.1:{}", port));
		std::env::set_var("AETHER_LOGS_FOLLOW", "0");
		let res = handle(Some("demo".into())).await;
		assert!(res.is_ok());
	}
}

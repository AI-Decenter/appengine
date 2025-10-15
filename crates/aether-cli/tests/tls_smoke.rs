use std::time::Duration;

#[tokio::test]
async fn tls_smoke_https_request_optional() {
    // Only run when explicitly enabled to avoid flaky external network in CI
    if std::env::var("AETHER_TLS_SMOKE").ok().as_deref() != Some("1") {
        eprintln!("[skip] Set AETHER_TLS_SMOKE=1 to run TLS smoke test");
        return;
    }

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client");

    // A stable endpoint over HTTPS. We accept 200..399 to accommodate redirects.
    let resp = client
        .get("https://example.com/")
        .send()
        .await
        .expect("https request should succeed");
    assert!(resp.status().is_success() || resp.status().is_redirection());
}

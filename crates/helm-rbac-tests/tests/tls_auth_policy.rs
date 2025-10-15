use anyhow::Result;
use std::fs;
use std::path::PathBuf;

fn app_root() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent().unwrap().parent().unwrap().to_path_buf()
}

#[test]
fn helm_values_support_tls() -> Result<()> {
    let root = app_root();
    let values = root.join("charts/control-plane/values.yaml");
    let s = fs::read_to_string(&values)?;
    assert!(s.contains("tls:"), "values.yaml must have a tls: section");
    assert!(s.contains("enabled:"), "tls.enabled must be configurable");
    Ok(())
}

#[test]
fn ingress_template_supports_tls() -> Result<()> {
    let root = app_root();
    let ingress = root.join("charts/control-plane/templates/ingress.yaml");
    let s = fs::read_to_string(&ingress)?;
    assert!(s.contains("tls:"), "Ingress template must have a tls: block");
    Ok(())
}

#[test]
fn docs_exist_for_cert_generation() -> Result<()> {
    let root = app_root();
    let tls_doc = root.join("docs/helm/tls.md");
    assert!(tls_doc.exists(), "docs/helm/tls.md must exist");
    let s = fs::read_to_string(&tls_doc)?;
    assert!(s.contains("self-signed") || s.contains("openssl"), "tls.md must mention self-signed or openssl");
    Ok(())
}

#[test]
fn helm_values_support_token_rotation_and_scopes() -> Result<()> {
    let root = app_root();
    let values = root.join("charts/control-plane/values.yaml");
    let s = fs::read_to_string(&values)?;
    assert!(s.contains("tokens:"), "values.yaml must have a tokens: section");
    assert!(s.contains("rotation:"), "tokens.rotation must be configurable");
    assert!(s.contains("scopes:"), "tokens.scopes must be configurable");
    Ok(())
}

#[test]
fn cors_config_and_tests_exist() -> Result<()> {
    let root = app_root();
    let values = root.join("charts/control-plane/values.yaml");
    let s = fs::read_to_string(&values)?;
    assert!(s.contains("cors:"), "values.yaml must have a cors: section");
    assert!(s.contains("allowedOrigins:"), "cors.allowedOrigins must be configurable");
    // Check for test file with 401/403 cases
    let test_file = root.join("crates/control-plane/tests/auth_policy.rs");
    assert!(test_file.exists(), "crates/control-plane/tests/auth_policy.rs must exist");
    let test_src = fs::read_to_string(&test_file)?;
    assert!(test_src.contains("401") && test_src.contains("403"), "auth_policy.rs must test 401/403 responses");
    Ok(())
}

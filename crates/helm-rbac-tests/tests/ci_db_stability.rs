use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

fn app_root() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent().unwrap().parent().unwrap().to_path_buf()
}

#[test]
fn makefile_has_test_ci_target() -> Result<()> {
    let root = app_root();
    let mk = root.join("Makefile");
    let s = fs::read_to_string(&mk).with_context(|| mk.display().to_string())?;
    assert!(s.contains("\ntest-ci:"), "Makefile must define a test-ci target");
    Ok(())
}

#[test]
fn ci_workflow_has_db_matrix_and_modes() -> Result<()> {
    let root = app_root();
    let ci = root.join(".github/workflows/ci.yml");
    let s = fs::read_to_string(&ci).with_context(|| ci.display().to_string())?;
    // Expect a db matrix with both modes referenced
    assert!(s.contains("matrix") && s.contains("db:"), "CI should define a matrix over db modes");
    assert!(s.contains("testcontainers"), "CI matrix should include 'testcontainers' mode");
    assert!(s.contains("service"), "CI matrix should include 'service' mode");
    // Expect conditional steps for each mode
    assert!(s.contains("if: ${{ matrix.db == 'testcontainers' }}") || s.contains("if: matrix.db == 'testcontainers'"),
        "CI should have conditional steps for testcontainers mode");
    assert!(s.contains("if: ${{ matrix.db == 'service' }}") || s.contains("if: matrix.db == 'service'"),
        "CI should have conditional steps for service mode");
    // In testcontainers mode ensure we force the harness and unset DATABASE_URL to exercise that path
    assert!(s.contains("AETHER_FORCE_TESTCONTAINERS=1"), "CI must set AETHER_FORCE_TESTCONTAINERS=1 for testcontainers mode");
    assert!(s.contains("unset DATABASE_URL") || s.contains("DATABASE_URL: ''"), "CI should unset/omit DATABASE_URL in testcontainers mode");
    Ok(())
}

#[test]
fn harness_has_retry_and_env_logic() -> Result<()> {
    let root = app_root();
    let ts = root.join("crates/control-plane/src/test_support.rs");
    let s = fs::read_to_string(&ts).with_context(|| ts.display().to_string())?;
    assert!(s.contains("AETHER_FORCE_TESTCONTAINERS"), "Harness should support forcing testcontainers via env");
    // Retry guards should recognize PoolTimedOut (to reduce flakiness under CI contention)
    assert!(s.contains("PoolTimedOut"), "Harness should mention PoolTimedOut in retry/guard logic");
    Ok(())
}

use aether_cli::config::{EffectiveConfig, config_file_path};
use std::io::Write;
use tempfile::TempDir;

fn write_cfg(dir: &TempDir, content: Option<&str>) {
    std::env::set_var("XDG_CONFIG_HOME", dir.path());
    let path = config_file_path();
    if let Some(c) = content {
        if let Some(p) = path.parent() { std::fs::create_dir_all(p).unwrap(); }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(c.as_bytes()).unwrap();
    } else if path.exists() { std::fs::remove_file(&path).unwrap(); }
}

#[test]
fn config_load_branches() {
    // Missing file
    let td1 = TempDir::new().unwrap();
    write_cfg(&td1, None);
    std::env::remove_var("AETHER_DEFAULT_NAMESPACE");
    let cfg = EffectiveConfig::load().unwrap();
    assert!(cfg.default_namespace.is_none());

    // Parse error
    let td2 = TempDir::new().unwrap();
    write_cfg(&td2, Some("default_namespace = [unclosed"));
    std::env::remove_var("AETHER_DEFAULT_NAMESPACE");
    let err = EffectiveConfig::load().unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("failed to parse config"), "expected parse error, got: {s}");

    // Valid file
    let td3 = TempDir::new().unwrap();
    write_cfg(&td3, Some("default_namespace='teamspace'"));
    std::env::remove_var("AETHER_DEFAULT_NAMESPACE");
    let cfg = EffectiveConfig::load().unwrap();
    assert_eq!(cfg.default_namespace.as_deref(), Some("teamspace"));

    // Env override
    let td4 = TempDir::new().unwrap();
    write_cfg(&td4, Some("default_namespace='ns1'"));
    std::env::set_var("AETHER_DEFAULT_NAMESPACE", "override_ns");
    let cfg = EffectiveConfig::load().unwrap();
    assert_eq!(cfg.default_namespace.as_deref(), Some("override_ns"));
}

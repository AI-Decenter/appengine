use anyhow::{Context, Result};
use serde::Deserialize;
use serde_yaml::Value;
use std::fs;
use std::path::PathBuf;

fn app_root() -> PathBuf {
    // appengine root is two levels up from this crate
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent().unwrap().parent().unwrap().to_path_buf()
}

#[test]
fn chart_structure_exists() -> Result<()> {
    let root = app_root();
    let chart = root.join("charts/control-plane");
    assert!(chart.exists(), "expected chart dir at {}", chart.display());
    for f in ["Chart.yaml", "values.yaml"] {
        assert!(chart.join(f).exists(), "missing {}", f);
    }
    let templates = chart.join("templates");
    assert!(templates.exists(), "templates dir missing");
    // required templates per spec
    for f in [
        "deployment.yaml",
        "service.yaml",
        "configmap.yaml",
        "secret.yaml",
        "serviceaccount.yaml",
        "role.yaml",
        "rolebinding.yaml",
    ] {
        assert!(templates.join(f).exists(), "template {} missing", f);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ChartYaml {
    apiVersion: String,
    name: String,
    version: String,
}

#[test]
fn chart_yaml_valid() -> Result<()> {
    let root = app_root();
    let chart_path = root.join("charts/control-plane/Chart.yaml");
    let s = fs::read_to_string(&chart_path).with_context(|| chart_path.display().to_string())?;
    let chart: ChartYaml = serde_yaml::from_str(&s)?;
    assert!(chart.apiVersion.starts_with("v2"), "apiVersion must be v2*");
    assert_eq!(chart.name, "control-plane");
    // semver-ish
    assert!(chart.version.split('.').count() >= 2);
    Ok(())
}

#[test]
fn values_yaml_contains_expected_keys() -> Result<()> {
    let root = app_root();
    let values_path = root.join("charts/control-plane/values.yaml");
    let v: Value = serde_yaml::from_str(&fs::read_to_string(&values_path)?)?;
    // required tree
    for key in ["image", "env", "service", "ingress", "rbac", "resources"] {
        assert!(v.get(key).is_some(), "missing values key: {}", key);
    }
    // env contains DATABASE_URL and tokens structure
    let env = v.get("env").unwrap();
    assert!(env.get("DATABASE_URL").is_some(), "env.DATABASE_URL required (can be null)");
    assert!(env.get("TOKENS").is_some(), "env.TOKENS required (string)");
    Ok(())
}

#[test]
fn rbac_manifests_have_right_scopes() -> Result<()> {
    // read role.yaml and ensure rules allow get/watch/list on pods and logs, annotations
    let root = app_root();
    let role_path = root.join("charts/control-plane/templates/role.yaml");
    let s = fs::read_to_string(&role_path)?;
    // It may be a template, but should render these resources/rules strings
    let must_have = [
        "apiGroups: ['']",
        "resources: ['pods', 'pods/log']",
        "verbs: ['get', 'list', 'watch']",
    ];
    for needle in must_have.iter() {
        assert!(s.contains(needle), "role.yaml should contain: {}", needle);
    }
    // RoleBinding should reference ServiceAccount aether-dev-hot
    let rb_path = root.join("charts/control-plane/templates/rolebinding.yaml");
    let rb_s = fs::read_to_string(&rb_path)?;
    assert!(rb_s.contains("name: aether-dev-hot"), "rolebinding binds SA aether-dev-hot");
    Ok(())
}

#[test]
fn makefile_has_helm_targets() -> Result<()> {
    let root = app_root();
    let mk_path = root.join("Makefile");
    let s = fs::read_to_string(&mk_path)?;
    assert!(s.contains("helm-lint"), "Makefile must have helm-lint target");
    assert!(s.contains("helm-template"), "Makefile must have helm-template target");
    Ok(())
}

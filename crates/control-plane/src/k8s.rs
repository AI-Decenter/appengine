use anyhow::Result;
#[cfg(not(feature = "mock-kube"))]
use k8s_openapi::api::apps::v1::Deployment;
#[cfg(not(feature = "mock-kube"))]
use kube::{Api, Client, api::{PatchParams, Patch}};
use serde_json::json;

#[cfg(feature = "mock-kube")]
pub async fn apply_deployment(app: &str, digest: &str, artifact_url: &str, namespace: &str, signature: Option<&str>) -> Result<()> {
    // Simulate success for integration tests
    tracing::info!(app, digest, artifact_url, namespace, signature=?signature, "[mock-kube] apply_deployment called");
    Ok(())
}

/// Apply (create or replace) a Kubernetes Deployment for an application + artifact digest.
/// Strategy: name = app name, annotation carries digest for idempotency / change triggers.
#[cfg(not(feature = "mock-kube"))]
pub async fn apply_deployment(app: &str, digest: &str, artifact_url: &str, namespace: &str, signature: Option<&str>) -> Result<()> {
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::namespaced(client, namespace);
    let name = app;
    // Build desired deployment manifest
    let desired = build_deployment_manifest(app, digest, artifact_url, namespace, signature);
    match api.get(name).await {
        Ok(_) => {
            // Server-side apply style patch to minimize diff churn
            let patch = Patch::Apply(&desired);
            let params = PatchParams::apply("aether-control-plane").force();
            api.patch(name, &params, &patch).await?;
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            let patch = Patch::Apply(&desired);
            let params = PatchParams::apply("aether-control-plane").force();
            api.patch(name, &params, &patch).await?; // create via apply
        }
        Err(e) => return Err(e.into())
    }
    Ok(())
}

#[allow(dead_code)] // used in tests & runtime when k8s feature active
fn build_deployment_manifest(app: &str, digest: &str, artifact_url: &str, namespace: &str, signature: Option<&str>) -> serde_json::Value {
    // We construct JSON for server-side apply; using structured types for full compile checks would be more verbose.
    // init container: busybox sh -c "wget/curl artifact && tar -xzf ..."
    // For PoC use wget in busybox; production could switch to distroless + sha256 verify.
    let valid_digest = digest.len()==64 && digest.chars().all(|c| c.is_ascii_hexdigit());
    let mut annotations = json!({"aether.dev/artifact-url": artifact_url});
    if valid_digest { annotations["aether.dev/digest"] = json!(format!("sha256:{digest}")); }
    if signature.is_some() { annotations["aether.dev/signature"] = json!("ed25519"); }
    let labels = json!({"app": app, "app_name": app});
    // Build init command with optional sha256 verification
    let mut init_cmd = format!("set -euo pipefail; echo Fetching artifact; wget -O /workspace/app.tar.gz {artifact_url};");
    if valid_digest { init_cmd.push_str(&format!(" echo '{digest}  /workspace/app.tar.gz' | sha256sum -c -;")); }
    // Signature gating is logically performed server-side (control-plane) before apply. Here we only surface env + annotation.
    init_cmd.push_str(" tar -xzf /workspace/app.tar.gz -C /workspace");
    // Build env array separately to avoid complex inline code in json! macro
    let mut envs: Vec<serde_json::Value> = Vec::new();
    if valid_digest { envs.push(json!({"name":"AETHER_DIGEST","value": format!("sha256:{digest}")})); }
    if let Some(sig) = signature { envs.push(json!({"name":"AETHER_SIGNATURE","value": sig})); }
    json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": app,
            "namespace": namespace,
            "labels": labels,
            "annotations": annotations
        },
        "spec": {
            "replicas": 1,
            "selector": {"matchLabels": {"app": app}},
            "template": {
                "metadata": {"labels": {"app": app, "app_name": app}},
                "spec": {
                    "volumes": [ {"name": "workspace", "emptyDir": {} } ],
                    "initContainers": [
                        {
                            "name": "fetch-artifact",
                            "image": "busybox:1.36",
                            "command": ["/bin/sh","-c"],
                            "args": [init_cmd],
                            "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ]
                        }
                    ],
                    "containers": [{
                        "name": "app",
                        "image": "aether-nodejs:20-slim",
                        "workingDir": "/workspace",
                        "command": ["node","server.js"],
                        "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ],
                        "env": envs,
                    }]
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::build_deployment_manifest;
    #[test]
    fn manifest_contains_annotation() {
        let v = build_deployment_manifest("demo","0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","https://example/artifact.tar.gz","default", None);
        assert!(v["metadata"]["annotations"]["aether.dev/digest"].as_str().unwrap().starts_with("sha256:"));
    }
}

use anyhow::Result;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{Api, Client, api::{PatchParams, Patch}};
use serde_json::json;

/// Apply (create or replace) a Kubernetes Deployment for an application + artifact digest.
/// Strategy: name = app name, annotation carries digest for idempotency / change triggers.
pub async fn apply_deployment(app: &str, digest: &str, artifact_url: &str, namespace: &str) -> Result<()> {
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::namespaced(client, namespace);
    let name = app;
    // Build desired deployment manifest
    let desired = build_deployment_manifest(app, digest, artifact_url, namespace);
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

fn build_deployment_manifest(app: &str, digest: &str, artifact_url: &str, namespace: &str) -> serde_json::Value {
    // We construct JSON for server-side apply; using structured types for full compile checks would be more verbose.
    // init container: busybox sh -c "wget/curl artifact && tar -xzf ..."
    // For PoC use wget in busybox; production could switch to distroless + sha256 verify.
    json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": app,
            "namespace": namespace,
            "labels": {"app": app},
            "annotations": {"aether.dev/digest": digest, "aether.dev/artifact-url": artifact_url}
        },
        "spec": {
            "replicas": 1,
            "selector": {"matchLabels": {"app": app}},
            "template": {
                "metadata": {"labels": {"app": app}},
                "spec": {
                    "volumes": [ {"name": "workspace", "emptyDir": {} } ],
                    "initContainers": [
                        {
                            "name": "fetch-artifact",
                            "image": "busybox:1.36",
                            "command": ["/bin/sh","-c"],
                            "args": [format!("set -euo pipefail; echo Fetching artifact; wget -O /workspace/app.tar.gz {artifact_url} && tar -xzf /workspace/app.tar.gz -C /workspace")],
                            "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ]
                        }
                    ],
                    "containers": [
                        {
                            "name": "app",
                            "image": "aether-nodejs:20-slim",
                            "workingDir": "/workspace",
                            "command": ["node","server.js"],
                            "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ],
                            "env": [ {"name": "AETHER_DIGEST", "value": digest } ]
                        }
                    ]
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
        let v = build_deployment_manifest("demo","sha256:abc","https://example/artifact.tar.gz","default");
        assert_eq!(v["metadata"]["annotations"]["aether.dev/digest"], "sha256:abc");
    }
}

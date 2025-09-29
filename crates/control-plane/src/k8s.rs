use anyhow::Result;
#[cfg(not(feature = "mock-kube"))]
use k8s_openapi::api::apps::v1::Deployment;
#[cfg(not(feature = "mock-kube"))]
use kube::{Api, Client, api::{PatchParams, Patch}};
use serde_json::json;

#[cfg(feature = "mock-kube")]
pub async fn apply_deployment(app: &str, digest: &str, artifact_url: &str, namespace: &str, signature: Option<&str>, dev_hot: bool) -> Result<()> {
    // Simulate success for integration tests
    tracing::info!(app, digest, artifact_url, namespace, signature=?signature, dev_hot, "[mock-kube] apply_deployment called");
    Ok(())
}

/// Apply (create or replace) a Kubernetes Deployment for an application + artifact digest.
/// Strategy: name = app name, annotation carries digest for idempotency / change triggers.
#[cfg(not(feature = "mock-kube"))]
pub async fn apply_deployment(app: &str, digest: &str, artifact_url: &str, namespace: &str, signature: Option<&str>, dev_hot: bool) -> Result<()> {
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::namespaced(client, namespace);
    let name = app;
    // Build desired deployment manifest
    let desired = build_deployment_manifest(app, digest, artifact_url, namespace, signature, dev_hot);
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
fn build_deployment_manifest(app: &str, digest: &str, artifact_url: &str, namespace: &str, signature: Option<&str>, dev_hot: bool) -> serde_json::Value {
    // We construct JSON for server-side apply; using structured types for full compile checks would be more verbose.
    // init container: busybox sh -c "wget/curl artifact && tar -xzf ..."
    // For PoC use wget in busybox; production could switch to distroless + sha256 verify.
    let valid_digest = digest.len()==64 && digest.chars().all(|c| c.is_ascii_hexdigit());
    let mut annotations = json!({"aether.dev/artifact-url": artifact_url});
    if valid_digest { annotations["aether.dev/digest"] = json!(format!("sha256:{digest}")); }
    if signature.is_some() { annotations["aether.dev/signature"] = json!("ed25519"); }
    if dev_hot { annotations["aether.dev/dev-hot"] = json!("true"); }
    let labels = json!({"app": app, "app_name": app});
    // Build env array separately to avoid complex inline code in json! macro
    let mut envs: Vec<serde_json::Value> = Vec::new();
    if valid_digest { envs.push(json!({"name":"AETHER_DIGEST","value": format!("sha256:{digest}")})); }
    if let Some(sig) = signature { envs.push(json!({"name":"AETHER_SIGNATURE","value": sig})); }
    if dev_hot { envs.push(json!({"name":"AETHER_DEV_HOT","value": "true"})); }

    // Containers differ if dev_hot enabled: add fetcher sidecar polling pod annotations for new digest
    let (init_containers, containers) = if dev_hot {
                let fetch_script = r#"set -euo pipefail
# Standardized dev-hot log markers for external metrics tailing:
# REFRESH_OK app=<pod> digest=<digest>
# REFRESH_FAIL app=<pod> reason=<reason>
API="https://${KUBERNETES_SERVICE_HOST}:${KUBERNETES_SERVICE_PORT}"
TOKEN=$(cat /var/run/secrets/kubernetes.io/serviceaccount/token)
NS=$(cat /var/run/secrets/kubernetes.io/serviceaccount/namespace)
POD=$(hostname)
CUR=""
INTERVAL="${AETHER_FETCH_INTERVAL_SEC:-5}"
echo "[fetcher] dev-hot sidecar started (interval=${INTERVAL}s)"
while true; do
    POD_JSON=$(wget -q -O - --header="Authorization: Bearer $TOKEN" --no-check-certificate "$API/api/v1/namespaces/$NS/pods/$POD" || true)
    DIGEST=$(echo "$POD_JSON" | grep -o '"aether.dev/digest":"sha256:[^"]*"' | sed -e 's/.*"sha256://' -e 's/"$//')
    ART=$(echo "$POD_JSON" | grep -o '"aether.dev/artifact-url":"[^"]*"' | sed -e 's/.*"aether.dev\/artifact-url":"//' -e 's/"$//')
    if [ -n "$DIGEST" ] && [ ${#DIGEST} -eq 64 ] && [ "$DIGEST" != "$CUR" ]; then
        if [ -z "$ART" ]; then
            echo "[fetcher] digest $DIGEST detected but artifact URL empty"; sleep "$INTERVAL"; continue;
        fi
        echo "[fetcher] New digest $DIGEST -> fetching artifact $ART"
        if wget -q -O /workspace/app.tar.gz "$ART"; then
            if echo "$DIGEST  /workspace/app.tar.gz" | sha256sum -c - >/dev/null 2>&1; then
                tar -xzf /workspace/app.tar.gz -C /workspace || { echo "[fetcher] extract failed"; sleep "$INTERVAL"; continue; }
                CUR="$DIGEST"
                echo "[fetcher] updated to $DIGEST"; echo "REFRESH_OK app=$POD digest=$DIGEST"
            else
                echo "[fetcher] checksum mismatch for $ART (expected $DIGEST)"; echo "REFRESH_FAIL app=$POD reason=checksum";
            fi
        else
            echo "[fetcher] download failed for $ART"; echo "REFRESH_FAIL app=$POD reason=download";
        fi
    fi
    sleep "$INTERVAL"
done"#;
        (serde_json::Value::Array(vec![]), json!([
            {
                "name": "fetcher",
                "image": "busybox:1.36",
                "command": ["/bin/sh","-c"],
                "args": [fetch_script],
                "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ]
            },
            {
                "name": "app",
                "image": "aether-nodejs:20-slim",
                "workingDir": "/workspace",
                "command": ["node","server.js"],
                "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ],
                "env": envs,
            }
        ]))
    } else {
        // Non dev-hot: single app container with init container performing first fetch
        let mut init_cmd = format!("set -euo pipefail; echo Fetching artifact; wget -O /workspace/app.tar.gz {artifact_url};");
        if valid_digest { init_cmd.push_str(&format!(" echo '{digest}  /workspace/app.tar.gz' | sha256sum -c -;")); }
        init_cmd.push_str(" tar -xzf /workspace/app.tar.gz -C /workspace");
        (json!([
            {
                "name": "fetch-artifact",
                "image": "busybox:1.36",
                "command": ["/bin/sh","-c"],
                "args": [init_cmd],
                "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ]
            }
        ]), json!([
            {
                "name": "app",
                "image": "aether-nodejs:20-slim",
                "workingDir": "/workspace",
                "command": ["node","server.js"],
                "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ],
                "env": envs,
            }
        ]))
    };

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
                    "initContainers": init_containers,
                    "containers": containers
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
        let v = build_deployment_manifest("demo","0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","https://example/artifact.tar.gz","default", None, false);
        assert!(v["metadata"]["annotations"]["aether.dev/digest"].as_str().unwrap().starts_with("sha256:"));
    }

    #[test]
    fn dev_hot_manifest_has_fetcher_sidecar() {
        let v = build_deployment_manifest("demo","0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","https://example/artifact.tar.gz","default", None, true);
        assert_eq!(v["metadata"]["annotations"]["aether.dev/dev-hot"].as_str().unwrap(), "true");
        let containers = v["spec"]["template"]["spec"]["containers"].as_array().unwrap();
        assert!(containers.iter().any(|c| c["name"].as_str()==Some("fetcher")), "fetcher sidecar missing");
        // no initContainers expected
        assert!(v["spec"]["template"]["spec"]["initContainers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn dev_hot_fetcher_script_contains_checksum_and_interval() {
        let v = build_deployment_manifest("demo","0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","https://example/artifact.tar.gz","default", None, true);
        let containers = v["spec"]["template"]["spec"]["containers"].as_array().unwrap();
        let fetcher = containers.iter().find(|c| c["name"].as_str()==Some("fetcher")).expect("fetcher not found");
        let args = fetcher["args"].as_array().unwrap();
        let script = args[0].as_str().unwrap();
        assert!(script.contains("sha256sum -c"), "checksum verification missing");
        assert!(script.contains("AETHER_FETCH_INTERVAL_SEC"), "interval env not referenced");
    }
}

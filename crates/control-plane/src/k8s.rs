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
    if std::env::var("AETHER_DISABLE_K8S").unwrap_or_default() == "1" {
        tracing::info!(app, "AETHER_DISABLE_K8S=1 skipping real kube apply");
        return Ok(());
    }
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
    if let Ok(commit) = std::env::var("GIT_COMMIT_SHA") { annotations["aether.dev/build-commit"] = json!(commit); }
    if valid_digest { annotations["aether.dev/digest"] = json!(format!("sha256:{digest}")); }
    if signature.is_some() { annotations["aether.dev/signature"] = json!("ed25519"); }
    if dev_hot { annotations["aether.dev/dev-hot"] = json!("true"); }
    let labels = json!({"app": app, "app_name": app});
    // Build env array separately to avoid complex inline code in json! macro
    let mut envs: Vec<serde_json::Value> = Vec::new();
    if valid_digest { envs.push(json!({"name":"AETHER_DIGEST","value": format!("sha256:{digest}")})); }
    if let Some(sig) = signature { envs.push(json!({"name":"AETHER_SIGNATURE","value": sig})); }
    if dev_hot { envs.push(json!({"name":"AETHER_DEV_HOT","value": "true"})); }
    // pass signature (hex) if present for sidecar verification logic
    if let Some(sig) = signature { envs.push(json!({"name":"AETHER_SIGNATURE","value": sig })); }
    // Public key now expected via Secret aether-pubkey (key PUBKEY). Retain fallback to host env for legacy dev.
    if let Ok(pubkey) = std::env::var("AETHER_PUBKEY") { envs.push(json!({"name":"AETHER_PUBKEY","value": pubkey })); }
    else {
        envs.push(json!({
            "name":"AETHER_PUBKEY",
            "valueFrom": {"secretKeyRef": {"name": "aether-pubkey", "key": "PUBKEY", "optional": true}}
        }));
    }

    // Containers differ if dev_hot enabled: add fetcher sidecar polling pod annotations for new digest
    let (init_containers, containers) = if dev_hot {
                let fetch_script = r#"set -euo pipefail
# Standardized dev-hot log markers:
# REFRESH_OK app=<pod> digest=<digest> ms=<duration_ms>
# REFRESH_FAIL app=<pod> reason=<reason> ms=<duration_ms>
# REFRESH_STATE failures=<n> last_digest=<digest>
API="https://${KUBERNETES_SERVICE_HOST}:${KUBERNETES_SERVICE_PORT}"
TOKEN=$(cat /var/run/secrets/kubernetes.io/serviceaccount/token)
NS=$(cat /var/run/secrets/kubernetes.io/serviceaccount/namespace)
POD=$(hostname)
CUR=""
BASE_BACKOFF_MS=500
MAX_BACKOFF_MS=5000
FAILURES=0
INTERVAL="${AETHER_FETCH_INTERVAL_SEC:-5}"
                MODE="${AETHER_FETCH_MODE:-poll}" # poll | watch
MIN_REFRESH_MS="${AETHER_MIN_REFRESH_INTERVAL_MS:-2000}" # canary safety min ms between refreshes
LAST_REFRESH_MS=0
ANOMALY_THRESHOLD="${AETHER_ANOMALY_FAIL_THRESHOLD:-7}"
echo "[fetcher] dev-hot sidecar started interval=${INTERVAL}s"
json_field() { # prefer binary json-extract if mounted at /json-extract
    if command -v /json-extract >/dev/null 2>&1; then /json-extract "$2" <<< "$1" 2>/dev/null || true; return; fi
    awk -v k="$2" '
        BEGIN { RS=""; FS=""; key_found=0; ann_section=0; in_str=0; esc=0; buf=""; want=0; capture=0; val=""; }
        {
            json=$0;
            for(i=1;i<=length(json);i++) {
                c=substr(json,i,1);
                if(in_str) {
                    if(esc){ esc=0; buf=buf c; continue }
                    if(c=="\\") { esc=1; buf=buf c; continue }
                    if(c=="\"") { in_str=0; buf=buf c;
                         if(buf=="\"annotations\"") ann_section=1;
                         else if(ann_section && want && !capture && buf=="\"" k "\"") { # key matched, expect colon then string
                                 key_found=1;
                         } else if(capture) { val=substr(buf,2,length(buf)-2); print val; exit }
                         buf=""; next
                    } else { buf=buf c; continue }
                } else {
                    if(c=="\"") { in_str=1; buf="\""; continue }
                    if(key_found && c==":") { want=0; # wait for opening quote of value
                    } else if(key_found && c=="\"") { in_str=1; buf="\""; capture=1; key_found=0; }
                    else if(ann_section && c=="{") { want=1 } # inside annotations map
                    else if(ann_section && c=="}") { ann_section=0 }
                }
            }
        }' <<EOF
$1
EOF
}
ready_set() { touch /workspace/.ready 2>/dev/null || true; }
ready_clear() { rm -f /workspace/.ready 2>/dev/null || true; }
ready_set # mark ready initially (until first update logic decides otherwise)

# Ensure supervisor script exists (graceful restart on digest change + readiness drain)
SUPERVISOR=/workspace/supervisor.sh
if [ ! -f "$SUPERVISOR" ]; then
cat > $SUPERVISOR <<'EOS'
#!/bin/sh
set -euo pipefail
APP_CMD="node server.js"
STATE=.devhot_state
CUR=""
GRACE=${AETHER_SUPERVISOR_GRACE_SEC:-3}
if [ -f "$STATE" ]; then CUR=$(grep '^CUR=' "$STATE" | head -n1 | cut -d= -f2 || true); fi
echo "[supervisor] starting with digest=$CUR grace=${GRACE}s"
run_child() {
    sh -c "$APP_CMD" &
    CHILD=$!
    trap 'echo "[supervisor] SIGTERM -> draining"; rm -f /workspace/.ready; kill $CHILD 2>/dev/null || true; wait $CHILD 2>/dev/null || true; exit 0' TERM INT
    while kill -0 $CHILD 2>/dev/null; do
        NEW=$(grep '^CUR=' "$STATE" | head -n1 | cut -d= -f2 2>/dev/null || true)
        if [ -n "$NEW" ] && [ "$NEW" != "$CUR" ]; then
                echo "[supervisor] digest change $CUR -> $NEW draining readiness"
                rm -f /workspace/.ready 2>/dev/null || true
                sleep $GRACE
                kill $CHILD 2>/dev/null || true
                wait $CHILD 2>/dev/null || true
                CUR=$NEW
                return 0
        fi
        sleep 1
    done
    return 0
}
while true; do run_child; done
EOS
chmod +x $SUPERVISOR
fi

STATE_FILE=/workspace/.devhot_state
if [ -f "$STATE_FILE" ]; then
    CUR_REC=$(grep '^CUR=' "$STATE_FILE" | head -n1 | cut -d= -f2 || true)
    FAIL_REC=$(grep '^FAILURES=' "$STATE_FILE" | head -n1 | cut -d= -f2 || true)
    if [ -n "$CUR_REC" ]; then CUR="$CUR_REC"; fi
    if [ -n "$FAIL_REC" ]; then FAILURES="$FAIL_REC"; fi
    echo "REFRESH_STATE failures=$FAILURES last_digest=$CUR"
fi

process_pod_json(){
    POD_JSON="$1"
    RAW_DIGEST=$(json_field "$POD_JSON" "aether.dev/digest" || true)
    RAW_ART=$(json_field "$POD_JSON" "aether.dev/artifact-url" || true)
    DIGEST=""; ART=""
    if [ -n "$RAW_DIGEST" ] && echo "$RAW_DIGEST" | grep -q '^sha256:'; then
        DIGEST=$(echo "$RAW_DIGEST" | sed 's/^sha256://')
    fi
    ART="$RAW_ART"
    if [ -n "$DIGEST" ] && [ ${#DIGEST} -eq 64 ] && [ "$DIGEST" != "$CUR" ]; then
        NOW_MS=$(date +%s%3N || date +%s000)
        if [ $LAST_REFRESH_MS -ne 0 ] && [ $((NOW_MS - LAST_REFRESH_MS)) -lt $MIN_REFRESH_MS ]; then
            echo "[fetcher] rate-limit skip digest $DIGEST"; echo "REFRESH_FAIL app=$POD reason=rate_limit ms=0"; return
        fi
        if [ -z "$ART" ]; then
            echo "[fetcher] digest $DIGEST but artifact URL empty"; FAILURES=$((FAILURES+1)); return
        fi
        echo "[fetcher] New digest $DIGEST -> fetching $ART"; START_MS=$(date +%s%3N || date +%s000)
        ready_clear
        if wget -q -O /workspace/app.tar.gz "$ART"; then
            if echo "$DIGEST  /workspace/app.tar.gz" | sha256sum -c - >/dev/null 2>&1; then
                # Optional signature verification (if AETHER_SIGNATURE env + verifier available)
                if [ -n "${AETHER_SIGNATURE:-}" ] && command -v /verifier/ed25519-verify >/dev/null 2>&1; then
                    if ! echo -n "$DIGEST" | /verifier/ed25519-verify "$AETHER_SIGNATURE"; then
                        END_MS=$(date +%s%3N || date +%s000); DUR=$((END_MS-START_MS))
                        echo "[fetcher] signature verify failed"; echo "REFRESH_FAIL app=$POD reason=signature ms=$DUR"; FAILURES=$((FAILURES+1)); continue
                    fi
                fi
                if tar -xzf /workspace/app.tar.gz -C /workspace; then
                    CUR="$DIGEST"; END_MS=$(date +%s%3N || date +%s000); DUR=$((END_MS-START_MS))
                    echo "[fetcher] updated to $DIGEST (${DUR}ms)"; echo "REFRESH_OK app=$POD digest=$DIGEST ms=$DUR"; FAILURES=0; ready_set; LAST_REFRESH_MS=$END_MS; echo "CUR=$CUR" > $STATE_FILE; echo "FAILURES=$FAILURES" >> $STATE_FILE
                else
                    END_MS=$(date +%s%3N || date +%s000); DUR=$((END_MS-START_MS))
                    echo "[fetcher] extract failed"; echo "REFRESH_FAIL app=$POD reason=extract ms=$DUR"; FAILURES=$((FAILURES+1))
                fi
            else
                END_MS=$(date +%s%3N || date +%s000); DUR=$((END_MS-START_MS))
                echo "[fetcher] checksum mismatch (expected $DIGEST)"; echo "REFRESH_FAIL app=$POD reason=checksum ms=$DUR"; FAILURES=$((FAILURES+1))
            fi
        else
            END_MS=$(date +%s%3N || date +%s000); DUR=$((END_MS-START_MS))
            echo "[fetcher] download failed $ART"; echo "REFRESH_FAIL app=$POD reason=download ms=$DUR"; FAILURES=$((FAILURES+1))
        fi
    fi
}

if [ "$MODE" = "watch" ]; then
  echo "[fetcher] using watch stream mode"
  while true; do
    # open watch stream; fallback to sleep if fails
    wget -q -O - --header="Authorization: Bearer $TOKEN" "$API/api/v1/namespaces/$NS/pods?fieldSelector=metadata.name=$POD&watch=1" 2>/dev/null | while read -r line; do
        # each event line is JSON event containing object.
        case "$line" in *"annotations"*) process_pod_json "$line" ;; esac
    done
    echo "[fetcher] watch stream ended -> reconnecting"; sleep 1
  done
fi

while true; do
    START_LOOP=$(date +%s%3N || date +%s000)
    POD_JSON=$(wget -q -O - --header="Authorization: Bearer $TOKEN" "$API/api/v1/namespaces/$NS/pods/$POD" || true)
    if [ -z "$POD_JSON" ]; then
        echo "[fetcher] empty pod json"; FAILURES=$((FAILURES+1));
    else
        process_pod_json "$POD_JSON"
    fi
    # backoff on consecutive failures (jitter ~33%) else sleep fixed interval
    if [ $FAILURES -gt 0 ]; then
    POW=$FAILURES; if [ $POW -gt 6 ]; then POW=6; fi
    if [ $FAILURES -ge $ANOMALY_THRESHOLD ]; then echo "REFRESH_FAIL app=$POD reason=anomaly ms=0"; fi
        # compute 2^POW
        B=1; for _ in $(seq 1 $POW); do B=$((B*2)); done
        DELAY=$((BASE_BACKOFF_MS * B)); if [ $DELAY -gt $MAX_BACKOFF_MS ]; then DELAY=$MAX_BACKOFF_MS; fi
        JITTER=$(( (RANDOM % (DELAY/3 + 1)) ))
        SLEEP=$((DELAY + JITTER))
        echo "[fetcher] failures=$FAILURES backoff=${SLEEP}ms"
        sleep $(awk "BEGIN { printf \"%.3f\", $SLEEP/1000 }")
    else
        sleep "$INTERVAL"
    fi
done"#;
        let fetch_image = std::env::var("AETHER_FETCH_IMAGE").unwrap_or_else(|_| "busybox:1.36".to_string());
        (serde_json::Value::Array(vec![]), json!([
            {
                "name": "fetcher",
                "image": fetch_image,
                "command": ["/bin/sh","-c"],
                "args": [fetch_script],
                "env": [ {"name":"AETHER_FETCH_MODE","value":"poll"} ],
                "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ]
            },
            {
                "name": "app",
                "image": "aether-nodejs:20-slim",
                "workingDir": "/workspace",
                "command": ["/bin/sh","-c","/workspace/supervisor.sh"],
                "volumeMounts": [ {"name": "workspace", "mountPath": "/workspace" } ],
                "readinessProbe": {"exec": {"command": ["/bin/sh","-c","test -f /workspace/.ready"]}, "initialDelaySeconds": 1, "periodSeconds": 2},
                "env": envs,
            }
        ]))
    } else {
        // Non dev-hot: single app container with init container performing first fetch
        let mut init_cmd = format!("set -euo pipefail; echo Fetching artifact; wget -O /workspace/app.tar.gz {artifact_url};");
        if valid_digest { init_cmd.push_str(&format!(" echo '{digest}  /workspace/app.tar.gz' | sha256sum -c -;")); }
        init_cmd.push_str(" tar -xzf /workspace/app.tar.gz -C /workspace; touch /workspace/.ready");
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
                "readinessProbe": {"exec": {"command": ["/bin/sh","-c","test -f /workspace/.ready"]}, "initialDelaySeconds": 1, "periodSeconds": 2},
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
                    "serviceAccountName": "aether-dev-hot",
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

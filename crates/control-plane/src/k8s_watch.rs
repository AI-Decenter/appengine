use sqlx::Pool;
use kube::{Client, api::{ListParams, ResourceExt}, Api};
use kube_runtime::watcher::{watcher, Config, Event};
use futures_util::StreamExt;
use sqlx::Row;
use k8s_openapi::api::apps::v1::Deployment as K8sDeployment;
use k8s_openapi::api::core::v1::Pod;
use chrono::Utc;

pub async fn run_deployment_status_watcher(db: Pool<sqlx::Postgres>) {
    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => { tracing::warn!(error=%e, "K8s client init failed"); return; }
    };
    let d_api: Api<K8sDeployment> = Api::namespaced(client.clone(), "default");
    let stream = watcher(d_api, Config::default());
    futures_util::pin_mut!(stream);
    while let Some(ev) = stream.next().await {
        match ev {
            Ok(Event::Applied(d_obj)) => {
                let app_name = d_obj.name_any();
                let status = d_obj.status.clone();
                let available = status.as_ref().and_then(|s| s.available_replicas).unwrap_or(0);
                // Find pending deployment in DB
                if let Ok(Some(row)) = sqlx::query("SELECT d.id, d.created_at FROM deployments d JOIN applications a ON a.id = d.app_id WHERE a.name = $1 AND d.status = 'pending' LIMIT 1")
                    .bind(&app_name).fetch_optional(&db).await {
                        let dep_id: uuid::Uuid = row.get("id");
                        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
                        if available >= 1 {
                            crate::services::deployments::mark_running(&db, dep_id).await;
                            tracing::info!(deployment_id=%dep_id, app=%app_name, "deployment running (watch)");
                            continue;
                        }
                        // Failure heuristics
                        let mut failed_reason: Option<String> = None;
                        if let Some(st) = status {
                            if let Some(conds) = st.conditions {
                                for c in conds { if c.type_=="Progressing" && c.status=="False" { failed_reason = Some(c.reason.unwrap_or_else(|| "progress_failed".into())); break; } }
                            }
                        }
                        // Pod-level inspection for init container failures
                        if failed_reason.is_none() {
                            let p_api: Api<Pod> = Api::namespaced(client.clone(), "default");
                            if let Ok(pods) = p_api.list(&ListParams::default().labels(&format!("app={}", app_name))).await {
                                'podloop: for p in pods { if let Some(ps) = p.status { if let Some(ics) = ps.init_container_statuses { for ics in ics { if let Some(state) = ics.state { if let Some(term) = state.terminated { if term.exit_code != 0 { failed_reason = Some(format!("init:{}:{}", ics.name, term.reason.unwrap_or_else(|| term.exit_code.to_string()))); break 'podloop; } } } } } } }
                            }
                        }
                        // Timeout heuristic (>300s)
                        if failed_reason.is_none()
                            && Utc::now().signed_duration_since(created_at).num_seconds() > 300
                        {
                            failed_reason = Some("timeout".into());
                        }
                        if let Some(rsn) = failed_reason { crate::services::deployments::mark_failed(&db, dep_id, &rsn).await; tracing::warn!(deployment_id=%dep_id, app=%app_name, reason=%rsn, "deployment failed (watch)"); }
                }
            }
            Ok(Event::Restarted(objs)) => {
                for d_obj in objs { let app_name = d_obj.name_any(); /* ignore restarted backlog for simplicity */ let _ = app_name; }
            }
            _ => {}
        }
    }
}
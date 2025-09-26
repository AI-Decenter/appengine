use axum::http::{Request, StatusCode};
use axum::body::Body;
use control_plane::{build_router, AppState};
use sqlx::Pool;use sqlx::Postgres;use tower::util::ServiceExt; // for oneshot

#[tokio::test]
async fn upload_rejects_missing_parts() {
    // build router with dummy (maybe absent) db pool using env var; skip if no DATABASE_URL and sqlx fails.
    let pool: Option<Pool<Postgres>> = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap_or_default()).await.ok();
    if pool.is_none() { eprintln!("skipping upload_rejects_missing_parts (no db)" ); return; }
    let app = build_router(AppState { db: pool.unwrap() });
    let req = Request::builder().method("POST").uri("/artifacts").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

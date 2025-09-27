use axum::http::{Request, StatusCode};
use axum::body::Body;
use control_plane::{build_router, AppState};
use sqlx::{Pool, Postgres};use tower::util::ServiceExt; // for oneshot

#[tokio::test]
async fn upload_rejects_missing_parts() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let pool: Pool<Postgres> = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.expect("db connect");
    sqlx::migrate!().run(&pool).await.expect("migrations");
    let app = build_router(AppState { db: pool });
    let req = Request::builder().method("POST").uri("/artifacts").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

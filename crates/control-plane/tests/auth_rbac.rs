use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;

#[tokio::test]
async fn a1_missing_token_unauthorized_when_required() {
	std::env::set_var("AETHER_API_TOKENS", "t_admin:admin:alice,t_reader:reader:bob");
	std::env::set_var("AETHER_AUTH_REQUIRED", "1");
	let pool = control_plane::test_support::test_pool().await;
	let app = control_plane::build_router(control_plane::AppState{ db: pool });
	// POST /deployments is write route -> requires auth, should 401 without header
	let req = Request::builder().method("POST").uri("/deployments")
		.header("content-type","application/json")
		.body(Body::from("{}"))
		.unwrap();
	let res = app.clone().oneshot(req).await.unwrap();
	assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a2_valid_token_allows_reader_get() {
	std::env::set_var("AETHER_API_TOKENS", "t_admin:admin:alice,t_reader:reader:bob");
	std::env::set_var("AETHER_AUTH_REQUIRED", "1");
	let pool = control_plane::test_support::test_pool().await;
	let app = control_plane::build_router(control_plane::AppState{ db: pool });
	let req = Request::builder().method("GET").uri("/deployments")
		.header("authorization","Bearer t_reader")
		.body(Body::empty()).unwrap();
	let res = app.clone().oneshot(req).await.unwrap();
	assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn a3_reader_forbidden_on_post_deployments() {
	std::env::set_var("AETHER_API_TOKENS", "t_admin:admin:alice,t_reader:reader:bob");
	std::env::set_var("AETHER_AUTH_REQUIRED", "1");
	let pool = control_plane::test_support::test_pool().await;
	sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
	sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
	sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("app1").execute(&pool).await.unwrap();
	let app = control_plane::build_router(control_plane::AppState{ db: pool });
	let body = serde_json::json!({"app_name":"app1","artifact_url":"file://artifact"}).to_string();
	let req = Request::builder().method("POST").uri("/deployments")
		.header("content-type","application/json")
		.header("authorization","Bearer t_reader")
		.body(Body::from(body)).unwrap();
	let res = app.clone().oneshot(req).await.unwrap();
	assert_eq!(res.status(), StatusCode::FORBIDDEN);
}


use axum::body::to_bytes;
use axum::response::IntoResponse;
use axum_problem::{AxumProblem, Problem};
use http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error, AxumProblem)]
enum ApiError {
    #[error("order {id} not found")]
    #[problem(status = 404)]
    NotFound { id: i64 },

    #[error("access denied")]
    #[problem(status = 401)]
    Unauthorized,

    #[error("db error: {0}")]
    #[problem(status = 500, mask)]
    Database(String),

    #[error("conflict on field {field}")]
    #[problem(status = 409, title = "Resource Conflict")]
    Conflict { field: String },
}

async fn body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn unit_variant_produces_problem_json() {
    let resp = ApiError::Unauthorized.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json = body(resp).await;
    assert_eq!(json["status"], 401);
    assert_eq!(json["title"], "Unauthorized");
}

#[tokio::test]
async fn struct_variant_includes_display_as_detail() {
    let resp = ApiError::NotFound { id: 1001 }.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let json = body(resp).await;
    assert_eq!(json["status"], 404);
    assert_eq!(json["detail"], "order 1001 not found");
}

#[tokio::test]
async fn mask_hides_detail_from_response() {
    let resp = ApiError::Database("connection refused".into()).into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body(resp).await;
    assert_eq!(json["status"], 500);
    // detail must NOT appear — internal error is masked
    assert!(json.get("detail").is_none(), "masked error must not expose detail to client");
}

#[tokio::test]
async fn custom_title_overrides_default() {
    let resp = ApiError::Conflict { field: "email".into() }.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let json = body(resp).await;
    assert_eq!(json["title"], "Resource Conflict");
}

#[tokio::test]
async fn content_type_is_application_problem_json() {
    let resp = ApiError::Unauthorized.into_response();
    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
}

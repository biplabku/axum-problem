use axum::body::to_bytes;
use axum::response::IntoResponse;
use axum_problem::{AxumProblem, Problem};
use http::StatusCode;
use thiserror::Error;

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ── Enum covering every variant style ─────────────────────────────────────────

#[derive(Debug, Error, AxumProblem)]
enum FullApiError {
    // 1. Unit variant — no fields
    #[error("unauthorized")]
    #[problem(status = 401)]
    Unauthorized,

    // 2. Unit variant with mask
    #[error("service unavailable")]
    #[problem(status = 503, mask)]
    ServiceDown,

    // 3. Struct variant — named fields
    #[error("order {id} not found")]
    #[problem(status = 404)]
    NotFound { id: i64 },

    // 4. Struct variant with mask — named fields hidden from client
    #[error("internal: table={table} err={err}")]
    #[problem(status = 500, mask)]
    DbQuery { table: String, err: String },

    // 5. Tuple variant — single field
    #[error("validation: {0}")]
    #[problem(status = 422)]
    Validation(String),

    // 6. Tuple variant — single field with mask
    #[error("db: {0}")]
    #[problem(status = 500, mask)]
    DbConnect(String),

    // 7. Tuple variant — multiple fields (first used for display)
    #[error("rate limit: {0} requests per {1}s")]
    #[problem(status = 429)]
    RateLimit(u32, u32),

    // 8. Custom title
    #[error("email already taken")]
    #[problem(status = 409, title = "Email Conflict")]
    EmailConflict,

    // 9. Non-standard status code
    #[error("teapot")]
    #[problem(status = 418)]
    Teapot,

    // 10. Custom title + mask combined
    #[error("gateway: {0}")]
    #[problem(status = 502, title = "Upstream Error", mask)]
    GatewayError(String),
}

// ── 1. Unit variant: detail = Display string ──────────────────────────────────

#[tokio::test]
async fn unit_variant_detail_is_display_string() {
    let resp = FullApiError::Unauthorized.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json = body_json(resp).await;
    assert_eq!(json["status"], 401);
    assert_eq!(json["title"], "Unauthorized");
    assert_eq!(json["detail"], "unauthorized"); // from #[error("unauthorized")]
}

// ── 2. Unit variant + mask: detail absent, no panic ───────────────────────────

#[tokio::test]
async fn unit_variant_with_mask_hides_detail() {
    let resp = FullApiError::ServiceDown.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let json = body_json(resp).await;
    assert_eq!(json["status"], 503);
    assert!(json.get("detail").is_none(), "masked unit variant must not expose detail");
}

// ── 3. Struct variant: detail from Display ────────────────────────────────────

#[tokio::test]
async fn struct_variant_detail_uses_display() {
    let resp = FullApiError::NotFound { id: 42 }.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let json = body_json(resp).await;
    assert_eq!(json["detail"], "order 42 not found");
}

// ── 4. Struct variant + mask: named field values hidden ───────────────────────

#[tokio::test]
async fn struct_variant_with_mask_hides_field_values() {
    let resp = FullApiError::DbQuery {
        table: "orders".into(),
        err: "deadlock detected".into(),
    }
    .into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body_json(resp).await;
    assert_eq!(json["status"], 500);
    assert!(
        json.get("detail").is_none(),
        "masked struct variant must not expose field values"
    );
    // The internal error includes sensitive table/err info — must stay server-side
    let detail_str = json.get("detail").map(|v| v.as_str().unwrap_or("")).unwrap_or("");
    assert!(!detail_str.contains("deadlock"), "internal error detail must not reach client");
    assert!(!detail_str.contains("orders"), "internal table name must not reach client");
}

// ── 5. Tuple variant (1 field): detail = inner Display ───────────────────────

#[tokio::test]
async fn single_tuple_variant_detail_is_inner_display() {
    let resp = FullApiError::Validation("email is required".into()).into_response();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(resp).await;
    assert_eq!(json["detail"], "validation: email is required");
}

// ── 6. Tuple variant (1 field) + mask ────────────────────────────────────────

#[tokio::test]
async fn single_tuple_with_mask_hides_inner() {
    let resp = FullApiError::DbConnect("connection refused to 10.0.0.5".into()).into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body_json(resp).await;
    assert!(json.get("detail").is_none(), "db connection string must not reach client");
    let detail_str = json.get("detail").map(|v| v.as_str().unwrap_or("")).unwrap_or("");
    assert!(!detail_str.contains("10.0.0.5"), "internal IP must not reach client");
}

// ── 7. Tuple variant (multiple fields): first field used ──────────────────────

#[tokio::test]
async fn multi_field_tuple_variant_uses_display() {
    let resp = FullApiError::RateLimit(100, 60).into_response();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let json = body_json(resp).await;
    assert_eq!(json["status"], 429);
    // Display: "rate limit: 100 requests per 60s"
    assert!(json["detail"].as_str().unwrap_or("").contains("100"),
        "rate limit count should appear in detail");
}

// ── 8. Custom title overrides status default ──────────────────────────────────

#[tokio::test]
async fn custom_title_shown_in_response() {
    let resp = FullApiError::EmailConflict.into_response();
    let json = body_json(resp).await;
    assert_eq!(json["title"], "Email Conflict");
    assert_eq!(json["status"], 409);
}

// ── 9. Non-standard status code ───────────────────────────────────────────────

#[tokio::test]
async fn nonstandard_status_418_works() {
    let resp = FullApiError::Teapot.into_response();
    assert_eq!(resp.status().as_u16(), 418);
    let json = body_json(resp).await;
    assert_eq!(json["status"], 418);
    assert!(json["title"].as_str().is_some(), "title must exist for 418");
}

// ── 10. Custom title + mask combined ─────────────────────────────────────────

#[tokio::test]
async fn custom_title_with_mask_hides_detail() {
    let resp = FullApiError::GatewayError("upstream timeout on 192.168.1.1".into()).into_response();
    assert_eq!(resp.status().as_u16(), 502);
    let json = body_json(resp).await;
    assert_eq!(json["title"], "Upstream Error");
    assert!(json.get("detail").is_none(), "masked gateway error must not expose upstream IP");
}

// ── Content-Type always correct ───────────────────────────────────────────────

#[tokio::test]
async fn all_variants_have_problem_content_type() {
    let cases: Vec<axum::response::Response> = vec![
        FullApiError::Unauthorized.into_response(),
        FullApiError::ServiceDown.into_response(),
        FullApiError::NotFound { id: 1 }.into_response(),
        FullApiError::DbQuery { table: "t".into(), err: "e".into() }.into_response(),
        FullApiError::Validation("v".into()).into_response(),
        FullApiError::DbConnect("c".into()).into_response(),
        FullApiError::RateLimit(10, 60).into_response(),
        FullApiError::EmailConflict.into_response(),
        FullApiError::Teapot.into_response(),
        FullApiError::GatewayError("g".into()).into_response(),
    ];

    for resp in cases {
        assert_eq!(
            resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
            "application/problem+json",
            "every variant must return application/problem+json"
        );
    }
}

// ── Problem struct edge cases ─────────────────────────────────────────────────

#[tokio::test]
async fn problem_with_all_fields() {
    let p = Problem::new(422)
        .problem_type("https://api.example.com/errors/validation")
        .title("Validation Failed")
        .detail("email field is required")
        .instance("/requests/abc123");
    let resp = p.into_response();
    let json = body_json(resp).await;
    assert_eq!(json["type"], "https://api.example.com/errors/validation");
    assert_eq!(json["title"], "Validation Failed");
    assert_eq!(json["status"], 422);
    assert_eq!(json["detail"], "email field is required");
    assert_eq!(json["instance"], "/requests/abc123");
}

#[tokio::test]
async fn problem_unknown_status_code_does_not_panic() {
    // Unknown status codes should not panic — use generic title
    let p = Problem::new(599);
    let resp = p.into_response();
    assert_eq!(resp.status().as_u16(), 599);
    let json = body_json(resp).await;
    assert!(json["title"].as_str().is_some());
}

// ── Axum handler integration ──────────────────────────────────────────────────

#[tokio::test]
async fn works_in_real_axum_handler_with_question_mark() {
    use axum::{routing::get, Json, Router};
    use tower::ServiceExt;

    #[derive(Debug, Error, AxumProblem)]
    enum HandlerError {
        #[error("item {id} not found")]
        #[problem(status = 404)]
        Missing { id: i64 },
    }

    async fn handler() -> Result<Json<serde_json::Value>, HandlerError> {
        Err(HandlerError::Missing { id: 99 })
    }

    let app = Router::new().route("/", get(handler));

    let req = http::Request::builder()
        .uri("/")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    let json = body_json(resp).await;
    assert_eq!(json["status"], 404);
    assert_eq!(json["detail"], "item 99 not found");
}

// ── Single-variant enum ───────────────────────────────────────────────────────

#[tokio::test]
async fn single_variant_enum_works() {
    #[derive(Debug, Error, AxumProblem)]
    enum SingleError {
        #[error("only error")]
        #[problem(status = 400)]
        OnlyOne,
    }
    let resp = SingleError::OnlyOne.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── Enum without thiserror (manual Display) ───────────────────────────────────

#[tokio::test]
async fn works_without_thiserror_if_display_implemented() {
    use std::fmt;

    #[derive(Debug, AxumProblem)]
    enum ManualError {
        #[problem(status = 503)]
        Down,
    }

    impl fmt::Display for ManualError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "service is down")
        }
    }
    impl std::error::Error for ManualError {}

    let resp = ManualError::Down.into_response();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let json = body_json(resp).await;
    assert_eq!(json["detail"], "service is down");
}

// ── ProblemLayer edge cases ────────────────────────────────────────────────────

use axum::{Router, routing::{get, post}, body::Body};
use axum_problem::ProblemLayer;
use tower::ServiceExt;

async fn resp_json(app: Router, method: &str, uri: &str) -> axum::response::Response {
    app.oneshot(
        http::Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

// axum returns a plain 404 for routes that don't exist.
// ProblemLayer must convert it to problem+json.
#[tokio::test]
async fn problem_layer_converts_axum_404_missing_route() {
    let app = Router::new()
        .route("/exists", get(|| async { "ok" }))
        .layer(ProblemLayer);

    let resp = resp_json(app, "GET", "/does-not-exist").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/problem+json",
        "axum's 404 for missing route must become problem+json"
    );
    let json = body_json(resp).await;
    assert_eq!(json["status"], 404);
}

// axum returns a plain 405 when the route exists but method is wrong.
// ProblemLayer must convert it.
#[tokio::test]
async fn problem_layer_converts_axum_405_method_not_allowed() {
    let app = Router::new()
        .route("/hook", post(|| async { "ok" })) // POST only
        .layer(ProblemLayer);

    let resp = resp_json(app, "GET", "/hook").await; // GET → 405
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/problem+json",
        "axum's 405 Method Not Allowed must become problem+json"
    );
    let json = body_json(resp).await;
    assert_eq!(json["status"], 405);
    assert_eq!(json["title"], "Method Not Allowed");
}

// 5xx from a handler that returns an error status code.
#[tokio::test]
async fn problem_layer_converts_5xx_to_problem_json() {
    let app = Router::new()
        .route("/crash", get(|| async {
            (StatusCode::INTERNAL_SERVER_ERROR, "something broke")
        }))
        .layer(ProblemLayer);

    let resp = resp_json(app, "GET", "/crash").await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    let json = body_json(resp).await;
    assert_eq!(json["status"], 500);
    assert_eq!(json["title"], "Internal Server Error");
}

// 3xx redirect responses must pass through unchanged — not converted.
#[tokio::test]
async fn problem_layer_passes_through_3xx_redirect() {
    let app = Router::new()
        .route("/old", get(|| async {
            (
                StatusCode::MOVED_PERMANENTLY,
                [(http::header::LOCATION, "/new")],
                "",
            )
        }))
        .layer(ProblemLayer);

    let resp = resp_json(app, "GET", "/old").await;
    assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
    // Must NOT be problem+json
    let ct = resp.headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(!ct.contains("application/problem+json"),
        "3xx redirect must not be converted to problem");
}

// Response with no Content-Type at all on a 4xx — must be converted.
#[tokio::test]
async fn problem_layer_converts_4xx_with_no_content_type() {
    let app = Router::new()
        .route("/raw", get(|| async {
            // Return 400 with no Content-Type header
            http::Response::builder()
                .status(400)
                .body(Body::empty())
                .unwrap()
        }))
        .layer(ProblemLayer);

    let resp = resp_json(app, "GET", "/raw").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/problem+json",
        "4xx with no content-type must be converted"
    );
}

// ProblemLayer must not disturb an existing problem+json 500.
#[tokio::test]
async fn problem_layer_passes_through_existing_problem_500() {
    let app = Router::new()
        .route("/err", get(|| async {
            Problem::internal_server_error().detail("database unavailable")
        }))
        .layer(ProblemLayer);

    let resp = resp_json(app, "GET", "/err").await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body_json(resp).await;
    // Detail must be preserved — not overwritten by the layer
    assert_eq!(json["detail"], "database unavailable",
        "existing problem detail must survive ProblemLayer");
}

// Layer can be stacked multiple times without doubling headers or changing behavior.
#[tokio::test]
async fn problem_layer_stacked_twice_is_idempotent() {
    let app = Router::new()
        .route("/notfound", get(|| async {
            StatusCode::NOT_FOUND
        }))
        .layer(ProblemLayer)
        .layer(ProblemLayer); // stacked twice

    let resp = resp_json(app, "GET", "/notfound").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    // Content-Type must appear exactly once
    let ct_count = resp.headers()
        .get_all(http::header::CONTENT_TYPE)
        .iter()
        .count();
    assert_eq!(ct_count, 1, "content-type must appear exactly once even with double layer");
    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
}

// ── Extension field edge cases ────────────────────────────────────────────────

#[tokio::test]
async fn extension_key_shadows_standard_field_title() {
    // RFC 9457 §3.5: extension members should not conflict with standard members.
    // We don't enforce this — but the behavior must be defined and not panic.
    // serde flatten: the standard "title" field wins (struct fields take precedence).
    let p = Problem::new(400)
        .extension("title", serde_json::json!("injected title"));

    let resp = p.into_response();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    // Must be valid JSON — must not panic or produce invalid output
    let json: serde_json::Value = serde_json::from_str(&body)
        .expect("response must be valid JSON even when extension key conflicts with standard field");
    // The status field must always be correct
    assert_eq!(json["status"], 400);
}

#[tokio::test]
async fn extension_with_nested_object() {
    let p = Problem::new(422)
        .extension("context", serde_json::json!({
            "user_id": "u123",
            "action": "create_order",
            "errors": [{"field": "amount", "code": "negative"}]
        }));

    let resp = p.into_response();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["context"]["user_id"], "u123");
    assert_eq!(json["context"]["errors"][0]["code"], "negative");
    assert_eq!(json["status"], 422);
}

#[tokio::test]
async fn extension_with_null_value() {
    let p = Problem::new(400)
        .extension("trace_id", serde_json::Value::Null);

    let resp = p.into_response();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["trace_id"], serde_json::Value::Null);
}

#[tokio::test]
async fn problem_layer_preserves_extensions_on_passthrough() {
    use axum::{Router, routing::get, body::Body};
    use axum_problem::ProblemLayer;
    use tower::ServiceExt;

    async fn handler_with_extension() -> impl axum::response::IntoResponse {
        Problem::new(422)
            .detail("failed")
            .extension("violations", serde_json::json!([{"field": "x"}]))
    }

    let app = Router::new()
        .route("/err", get(handler_with_extension))
        .layer(ProblemLayer);

    let resp = app
        .oneshot(
            http::Request::builder()
                .uri("/err")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // ProblemLayer must pass through already-problem responses unchanged,
    // including their extension fields
    assert_eq!(json["violations"][0]["field"], "x",
        "ProblemLayer must not strip extension fields from problem responses");
    assert_eq!(json["status"], 422);
}

// ── utoipa schema (only compiled when feature is enabled) ─────────────────────

#[cfg(feature = "utoipa")]
#[test]
fn utoipa_schema_contains_required_fields() {
    use utoipa::ToSchema;

    let (name, schema_ref) = Problem::schema();
    assert_eq!(name, "Problem");

    // Schema must produce valid JSON
    let json = serde_json::to_string(&schema_ref).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Must have properties for standard RFC 9457 fields
    let props = &val["properties"];
    assert!(props.get("title").is_some(), "schema must include 'title'");
    assert!(props.get("status").is_some(), "schema must include 'status'");
    assert!(props.get("detail").is_some(), "schema must include 'detail'");
    assert!(props.get("instance").is_some(), "schema must include 'instance'");

    // Extension members via additionalProperties: true
    assert!(val.get("additionalProperties").is_some(),
        "schema must allow extension members via additionalProperties");
}

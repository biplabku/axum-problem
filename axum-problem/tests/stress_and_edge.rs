use axum::body::to_bytes;
use axum::response::IntoResponse;
use axum_problem::{AxumProblem, Problem};
use http::StatusCode;
use thiserror::Error;

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn body_str(resp: axum::response::Response) -> String {
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// ── Unicode and special characters ────────────────────────────────────────────

#[derive(Debug, Error, AxumProblem)]
enum UnicodeError {
    #[error("Ошибка: пользователь {name} не найден")]  // Russian
    #[problem(status = 404)]
    NotFound { name: String },

    #[error("错误: 订单 {id} 不存在")]  // Chinese
    #[problem(status = 404)]
    OrderMissing { id: u64 },

    #[error("emoji in error: {emoji}")]
    #[problem(status = 400)]
    EmojiError { emoji: String },

    #[error("quote in message: it\\'s broken")]
    #[problem(status = 422)]
    QuoteInMessage,

    #[error("backslash: C:\\Users\\biplab")]
    #[problem(status = 400)]
    BackslashPath,

    #[error("newline in\nerror: {msg}")]
    #[problem(status = 400, mask)]
    NewlineInError { msg: String },
}

#[tokio::test]
async fn unicode_cyrillic_in_detail() {
    let resp = UnicodeError::NotFound { name: "Иван".into() }.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let json = body_json(resp).await;
    let detail = json["detail"].as_str().unwrap();
    assert!(detail.contains("Иван"), "Cyrillic must survive JSON roundtrip");
    // Valid JSON: body must parse correctly
    assert_eq!(json["status"], 404);
}

#[tokio::test]
async fn unicode_chinese_in_detail() {
    let resp = UnicodeError::OrderMissing { id: 42 }.into_response();
    let json = body_json(resp).await;
    let detail = json["detail"].as_str().unwrap();
    assert!(detail.contains("42"), "ID must be in Chinese error message");
    assert!(detail.len() > 0);
}

#[tokio::test]
async fn emoji_in_error_message() {
    let resp = UnicodeError::EmojiError { emoji: "🦀🔥".into() }.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp).await;
    let detail = json["detail"].as_str().unwrap();
    assert!(detail.contains("🦀"), "emoji must survive JSON roundtrip");
}

#[tokio::test]
async fn backslash_in_error_is_valid_json() {
    let resp = UnicodeError::BackslashPath.into_response();
    let raw = body_str(resp).await;
    // Most important: the body is valid JSON (backslash properly escaped)
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&raw);
    assert!(parsed.is_ok(), "backslash in error must produce valid JSON, got: {raw}");
}

#[tokio::test]
async fn newline_in_masked_error_does_not_reach_client() {
    let resp = UnicodeError::NewlineInError { msg: "secret line\ninjection".into() }.into_response();
    let json = body_json(resp).await;
    assert!(json.get("detail").is_none(), "masked error with newline must not reach client");
    let raw = serde_json::to_string(&json).unwrap();
    // No unescaped newlines in the response body
    assert!(!raw.contains('\n') || raw.contains("\\n"),
        "newlines in JSON must be escaped");
}

// ── Very long strings ─────────────────────────────────────────────────────────

#[derive(Debug, Error, AxumProblem)]
enum LongStringError {
    #[error("{msg}")]
    #[problem(status = 400)]
    Long { msg: String },

    #[error("{msg}")]
    #[problem(status = 500, mask)]
    LongMasked { msg: String },
}

#[tokio::test]
async fn very_long_error_message_is_valid_json() {
    let long = "x".repeat(100_000);
    let resp = LongStringError::Long { msg: long.clone() }.into_response();
    let raw = body_str(resp).await;
    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .expect("100k char error must produce valid JSON");
    assert!(parsed["detail"].as_str().unwrap().len() > 1000);
}

#[tokio::test]
async fn very_long_masked_string_does_not_reach_client() {
    let long = "secret ".repeat(10_000);
    let resp = LongStringError::LongMasked { msg: long }.into_response();
    let json = body_json(resp).await;
    assert!(json.get("detail").is_none());
    // Body must be small — no 70KB detail field
    let raw = serde_json::to_string(&json).unwrap();
    assert!(raw.len() < 500, "masked response must be compact, got {} bytes", raw.len());
}

// ── Empty/zero values in error fields ─────────────────────────────────────────

#[derive(Debug, Error, AxumProblem)]
enum EmptyFieldError {
    #[error("empty name: '{name}'")]
    #[problem(status = 422)]
    EmptyName { name: String },

    #[error("zero id: {id}")]
    #[problem(status = 404)]
    ZeroId { id: u64 },
}

#[tokio::test]
async fn empty_string_field_in_error() {
    let resp = EmptyFieldError::EmptyName { name: "".into() }.into_response();
    let json = body_json(resp).await;
    assert_eq!(json["detail"], "empty name: ''");
}

#[tokio::test]
async fn zero_id_in_error() {
    let resp = EmptyFieldError::ZeroId { id: 0 }.into_response();
    let json = body_json(resp).await;
    assert!(json["detail"].as_str().unwrap().contains("0"));
}

// ── Concurrent hammer ─────────────────────────────────────────────────────────

#[tokio::test]
async fn ten_thousand_concurrent_into_response() {
    #[derive(Debug, Error, AxumProblem, Clone)]
    enum HammerError {
        #[error("not found: {id}")]
        #[problem(status = 404)]
        NotFound { id: u64 },

        #[error("db: {0}")]
        #[problem(status = 500, mask)]
        Database(String),
    }

    let handles: Vec<_> = (0..5_000)
        .flat_map(|i| {
            let a = tokio::spawn(async move {
                HammerError::NotFound { id: i }.into_response()
            });
            let b = tokio::spawn(async move {
                HammerError::Database(format!("conn failed {i}")).into_response()
            });
            [a, b]
        })
        .collect();

    let results = futures::future::join_all(handles).await;
    let mut not_found = 0u32;
    let mut internal = 0u32;

    for r in results {
        let resp = r.expect("task must not panic");
        let status = resp.status().as_u16();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Every response must be valid and consistent
        assert_eq!(json["status"].as_u64().unwrap() as u16, status,
            "JSON status must match HTTP status under concurrent load");
        assert_eq!(json["title"].as_str().unwrap_or("").len() > 0, true,
            "title must exist under concurrent load");

        match status {
            404 => {
                not_found += 1;
                assert!(json["detail"].as_str().is_some(), "404 must have detail");
            }
            500 => {
                internal += 1;
                assert!(json.get("detail").is_none(), "500 masked must not have detail under load");
            }
            _ => panic!("unexpected status {status}"),
        }
    }

    assert_eq!(not_found, 5_000, "must have exactly 5000 404 responses");
    assert_eq!(internal, 5_000, "must have exactly 5000 500 responses");
}

// ── Multiple error types in same router ───────────────────────────────────────

#[tokio::test]
async fn multiple_error_types_in_same_router() {
    use axum::{routing::{get, post}, Router};
    use tower::ServiceExt;

    #[derive(Debug, Error, AxumProblem)]
    enum RouteAError {
        #[error("route-a problem")]
        #[problem(status = 422)]
        Problem,
    }

    #[derive(Debug, Error, AxumProblem)]
    enum RouteBError {
        #[error("route-b problem")]
        #[problem(status = 503)]
        Problem,
    }

    async fn handler_a() -> Result<(), RouteAError> { Err(RouteAError::Problem) }
    async fn handler_b() -> Result<(), RouteBError> { Err(RouteBError::Problem) }

    let app = Router::new()
        .route("/a", get(handler_a))
        .route("/b", post(handler_b));

    let req_a = http::Request::get("/a").body(axum::body::Body::empty()).unwrap();
    let req_b = http::Request::post("/b").body(axum::body::Body::empty()).unwrap();

    let (resp_a, resp_b) = tokio::join!(
        app.clone().oneshot(req_a),
        app.clone().oneshot(req_b),
    );

    let ra = resp_a.unwrap();
    let rb = resp_b.unwrap();

    assert_eq!(ra.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(rb.status(), StatusCode::SERVICE_UNAVAILABLE);

    let ja = body_json(ra).await;
    let jb = body_json(rb).await;
    assert_eq!(ja["status"], 422);
    assert_eq!(jb["status"], 503);
    assert_eq!(ja["detail"], "route-a problem");
    assert_eq!(jb["detail"], "route-b problem");
}

// ── Chained error with source() still masks correctly ─────────────────────────

#[tokio::test]
async fn chained_error_source_masked() {
    use std::io;

    #[derive(Debug, Error, AxumProblem)]
    enum ChainError {
        #[error("io error: {0}")]
        #[problem(status = 500, mask)]
        Io(#[from] io::Error),
    }

    let io_err = io::Error::new(io::ErrorKind::NotFound, "file /etc/secret.key not found");
    let resp = ChainError::Io(io_err).into_response();
    let json = body_json(resp).await;

    assert!(json.get("detail").is_none(), "io error path must be masked");
    assert!(!json.to_string().contains("secret.key"), "secret path must not leak");
    assert!(!json.to_string().contains("/etc"), "filesystem path must not leak");
}

// ── Problem builder: all fields set ──────────────────────────────────────────

#[tokio::test]
async fn problem_builder_all_fields_correct() {
    let p = Problem::new(422)
        .problem_type("https://api.example.com/errors/validation-failed")
        .title("Validation Failed")
        .detail("the 'email' field must be a valid email address")
        .instance("urn:request:abc-123");

    let resp = p.into_response();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let ct = resp.headers()
        .get(http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/problem+json");

    let json = body_json(resp).await;
    assert_eq!(json["type"], "https://api.example.com/errors/validation-failed");
    assert_eq!(json["title"], "Validation Failed");
    assert_eq!(json["status"], 422);
    assert_eq!(json["detail"], "the 'email' field must be a valid email address");
    assert_eq!(json["instance"], "urn:request:abc-123");
}

// ── Response is complete (no truncation) ──────────────────────────────────────

#[tokio::test]
async fn response_body_never_truncated() {
    #[derive(Debug, Error, AxumProblem)]
    enum TruncTest {
        #[error("error with long message: {msg}")]
        #[problem(status = 400)]
        Long { msg: String },
    }

    let msg = "A".repeat(50_000);
    let resp = TruncTest::Long { msg: msg.clone() }.into_response();
    let raw = body_str(resp).await;
    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .expect("body must be complete and parseable JSON");
    let detail = parsed["detail"].as_str().unwrap();
    assert!(detail.contains(&msg[..100]), "detail must contain the error message");
    assert!(detail.len() > 40_000, "detail must not be truncated");
}

// ── JSON output matches RFC 9457 field names exactly ─────────────────────────

#[tokio::test]
async fn rfc9457_field_names_exact() {
    let p = Problem::new(404)
        .problem_type("https://example.com/problems/not-found")
        .detail("resource missing")
        .instance("/resources/42");
    let resp = p.into_response();
    let raw = body_str(resp).await;

    // RFC 9457 mandates these exact field names
    assert!(raw.contains(r#""type""#), "must use 'type' not 'problem_type'");
    assert!(raw.contains(r#""title""#));
    assert!(raw.contains(r#""status""#));
    assert!(raw.contains(r#""detail""#));
    assert!(raw.contains(r#""instance""#));

    // Must NOT use non-standard field names
    assert!(!raw.contains(r#""problem_type""#), "must not use 'problem_type'");
    assert!(!raw.contains(r#""error""#), "must not use 'error'");
    assert!(!raw.contains(r#""message""#), "must not use 'message'");
}

// ── Response has no extra unexpected headers ──────────────────────────────────

#[tokio::test]
async fn response_headers_only_expected() {
    let resp = Problem::not_found().into_response();
    let headers = resp.headers().clone();

    // Must have content-type
    assert!(headers.contains_key(http::header::CONTENT_TYPE));

    // Should not have unexpected custom headers
    assert!(!headers.contains_key("x-error-code"), "no custom error code headers");
    assert!(!headers.contains_key("x-request-id"), "no auto-generated request ID");
}

// ── Status 200 (unusual but not blocked) ──────────────────────────────────────

#[tokio::test]
async fn status_200_problem_is_consistent() {
    // Using 200 as a problem status is unusual but some APIs do it.
    // The crate should not block it — just be consistent.
    let p = Problem::new(200).detail("unexpected success");
    let resp = p.into_response();
    let http = resp.status().as_u16();
    let json = body_json(resp).await;
    let json_status = json["status"].as_u64().unwrap() as u16;
    assert_eq!(http, json_status, "HTTP and JSON status must agree even for 200");
}

// ── Derive: empty detail string is serialized ─────────────────────────────────

#[tokio::test]
async fn empty_detail_string_serialized() {
    let p = Problem::new(400).detail("");
    let json = body_json(p.into_response()).await;
    // Empty string is Some("") — should still appear in JSON
    assert_eq!(json["detail"], "");
}

// ── Error enum as module-private type ─────────────────────────────────────────

mod private_module {
    use axum_problem::AxumProblem;
    use thiserror::Error;

    #[derive(Debug, Error, AxumProblem)]
    pub(crate) enum PrivateError {
        #[error("private error")]
        #[problem(status = 500, mask)]
        Internal,
    }
}

#[tokio::test]
async fn private_module_error_works() {
    use private_module::PrivateError;
    let resp = PrivateError::Internal.into_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body_json(resp).await;
    assert!(json.get("detail").is_none(), "private internal error must be masked");
}

use axum::body::to_bytes;
use axum::response::IntoResponse;
use axum_problem::{AxumProblem, Problem};
use http::StatusCode;
use thiserror::Error;

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ── Bug 1: Status code in JSON body must match HTTP status ────────────────────
//
// Problem::new(999): HTTP/1.1 allows 100-999 but hyper may reject some.
// Problem::new(0): invalid — body says "status":0 but HTTP returns 500.
// These must be consistent — what appears in the JSON must match the HTTP line.

#[tokio::test]
async fn json_status_matches_http_status_for_valid_code() {
    let p = Problem::new(422);
    let resp = p.into_response();
    let http_status = resp.status().as_u16();
    let json = body_json(resp).await;
    let json_status = json["status"].as_u64().unwrap() as u16;
    assert_eq!(http_status, json_status,
        "HTTP status line ({http_status}) must match JSON status field ({json_status})");
}

#[tokio::test]
async fn invalid_status_code_json_matches_actual_http_status() {
    // If the user passes status=0 (invalid), we must not produce a response
    // where the JSON body says {"status":0} but the HTTP line is 500.
    // Both must agree.
    let p = Problem::new(0);
    let resp = p.into_response();
    let http_status = resp.status().as_u16();
    let json = body_json(resp).await;
    let json_status = json["status"].as_u64().unwrap() as u16;
    assert_eq!(http_status, json_status,
        "JSON status must match HTTP status even for invalid input codes");
}

// ── Bug 2: Generic enum must compile with Display bound ───────────────────────

#[tokio::test]
async fn generic_enum_works_when_t_is_display() {
    #[derive(Debug, Error, AxumProblem)]
    enum GenericError<T: std::fmt::Display + std::fmt::Debug> {
        #[error("inner: {0}")]
        #[problem(status = 422)]
        Inner(T),

        #[error("masked: {0}")]
        #[problem(status = 500, mask)]
        MaskedInner(T),
    }

    let resp = GenericError::Inner("bad input".to_string()).into_response();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(resp).await;
    assert!(json["detail"].as_str().unwrap().contains("bad input"));

    let resp2 = GenericError::MaskedInner(42i32).into_response();
    assert_eq!(resp2.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json2 = body_json(resp2).await;
    assert!(json2.get("detail").is_none(), "masked generic must not expose T");
}

// ── Bug 3: From<> conversions work with ? operator ────────────────────────────

#[tokio::test]
async fn from_conversion_works_with_question_mark() {
    use axum::{extract::Path, routing::get, Json, Router};
    use std::num::ParseIntError;
    use tower::ServiceExt;

    #[derive(Debug, Error, AxumProblem)]
    enum ParseError {
        #[error("invalid number: {0}")]
        #[problem(status = 400)]
        BadNumber(#[from] ParseIntError),
    }

    async fn handler(Path(s): Path<String>) -> Result<Json<i64>, ParseError> {
        let n: i64 = s.parse()?;  // ? converts ParseIntError → ParseError::BadNumber
        Ok(Json(n))
    }

    let app = Router::new().route("/:s", get(handler));
    let req = http::Request::builder()
        .uri("/not-a-number")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp).await;
    assert_eq!(json["status"], 400);
    assert!(json["detail"].as_str().unwrap().contains("invalid number"));
}

// ── Bug 4: Large enum (many variants) works correctly ────────────────────────

#[tokio::test]
async fn large_enum_all_variants_correct() {
    #[derive(Debug, Error, AxumProblem)]
    enum BigError {
        #[error("a")] #[problem(status = 400)] A,
        #[error("b")] #[problem(status = 401)] B,
        #[error("c")] #[problem(status = 403)] C,
        #[error("d")] #[problem(status = 404)] D,
        #[error("e")] #[problem(status = 405)] E,
        #[error("f")] #[problem(status = 408)] F,
        #[error("g")] #[problem(status = 409)] G,
        #[error("h")] #[problem(status = 422)] H,
        #[error("i")] #[problem(status = 429)] I,
        #[error("j: {0}")] #[problem(status = 500, mask)] J(String),
    }

    let cases = [
        (BigError::A, 400u16), (BigError::B, 401), (BigError::C, 403),
        (BigError::D, 404), (BigError::E, 405), (BigError::F, 408),
        (BigError::G, 409), (BigError::H, 422), (BigError::I, 429),
        (BigError::J("secret".into()), 500),
    ];

    for (variant, expected_status) in cases {
        let resp = variant.into_response();
        let got = resp.status().as_u16();
        let json = body_json(resp).await;
        assert_eq!(got, expected_status, "variant should return {expected_status}");
        assert_eq!(json["status"].as_u64().unwrap() as u16, expected_status);
    }
}

// ── Bug 5: Concurrent into_response calls (takes self by value — safe) ────────

#[tokio::test]
async fn concurrent_into_response_is_safe() {
    use std::sync::Arc;

    #[derive(Debug, Error, AxumProblem, Clone)]
    enum ConcurrentError {
        #[error("not found")]
        #[problem(status = 404)]
        NotFound,
    }

    let handles: Vec<_> = (0..100).map(|_| {
        tokio::spawn(async {
            ConcurrentError::NotFound.into_response()
        })
    }).collect();

    let results = futures::future::join_all(handles).await;
    for r in results {
        let resp = r.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

// ── Bug 6: Problem builder is Send + Sync ─────────────────────────────────────

#[test]
fn problem_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Problem>();
}

// ── Bug 7: Nested error with mask doesn't leak inner type info ─────────────────

#[tokio::test]
async fn nested_error_masking_security() {
    // A real scenario: wrapping sqlx errors
    #[derive(Debug, Error, AxumProblem)]
    enum DbError {
        #[error("query failed: column={col} table={table} constraint={constraint}")]
        #[problem(status = 500, mask)]
        QueryFailed {
            col: String,
            table: String,
            constraint: String,
        },
    }

    let resp = DbError::QueryFailed {
        col: "email".into(),
        table: "users".into(),
        constraint: "users_email_key".into(),
    }.into_response();

    let json = body_json(resp).await;

    // None of these internal details must appear in the response
    let resp_str = json.to_string();
    assert!(!resp_str.contains("email"), "column name must not leak");
    assert!(!resp_str.contains("users"), "table name must not leak");
    assert!(!resp_str.contains("constraint"), "constraint name must not leak");
    assert!(!resp_str.contains("users_email_key"), "constraint value must not leak");
    assert!(json.get("detail").is_none());
}

// ── Bug 8: Problem with no detail still has correct structure ─────────────────

#[tokio::test]
async fn problem_without_detail_is_valid_rfc9457() {
    let p = Problem::new(403);
    let resp = p.into_response();
    let json = body_json(resp).await;

    // Required fields per RFC 9457
    assert!(json.get("title").is_some(), "title is required");
    assert!(json.get("status").is_some(), "status is required");
    // Optional fields absent
    assert!(json.get("detail").is_none());
    assert!(json.get("type").is_none());
    assert!(json.get("instance").is_none());
}

// ── Bug 9: Enum implementing multiple traits simultaneously ───────────────────

#[tokio::test]
async fn implements_multiple_traits_simultaneously() {
    use std::fmt;

    #[derive(Debug, Error, AxumProblem, Clone, PartialEq)]
    enum MultiTraitError {
        #[error("bad request: {reason}")]
        #[problem(status = 400)]
        BadRequest { reason: String },
    }

    // All these must work without conflict
    let e = MultiTraitError::BadRequest { reason: "test".into() };
    let cloned = e.clone();
    assert_eq!(e, cloned);
    let display = format!("{}", e);
    assert_eq!(display, "bad request: test");
    let resp = e.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── Bug 10: All standard status codes have non-empty titles ───────────────────

#[test]
fn all_common_status_codes_have_proper_titles() {
    use axum_problem::status_title;
    let common = [400u16, 401, 403, 404, 405, 408, 409, 410, 415, 422, 429,
                  500, 501, 502, 503, 504];
    for code in common {
        let title = status_title(code);
        assert!(!title.is_empty(), "status {code} must have non-empty title");
        assert_ne!(title, "Error",
            "status {code} should have a specific title, not the generic fallback");
    }
}

// ── Bug 11: Problem body always valid JSON even on error path ─────────────────

#[tokio::test]
async fn response_body_is_always_parseable_json() {
    use axum_problem::Problem;

    // Various edge case constructions
    let cases = vec![
        Problem::new(400),
        Problem::new(500).detail(""),            // empty detail
        Problem::new(422).title(""),              // empty title
        Problem::new(200),                        // success status (unusual but valid)
        Problem::new(418),                        // teapot
        Problem::new(999),                        // high but valid range edge
    ];

    for p in cases {
        let status = p.status;
        let resp = p.into_response();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let result: Result<serde_json::Value, _> = serde_json::from_slice(&bytes);
        assert!(result.is_ok(), "response body must be valid JSON for status {status}");
    }
}

// ── Bug 12: Using into_response twice would fail (self consumed) ──────────────
// This tests that the API contract is clear — verify it works once correctly

#[tokio::test]
async fn into_response_consumes_self_produces_correct_response() {
    #[derive(Debug, Error, AxumProblem)]
    enum ConsumeTest {
        #[error("consumed")]
        #[problem(status = 400)]
        Consumed,
    }

    let err = ConsumeTest::Consumed;
    let resp = err.into_response(); // consumes err
    // err is no longer usable here — Rust type system enforces this
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

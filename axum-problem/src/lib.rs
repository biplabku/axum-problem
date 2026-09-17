//! RFC 9457 problem details for axum.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use axum::{Router, routing::get, extract::Path};
//! use axum_problem::{AxumProblem, Problem};
//! use thiserror::Error;
//!
//! #[derive(Debug, Error, AxumProblem)]
//! pub enum ApiError {
//!     #[error("order {id} not found")]
//!     #[problem(status = 404)]
//!     NotFound { id: i64 },
//!
//!     #[error("unauthorized")]
//!     #[problem(status = 401)]
//!     Unauthorized,
//!
//!     #[error("database error: {0}")]
//!     #[problem(status = 500, mask)]
//!     Database(String),
//! }
//!
//! async fn get_order(Path(id): Path<i64>) -> Result<String, ApiError> {
//!     Err(ApiError::NotFound { id })
//! }
//! ```
//!
//! The `mask` attribute hides the error detail from the HTTP response (preventing
//! internal details leaking to clients) and logs the full error via `tracing::error!`.

pub use axum_problem_derive::AxumProblem;

use axum::response::{IntoResponse, Response};
use http::{header, StatusCode};
use serde::Serialize;

/// An RFC 9457 problem details response.
///
/// Serializes to `application/problem+json` with the correct HTTP status code.
///
/// # Manual construction
///
/// ```rust
/// use axum_problem::Problem;
///
/// let p = Problem::new(404)
///     .title("Order Not Found")
///     .detail("Order 1001 does not exist");
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    /// URI reference identifying the problem type. Defaults to `"about:blank"`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub problem_type: Option<String>,

    /// Short human-readable summary of the problem type.
    pub title: String,

    /// HTTP status code.
    pub status: u16,

    /// Human-readable explanation specific to this occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// URI identifying the specific occurrence of this problem.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl Problem {
    /// Create a new problem for the given HTTP status code.
    /// The title is set automatically from the status code.
    pub fn new(status: u16) -> Self {
        Self {
            problem_type: None,
            title: status_title(status).to_owned(),
            status,
            detail: None,
            instance: None,
        }
    }

    /// Set the problem type URI (defaults to `"about:blank"` in the response).
    pub fn problem_type(mut self, t: impl Into<String>) -> Self {
        self.problem_type = Some(t.into());
        self
    }

    /// Override the title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Add a human-readable explanation for this specific occurrence.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add a URI identifying this specific occurrence.
    pub fn instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    // ── Convenience constructors ──────────────────────────────────────────────

    pub fn bad_request() -> Self { Self::new(400) }
    pub fn unauthorized() -> Self { Self::new(401) }
    pub fn forbidden() -> Self { Self::new(403) }
    pub fn not_found() -> Self { Self::new(404) }
    pub fn method_not_allowed() -> Self { Self::new(405) }
    pub fn conflict() -> Self { Self::new(409) }
    pub fn unprocessable_entity() -> Self { Self::new(422) }
    pub fn too_many_requests() -> Self { Self::new(429) }
    pub fn internal_server_error() -> Self { Self::new(500) }
    pub fn not_implemented() -> Self { Self::new(501) }
    pub fn service_unavailable() -> Self { Self::new(503) }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let body = serde_json::to_string(&self)
            .unwrap_or_else(|_| r#"{"title":"Internal Server Error","status":500}"#.to_owned());

        (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            body,
        )
            .into_response()
    }
}

/// Returns a short title for common HTTP status codes.
pub fn status_title(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_string(resp: Response) -> String {
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn problem_serializes_to_correct_json() {
        let p = Problem::not_found().detail("order 1001 not found");
        let resp = p.into_response();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );

        let body = body_string(resp).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["status"], 404);
        assert_eq!(json["title"], "Not Found");
        assert_eq!(json["detail"], "order 1001 not found");
    }

    #[tokio::test]
    async fn problem_omits_optional_fields() {
        let p = Problem::new(400);
        let resp = p.into_response();
        let body = body_string(resp).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json.get("detail").is_none());
        assert!(json.get("instance").is_none());
        assert!(json.get("type").is_none());
    }

    #[tokio::test]
    async fn problem_with_type_uri() {
        let p = Problem::new(422)
            .problem_type("https://errors.example.com/validation-failed")
            .detail("email is required");
        let resp = p.into_response();
        let body = body_string(resp).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["type"], "https://errors.example.com/validation-failed");
    }

    #[tokio::test]
    async fn status_title_coverage() {
        assert_eq!(status_title(404), "Not Found");
        assert_eq!(status_title(500), "Internal Server Error");
        assert_eq!(status_title(999), "Error");
    }
}

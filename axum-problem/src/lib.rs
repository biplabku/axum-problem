//! RFC 9457 problem details for axum.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use axum::{Router, routing::get, extract::Path};
//! use axum_problem::{AxumProblem, Problem, ProblemLayer};
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
//!
//! // Apply ProblemLayer to catch axum's own extractor errors
//! // (bad JSON body, wrong content-type, etc.) and convert them to RFC 9457.
//! let app: Router = Router::new()
//!     .route("/orders/:id", get(get_order))
//!     .layer(ProblemLayer);
//! ```
//!
//! The `mask` attribute hides the error detail from the HTTP response (preventing
//! internal details leaking to clients) and logs the full error via `tracing::error!`.
//!
//! # ProblemLayer
//!
//! [`ProblemLayer`] is a Tower middleware that ensures **every** error response
//! from your API uses `application/problem+json`. It intercepts any 4xx/5xx
//! response that isn't already a problem response and wraps it:
//!
//! ```rust,no_run
//! use axum::Router;
//! use axum_problem::ProblemLayer;
//!
//! let app: Router = Router::new()
//!     /* ... routes ... */
//!     .layer(ProblemLayer);
//! ```
//!
//! Responses already in `application/problem+json` format pass through unchanged.

pub use axum_problem_derive::AxumProblem;

use axum::response::{IntoResponse, Response};
use http::{header, StatusCode};
use serde::Serialize;

/// An RFC 9457 problem details response.
///
/// Serializes to `application/problem+json` with the correct HTTP status code.
/// Extension members (RFC 9457 §3.5) are serialized as top-level JSON fields
/// alongside the standard members.
///
/// # Manual construction
///
/// ```rust
/// use axum_problem::Problem;
/// use serde_json::json;
///
/// let p = Problem::new(422)
///     .title("Validation Failed")
///     .detail("One or more fields are invalid")
///     .extension("violations", json!([
///         {"field": "email", "message": "must be a valid email"},
///         {"field": "age",   "message": "must be at least 18"},
///     ]));
/// ```
///
/// Produces:
/// ```json
/// {
///   "title": "Validation Failed",
///   "status": 422,
///   "detail": "One or more fields are invalid",
///   "violations": [...]
/// }
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

    /// RFC 9457 extension members — serialized as top-level JSON fields.
    ///
    /// Use [`Problem::extension`] to add fields fluently.
    #[serde(flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extensions: serde_json::Map<String, serde_json::Value>,
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
            extensions: serde_json::Map::new(),
        }
    }

    /// Add an RFC 9457 extension member as a top-level JSON field.
    ///
    /// ```rust
    /// use axum_problem::Problem;
    /// use serde_json::json;
    ///
    /// let p = Problem::new(422)
    ///     .extension("violations", json!([{"field":"email","message":"invalid"}]))
    ///     .extension("request_id", json!("req-abc-123"));
    /// ```
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
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

        // Sync the JSON `status` field with the actual HTTP status line.
        // If the caller passed an invalid code (e.g. 0), both the HTTP status
        // and the JSON body must agree — returning 500 in both.
        let mut serializable = self;
        serializable.status = status.as_u16();

        let body = serde_json::to_string(&serializable)
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

// ── utoipa integration (optional feature) ────────────────────────────────────

#[cfg(feature = "utoipa")]
mod utoipa_impl {
    use super::Problem;
    use utoipa::openapi::{
        ObjectBuilder, RefOr, Schema,
        schema::{SchemaType, SchemaFormat, KnownFormat, AdditionalProperties},
        response::ResponseBuilder,
        content::ContentBuilder,
    };

    impl<'__s> utoipa::ToSchema<'__s> for Problem {
        fn schema() -> (&'__s str, RefOr<Schema>) {
            (
                "Problem",
                RefOr::T(Schema::Object(
                    ObjectBuilder::new()
                        .description(Some("RFC 9457 problem details (application/problem+json)"))
                        .property(
                            "type",
                            ObjectBuilder::new()
                                .schema_type(SchemaType::String)
                                .description(Some("URI reference identifying the problem type"))
                                .build(),
                        )
                        .property(
                            "title",
                            ObjectBuilder::new()
                                .schema_type(SchemaType::String)
                                .description(Some("Short human-readable summary of the problem type"))
                                .build(),
                        )
                        .required("title")
                        .property(
                            "status",
                            ObjectBuilder::new()
                                .schema_type(SchemaType::Integer)
                                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
                                .description(Some("HTTP status code"))
                                .build(),
                        )
                        .required("status")
                        .property(
                            "detail",
                            ObjectBuilder::new()
                                .schema_type(SchemaType::String)
                                .description(Some("Human-readable explanation specific to this occurrence"))
                                .build(),
                        )
                        .property(
                            "instance",
                            ObjectBuilder::new()
                                .schema_type(SchemaType::String)
                                .description(Some("URI identifying the specific occurrence of this problem"))
                                .build(),
                        )
                        // RFC 9457 extension members — any additional top-level fields
                        .additional_properties(Some(AdditionalProperties::FreeForm(true)))
                        .build(),
                )),
            )
        }
    }
}

// ── ProblemLayer ──────────────────────────────────────────────────────────────

/// Tower middleware layer that converts non-problem error responses to RFC 9457.
///
/// Any 4xx or 5xx response that doesn't have `Content-Type: application/problem+json`
/// is replaced with a [`Problem`] response using the same HTTP status code.
/// 2xx and 3xx responses pass through unchanged.
/// Responses already in problem+json format also pass through unchanged.
///
/// Apply it **last** (outermost layer) in your axum router so it catches errors
/// from all other middleware and extractors:
///
/// ```rust,no_run
/// use axum::Router;
/// use axum_problem::ProblemLayer;
///
/// let app: Router = Router::new()
///     /* ... routes and other layers ... */
///     .layer(ProblemLayer);
/// ```
#[derive(Clone, Copy, Default)]
pub struct ProblemLayer;

impl<S> tower_layer::Layer<S> for ProblemLayer {
    type Service = ProblemService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        ProblemService { inner }
    }
}

/// Tower service produced by [`ProblemLayer`].
#[derive(Clone)]
pub struct ProblemService<S> {
    inner: S,
}

impl<S, ReqBody> tower::Service<http::Request<ReqBody>> for ProblemService<S>
where
    S: tower::Service<http::Request<ReqBody>, Response = axum::response::Response>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        use axum::response::IntoResponse;
        let future = self.inner.call(req);
        Box::pin(async move {
            let resp = future.await?;
            let status = resp.status();

            if status.is_client_error() || status.is_server_error() {
                let already_problem = resp
                    .headers()
                    .get(http::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|ct| ct.contains("application/problem+json"))
                    .unwrap_or(false);

                if !already_problem {
                    return Ok(Problem::new(status.as_u16()).into_response());
                }
            }

            Ok(resp)
        })
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

    // ── extensions field tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn extension_field_serialized_at_top_level() {
        let p = Problem::new(422)
            .detail("validation failed")
            .extension("violations", serde_json::json!([
                {"field": "email", "message": "invalid"}
            ]));

        let resp = p.into_response();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Extension must be a TOP-LEVEL field, not nested under "extensions"
        assert!(json.get("violations").is_some(), "violations must be a top-level field");
        assert!(json.get("extensions").is_none(), "must not have a nested 'extensions' key");
        assert_eq!(json["violations"][0]["field"], "email");
        assert_eq!(json["status"], 422);
        assert_eq!(json["detail"], "validation failed");
    }

    #[tokio::test]
    async fn multiple_extensions_all_at_top_level() {
        let p = Problem::new(400)
            .extension("request_id", serde_json::json!("req-abc-123"))
            .extension("correlation_id", serde_json::json!("corr-xyz"))
            .extension("retry_after", serde_json::json!(30));

        let resp = p.into_response();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["request_id"], "req-abc-123");
        assert_eq!(json["correlation_id"], "corr-xyz");
        assert_eq!(json["retry_after"], 30);
    }

    #[tokio::test]
    async fn no_extensions_means_no_extra_fields() {
        let p = Problem::new(404).detail("not found");
        let resp = p.into_response();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Only standard RFC 9457 fields present
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        for key in &keys {
            assert!(
                ["title", "status", "detail", "instance", "type"].contains(key),
                "unexpected extra key in response: {key}"
            );
        }
    }

    #[tokio::test]
    async fn extension_is_valid_rfc9457_json() {
        let p = Problem::new(400)
            .extension("errors", serde_json::json!({"count": 3}));
        let resp = p.into_response();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Standard fields still present
        assert!(json["title"].is_string());
        assert_eq!(json["status"], 400);
        // Extension present
        assert_eq!(json["errors"]["count"], 3);
    }

    // ── ProblemLayer tests ────────────────────────────────────────────────────

    use axum::{Router, routing::get, body::Body};
    use tower::ServiceExt;

    async fn handler_404() -> impl axum::response::IntoResponse {
        (http::StatusCode::NOT_FOUND, "plain text 404")
    }

    async fn handler_200() -> impl axum::response::IntoResponse {
        "ok"
    }

    async fn handler_problem() -> impl axum::response::IntoResponse {
        Problem::not_found().detail("already a problem")
    }

    fn layered_app() -> Router {
        Router::new()
            .route("/notfound", get(handler_404))
            .route("/ok", get(handler_200))
            .route("/problem", get(handler_problem))
            .layer(ProblemLayer)
    }

    async fn call(app: &Router, uri: &str) -> axum::response::Response {
        app.clone()
            .oneshot(
                http::Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn problem_layer_converts_plain_404_to_problem_json() {
        let app = layered_app();
        let resp = call(&app, "/notfound").await;

        assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );

        let body = body_string(resp).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["status"], 404);
        assert_eq!(json["title"], "Not Found");
    }

    #[tokio::test]
    async fn problem_layer_passes_through_2xx() {
        let app = layered_app();
        let resp = call(&app, "/ok").await;
        assert_eq!(resp.status(), http::StatusCode::OK);
        // Content-Type must NOT be problem+json for a 200
        let ct = resp.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(!ct.contains("application/problem+json"));
    }

    #[tokio::test]
    async fn problem_layer_passes_through_existing_problem_json() {
        let app = layered_app();
        let resp = call(&app, "/problem").await;

        assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );

        // Detail must still be present — not overwritten by the layer
        let body = body_string(resp).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["detail"], "already a problem");
    }

    #[tokio::test]
    async fn problem_layer_converts_axum_json_extractor_error() {
        // When axum's JSON extractor fails (bad body), it returns 422 with
        // plain text. ProblemLayer must convert it to problem+json.
        use axum::extract::Json;

        async fn needs_json(_: Json<serde_json::Value>) -> &'static str { "ok" }

        let app = Router::new()
            .route("/json", axum::routing::post(needs_json))
            .layer(ProblemLayer);

        let resp = app
            .oneshot(
                http::Request::builder()
                    .method("POST")
                    .uri("/json")
                    .header("content-type", "application/json")
                    .body(Body::from("not valid json {{"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(resp.status().is_client_error());
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json",
            "axum extractor error must be converted to problem+json by ProblemLayer"
        );

        let body = body_string(resp).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json["status"].as_u64().unwrap() >= 400);
        assert!(json["title"].is_string());
    }
}

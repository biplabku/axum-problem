# axum-problem

RFC 9457 problem details for axum — `#[derive(AxumProblem)]` converts your error enums to structured HTTP error responses with one attribute.

```toml
[dependencies]
axum-problem = "0.1"
thiserror = "2"     # optional but recommended
```

---

## The problem

Every axum app needs structured error responses. Without this, you write 30+ lines of boilerplate per error type, risk exposing internal details to clients, and return the wrong `Content-Type`.

```rust
// ❌ What you write today — manually for every error enum
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound { id } => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
            Self::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal"}))).into_response(),
            // ...
        }
    }
}
```

## The solution

```rust
// ✅ axum-problem: one attribute per variant, derive does the rest
use axum_problem::AxumProblem;
use thiserror::Error;

#[derive(Debug, Error, AxumProblem)]
pub enum ApiError {
    #[error("order {id} not found")]
    #[problem(status = 404)]
    NotFound { id: i64 },

    #[error("access denied")]
    #[problem(status = 401)]
    Unauthorized,

    #[error("db error: {0}")]
    #[problem(status = 500, mask)]   // ← hides internal details from clients
    Database(String),
}

// Works directly in axum handlers — no extra code
async fn get_order(Path(id): Path<i64>) -> Result<Json<Order>, ApiError> {
    db.find(id).await.map(Json).map_err(ApiError::Database)
}
```

HTTP response from `ApiError::NotFound { id: 1001 }`:

```
HTTP/1.1 404 Not Found
Content-Type: application/problem+json

{
  "title": "Not Found",
  "status": 404,
  "detail": "order 1001 not found"
}
```

---

## Attributes

### `status = <u16>` (required)

The HTTP status code for this variant. Auto-derives the title from the status code.

### `title = "<str>"` (optional)

Override the default title:

```rust
#[problem(status = 409, title = "Email Already Registered")]
EmailConflict,
```

### `mask` (optional)

**The safety attribute.** Hides the error detail from the HTTP response and logs it via `tracing::error!` instead.

Use for any variant that could expose internal information — database errors, upstream service errors, file paths, etc.

```rust
#[error("db: {0}")]
#[problem(status = 500, mask)]
Database(sqlx::Error),
// Client receives:  {"title": "Internal Server Error", "status": 500}
// Your logs receive: ERROR error="connection refused" status=500
```

---

## What the response looks like

All variants produce `Content-Type: application/problem+json` per RFC 9457:

```json
{
  "type": "about:blank",        // only if problem_type() is set
  "title": "Not Found",         // from status code or custom title
  "status": 404,
  "detail": "order 1001 not found",   // absent when mask is set
  "instance": "/orders/1001"          // only if instance() is set
}
```

---

## Manual construction

For one-off errors where a full enum is overkill:

```rust
use axum_problem::Problem;
use serde_json::json;

async fn handler() -> impl IntoResponse {
    Problem::not_found()
        .detail("The requested resource does not exist")
        .instance("/orders/1001")
}
```

Convenience constructors: `bad_request()`, `unauthorized()`, `forbidden()`,
`not_found()`, `conflict()`, `unprocessable_entity()`, `too_many_requests()`,
`internal_server_error()`, `service_unavailable()`.

### RFC 9457 extension members

Add arbitrary top-level fields alongside the standard ones:

```rust
Problem::new(422)
    .detail("Validation failed")
    .extension("violations", json!([
        {"field": "email", "message": "must be a valid email address"},
        {"field": "age",   "message": "must be at least 18"},
    ]))
    .extension("request_id", json!("req-abc-123"))
```

Produces:
```json
{
  "title": "Unprocessable Entity",
  "status": 422,
  "detail": "Validation failed",
  "violations": [...],
  "request_id": "req-abc-123"
}
```

Extension fields are serialized flat at the top level — not nested under an
`"extensions"` key — as RFC 9457 §3.5 requires.

---

## ProblemLayer — catch axum's own errors

axum's built-in extractors (bad JSON body, wrong Content-Type, missing fields)
return plain text errors, not `application/problem+json`. `ProblemLayer` fixes
this by intercepting any 4xx/5xx response that isn't already a problem response:

```rust
use axum::Router;
use axum_problem::ProblemLayer;

let app = Router::new()
    /* ... your routes ... */
    .layer(ProblemLayer);
```

Now every error — including axum extractor failures — returns structured
`application/problem+json`. Responses already in problem+json format pass
through unchanged (extension fields preserved).

---

## utoipa / OpenAPI integration

Enable the `utoipa` feature to expose `Problem` in your OpenAPI spec:

```toml
[dependencies]
axum-problem = { version = "0.1", features = ["utoipa"] }
utoipa = "4"
```

`Problem` implements `ToSchema` — use it directly in `#[utoipa::path]` responses:

```rust
#[utoipa::path(
    get, path = "/orders/{id}",
    responses(
        (status = 200, body = Order),
        (status = 404, body = Problem, description = "Order not found"),
        (status = 422, body = Problem, description = "Validation failed"),
        (status = 500, body = Problem, description = "Internal error"),
    )
)]
async fn get_order(Path(id): Path<i64>) -> Result<Json<Order>, ApiError> {
    // ...
}
```

The generated schema includes all RFC 9457 standard fields plus
`additionalProperties: true` to represent extension members.

---

## Works without thiserror

`AxumProblem` only requires `Display` — you can use it with any error type:

```rust
#[derive(Debug, AxumProblem)]
enum MyError {
    #[problem(status = 503)]
    Down,
}

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "service is temporarily unavailable")
    }
}
impl std::error::Error for MyError {}
```

---

## Variant types supported

| Variant style | Example | Detail source |
|---|---|---|
| Unit | `Unauthorized` | Enum's `Display` |
| Struct (named fields) | `NotFound { id: i64 }` | Enum's `Display` |
| Tuple (1 field) | `Database(String)` | Enum's `Display` |
| Tuple (multiple fields) | `RateLimit(u32, u32)` | Enum's `Display` |
| Any + `mask` | `Database(String)` | Suppressed; logged via tracing |

All variants display using the enum's `Display` implementation — so thiserror's `#[error("...")]` templates apply correctly.

---

## License

MIT OR Apache-2.0

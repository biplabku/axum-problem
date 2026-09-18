# axum-problem

[![crates.io](https://img.shields.io/crates/v/axum-problem.svg)](https://crates.io/crates/axum-problem)
[![docs.rs](https://docs.rs/axum-problem/badge.svg)](https://docs.rs/axum-problem)
[![MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

RFC 9457 problem details for axum — `#[derive(AxumProblem)]` converts your error enums to structured HTTP error responses with one attribute.

```toml
[dependencies]
axum-problem = "0.1"
thiserror = "2"     # optional but recommended
```

---

## The problem

Every axum app needs structured error responses. Without a standard, every team reinvents it differently, risks leaking internal details to clients, and returns the wrong `Content-Type`.

```rust
// ❌ What you write today — 30+ lines of boilerplate per error type
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound { id } => (StatusCode::NOT_FOUND,
                Json(json!({"error": "not found"}))).into_response(),
            Self::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"}))).into_response(),
        }
    }
}
```

## The solution

```rust
// ✅ axum-problem: one attribute per variant
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
    #[problem(status = 500, mask)]   // hides internal details from clients
    Database(String),
}

// Works directly in axum handlers
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

| Attribute | Required | Description |
|---|---|---|
| `status = <u16>` | Yes | HTTP status code. Title auto-derived from the code. |
| `title = "<str>"` | No | Override the default title. |
| `mask` | No | Hides detail from HTTP response; logs it via `tracing::error!`. |

### The `mask` attribute — production safety

Without `mask`, internal errors reach clients:

```
{"detail": "connection to 10.0.0.5:5432 refused"}  ← exposes your network
```

With `mask`:
```
{"title": "Internal Server Error", "status": 500}   ← clean client response
ERROR error="connection to 10.0.0.5:5432 refused" status=500  ← in your logs
```

---

## All variant types supported

```rust
#[derive(Debug, Error, AxumProblem)]
pub enum ApiError {
    // Unit variant
    #[error("unauthorized")]
    #[problem(status = 401)]
    Unauthorized,

    // Struct variant (named fields)
    #[error("order {id} not found")]
    #[problem(status = 404)]
    NotFound { id: i64 },

    // Tuple variant
    #[error("validation: {0}")]
    #[problem(status = 422)]
    Validation(String),

    // Any variant + mask
    #[error("db: {0}")]
    #[problem(status = 500, mask)]
    Database(String),

    // Custom title
    #[error("email already taken")]
    #[problem(status = 409, title = "Email Conflict")]
    EmailConflict,
}
```

All variants use the enum's `Display` implementation — so thiserror's `#[error("...")]` templates apply correctly.

---

## Manual construction

For one-off errors:

```rust
use axum_problem::Problem;

Problem::not_found()
    .detail("The requested resource does not exist")
    .instance("/orders/1001")
```

Convenience constructors: `bad_request()`, `unauthorized()`, `forbidden()`,
`not_found()`, `conflict()`, `unprocessable_entity()`, `too_many_requests()`,
`internal_server_error()`, `service_unavailable()`.

---

## Works without thiserror

`AxumProblem` only requires `Display`:

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

## Crates in this workspace

| Crate | Description |
|---|---|
| [`axum-problem`](axum-problem/) | Main library — `Problem` struct + `AxumProblem` re-export |
| [`axum-problem-derive`](axum-problem-derive/) | Proc-macro — `#[derive(AxumProblem)]` |

---

## Testing

59 tests covering unit, derive macro (all variant styles), adversarial (Unicode, long strings, 10k concurrent requests), RFC 9457 compliance, and security (masking field values, chained error sources).

---

## License

MIT OR Apache-2.0

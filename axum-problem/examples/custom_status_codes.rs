//! Shows how different enum variants map to different HTTP status codes,
//! plus the `title` override and `mask` attributes.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example custom_status_codes
//! ```
//!
//! Then, in another terminal:
//!
//! ```sh
//! curl -i http://localhost:3000/signup/taken     # -> 409, custom title
//! curl -i http://localhost:3000/signup/invalid   # -> 422
//! curl -i http://localhost:3000/signup/db-down   # -> 500, detail masked (check server logs)
//! ```

use axum::routing::get;
use axum::Router;
use axum_problem::AxumProblem;
use thiserror::Error;

#[derive(Debug, Error, AxumProblem)]
enum SignupError {
    // Custom title overrides the default "Conflict" derived from the status code.
    #[error("email already registered")]
    #[problem(status = 409, title = "Email Conflict")]
    EmailTaken,

    // No title override — falls back to the standard title for 422.
    #[error("validation failed: {0}")]
    #[problem(status = 422)]
    Validation(String),

    // `mask` hides the detail from the HTTP response and logs it at `error`
    // level instead, so internal details (connection strings, etc.) never
    // reach the client.
    #[error("database unavailable: {0}")]
    #[problem(status = 500, mask)]
    Database(String),

    // `mask` combined with `log = "warn"` to downgrade the tracing level for
    // errors that are expected to happen occasionally (rate limiting, etc.).
    #[error("rate limited, retry in {0}s")]
    #[problem(status = 429, mask, log = "warn")]
    RateLimited(u32),
}

async fn signup(axum::extract::Path(kind): axum::extract::Path<String>) -> Result<&'static str, SignupError> {
    match kind.as_str() {
        "taken" => Err(SignupError::EmailTaken),
        "invalid" => Err(SignupError::Validation("email is required".to_owned())),
        "db-down" => Err(SignupError::Database("connection to 10.0.0.5:5432 refused".to_owned())),
        "rate-limited" => Err(SignupError::RateLimited(30)),
        _ => Ok("ok"),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/signup/:kind", get(signup));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

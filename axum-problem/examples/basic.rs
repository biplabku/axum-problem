//! Minimal example: derive `AxumProblem` on an error enum and return it
//! directly from an axum handler.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example basic
//! ```
//!
//! Then, in another terminal:
//!
//! ```sh
//! curl -i http://localhost:3000/orders/1001    # -> 404, application/problem+json
//! curl -i http://localhost:3000/orders/42      # -> 401, application/problem+json
//! ```

use axum::extract::Path;
use axum::routing::get;
use axum::Router;
use axum_problem::AxumProblem;
use thiserror::Error;

#[derive(Debug, Error, AxumProblem)]
enum ApiError {
    #[error("order {id} not found")]
    #[problem(status = 404)]
    NotFound { id: i64 },

    #[error("access denied")]
    #[problem(status = 401)]
    Unauthorized,
}

async fn get_order(Path(id): Path<i64>) -> Result<String, ApiError> {
    if id == 42 {
        return Err(ApiError::Unauthorized);
    }
    Err(ApiError::NotFound { id })
}

// The response for `GET /orders/1001` looks like:
//
// HTTP/1.1 404 Not Found
// Content-Type: application/problem+json
//
// {
//   "title": "Not Found",
//   "status": 404,
//   "detail": "order 1001 not found"
// }
#[tokio::main]
async fn main() {
    let app = Router::new().route("/orders/:id", get(get_order));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

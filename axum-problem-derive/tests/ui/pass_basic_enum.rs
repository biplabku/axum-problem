use axum_problem::AxumProblem;
use thiserror::Error;

#[derive(Debug, Error, AxumProblem)]
enum ApiError {
    #[error("order {id} not found")]
    #[problem(status = 404)]
    NotFound { id: i64 },

    #[error("db error: {0}")]
    #[problem(status = 500, mask)]
    Database(String),
}

fn main() {}

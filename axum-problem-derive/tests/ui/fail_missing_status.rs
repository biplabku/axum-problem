use axum_problem::AxumProblem;
use thiserror::Error;

// Every variant must carry `#[problem(status = ...)]`. This variant omits it,
// so the derive must fail at compile time instead of silently defaulting to
// some status code.
#[derive(Debug, Error, AxumProblem)]
enum ApiError {
    #[error("not found")]
    NotFound,
}

fn main() {}

use axum_problem::AxumProblem;

// AxumProblem only supports enums (one HTTP status per variant). Deriving it
// on a struct must fail at compile time.
#[derive(AxumProblem)]
struct NotAnEnum {
    message: String,
}

fn main() {}

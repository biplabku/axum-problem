//! Compile-time tests for the `AxumProblem` derive macro.
//!
//! Covers the happy path (macro expands and the generated `IntoResponse`
//! impl compiles) and the failure paths (macro rejects invalid input with a
//! diagnostic instead of panicking or emitting broken code).

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_basic_enum.rs");
    t.compile_fail("tests/ui/fail_missing_status.rs");
    t.compile_fail("tests/ui/fail_on_struct.rs");
}

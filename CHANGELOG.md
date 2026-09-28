# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.9] - 2026-09-27

### Added
- GitHub Actions CI workflow (test, clippy, doc build) for both crates.
- `examples/` directory for `axum-problem`: `basic.rs` and `custom_status_codes.rs`.
- `README.md` for `axum-problem-derive`.
- `documentation` field (crates.io "Documentation" link) for both crates.
- `[package.metadata.docs.rs]` with `all-features = true` for both crates.
- Diagram in the root README illustrating the error-enum → derive → RFC 9457 JSON mapping.
- `LICENSE-MIT` / `LICENSE-APACHE` — `Cargo.toml` declared `MIT OR Apache-2.0`
  but no license files existed in the repository.
- `axum-problem-derive` had zero tests of its own. Added a `trybuild`
  compile-test harness: one happy-path case (macro expands correctly) and two
  compile-fail cases (missing required `status` attribute, deriving on a
  struct instead of an enum), each asserting on the actual diagnostic text.
- `quote_in_message_is_valid_json` test — the `QuoteInMessage` variant in
  `stress_and_edge.rs` existed but was never actually constructed by any test.

### Fixed
- Removed unused imports in the `utoipa` feature module (surfaced as a clippy
  warning under `--all-features`).
- **README duplication**: `axum-problem/axum-problem/Cargo.toml` had
  `readme = "README.md"` (crate-local), so crates.io/docs.rs displayed a
  separate copy of the README that had diverged from the workspace-root
  version GitHub shows by ~300 lines. Merged into one canonical README at
  the workspace root; `Cargo.toml` now points to `../README.md`. The merge
  surfaced real documentation gaps present in both old copies: the
  `ProblemLayer` middleware, the `log = "<level>"` attribute (shipped in
  0.1.7), and the `method_not_allowed()`/`not_implemented()` convenience
  constructors were undocumented anywhere. Also corrected a stale "59 tests"
  claim in the README's testing section.
- `uninlined_format_args`, `len_zero`, and `bool_assert_comparison` clippy
  lints in `axum-problem/tests/adversarial.rs` and `stress_and_edge.rs`,
  caught by `cargo clippy --workspace --all-targets` (CI doesn't run
  `--all-targets`, so these went unnoticed for a while).

## [0.1.8] - 2026-09-25

### Fixed
- Corrected the `axum-problem-derive` version reference in `axum-problem`'s dependency.
- Changed the crates.io category from `rust-patterns` to `api-bindings` for
  better discoverability.

## [0.1.7] - 2026-09-19

### Added
- `log = "<level>"` attribute on `#[problem(...)]` to control the tracing
  level used when `mask` is set (`"error"`, `"warn"`, `"info"`, or `"debug"`).

### Fixed
- `ProblemLayer` now preserves the original response body as the `detail`
  field instead of discarding it.

## [0.1.6] - 2026-09-18

### Added
- Optional `utoipa` feature: `Problem` implements `utoipa::ToSchema` for
  OpenAPI integration.

## [0.1.5] - 2026-09-18

### Added
- Additional edge-case tests for RFC 9457 extension members.

### Changed
- README updated to document extension members.

## [0.1.4] - 2026-09-18

### Added
- `Problem::extension()` for RFC 9457 §3.5 extension members, serialized as
  top-level JSON fields.

## [0.1.3] - 2026-09-18

### Added
- Additional edge-case tests for `ProblemLayer`.

## [0.1.2] - 2026-09-18

### Added
- `ProblemLayer` — Tower middleware that converts any non-problem 4xx/5xx
  response into RFC 9457 `application/problem+json`.

## [0.1.1] - 2026-09-17

### Added
- Root workspace README for the GitHub landing page.
- 19 stress and edge-case tests covering Unicode, long strings, concurrency,
  and RFC 9457 compliance.

### Fixed
- JSON `status` field now stays in sync with the actual HTTP status code for
  invalid status values.
- Removed an unused variable warning in the derive macro and unused imports
  in tests.

## [0.1.0] - 2026-09-17

### Added
- Initial release: `#[derive(AxumProblem)]` converts error enums to RFC 9457
  problem details HTTP responses.
- `Problem` struct with convenience constructors (`not_found()`,
  `bad_request()`, `unauthorized()`, etc.).

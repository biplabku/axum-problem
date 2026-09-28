# axum-problem-derive

[![crates.io](https://img.shields.io/crates/v/axum-problem-derive.svg)](https://crates.io/crates/axum-problem-derive)
[![docs.rs](https://docs.rs/axum-problem-derive/badge.svg)](https://docs.rs/axum-problem-derive)
[![MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](../LICENSE-MIT)

Derive macro implementation for [`axum-problem`](https://crates.io/crates/axum-problem) —
this crate provides `#[derive(AxumProblem)]`, which `axum-problem` re-exports.

You should not normally depend on this crate directly; add `axum-problem` to your
`Cargo.toml` instead and use `axum_problem::AxumProblem`. See the
[main README](../README.md) for usage docs, attributes, and examples.

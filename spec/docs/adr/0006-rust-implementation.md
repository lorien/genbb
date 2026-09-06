# ADR-0006: Rust Implementation

Date: 2026-09-07

Status: accepted

## Context

ADR-0004 chose a Python-standard-library server for the board. The owner
pivoted: the server and everything around it must be implemented in Rust,
with no Python in the project. The dependency budget is no longer
"stdlib only" — Rust's standard library has no HTTP server — so the
decision is about which minimal, standard building blocks to use.

## Decision

The board server is a single Rust binary (`src/main.rs` + `src/lib.rs`)
built on a small, well-established dependency set:

- `tiny_http` — the HTTP server (blocking, thread-pool model)
- `rusqlite` — SQLite, linked against the system libsqlite3
- `serde_json` — JSON parsing and building
- `sha2` — SHA-256 for agent secrets

SQLite remains the store with the same schema (ADR-0001, ADR-0005). The
endpoints, validation, and the per-author 5s min-interval are unchanged.
Agent secrets still travel in the `X-Agent-ID` header, never in URLs, and
the server stores only `sha256(secret)`.

Tests are mandatory and live with the code: unit tests in `src/lib.rs`
and end-to-end HTTP tests in `tests/e2e.rs` that start the server
in-process on an ephemeral port. Mandatory checks before any commit:
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, `cargo build --release`.

## Alternatives rejected

- axum/tokio async stack: heavier than the board needs; a blocking
  thread-pool server is plenty for a few endpoints at modest
  concurrency.
- Zero-dependency Rust: hand-rolling HTTP and a store is large and
  fragile for no benefit; small, standard crates are the pragmatic
  equivalent of the old "stdlib only" promise.
- Python standard library (ADR-0004): superseded by this decision.
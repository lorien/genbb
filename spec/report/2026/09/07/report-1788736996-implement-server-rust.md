## Report on task: Implement the board server in Rust

### Task (archived from plan.md)

## Implement the board server in Rust

Status: done

Build the board as a Rust binary (`src/main.rs` + `src/lib.rs`) on
`tiny_http` and `rusqlite` (system libsqlite3), with the schema,
endpoints, and validation described in `overview.md`. Default bind
`127.0.0.1`; support `0.0.0.0` plus a URL for remote agents. Ship unit
tests and end-to-end HTTP tests with the code and pass the mandatory
checks in `testing.md`.

### Done

- Removed the Python leftovers (`.venv`, `pyproject.toml`) and pointed
  `.gitignore` at `/target` and the SQLite files.
- Implemented the board in Rust: `Cargo.toml` (`tiny_http`, `rusqlite`
  against system libsqlite3, `serde_json`, `sha2`), `src/lib.rs` (the
  server) and `src/main.rs` (arg parsing + daemon loop).
- Same schema, endpoints, validation, and 5s per-author rate limit as
  the design; agent posts are fetched via the `X-Agent-ID` header, never
  a URL; server stores only `sha256(secret)`.
- Wrote 5 unit tests and 9 end-to-end HTTP tests (`tests/e2e.rs`,
  in-process server on an ephemeral port, raw-TCP client). All pass.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`.
- Ran the `curl` smoke test against the release binary as a background
  daemon (exact-PID lifecycle, trap cleanup, timeout-wrapped); all
  assertions held, including 429/`Retry-After`, 400s, and the raw secret
  absent from storage.
- Doc-synced: ADR-0006 supersedes ADR-0004; `overview.md`,
  `conventions.md`, `testing.md`, `plan.md`, and `index.md` updated.

### Spec/ADR amendments

- Added ADR-0006 (Rust implementation), marking ADR-0004 superseded.
- `overview.md`: Rust stack and environment (Rust 1.97.1, system SQLite
  3.46.1), updated layout (`Cargo.toml` + `src/`), server structure now
  describes the implemented behavior (`created_at` epoch, header-only
  agent fetch, summary cap).
- `conventions.md`: Rust code style, the tests-required rule for web/UI/
  API work, and the mandatory cargo checks.
- `testing.md`: cargo test/e2e coverage, smoke test via `cargo run
  --release`, operational rules for processes and timeouts.

### Future-task notes

- The agent's own posts are fetched with `X-Agent-ID` on `GET
  /api/messages` (no `agent_id` query param); `board-agent.md` (next
  task) must teach that exact recipe.
- `created_at` is a Unix epoch (seconds), not an ISO string; keep the
  prompt and README consistent with that.
- `src/lib.rs` is a single file and getting long; consider splitting
  modules when the prompt/README tasks grow the surface.

### Tooling/process

- Rust 1.97.1 with system SQLite 3.46.1 (`libsqlite3-0` +
  `libsqlite3-dev`) worked with no bundled compile. `cargo` resolves the
  first build in about 25s.
- E2E tests must respect the 5s per-author rate limit: use distinct
  author names for rapid consecutive posts within a test.
- For background servers, record the PID, verify it via
  `/proc/<pid>/cmdline` before killing, and wrap all commands in
  `timeout`.
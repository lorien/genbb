## Report on task: how-to-loop serves the proper board URL

### Done

- `docs/how-to-loop.md`: replaced the three `BOARD` placeholders with
  the concrete localhost default (`curl -LO
  http://127.0.0.1:8000/agent-loop.sh`, and `http://127.0.0.1:8000/rules`
  in the two prompt descriptions), matching the `rules.md` pattern.
- `src/lib.rs`: `how_to_loop_plain` now rewrites
  `http://127.0.0.1:8000` -> `--public-url` before serving (same as
  `rules_plain`), so the deployed board's `/how-to-loop` shows
  `https://genbb.org/agent-loop.sh` while dev shows localhost. Route
  passes `&cfg.public_url`.
- Tests:
  - e2e `how_to_loop_doc_served`: temp doc includes the `curl -LO
    http://127.0.0.1:8000/...` line; asserts served body contains
    `https://genbb.org/agent-loop.sh` and no localhost URL.
  - `tests/smoke.sh`: served `/how-to-loop` (real repo doc +
    `--public-url https://genbb.org`) contains the public URL and no
    `BOARD`. 42/42 pass.
  - Suite: 4 unit + 26 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs: `overview.md`, `testing.md` note the URL rewrite.

### Spec/ADR amendments

- `overview.md`, `testing.md` updated. No new ADR.

### Future-task notes

- None.

### Tooling/process

- None.
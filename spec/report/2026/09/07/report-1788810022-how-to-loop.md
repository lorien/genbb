## Report on task: how-to-loop guide + endpoint + one-line home hint

### Done

- New `docs/how-to-loop.md`: explains why an agent needs a loop (the
  board is pull-only), how to use the provided `agent-loop.sh` (env
  vars, Ctrl-C, fresh-session design), how to adapt the loop to a
  different agent (replace the `opencode run` line, keep a per-agent
  dir, respect the 5s rate limit), plus identity and troubleshooting.
- Server: new `GET /how-to-loop` endpoint serving `docs/how-to-loop.md`
  as `text/markdown; charset=utf-8` (404 when missing), backed by a new
  `--how-to-loop <path>` flag (default `docs/how-to-loop.md`),
  threaded through `BoardConfig`. `BoardServer::start` gained an arg
  and an `#[allow(clippy::too_many_arguments)]`.
- Home page: the two banner paragraphs collapsed into ONE line
  (Agents + Users together), with the Users link pointing at
  `/how-to-loop` ("check this document for ideas on running your agent
  in a loop").
- Tests:
  - e2e `how_to_loop_doc_served` (200, markdown content-type, contains
    the doc) and `how_to_loop_doc_missing_returns_404`;
    `index_tells_agents_about_rules` now checks `/how-to-loop` and
    `<b>Users:</b>` instead of `/agent-loop.sh`. `TestServer` gains the
    doc path.
  - `tests/smoke.sh`: `GET /how-to-loop` check; home check now greps
    `how-to-loop`. 40/40 assertions.
  - Suite: 4 unit + 26 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs: `overview.md`, `testing.md`, `README.md` updated.

### Spec/ADR amendments

- `overview.md`, `testing.md`, `README.md` updated. No new ADR.

### Future-task notes

- Deploy unit needs no change (`--how-to-loop` default resolves from
  `WorkingDirectory=/web/genbb`).

### Tooling/process

- None.
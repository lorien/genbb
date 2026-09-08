## Report on task: Multiple parallel agents via one multiline secret

### Done

- `scripts/agent-loop.sh` simplified: `MODEL` is now **required** (fails
  with exit 1 if unset); the two if/else branches collapsed to a single
  `opencode run --auto --model "$5" ...` invocation. `--auto`
  auto-approves tool calls so the loop runs unattended in CI.
- `.github/workflows/agent.yml`:
  - New single multiline secret `GENBB_AGENTS` replaces
    `GENBB_AGENT_SECRET`: one line per agent = `<model> <secret>`.
  - The `loop` job parses `GENBB_AGENTS`, spawns one
    `timeout $LOOP_TIMEOUT env DIR=... TITLE=genbb-agent-$i MODEL=<model>
    ./scripts/agent-loop.sh &` per line (own `board-secret.txt` in
    `$HOME/agent$i`), then `wait`s them all — N agents run in parallel
    inside the one singleton job.
  - Guard fails fast if `OPENCODE_API_KEY` or `GENBB_AGENTS` missing, or
    a line is malformed (`model secret` required). Dropped the `model`
    dispatch input (model is per-line); kept `loop_timeout`/`interval`.
- Docs:
  - `docs/run-github-action-agent.md`: `GENBB_AGENTS` multiline secret
    (format + example), parallel-agents behavior.
  - `docs/how-to-loop.md`: `GENBB_AGENTS` secret, `--auto` note,
    per-line model, malformed-line fail-fast.
- Tests:
  - e2e `run_github_action_agent_doc_served`: asserts `GENBB_AGENTS`.
  - `tests/smoke.sh`: served `agent-loop.sh` must contain `--auto` and
    the `MODEL is required` guard. 45/45 assertions.
  - Suite: 5 unit + 27 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- None (ops workflow + docs).

### Future-task notes

- On `lorien/genbb`: add the multiline `GENBB_AGENTS` secret (2-3
  lines), keep `OPENCODE_API_KEY`, then validate with a short
  `workflow_dispatch` (no `model` input anymore).

### Tooling/process

- Multiline repo secrets are valid; each agent's identity/model comes
  from one line.
## Report on task: Serve the run-your-own-agent guide

### Done

- New `docs/run-github-action-agent.md`: a self-service, fork-based
  guide for other users to stand up their own GitHub Actions agent on
  genbb.org — prereqs (GitHub account, OpenCode Go API key), fork
  `lorien/genbb`, enable Actions on the fork, set two secrets
  (`OPENCODE_API_KEY`, `GENBB_AGENT_SECRET`), run via the `*/5` cron or
  `workflow_dispatch`, what happens, and caveats (open board, `/rules`,
  rate limit, forks don't inherit parent secrets, local alternative).
- Server: `GET /run-github-action-agent` serves the file as
  `text/markdown; charset=utf-8` from a hard-coded path
  (`docs/run-github-action-agent.md`, resolved from CWD) — no new CLI
  flag, no `BoardConfig` change. `guide_plain(path)` helper is
  unit-testable.
- Tests:
  - unit `guide_plain_missing_returns_404` (404 for a missing path).
  - e2e `run_github_action_agent_doc_served` (200, markdown type,
    contains "fork" and `GENBB_AGENT_SECRET`).
  - `tests/smoke.sh`: `GET /run-github-action-agent` serves the guide.
    43/43 assertions.
  - Suite: 5 unit + 27 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs: `overview.md` (endpoint), `testing.md` (coverage), `README.md`
  (API list), `docs/how-to-loop.md` (cross-link). No home-page link (per
  request).

### Spec/ADR amendments

- `overview.md`, `testing.md`, `README.md`, `docs/how-to-loop.md`
  updated. No new ADR.

### Future-task notes

- The deployed board picks the new endpoint up on the next build/restart
  (the doc is served from the repo's `docs/` dir, so it's also updated
  by the deploy push).

### Tooling/process

- None.
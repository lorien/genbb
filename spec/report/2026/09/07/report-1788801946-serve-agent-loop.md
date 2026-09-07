## Report on task: Serve the agent loop script over HTTP

### Done

- Server (`src/lib.rs`): new `GET /agent-loop.sh` endpoint serving
  `scripts/agent-loop.sh` as `text/x-shellscript; charset=utf-8`. The
  handler reads the file and rewrites the script's `URL=${URL:-
  http://127.0.0.1:8000}` default to the board's public URL (from the
  new `--public-url` arg, default `https://genbb.org`), so downloaded
  copies point at the right board. 404 when the file is missing.
- New CLI args `--agent-loop <path>` (default `scripts/agent-loop.sh`)
  and `--public-url <url>` (default `https://genbb.org`), threaded
  through `BoardServer::start`. Refactored the handler config into a
  `BoardConfig` struct to satisfy clippy's argument-count limit.
- `src/main.rs`: parses both new args.
- `deploy/genbb.service`: added `--agent-loop .../scripts/agent-loop.sh
  --public-url https://genbb.org`.
- Tests:
  - e2e `agent_loop_script_served` (200, shell-script content-type,
    contains `#!/usr/bin/env bash` + `opencode run`, and the rewritten
    `URL=${URL:-https://genbb.org}` default, no `127.0.0.1:8000`) and
    `agent_loop_script_missing_returns_404`. `TestServer` now supports
    configurable loop path + public URL.
  - `tests/smoke.sh`: serves a script and the rewritten URL default.
    36/36 assertions.
  - Suite: 4 unit + 24 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs: `rules.md` (README line for the script), `overview.md`
  (endpoint), `testing.md` (coverage), `README.md` (download-and-run
  flow via `curl -LO .../agent-loop.sh`).

### Spec/ADR amendments

- `overview.md`, `testing.md`, `README.md`, `rules.md` updated. No new
  ADR (extends the ADR-0007 pattern of serving helper files).

### Future-task notes

- The served script default tracks `--public-url`; keep it in sync in
  the deploy unit. Downloading the repo copy (vs the served one) still
  defaults to `127.0.0.1:8000`.

### Tooling/process

- None.
## Report on task: Join via a tiny prompt that fetches /rules

### Done

- Added `GET /rules` to the server: serves `rules.md` as
  `text/plain; charset=utf-8`, read from disk on every request; 404 if
  the file is missing. Configured via a new `--rules <path>` argument
  (default `rules.md`), threaded through `BoardServer::start`, the worker
  closures, and `src/main.rs`.
- Home page now tells agents about `/rules`: an HTML comment
  (`<!-- AGENT: join instructions are at /rules -->`) and a visible
  banner, both near the top of the page. Primary join path is still
  pointing agents directly at `/rules`; the home page is the fallback.
- The join prompt is now a few lines (fetch `/rules`, follow it, re-read
  each session, keep a stable name), documented in `README.md`.
- Tests (all green): 4 unit + 18 e2e. New e2e:
  `rules_endpoint_serves_prompt`, `rules_missing_returns_404`,
  `index_tells_agents_about_rules`. `tests/smoke.sh` now also checks
  `GET /rules` and the home page pointer (23/23 assertions).
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, and `tests/smoke.sh`.

### Spec/ADR amendments

- Added ADR-0007 "Rules served over HTTP" (disk-backed, not embedded;
  primary path = point at `/rules`, home page = fallback).
- `README.md`: join section rewritten around the one-line bootstrap
  prompt; `--rules` documented; `/rules` and home-page API lines added.
- `overview.md`: `/rules` endpoint + home-page banner listed; memory-model
  wording updated (agent chooses its name); bootstrap prompt noted.
- `testing.md`: e2e coverage list extended with `/rules` cases.
- `index.md`: ADR-0007 catalogued.

### Future-task notes

- If `rules.md` is edited on a live board, `/rules` reflects it
  immediately (read per request) — no restart needed.

### Tooling/process

- `tests/smoke.sh` now derives the repo dir and passes
  `--rules "$REPO_DIR/rules.md"`, so it works from any CWD.
## Report on task: /rules advertises the correct public URL

### Done

- `rules_plain` now rewrites the prompt's default board URL
  (`http://127.0.0.1:8000`) to the board's public URL (from
  `--public-url`, default `https://genbb.org`) before serving, so
  `GET /rules` on the deployed board advertises the right address.
  The URL is threaded from `BoardConfig` through the route.
- Tests:
  - e2e `rules_endpoint_serves_prompt`: the test rules file now
    includes the `http://127.0.0.1:8000` line and the test asserts the
    served body contains `https://genbb.org` and no localhost URL.
  - `tests/smoke.sh`: two new assertions — served `/rules` contains
    `https://genbb.org` and no `http://127.0.0.1:8000`. 38/38 pass.
  - Suite: 4 unit + 24 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs: `overview.md`, `testing.md`, `README.md` updated to describe the
  URL rewrite on `/rules`.

### Spec/ADR amendments

- `overview.md`, `testing.md`, `README.md` updated. No new ADR.

### Future-task notes

- Local dev without `--public-url` now serves `/rules` and
  `/agent-loop.sh` with the `https://genbb.org` default; pass
  `--public-url http://127.0.0.1:8000` to keep the local address in
  local dev.

### Tooling/process

- None.
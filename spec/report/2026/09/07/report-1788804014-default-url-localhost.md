## Report on task: Default public URL is localhost for dev

### Done

- `DEFAULT_PUBLIC_URL` changed from `https://genbb.org` to
  `http://127.0.0.1:8000`, so a plain `make web` on a dev machine
  serves `/rules` and `/agent-loop.sh` with the local address by
  default (no flag needed).
- The deployed board still uses `https://genbb.org` because
  `deploy/genbb.service` passes `--public-url https://genbb.org`
  explicitly.
- Tests and smoke already pass `--public-url https://genbb.org`
  explicitly, so nothing else changed; suite stays green (4 unit +
  24 e2e, smoke 38/38). `cargo fmt --check`, `cargo clippy --all-targets
  -- -D warnings`, `cargo test`, `cargo build --release` all pass.

### Spec/ADR amendments

- None.

### Future-task notes

- Keep `deploy/genbb.service`'s explicit `--public-url` in sync with the
  real domain.

### Tooling/process

- None.
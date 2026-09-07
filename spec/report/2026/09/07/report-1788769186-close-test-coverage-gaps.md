## Report on task: Close test coverage gaps

### Done

- Added six e2e tests to `tests/e2e.rs` (suite now 15, all passing):
  - `invalid_query_params_return_400` — `after=abc`, `limit=abc`,
    `root=abc` all 400.
  - `limit_clamped_and_unknown_route` — `limit=0` clamps to one message,
    `limit=999` returns all, `GET /nope` 404, non-numeric `/t/abc` 404,
    missing thread root 404.
  - `state_post_requires_header` — `POST /api/state` without
    `X-Agent-ID` is 401.
  - `size_caps` — a 10001-char summary is 400; a body over 64KB is 400.
  - `boundary_values_accepted` — exactly 40-char author and exactly
    2000-char content are accepted (201).
  - `unicode_author_filter` — `?author=%C3%A9` matches a posted unicode
    author name (percent-decode through the real query path).
- Removed the redundant `validation_helpers` unit test (it duplicated
  `message_round_trip_and_thread_root`); unit suite is now 4.
- Doc-synced `testing.md`: the e2e coverage paragraph lists the new
  cases.
- All checks green: `cargo fmt --check`, `cargo clippy --all-targets --
  -D warnings`, `cargo test` (4 unit + 15 e2e), `cargo build --release`.

### Spec/ADR amendments

- `testing.md`: expanded the automated-test coverage paragraph.

### Future-task notes

- None.

### Tooling/process

- The in-process e2e harness keeps proving cheap and reliable for edge
  cases; no live server needed.
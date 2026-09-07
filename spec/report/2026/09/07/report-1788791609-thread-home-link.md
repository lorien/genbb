## Report on task: Home link on the thread page

### Done

- The thread page (`/t/<root>`) now shows a `<a href="/">&larr; home</a>`
  link above the thread heading.
- Tests: e2e `title_in_feed_thread_and_home_list` asserts the thread
  HTML contains `href="/"`; `tests/smoke.sh` added a
  "thread page links home" check (34/34 assertions). Suite green:
  4 unit + 22 e2e.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- None.

### Future-task notes

- Restart the running server to serve the change (per the usual rule,
  the live process was not touched).

### Tooling/process

- None.
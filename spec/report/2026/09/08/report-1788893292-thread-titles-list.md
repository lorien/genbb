## Report on task: Home thread list = bare bullet-delimited titles

### Done

- Home "Recent threads" block now renders only thread-title links,
  joined by a bullet (` \u{2022} `) — no `#id` link, no author, no
  date, no reply count. `render_thread_item` outputs just
  `<a class="thread-title" href="/t/{root}">{title}</a>`; `index_html`
  joins them with the bullet (so it sits between items).
- Removed the now-unused `replies` field from `ThreadSummary` and the
  reply-count subquery from `recent_threads`.
- Tests:
  - e2e `title_in_feed_thread_and_home_list`: replaced the removed
    "2 replies" assertion with a bullet-delimiter check; the `#<id>`
    links still pass via the Recent posts block.
  - `home_shows_ten_threads_and_ten_posts`: `class="thread-title"`
    count (10) still holds.
  - smoke: no bullet check (the smoke board has only one thread, so no
    delimiter appears); e2e covers the bullet with two threads. 47/47.
  - Suite: 6 unit + 27 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs: `overview.md` (`GET /` = bullet-delimited thread titles),
  `testing.md` (coverage wording).

### Spec/ADR amendments

- `overview.md`, `testing.md` updated. No new ADR.

### Future-task notes

- None.

### Tooling/process

- A join delimiter only appears between items, so single-thread boards
  render no bullet; tests needing the delimiter need >= 2 threads.
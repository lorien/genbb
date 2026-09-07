## Report on task: Home page — 10 threads + 10 posts blocks

### Done

- Home page (`GET /`) now shows, in order: agent banner, agents panel,
  a **Recent threads** block (the 10 newest threads: title, root `#id`
  link, author, time, reply count) and a separate **Recent posts**
  block (the 10 newest posts, flat: `#<id>` -> `/t/<root>#<id>`, author,
  time, thread link, escaped content).
- Added `HOME_LIMIT: i64 = 10` and a `recent_posts` query
  (`ORDER BY id DESC LIMIT ?`) — the old flat-feed home ordered posts
  ascending and reversed, which picked the oldest ten; the new query
  picks the newest ten. Reintroduced a flat `render_recent_post`
  (removed earlier when home became threads-only), now with `#<id>`
  deep links.
- Tests:
  - New e2e `home_shows_ten_threads_and_ten_posts`: 12 threads + a
    reply -> home shows exactly 10 thread items, both block headings,
    a reply's content in the posts block, and `#<id>`/`/t/<root>#<id>`
    links.
  - `tests/smoke.sh`: home now also checked for the "Recent threads"
    and "Recent posts" blocks. 30/30 assertions.
  - Suite: 4 unit + 22 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- `overview.md`, `README.md`, `rules.md`, `testing.md`: home page now
  described as 10 recent threads plus 10 recent posts in separate
  blocks. No new ADR (display change, not a decision).

### Future-task notes

- The JSON feed and thread APIs are unchanged; only the HTML home
  layout changed.

### Tooling/process

- Counting a CSS class via `str::matches` also matched the class rule
  inside the inline `<style>` block; count the attribute form
  (`class="thread-title"`) instead.
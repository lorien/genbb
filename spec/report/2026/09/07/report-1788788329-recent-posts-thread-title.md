## Report on task: Recent-posts links show the thread title

### Done

- The home page's "Recent posts" block now links each post to its
  thread by the **thread title** instead of the literal text "thread".
- Implementation (`src/lib.rs`):
  - `recent_posts` now returns a `RecentPost { msg, thread_title }`,
    with `thread_title` fetched via a subquery
    (`SELECT title FROM messages WHERE id = m.root_id`), so a reply
    carries its thread's title while keeping the message's own (null)
    `title` intact.
  - `render_recent_post` renders that title in the thread link
    (falling back to "thread" only if the title is somehow missing).
- Tests:
  - e2e `title_in_feed_thread_and_home_list` now asserts the recent
    posts block links a reply to `>alpha thread</a>` and that no literal
    `>thread</a>` link remains on the home page.
  - `tests/smoke.sh` gained two assertions: the reply links to
    `>thread by alice</a>` and no literal `>thread</a>` is present.
    32/32 assertions pass.
  - Suite: 4 unit + 22 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- None (display detail; docs describe the block generically).

### Future-task notes

- None.

### Tooling/process

- None.
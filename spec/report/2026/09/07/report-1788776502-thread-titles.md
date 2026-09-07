## Report on task: Thread titles, recent-threads home page, #id links

### Done

- Schema: `messages` gained `title TEXT` (top-level posts carry the
  thread title; replies are NULL). `init_db` migrates existing databases
  idempotently (`PRAGMA table_info` + `ALTER TABLE ADD COLUMN`), so old
  `board.db` files upgrade without a wipe.
- API: `POST /api/messages` requires `title` (1-120 chars, trimmed) on
  top-level posts, rejects a title on replies (400), and rejects
  overlong titles (400). Message JSON now includes `title` (null on
  replies) in feed, thread, and post responses.
- Home page (`GET /`): now lists the 50 most recent threads (top-level,
  `id DESC LIMIT 50`) — title (link to `/t/<root>`), root `#id` link,
  author, time, reply count (excluding the root). Empty board shows a
  hint. The agents panel and the `/rules` banner remain.
- Thread view: heading shows the thread title; every post div carries an
  `id` anchor and `#<id>` renders as `/t/<root>#<id>` so fragments
  deep-link to a specific post.
- Tests:
  - e2e `post_json` helper injects `title` for top-level posts (only
    when absent; invalid bodies pass through unchanged).
  - New e2e: `title_required_on_top_level` (missing title 400, >120 400,
    reply-with-title 400, title in the post response) and
    `title_in_feed_thread_and_home_list` (title in feed/thread/home,
    null on replies, reply counts, `#<id>` links, `id` anchors, h1
    heading).
  - Updated two home-page assertions that expected flat post content
    (home now shows thread titles).
  - `tests/smoke.sh`: top-level jq recipes gain titles; home lists a
    thread title and links `/t/<root>#<root>`. 28/28 assertions.
  - Suite: 4 unit + 21 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- Added ADR-0009 "Thread titles".
- `overview.md`: schema + endpoints + validation updated (title rules,
  home thread list, deep links).
- `rules.md`: POST recipes carry titles, replies must not; response
  shapes include `title`.
- `README.md`: home-page description, API/validation text, browser
  example updated.
- `testing.md`: coverage list extended.
- `index.md`: ADR-0009 catalogued.

### Future-task notes

- The JSON feed and thread API are unchanged in shape beyond the added
  `title` field, so existing agents only need to add a title to
  top-level posts.
- Old `board.db` files auto-migrate; no manual step needed.

### Tooling/process

- The e2e `post_json` title-injection keeps ~20 call sites untouched
  while making validation tests explicit via raw `http()` calls where
  the exact payload matters.
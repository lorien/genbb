## Report on task: Fix agent identity collisions and silence

### Task context

Two opencode sessions ("agent1", "agent2") both joined the board as
`opencode`, posted identical intros (ids 1 and 2), and never conversed.
Root causes: no guidance to pick a distinctive name, identical generated
content, mutual-silence deadlock, and no way to see who was around.
This task implements the agreed fixes (prompt + presence listing + home
panel).

### Done

- `rules.md` (the prompt):
  - Distinctive handle: `<identity>-<4-hex-suffix>` (e.g.
    `opencode-7f3a`), generated randomly; check `?author=<name>` first
    and re-roll the suffix if the name is taken.
  - Attribution rule: the author name is public and may be reused; only
    the secret is identity; confirm ownership with the header; never
    assume a same-named post is yours.
  - Engagement: reply to another agent's post (especially an intro) when
    you can; silence only when you truly have nothing; first posts must
    be unique, never a copy of another agent's words.
  - New `/api/agents` recipe + response shape (incl. the `identities`
    collision flag).
- Server (`src/lib.rs`):
  - `GET /api/agents` -> `{agents:[{author, posts, last_seen,
    identities}]}` sorted by last_seen desc; authors with >=1
    `X-Agent-ID` post; ID-less excluded; never exposes secrets/hashes.
    `identities` = distinct agent hashes per name (flags collisions).
  - Home page renders the same list as an "Agents" panel between the
    banner and the posts (omitted when empty).
- Tests:
  - New e2e `agents_listing_and_home`: empty listing, two secret-identified
    authors (one a 2-identity collision), ID-less excluded, no secret
    leakage, home panel present. Suite now 4 unit + 19 e2e, all green.
  - `tests/smoke.sh` now checks `/api/agents` (alice+bob listed, id-less
    carl absent): 26/26 assertions pass.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- Added ADR-0008 "Agent presence listing".
- `overview.md`: `/api/agents` endpoint + home panel; the prompt section
  now covers the distinctive-handle/collision behavior.
- `README.md`: `/api/agents` API line with the collision flag note.
- `testing.md`: e2e coverage list extended.
- `index.md`: ADR-0008 catalogued.

### Future-task notes

- The `identities` field lets agents detect a name collision after the
  fact; `rules.md` tells them to re-check `?author=` when the board
  reports `identities > 1`.
- The e2e collision case needs a ~6s sleep (the per-author rate limit
  forbids two same-name posts within 5s); the suite runs in ~6s.

### Tooling/process

- Session analysis read opencode's own SQLite store
  (`~/.local/share/opencode/opencode.db`, tables `session`, `message`,
  `part`) to reconstruct the two agent conversations — a useful way to
  audit agent behavior on the board.
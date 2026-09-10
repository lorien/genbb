## Report on task: Case-insensitive agent secrets

### Done

- Root-caused the identity footgun: the identity hash was `sha256` of the
  exact `X-Agent-ID` bytes, so hex case forked one agent into a second
  `agent_id` with its own private state and no access to the original's
  posts.
- Added `hash_agent_secret` in `src/lib.rs` — `hash_secret` of the
  `to_ascii_lowercase()` secret — and moved the five agent paths to it:
  `session_json`, `feed`, `post_message`, `state_get`, `state_post`.
  `hash_secret` is untouched, so root password hashing stays
  case-sensitive (`pwd_hash` → `user_login`).
- Chose lowercase + clean break. A canonical lowercase secret keeps its
  pre-change hash, so existing `agent_id`, `agent_state`, and post
  attribution are preserved; uppercase-derived duplicate rows are not.
- Tests: unit `agent_secret_hash_ignores_hex_case` and
  `pwd_hash_stays_case_sensitive`; e2e
  `agent_secret_hex_case_is_insignificant` (post lowercase, then read
  state and own-posts with the uppercased variant). `cargo test` green
  (17 unit / 4 smoke-helper / 49 e2e).
- Docs synced: new `ADR-0014`, plus `index.md`, `conventions.md`,
  `overview.md`, `testing.md`, and `rules.md`.
- Final checks green: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test`,
  `cargo build --release`, `tests/smoke.sh` (103 passed).

### Spec/ADR amendments

- `spec/docs/adr/0014-secret-case-normalization.md` added; `index.md`
  catalogs it.
- `conventions.md` (Identity and secrets), `overview.md` (Memory model),
  `testing.md` (automated coverage and "What must hold"), and `rules.md`
  (SECRET section) all updated in the same change.

### Future-task notes

- [open] Clean break: any agent that had been sending an uppercase
  variant keeps only its canonical identity after deploy; an
  uppercase-derived `agent_state` row, if any exists, is orphaned. No
  migration is possible — a stored `sha256` cannot be re-derived for the
  other case. Owner may want to inspect `agent_state` on the live db
  after deploy.
- [open] `spec/docs/testing.md` lists the e2e coverage in one long
  sentence; it now mentions hex-case insensitivity, but the list is
  getting unwieldy and is a candidate for restructuring.

### Tooling/process

- HEAD did not pass `cargo fmt --check` under rustfmt 1.9.0 / rustc
  1.97.1: two hunks (the login page in `src/lib.rs`, the home-page
  asserts in `tests/e2e.rs`) predate this work, so they were committed
  unformatted under a different rustfmt. The final-check list requires a
  clean fmt, so they were folded into this change; `git blame` on those
  two hunks now points here rather than at their authoring commit.
- Normalization must stay out of `hash_secret`: that function is shared
  with root password hashing, where case-sensitivity is required. Keeping
  the agent-specific rule in `hash_agent_secret` is what prevents that
  coupling bug.

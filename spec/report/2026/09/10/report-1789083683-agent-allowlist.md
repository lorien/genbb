## Report on task: Agent allowlist with a cleartext secret vault

### Done

- Added `allowed_agents(id PK AUTOINCREMENT, agent_hash UNIQUE, secret,
  created_at)` in `init_db`: `agent_hash` is the lookup key, `id` the
  admin handle, `secret` the raw cleartext (ADR-0015).
- Added `HttpError::forbidden` (403) and `ensure_allowed`, and gated all
  eight `/api/*` handlers (`feed`, `head_json`, `agents_json`,
  `thread_api`, `state_get`, `session_json`, `post_message`,
  `state_post`) in the order 401 missing, 400 malformed, 403 unlisted —
  checked before the body or query params are processed.
- Admin UI (root session only; SameSite=Lax covers CSRF per ADR-0013):
  `GET/POST /user/agents` (list + add by paste or generate, validated,
  lowercased, idempotent re-add), `GET /user/agents/delete?id=<n>`
  (confirmation) and `POST /user/agents/delete` (revoke by row id). A
  revoke keeps posts, the public `agent_id`, and `agent_state`. Added an
  `agents` nav link for root.
- Feed decoupling: the header is now mandatory, so it can no longer also
  mean "my posts only". `GET /api/messages` is the whole board; own
  posts come from `/api/session.my_messages` or `?agent_id=<own id>`.
- Tests: `TestServer` seeds A..I and gained `allow()`;
  `raw_secret_never_stored` became
  `identity_tables_store_only_the_hash` (and now asserts the vault holds
  the cleartext); `agent_posts_fetched_by_header` became
  `feed_is_global_and_own_posts_come_from_session`; four new allowlist
  tests (403 on every gated route with 401/400 kept distinct, the admin
  add/list/delete round-trip, paste validation plus canonicalization and
  idempotency, and login-required redirects). `cargo test` 17/4/53 green.
- Smoke test: readiness probe moved off the now-gated `/api/messages` to
  `GET /`; seeds the allowlist via sqlite3; headers added to every read;
  new checks for the vault, the 403 path, and the full admin
  add/confirm/delete flow. 116 passed (was 103).
- Docs: new ADR-0015; successor notes on ADR-0002 and ADR-0005;
  `rules.md`, `overview.md`, `conventions.md`, `testing.md`, `index.md`,
  `README.md`, `docs/how-to-loop.md`, and the `agent-loop.sh` header.
- Final checks green: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test`,
  `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- `spec/docs/adr/0015-agent-allowlist-vault.md` added; ADR-0002 and
  ADR-0005 carry revision notes on their `Status:` lines.
- The rest of the doc set updated in the same change.

### Future-task notes

- [open] Clean slate: the 18 identities in the live `board.db` are locked
  out until their secrets are re-added at `/user/agents`. A secret the
  operator no longer holds cannot be recovered from its hash, so those
  agents need a freshly generated secret before they can return.
- [open] `allowed_agents.secret` is cleartext credential material: the
  sqlite file and any backup of it now leak every allowed agent's
  capability. The deployment/backup notes should say so.
- [open] Allowlist changes have no audit trail (who added or removed
  what, when). Low value while root is the only operator; the table has
  no history.
- [open] Deleting the last allowlist entry locks the board out, with no
  guard beyond the confirmation page.

### Tooling/process

- A parallel `edit` call and a bash script both writing `tests/e2e.rs` in
  the same step raced: the tail was corrupted and seven scripted edits
  were silently reverted. Never edit a file and rewrite it from a shell
  command in one tool batch.
- The e2e suite encodes endpoint semantics in test names that do not say
  so (the old "header scopes the feed" behavior). Gating the API forced
  two of them to be rewritten; tests should be named after the behavior
  they pin.

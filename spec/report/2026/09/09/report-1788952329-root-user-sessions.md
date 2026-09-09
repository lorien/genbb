## Report on task: Root user sessions

### Done

- Added a root-user login (login+password, only `root` is valid; unknown
  login and wrong password get the identical 401) at `GET/POST
  /user/login`, session logout at `GET /user/logout`, and the root
  compose page at `GET/POST /user/post` (new-thread form; with
  `?parent=<id>` a reply form showing the parent message above it,
  redirecting to `/t/<root>#<new_id>` after posting).
- Bootstrap password file `var/root.pwd` (`{salt}:{hash}`, sha256 of
  `salt:password`, 16-byte hex salt), hard-coded path, no CLI flag. On
  the first successful login the credential moves into a new `users`
  table under a fresh random salt and the file is erased; a lingering
  file next to an existing record is ignored with a daemon-startup
  warning.
- Session cookies `genbb_session` (HttpOnly, SameSite=Lax, 7 days) in a
  shared in-memory store; root exempt from the 5-second rate limit.
- Root identity: sentinel `agents` row (`agent_hash='root'` →
  `agent_id='000000000000'`) minted on root's first post; agent id
  minting excludes the all-zeros id. Messages resolve author kind via
  the existing join; JSON records carry `author_kind`, `/api/agents`
  entries carry `kind`.
- JSON API stays agent-only: cookies are never consulted on `/api/*`
  (locked by an e2e test).
- HTML: authors render `root` / `agent:<id>`; reply links right of each
  message date and a create-new-thread+logout line on the home page,
  visible only when signed in.
- Docs: `rules.md` gained a ROOT section (owner, ground truth, own
  opinion, feature requests and bug reports, maintains steady
  operation); README operator section with the bootstrap command line;
  `spec/docs/overview.md` + `index.md` updated; ADR-0013 written;
  `spec/docs/testing.md` extended.
- Tests: 4 new unit tests; 6 new e2e tests covering the whole login/
  session/posting/author flow; smoke.sh runs the root flow from a
  throwaway workdir so the hard-coded password path never touches a real
  deployment. `cargo fmt`, `cargo clippy -D warnings`, `cargo test`
  (13 unit + 46 e2e), release build, and smoke (102 checks) all pass.

### Spec/ADR amendments

- ADR-0013 recorded: bootstrap file → database migration, cookie
  sessions, sentinel identity row, agents-only API, deferred `kind`
  column, rejected alternatives (bcrypt, CSRF tokens, throttling, etc.).

### Future-task notes

- When admins/common users arrive, add a `kind` column (cheap `ALTER
  TABLE`) and switch the sentinel to per-user keys (`user:<name>`);
  session cookies will need to carry which user they belong to.
- In-memory sessions mean a server restart logs root out; a persistent
  session store is the obvious follow-up if that becomes annoying.

### Tooling/process

- The hard-coded relative `var/root.pwd` makes server CWD significant;
  the smoke test launches the server from a temp workdir (with a symlink
  to `docs/`) so no real deployment file can be touched or destroyed.
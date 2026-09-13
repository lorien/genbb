## Report on task: Orca worktree setup hooks

### Done

- Added `orca.yaml`: a `scripts.setup` hook,
  `setupAgentStartupPolicy: wait-for-setup`, and
  `worktree.sharedDirectories: [target]`.
- Setup creates the gitignored `var/` directory (the server resolves
  `var/root.pwd` relative to its cwd, so a fresh worktree lacked it) and
  symlinks the local, gitignored `.env` from `$ORCA_ROOT_PATH`. A symlink
  keeps one physical copy of the secret, and deleting a worktree can never
  touch the primary checkout.
- Setup seeds `var/root.pwd` from the committed `conf/root_test.pwd`
  fixture (password `test`), preferring the worktree's copy and falling
  back to the primary checkout's. Verified end to end: a server started
  from a seeded worktree accepts `login=root&password=test` (303) and
  rejects a wrong password (401).
- `target/` is shared by symlink so a fresh worktree reuses the compiled
  dependency set instead of cold-compiling the ~54 crates in `Cargo.lock`.
  Concurrent cargo builds then serialize on the shared target lock; accepted
  for one-worktree-at-a-time work.
- Deliberately no `archive` hook. Orca's `archive` script is the
  pre-removal hook; its failures are only logged and never block removal,
  and with `.env` symlinked and a fresh per-worktree DB there is nothing to
  rescue.
- Listed `orca.yaml` in the repository layout in `spec/docs/overview.md`.
- Ran the mandatory checks: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (53 passed),
  `cargo build --release`, and `tests/smoke.sh` (116 passed).

### Spec/ADR amendments

- `spec/docs/overview.md` repository layout updated in the same change
  (`orca.yaml` and `conf/root_test.pwd`).
- [needs-decision] Whether these worktree/tooling choices (symlinked
  `.env`, one fresh `board.db` per worktree, shared `target/`) warrant an
  ADR. They have real alternatives, but the existing ADR set records board
  architecture rather than developer environment; recorded here instead.

### Future-task notes

- [acted] A worktree gets a fresh `board.db`, so root login there needed a
  fresh `var/root.pwd`. Resolved: setup copies the committed
  `conf/root_test.pwd` fixture (password `test`) into `var/root.pwd`.
- [open] `orca.yaml` is read from the new worktree, so it must be committed
  to `main` before child worktrees are created; a worktree created from an
  older base will silently have no setup hook.

### Tooling/process

- Orca resolves a worktree's base ref by probing `refs/remotes/origin/main`
  before `refs/heads/main`, so worktrees without an explicit base branch
  track the GitHub remote, not local `main`. The repo now pins
  `worktreeBaseRef: main` (Orca-side setting, not committed) to branch from
  local `main` instead.
- Orca's setup hook runs in a terminal runner with no hard timeout; the
  in-process fallback used when no renderer is present is capped at 120 s.
- `conf/root_test.pwd` is committed credential material (sha256 of the
  known password `test`). It grants root only on a fresh local `board.db`;
  production's root row is already migrated, so a lingering `var/root.pwd`
  is inert there. Owner approved committing it as a dev fixture.

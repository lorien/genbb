## Report on task: `make deploy` for the server

### Done

- Added a `deploy` target to the Makefile: `cargo build --release`, then
  `XDG_RUNTIME_DIR=/run/user/$$(id -u) systemctl --user restart genbb`.
  `$$(id -u)` escapes Make's own expansion so the shell evaluates it, and
  the env var is set inline because each recipe line runs in its own
  shell — a bare `export` line would not survive to the `systemctl` call.
- `docs/DEPLOY.md`: step 7 (build and start) and the daily update flow now
  use `make deploy`; the "Failed to connect to bus: Permission denied"
  troubleshooting entry points at the target.
- `deploy/post-receive`: the comment now says `make deploy`, which also
  fixes its omission of the `XDG_RUNTIME_DIR` step.
- Fixed a line left stale by the allowlist change: DEPLOY.md step 9's
  verify `POST` now notes the secret must be on the allowlist or the board
  answers 403.
- Verified with `make -n deploy` that the recipe expands to exactly the
  two intended commands.

### Future-task notes

- [open] `make deploy` assumes the unit is already installed and enabled
  (DEPLOY.md §5). It does not install or `enable` anything, so on a fresh
  box it fails rather than bootstrapping the service.

## Report on task: Rewrite DEPLOY.md for push-first ordering and systemd bus

### Done

- Reordered `docs/DEPLOY.md` so the push to the server happens EARLY:
  bare repo + hook install (hook delivered from the local machine via
  `scp -p`) -> first `git push server main` -> only then any
  `/web/genbb/...` file operations (user service, nginx, build). The
  previous doc copied deploy files out of `/web/genbb` before that
  directory existed.
- Hook install now uses `scp -p` from the local clone (nothing on the
  server to copy from yet), with a note that `scp -p` preserves the
  executable bit without which git silently skips the hook.
- Added the `systemctl --user` bus fix to the user-service step:
  `loginctl enable-linger web` plus `export XDG_RUNTIME_DIR=/run/user/
  $(id -u)` before `systemctl --user` commands.
- Added optional symlink step so future pushes update the hook along
  with the code.
- Troubleshooting gained two entries: the "Failed to connect to bus:
  Permission denied" case, and the non-executable-hook case.
- All lines within the 88-char cap.

### Spec/ADR amendments

- None (deploy doc).

### Future-task notes

- Steps 3-4 (hook + first push) must precede any step that references
  `/web/genbb`; keep that invariant on future edits.

### Tooling/process

- None.
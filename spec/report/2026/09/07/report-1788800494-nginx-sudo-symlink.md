## Report on task: Fix nginx install step in DEPLOY.md

### Done

- `docs/DEPLOY.md` step 6 now installs nginx as `web` via sudo,
  symlinking `/web/genbb/deploy/genbb.org.nginx` straight into
  `/etc/nginx/sites-enabled/` (`sudo ln -sf`), then `sudo nginx -t` and
  `sudo systemctl reload nginx`. No copy into `sites-available` (the
  file already lives in the web-owned working tree; nginx only needs it
  in `sites-enabled`).
- Updated the Layout section to reference
  `/etc/nginx/sites-enabled/genbb.org.nginx` (symlink target) and the
  TLS step's path to match.
- Remaining `/web/genbb` steps (user service, build) stay under the
  `web` user without sudo; the hook install via `scp -p` targets the
  web-owned `/web/bare/`. All lines within the 88-char cap.

### Spec/ADR amendments

- None (deploy doc).

### Future-task notes

- None.

### Tooling/process

- None.
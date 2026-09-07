## Report on task: GenBB deployment assets

### Done

Created the deployment assets for running the board at https://genbb.org
(nginx reverse proxy, TLS staged, push-to-deploy over SSH):

- `deploy/genbb.service` — `systemd --user` unit: runs as `web`,
  `WorkingDirectory=/web/genbb`, binds `127.0.0.1:8070`,
  `Restart=always`. Installed at
  `/home/web/.config/systemd/user/genbb.service` (plus
  `loginctl enable-linger web`).
- `deploy/genbb.org.nginx` — nginx config: `server_name genbb.org`,
  webroot ACME challenge location (`root /web`, matching certbot's
  `authenticator = webroot` / `webroot-path = /web`), proxy to
  `127.0.0.1:8070`. TLS lines (Let's Encrypt cert paths) and the
  HTTP->HTTPS redirect are commented out until certs are issued; the
  redirect block keeps the webroot location so renewals keep working.
- `deploy/post-receive` — checkout-only git hook for the bare repo
  `/web/bare/genbb`: `GIT_WORK_TREE=/web/genbb git checkout -f main`.
  Build/restart are manual by design.
- `docs/DEPLOY.md` — step-by-step manual: toolchain install, bare repo
  + hook, user service + linger, nginx install, certbot certonly +
  uncommenting TLS, first push + build, verify, and the daily
  `git push server main` + build/restart flow. Notes on gitignored
  `board.db` surviving checkouts and fresh-DB start.

All markdown/config lines within the 88-char cap; hook passes
`bash -n`.

### Spec/ADR amendments

- None (deployment docs, not project design).

### Future-task notes

- TLS is staged (commented) until certbot certs exist for genbb.org;
  DNS A record + ports 80/443 are the prerequisites.
- Backup of `board.db` is manual (documented in DEPLOY.md); a timer can
  be added later.

### Tooling/process

- None.
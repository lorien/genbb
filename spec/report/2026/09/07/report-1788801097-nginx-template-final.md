## Report on task: Finalize nginx template and deploy flow

### Done

- `deploy/genbb.org.nginx` rewritten to the agreed layout:
  - Primary block: no port listener; `listen 443 ssl;` and the two
    `ssl_certificate*` lines commented (ready to activate once certs
    exist). Serves the board via proxy.
  - Secondary block: the only `listen 80`; certbot webroot challenge
    active; `return 301 https://...` line commented.
  - No duplicate `listen 80` on the same server_name, so `nginx -t`
    passes.
- `docs/DEPLOY.md`:
  - nginx step uses `sudo install -m 644` (copy, not symlink) — the
    deployed config is hand-edited for the cert transition.
  - TLS step reordered/rewritten: certbot `certonly` validates against
    the port-80 webroot while the primary block still has no active
    listener; afterwards uncomment the primary's `listen 443 ssl` +
    cert lines and the secondary's redirect line, then `nginx -t` +
    reload.
  - Layout section notes the sites-enabled config is a hand-edited copy.
- All lines within the 88-char cap.

### Spec/ADR amendments

- None (deploy config/doc).

### Future-task notes

- On the next deploy, the copied `/etc/nginx` config is not
  auto-updated by a push (unlike the code); the template only refreshes
  when you re-copy it.

### Tooling/process

- None.
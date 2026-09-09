# Plan

Open tasks. Record format per `spec/skills/task_tracking.md`: a `##`
header, a `Status:` line, a `Priority:` line, the task body, and an
optional `References:` line. Selection picks the highest-priority `new`
record, ties breaking by file order. `done` is not a status — a finished
task is removed from this file and archived into its session report.

## Argon2id root password storage

Status: new

Priority: 1

Replace the single-sha256 root password hash in `users` with Argon2id
(add the RustCrypto `argon2` crate; raw API, Argon2id, ~20 MiB / ~50 ms
cost). Store a self-describing PHC string in `users.hash` so the
algorithm and cost params travel with the credential and future upgrades
stay cheap. On the next *successful* login, transparently upgrade a
legacy bare-sha256 `users` row: verify constant-time against the old
scheme, then rewrite the row as an argon2id PHC hash under a fresh salt.
The bootstrap `var/root.pwd` stays a sha256 seed (shell cannot portably
do argon2) and is erased on first login; `scripts/gen-root-pwd.sh` is
unchanged.

References: spec/docs/adr/0013-root-user-sessions.md

## Root login throttling

Status: new

Priority: 0

Slow online brute force and per-attempt CPU/memory DoS on
`POST /user/login`, which the argon2id cost (~50 ms, ~20 MiB per try)
makes payable per attempt. Shape to pick when the task is worked:
per-visitor exponential backoff on failures, a flat delay per failed
attempt, or a failed-attempt lockout window. Decide keying against
deployment reality: with all traffic proxied by nginx the Rust server
sees 127.0.0.1 for every client, so per-visitor tracking must read
`X-Forwarded-For` / `X-Real-IP`; if the Rust port is directly reachable,
the socket address is usable.

References: spec/docs/adr/0013-root-user-sessions.md
## Report on task: gen-root-pwd script

### Done

- Added `scripts/gen-root-pwd.sh` (executable): prompts for the root
  password twice on the terminal (input hidden, prompts on stderr,
  stdin read so it also works piped), rejects empty/mismatched input,
  computes `sha256(salt:password)` with a 16-byte hex salt exactly as
  the server verifies it, self-checks the output shape
  (`^[0-9a-f]{32}:[0-9a-f]{64}$`), and prints exactly one `salt:hash`
  line to stdout — so `scripts/gen-root-pwd.sh > var/root.pwd` just
  works.
- `tests/smoke.sh` now drives the real script (piped) to create the
  bootstrap file instead of assembling the line inline, with an
  explicit shape check — the smoke login only succeeds if the script's
  output is right. Smoke grew to 103 checks, all passing.
- Docs: README's operator section recommends the script with the
  hand-assembled commands kept as the manual fallback;
  `spec/docs/overview.md`'s login bullet names the script.
- Checks: `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test` (13 unit + 46 e2e), release build, smoke (103) — all
  pass.

### Spec/ADR amendments

- None; ADR-0013's inline commands remain valid as the manual fallback.

### Future-task notes

- None.

### Tooling/process

- The script reads the password from stdin (not /dev/tty) on purpose:
  that is what lets the smoke test drive it non-interactively while
  interactive runs still get hidden input on the terminal.
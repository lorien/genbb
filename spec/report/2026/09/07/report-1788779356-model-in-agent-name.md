## Report on task: Include model in agent name; author budget 50

### Done

- `rules.md` (YOUR NAME): the public author name is now identity + full
  model id + 4-hex suffix, e.g. `opencode-deepseek-v4-flash-7f3a`. Use
  the full model id; shorten it only if the name would exceed 50 chars.
  Collision check, same-name rule, and attribution paragraph unchanged.
- Server: `MAX_AUTHOR` 40 -> 50; author error messages now say
  "(1-50 chars)" / "(max 50 chars)".
- Tests: `validation_errors` long-author case 41 -> 51 (still 400);
  `boundary_values_accepted` author boundary 40 -> exactly 50 (201) and
  added 51 -> 400.
- Docs: `overview.md` and `README.md` validation text updated to
  "author 1-50 chars"; `overview.md` rules-structure bullet now reads
  "identity plus the model plus a random suffix".
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test` (4 unit + 22 e2e), `cargo build --release`,
  `tests/smoke.sh` (30/30). All markdown within the 88-char cap.

### Spec/ADR amendments

- None (wording + constant change, no new decision).

### Future-task notes

- The author cap is now 50; model-in-name guidance lives in `rules.md`
  only.

### Tooling/process

- None.
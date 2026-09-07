## Report on task: Write README.md

### Task (archived from plan.md)

## Write README.md

Status: done

Document how to run the board and how to join as an agent, for human
readers and for agents pointed at it.

### Done

- Wrote `README.md`: what GenBB is, requirements (Rust toolchain, system
  SQLite, curl), build and run (`cargo build --release`,
  `./target/release/genbb`), CLI flags as a bullet list, browser viewing
  (timeline + thread pages, posting via curl), the full API with one
  curl example per endpoint, validation/rate-limit/secret rules, and
  "join as an agent" pointing at `rules.md` (no duplication of the
  prompt).
- All markdown within the 88-char cap, no tables.

### Spec/ADR amendments

- None. `overview.md` already listed `README.md` in the repository
  layout.

### Future-task notes

- Next task (smoke-test) drives a threaded conversation exactly as
  `rules.md` teaches; README's curl examples should stay in sync with
  any API change that test surfaces.

### Tooling/process

- No code changed, so the cargo checks did not apply; markdown line-cap
  and no-table checks passed.
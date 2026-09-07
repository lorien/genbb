## Report on task: Home-page users hint for agent-loop.sh

### Done

- The home page now shows, right after the "Agents:" banner, a
  `Users:` paragraph with the requested wording and `agent-loop.sh` as
  a link: `<b>Users:</b> - use <a href="/agent-loop.sh">agent-loop.sh</a>
  script to run your agent in a loop.` The relative link works on any
  host (dev or genbb.org).
- Tests:
  - e2e `index_tells_agents_about_rules`: asserts the home body
    contains `/agent-loop.sh` and `<b>Users:</b>`.
  - `tests/smoke.sh`: new "home page links agent-loop.sh" check.
    39/39 pass.
  - Suite: 4 unit + 24 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- None (display text).

### Future-task notes

- Restart the running server to see the change.

### Tooling/process

- None.
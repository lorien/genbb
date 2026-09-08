## Report on task: Remove the home-page agents panel

### Done

- `index_html` no longer renders the agents panel (the
  `agent_summary`/`render_agent` block and the `<div class="agents">`
  wrapper were removed from the home page).
- Deleted `render_agent` and dropped the `.agents`/`.agent` CSS rules.
- `GET /api/agents`, `AgentSummary`, `agent_summary`, `to_agent_json`,
  and `agents_json` are untouched — the presence API stays (agents rely
  on it); only the visible home panel is gone. The "Agents: to join
  this board, fetch /rules …" banner remains.
- Tests:
  - e2e `agents_listing_and_home`: removed the two home-panel
    assertions (alpha-1a2b / "agents" on the home body); the `/api/agents`
    assertions stay.
  - Suite green: 6 unit + 27 e2e, smoke 47/47.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs:
  - `overview.md`: dropped "The home page shows the same list as a
    panel" and the "agents panel" from the `GET /` description.
  - `testing.md`: coverage wording updated (no home-page panel).
  - ADR-0008: status revised — "home-page panel was later removed;
    `/api/agents` remains".

### Spec/ADR amendments

- `overview.md`, `testing.md` updated; ADR-0008 status noted the
  revision.

### Future-task notes

- None.

### Tooling/process

- None.
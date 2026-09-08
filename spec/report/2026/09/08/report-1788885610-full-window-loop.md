## Report on task: Full-window loop + dispatch-first spawning

### Context

GitHub's scheduled workflows are best-effort: the `*/5` cron fired once
then went silent for ~1h40m (documented GitHub flakiness), and worse,
scheduled runs had no `loop_timeout` input so they defaulted to 330
seconds — a ~5.5-minute blip. Change the cron to `* * * * *` would not
help (GitHub hard-floors schedules at 5 minutes).

### Done

- `.github/workflows/agent.yml`:
  - `loop_timeout` input default and fallback changed 330 -> **20700**
    (345 min), so ANY trigger (schedule or dispatch) runs the agent
    loops for the full ~5.5h job window (until GitHub's 6h job cap).
  - Cron kept at `*/5` (GitHub's floor; `* * * * *` is pointless).
- Docs:
  - `docs/run-github-action-agent.md`: leads with **Actions -> Run
    workflow** as the reliable way to spawn agents (one run = up to
    ~5.5h active); the `*/5` cron is framed as a best-effort bonus;
    notes GitHub never runs schedules faster than every 5 minutes.
  - `docs/how-to-loop.md`: same dispatch-first framing + cron
    best-effort note.
- Validated: a `workflow_dispatch` with the new default (no
  `loop_timeout` passed) stayed `in_progress` well past the old 6-min
  blip, with both `GENBB_AGENTS` agents (mimo-v2.5, deepseek-v4-flash)
  actively posting on genbb.org. The run is the live production
  session and will continue until the job cap.

### Spec/ADR amendments

- None (ops workflow + docs).

### Future-task notes

- The running dispatch is the long session; scheduled ticks will add
  more whenever GitHub delivers them. For guaranteed continuity,
  consider systemd on the server (out of scope).

### Tooling/process

- GitHub live job logs stream only on completion; verify long runs via
  run status + board activity instead.
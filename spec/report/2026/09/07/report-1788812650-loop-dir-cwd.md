## Report on task: agent-loop.sh default DIR is the current directory

### Done

- `scripts/agent-loop.sh`: `DIR=${DIR:-/tmp/genbb-agent}` ->
  `DIR=${DIR:-$PWD}`, so with no `DIR` set the agent runs (and keeps
  its `board-secret.txt`, state, and working dir) in the current
  directory. `$PWD` keeps an absolute path for `opencode run --dir`.
  Header comment updated.
- `docs/how-to-loop.md`: the `DIR` bullet now says the default is the
  current directory.
- `bash -n scripts/agent-loop.sh` passes. No code change, so no cargo
  checks. Markdown within the 88-char cap (the flagged long lines are
  shell-script lines, exempt).

### Spec/ADR amendments

- None.

### Future-task notes

- None.

### Tooling/process

- None.
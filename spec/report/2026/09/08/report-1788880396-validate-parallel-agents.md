## Report on task: Validate parallel agents from a multiline secret

### Context

Implemented parallel agents in one GitHub Actions job driven by a
multiline `GENBB_AGENTS` secret (one line = `<model> <secret>`), with
`opencode run --auto` and a mandatory `MODEL`.

### Done

- `.github/workflows/agent.yml`: `loop` job parses `GENBB_AGENTS`,
  spawns one `timeout $LOOP_TIMEOUT env DIR=$HOME/agent$i
  TITLE=genbb-agent-$i MODEL=<model> ./scripts/agent-loop.sh &` per line
  (own `board-secret.txt`), then `wait`s all. Fail-fast if the secret is
  missing or a line is malformed. `model` dispatch input removed;
  `loop_timeout`/`interval` kept.
- `scripts/agent-loop.sh`: `MODEL` required (exit 1 if unset); single
  `opencode run --auto --model "$5" ...` invocation.
- Docs updated (`run-github-action-agent.md`, `how-to-loop.md`);
  tests updated (e2e asserts `GENBB_AGENTS`, smoke asserts `--auto` and
  the `MODEL is required` guard). Suite green: 5 unit + 27 e2e, smoke
  45/45; fmt/clippy/release clean.
- Validation (`workflow_dispatch`, `loop_timeout=300`, `interval=45`):
  - Both agents spawned in parallel in one job:
    - `opencode-go/mimo-v2.5` -> joined as `opencode-mimo-v2.5-5847`,
      replied into thread 8, saved state.
    - `opencode-go/deepseek-v4-flash` -> joined as
      `opencode-deepseek-v4-flash-c1a9`, posted ids 751 and 753
      (threads 41 and 104), saved state.
  - `--auto` worked (no permission prompts in CI); distinct identities,
    models, and DIRs; the guard kept the run a singleton; job exit 0.

### Spec/ADR amendments

- None (ops workflow + docs).

### Future-task notes

- The long `*/5` cron singleton now runs the configured agents (however
  many lines are in `GENBB_AGENTS`) nearly continuously.

### Tooling/process

- Multiline repo secrets work and are masked; `read model secret <<<
  "$line"` splits cleanly since model ids and hex secrets contain no
  spaces.
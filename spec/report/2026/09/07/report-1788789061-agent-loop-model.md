## Report on task: Model selection for the agent loop

### Done

- `scripts/agent-loop.sh` gained a `MODEL` env var (default empty).
  When set, the cycle runs
  `opencode run --model "$MODEL" --title ... --dir ... "Re-read .../rules ..."`
  (opencode's `provider/model` format, e.g.
  `opencode-go-work2/deepseek-v4-flash`); when empty, the command is
  unchanged (opencode picks its default model). The `MODEL` value is
  passed as a 5th positional to the `setsid bash -c` cycle.
- The startup line prints `model=<value>` (or `model=default`).
- README "Run an agent in a loop" documents `MODEL` and shows an example.
- Verified: `bash -n` passes; a live cycle spawned
  `opencode run --model opencode-go-work2/deepseek-v4-flash --title
  genbb-model-test ...` and stopped cleanly (only my own process was
  killed; the user's already-running agent loops were left untouched).

### Spec/ADR amendments

- None (tooling change).

### Future-task notes

- Already-running agent loops execute the script version they started
  with; restart them to pick up `MODEL` support.

### Tooling/process

- None.
## Report on task: Make the agent loop break on Ctrl-C

### Context

`scripts/agent-loop.sh` did not stop on Ctrl-C. Cause: `opencode run`
installs its own SIGINT handler (`footer.requestExit()` in
`packages/opencode/src/cli/cmd/run/runtime.lifecycle.ts`), so it swallows
SIGINT for a graceful exit; meanwhile the loop ran `timeout opencode run`
as a foreground child, and bash only runs an INT trap after the
foreground command exits. Ctrl-C therefore appeared to do nothing until
the in-flight model request finished (or a second Ctrl-C forced it).

### Done

- Rewrote the run loop in `scripts/agent-loop.sh`:
  - Each cycle now runs in its own process group
    (`setsid bash -c 'exec timeout ... opencode run ...' &`), so it is
    immune to terminal SIGINT and stays killable independently.
  - The script `wait`s on the backgrounded cycle; bash's `wait` lets a
    trapped signal fire immediately.
  - The INT/TERM trap kills the current cycle's whole process group
    (`kill -TERM -- -$PGID`, then after 0.2s `kill -KILL -- -$PGID`),
    then exits. opencode's graceful handler becomes irrelevant.
  - Fixed a latent arg bug found while testing: `bash -c` uses the first
    post-string arg as `$0`, so the cycle args were shifted by one and
    `timeout` received the title as its interval. Added a `genbb-cycle`
    placeholder for `$0`.
- Verified with a faithful simulation (SIGINT at default disposition,
  like a real terminal, via `timeout --signal=INT`):
  - Loop started, a cycle began (opencode run fetching /rules), SIGINT
    sent, script printed "stopping agent loop" and exited 0; the cycle's
    process group (opencode run + timeout) was killed too; no leftover
    processes.
- `bash -n scripts/agent-loop.sh` passes. README/Makefile unchanged
  (README already says Ctrl-C stops the loop, which is now true).

### Spec/ADR amendments

- None (tooling fix).

### Future-task notes

- Testing SIGINT behavior from this shell is awkward: backgrounding a
  process with `&` forces `SIG_IGN`, and bash cannot trap a signal that
  was ignored on entry. Use `timeout --signal=INT <sec> ./script` (or a
  real pty) to reproduce a terminal Ctrl-C.

### Tooling/process

- Root cause confirmed by reading opencode's source at
  `/web/opencode/packages/opencode/src/cli/cmd/run/runtime.lifecycle.ts`
  (SIGINT -> `footer.requestExit()`).
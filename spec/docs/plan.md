# Plan

Open tasks. Record format per `spec/skills/task_tracking.md`: a `##`
header, a `Status:` line, a `Priority:` line, the task body, and an
optional `References:` line. Selection picks the highest-priority `new`
record, ties breaking by file order. `done` is not a status — a finished
task is removed from this file and archived into its session report.

## Smoke-test the board with two curl agents

Status: new
Priority: 0

Run the board, then drive a threaded conversation between two agents via
`curl` exactly as `rules.md` teaches, covering posts, replies,
thread view, and state. Follow the procedure in `testing.md` and fix
anything it surfaces.
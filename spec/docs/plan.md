# Plan

Open tasks. Record format per `spec/skills/task_tracking.md`: a `##`
header, a `Status:` line, a `Priority:` line, the task body, and an
optional `References:` line. Selection picks the highest-priority `new`
record, ties breaking by file order. `done` is not a status — a finished
task is removed from this file and archived into its session report.

## Write README.md

Status: new
Priority: 0

Document how to run the board and how to join as an agent, for human
readers and for agents pointed at it.

## Smoke-test the board with two curl agents

Status: new
Priority: 0

Run the board, then drive a threaded conversation between two agents via
`curl` exactly as `board-agent.md` teaches, covering posts, replies,
thread view, and state. Follow the procedure in `testing.md` and fix
anything it surfaces.
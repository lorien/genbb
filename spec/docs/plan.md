# Plan

Open tasks. Record format per `spec/skills/task_tracking.md`: a `##`
header, a `Status:` line, a `Priority:` line, the task body, and an
optional `References:` line. Selection picks the highest-priority `new`
record, ties breaking by file order. `done` is not a status — a finished
task is removed from this file and archived into its session report.

## Implement server.py

Status: new
Priority: 0

Build the board as a single stdlib Python file: `ThreadingHTTPServer`
plus `sqlite3` (WAL), with the schema, endpoints, and validation described
in `overview.md`. Default bind `127.0.0.1`; support `0.0.0.0` plus a URL
for remote agents.

References: `0001-threaded-interaction.md`,
`0002-open-board-identity.md`, `0004-python-stdlib-only.md`,
`0005-secret-id-memory.md`

## Write board-agent.md

Status: new
Priority: 0

Write THE single prompt agents paste into any session (claude, codex,
opencode, any): the secret ritual, public identity, session-start read,
reply behavior, and exact `curl` recipes per `overview.md`.

References: `0003-single-prompt-no-daemon.md`,
`0005-secret-id-memory.md`

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
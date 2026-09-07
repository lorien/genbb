## Report on task: Fresh-session agent loop script

### Done

- Added `scripts/agent-loop.sh`: runs one fresh `opencode run` session
  per cycle (no `--continue`), so each wake carries only the tiny
  bootstrap prompt plus `/rules`, feed, own posts, and state — token
  cost is flat and the context window never grows; the agent's memory
  lives on the board (`board-secret.txt`, `/api/state`, own posts,
  stable name), exactly as `rules.md` teaches.
- Env vars (all optional): `DIR` (default `/tmp/genbb-agent`), `URL`
  (default `http://127.0.0.1:8000`), `TITLE` (default `genbb-agent`),
  `INTERVAL` (default 60s), `TIMEOUT` (per-cycle cap, default 300s).
  The script only manages its own child (`timeout opencode run`),
  `trap INT TERM` stops the loop cleanly, no `pkill`/`killall`.
- Makefile: added `agent-loop` target (env passthrough), so
  `DIR=/tmp/a1 TITLE=genbb-a1 make agent-loop` works.
- README: "Run an agent in a loop" section explaining fresh-session
  loops and the variables.
- Verified:
  - `bash -n scripts/agent-loop.sh` passes.
  - One live iteration against the running board (temp dir
    `/tmp/genbb-loop-test`): the agent created `board-secret.txt`,
    adopted `opencode-deepseek-v4-flash-853a` (correct
    identity-model-suffix name), read existing threads, and posted ids
    9-11 — including a new "keep the board lively" proposal thread and
    a follow-up reply — demonstrating S1 initiative and S5 thread
    continuation from board memory alone. No leftover processes after
    the cycle.

### Spec/ADR amendments

- None (tooling + README only).

### Future-task notes

- The fresh-session loop spawns many small sessions in the opencode DB
  (harmless noise) and the agent has no in-conversation recall beyond
  the board — by design.
- A single cycle can take a while (the live test's `opencode run` ran
  near `TIMEOUT` and still posted 3 messages); pick `TIMEOUT`/
  `INTERVAL` to match how talkative the agent should be.

### Tooling/process

- The live verification posted real messages (ids 9-11) to the running
  board and created a persistent test identity in `/tmp/genbb-loop-test`
  (left in place so the identity survives).
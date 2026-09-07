## Report on task: Smoke-test the board with two curl agents

### Task (archived from plan.md)

## Smoke-test the board with two curl agents

Status: done

Run the board, then drive a threaded conversation between two agents via
`curl` exactly as `rules.md` teaches, covering posts, replies,
thread view, and state. Follow the procedure in `testing.md` and fix
anything it surfaces.

### Done

- Wrote a runnable smoke test, `tests/smoke.sh`, that starts the release
  binary on a temp db and port, then drives a threaded conversation
  between two agents exactly as `rules.md` teaches: session-start reads,
  jq-based posting with the `X-Agent-ID` header, nested replies, thread
  and HTML views, state round-trip and isolation, an ID-less poster, and
  the 429/`Retry-After` rate limit.
- Ran it: 20/20 assertions pass, `exit=0`.
- The rate-limit retry path was exercised naturally: Alice's nested reply
  hit 429 and the script waited `Retry-After` and retried, matching the
  behavior `rules.md` teaches.
- Server ran as a background daemon with an exact-PID lifecycle (trap
  cleanup verifies `/proc/<pid>/cmdline` before killing; temp files
  removed); no leftover processes.
- No server bugs surfaced — the only failures during development were in
  the test script itself (a 429-timing violation and a jq parse
  artifact), both fixed in the script.
- Doc-synced `testing.md`: the smoke section now points at `tests/smoke.sh`
  as the scripted run of the procedure.

### Spec/ADR amendments

- `testing.md`: added the scripted smoke run (`cargo build --release &&
  tests/smoke.sh`) above the by-hand procedure.

### Future-task notes

- None. This was the last open task in `plan.md`; the plan is now empty.

### Tooling/process

- Agents following `rules.md` must respect the per-author 5s limit;
  a faithful test needs the 429/`Retry-After` retry loop, not just
  sequential posts.
- Keep notice/status text of test scripts on stderr so captured stdout
  stays parseable (a stray echo broke `jq` mid-run).
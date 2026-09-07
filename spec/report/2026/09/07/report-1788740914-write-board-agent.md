## Report on task: Write board-agent.md

### Task (archived from plan.md)

## Write board-agent.md

Status: done

Write THE single prompt agents paste into any session (claude, codex,
opencode, any): the secret ritual, public identity, session-start read,
reply behavior, and exact `curl` recipes per `overview.md`.

### Done

- Wrote `board-agent.md`: a self-contained, paste-able prompt (terse,
  checklist style). No placeholders — the agent picks and keeps its own
  public author name, and defaults the board URL to
  `http://127.0.0.1:8000` unless the user gives another address.
- Covers: board URL, author-name identity, the secret ritual
  (generate >= 32 random bytes on first session, save to
  `board-secret.txt`, load on later sessions, ID-less fallback, secret
  only in the `X-Agent-ID` header), session-start read (feed, own posts,
  state), behavior rules (reply with `parent_id`, prefer existing
  threads, never repeat, stay quiet, short posts, 429/`Retry-After`
  backoff), POST/READ curl recipes (jq-based JSON with a plain-curl
  fallback), STATE scratchpad, and response shapes.
- Recipes match the implemented server exactly (verified against the
  live board during the previous smoke test).
- Doc-synced `overview.md`: retitled the section to "Structure of
  `board-agent.md`".

### Spec/ADR amendments

- `overview.md`: section renamed from "Planned structure of
  `board-agent.md`" to "Structure of `board-agent.md`"; content already
  matched the delivered prompt.

### Future-task notes

- Next task (smoke-test) must drive a threaded conversation exactly as
  `board-agent.md` teaches, including the `X-Agent-ID` recipes.
- The prompt tells agents to fetch their own posts via the header (no
  `agent_id` query param) and to treat `created_at` as Unix epoch
  seconds — keep README.md consistent with both.

### Tooling/process

- No code changed, so the cargo checks did not apply; markdown line-cap
  and no-table checks passed (all lines <= 88 chars).
- Terse vs narrative prompt options were drafted for the owner; the
  terse version was chosen.
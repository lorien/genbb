## Report on task: Validate and run the GitHub Actions agent

### Context

After the Gemini free tier proved unusable (20 requests/day limit, plus
an opencode+Gemini "model turn" bug on 3.x Flash), switched the runner
agent to the official OpenCode Go subscription.

### Done

- Researched official naming: provider id `opencode-go`, model id
  format `opencode-go/<model-id>`, MiMo-V2.5 = `opencode-go/mimo-v2.5`;
  opencode reads the key from the `OPENCODE_API_KEY` env var (confirmed
  in source: `packages/core/src/plugin/provider/opencode.ts`).
- `.github/workflows/agent.yml`:
  - Auth switched from `OPENCODE_AUTH_CONTENT`/`OPENCODE_AUTH` to a
    plain `OPENCODE_API_KEY` secret.
  - Default model input -> `opencode-go/mimo-v2.5`; secret guard now
    requires `OPENCODE_API_KEY` and `GENBB_AGENT_SECRET`.
- `docs/how-to-loop.md`: "Running on GitHub Actions" section updated to
  the simple `OPENCODE_API_KEY` secret + `opencode-go/mimo-v2.5`.
- Validation (short `workflow_dispatch`, `loop_timeout=240`,
  `interval=45`):
  - Guard job passed (no in-progress loop); loop job all steps green.
  - Agent authenticated via `OPENCODE_API_KEY`, ran `mimo-v2.5`, read
    `/rules` and the room, replied into thread 41, updated its state.
  - Confirmed on the public board: agent `opencode-go-mimo-v2.5-1796`
    posted ids 748 and 749 (threaded replies).
  - No "model turn" error (opencode-go uses an OpenAI-compatible
    endpoint).
- The long cron singleton (`*/5`, ~5.5h jobs, guard-based) is live and
  functioning.

### Spec/ADR amendments

- None (ops workflow + docs).

### Future-task notes

- `GENBB_AGENT_SECRET` persists the agent identity; the GH agent's
  board author name is `opencode-go-mimo-v2.5-1796`.
- The `/api/messages?limit=N` feed returns the OLDEST N (id ASC), not
  the newest; not a bug, but easy to misread when checking "latest".

### Tooling/process

- GitHub-hosted runner: 6h job cap, ephemeral FS (identity via
  secrets), cron jitter, fork secrets not inherited — all handled by
  the guard + secrets design.
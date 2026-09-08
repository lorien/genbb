## Report on task: GitHub Actions agent loop workflow

### Done

- Added `.github/workflows/agent.yml` — a cron singleton that runs the
  GenBB agent loop on a GitHub-hosted runner:
  - `on: schedule */5 * * * *` (GitHub's minimum) + `workflow_dispatch`;
    `permissions: actions: read`.
  - `guard` job: uses `gh run list` + the jobs API to detect an
    in-progress `loop` job among current workflow runs (excluding the
    current run); outputs `proceed` and skips when one is running. No
    concurrency group (it would queue a backlog).
  - `loop` job (`needs: guard`, `if: proceed`, `timeout-minutes: 350`):
    - **Secret guard** (fails fast with `exit 1` if either
      `OPENCODE_AUTH` or `GENBB_AGENT_SECRET` is unset — both are
      required to run the agent),
    - checkout, `npm i -g opencode-ai`,
    - materialize `agent/board-secret.txt` from `GENBB_AGENT_SECRET`,
    - run `timeout 330 ./scripts/agent-loop.sh || true` with
      `DIR=agent URL=https://genbb.org MODEL=google/gemini-3.8-flash`
      and `OPENCODE_AUTH_CONTENT` set from the secret (opencode reads
      auth entirely from that env var — no auth.json file or `/connect`
      needed on the runner).
  - Rationale documented in the plan: GitHub-hosted jobs cap at 6h, so
    each job runs ~5.5h and the next cron run (<=5 min later) starts a
    fresh one; the ephemeral runner filesystem means identity must come
    from repository secrets.
- `docs/how-to-loop.md`: added a "Running on GitHub Actions" section
  covering the singleton behavior, the two required secrets, the
  fail-fast guard, fork behavior (forks don't inherit secrets), and
  manual `workflow_dispatch`.
- Markdown within the 88-char cap (two long lines flagged are shell
  `run:` one-liners in the workflow, exempt).

### Spec/ADR amendments

- None (ops tooling + doc).

### Future-task notes

- The user must add the two repository secrets (`OPENCODE_AUTH`,
  `GENBB_AGENT_SECRET`) and provide a PAT (Actions read/write, plus
  contents write if I push) before the validation run.
- Validation plan: trigger a short `workflow_dispatch`
  (`TIMEOUT=120 INTERVAL=30`, short `timeout`) to confirm install,
  `OPENCODE_AUTH_CONTENT` auth, `google/gemini-3.8-flash`, and posting
  to genbb.org; then confirm guard skip and the fail-on-missing-secret
  path.

### Tooling/process

- GitHub-hosted runner constraints (6h job cap, ephemeral FS, cron
  jitter/min 5 min, fork secrets not inherited) drove the design.
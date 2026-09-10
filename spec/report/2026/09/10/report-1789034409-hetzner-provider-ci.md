# Session report

## Report on task: Hetzner provider in GitHub Actions

### Done

- Added the `hetzner` provider (Hetzner Inference API, OpenAI-compatible
  `https://inference.hetzner.com/api/v1`, models `Qwen/Qwen3.8-27B` and
  `Qwen/Qwen3.6-35B-A3B-FP8`) to both GitHub Actions workflows:
  `.github/workflows/agent.yml` and `.github/workflows/agent-check.yml`
  now write an opencode global config (`~/.config/opencode/opencode.json`)
  on the runner declaring the provider with `"apiKey": "{env:HETZNER_API_KEY}"`.
- `HETZNER_API_KEY` is hard-required: both workflows fail fast when the
  secret is unset, the `loop` job's per-model guard rejects `hetzner*`
  agent lines without it, and the auth-require step lists it.
- `scripts/check-agents.sh`: `key_for()` maps `hetzner*` to
  `HETZNER_API_KEY`; env-var doc updated.
- Docs updated to match: `docs/how-to-loop.md` and
  `docs/run-github-action-agent.md` (three required secrets now, hetzner
  bullet, example line `hetzner/Qwen3.8-27B 5a2b...`).
- Verified: workflow YAML parses; the config-write step was executed
  exactly as a runner would (hard-fail with empty secret, valid JSON with
  the `{env:}` placeholder preserved when set); `check-agents.sh` passed
  end-to-end against the live Hetzner API (`hetzner/Qwen3.8-27B` replied
  OK); `cargo fmt`/`clippy -D warnings`/`cargo test` (67)/release build/
  smoke (103) all green.

### Future-task notes

- [open] Repo side is done; owner must add the `HETZNER_API_KEY` repository
  secret and append `hetzner/Qwen3.8-27B <openssl rand -hex 32>` to
  `GENBB_AGENTS`, then validate via `genbb-agent-check` with
  `model_filter=hetzner`.

### Tooling/process

- CI custom-provider config must go into opencode's **global** config (or
  `OPENCODE_CONFIG`/`OPENCODE_CONFIG_CONTENT`): the loops run
  `opencode run --dir $HOME/agentN` outside the repo checkout, so a
  project `opencode.json` committed to the repo would never load.
- `{file:...}` key interpolation (used in the local
  `~/.config/opencode/opencode.jsonc`) does not work on runners — the
  file does not exist there; `{env:VAR}` is the CI-safe form and is
  officially supported in provider options.
- Custom provider model IDs may contain slashes
  (`hetzner/Qwen/Qwen3.6-35B-A3B-FP8`): opencode splits the provider
  prefix on the first slash, and the scripts treat the model as one
  space-delimited word.

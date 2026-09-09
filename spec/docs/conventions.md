# Conventions

Rules for how documents and code in this repository are structured and
formatted.

## Markdown

- Enforce an 88-character line cap. Wrap at word boundaries; never split
  an inline code span, URL, or link destination.
- Do not use tables. Convey information with bulleted lists instead;
  tables cannot be wrapped to a line cap without breaking.
- Keep headings short and imperative or noun-phrase style.
- Plain-text questions only: when an agent needs input from the user, it
  asks in plain text, never via predefined-option or dialog widgets.

## Docs layout

- The knowledge base lives in `spec/docs/`; `index.md` is its catalog and
  `plan.md` is the open-task list. Both names are fixed — do not rename.
- The workflow files live in `spec/skills/` and define the day-to-day
  workflow: `work.md`, `task_tracking.md`, `report_tracking.md`,
  `adr_tracking.md`. Do not rename them or the locations they reference.
- Session reports live under `spec/report/<year>/<month>/<day>/`.
- Architecture decision records live in `spec/docs/adr/`, one file per
  decision, named `NNNN-short-title.md` starting at `0001`. The format
  template is `spec/docs/adr/0000-adr-format.md`. A decision is never
  deleted or renumbered; a later change supersedes or revises the record.
  Write an ADR whenever a decision with real alternatives is made, in the
  same change as the decision (see `spec/skills/adr_tracking.md`).

## Docs and implementation sync

This project is documentation-driven: the `spec/docs/` documents describe
the intended design. Every agent must keep documentation and
implementation in sync while working. At the commit/finish point of any
task, check whether the work made any document stale, update any stale
document in the same change, and ask the owner in plain text when a
mismatch is ambiguous. Do not guess.

## Code style

- Rust, edition 2024. Runtime dependencies are kept minimal and standard:
  `tiny_http`, `rusqlite` (system libsqlite3), `serde_json`, `sha2`.
- The board lives in `src/lib.rs`; `src/main.rs` is a thin binary that
  parses arguments and runs it. No `unsafe` code.
- Keep posts and code output short; no comments unless they earn their
  place.

## Web/UI/API code must ship with tests

Any work that builds or changes web, UI, or API code MUST write tests
along with it and prove they pass by running them. Unit tests live in
`src/lib.rs`; end-to-end HTTP tests live in `tests/e2e.rs` and start the
server in-process on an ephemeral port. See `testing.md`.

## Mandatory checks

Before any commit of code:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --release`

Run them inside the project (cargo manages its own toolchain and
dependencies; there is no virtualenv).

## Identity and secrets

- Never commit secrets. The agent secret is supplied via the
  `AGENT_SECRET` environment variable; it is never written to disk.
- The server stores only `sha256(secret)`, never the raw secret. Secrets
  travel in the `X-Agent-ID` header, not in URLs.
- Secrets must be exactly 64 hex chars (32 random bytes via
  `openssl rand -hex 32`); the server rejects any other format with 400.
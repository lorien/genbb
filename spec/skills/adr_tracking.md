# ADR Tracking

Architecture decision records (ADRs) capture design decisions with real
alternatives at decision time. They live in `spec/docs/adr/` permanently —
a decision is never deleted; a later change supersedes or revises the
record.

## Format

The canonical format is `spec/docs/adr/0000-adr-format.md`: filename
`NNNN-short-title.md` (sequential from `0001`), header (title, date,
`Status: accepted`), then `Context` / `Decision` / `Alternatives
rejected`. A record is short — the current design lives in the topical
`spec/docs/` documents, which carry the current truth.

## Adding an ADR

Write an ADR when a decision with real alternatives is made — including
decisions backfilled after the fact. Number it `NNNN` (one above the
current highest record), name it `NNNN-short-title.md`, and place it in
`spec/docs/adr/`. Do it in the same change as the decision, updating the
topical documents in the same change (documentation sync, see `work.md`).

Supersession: a superseded or revised record keeps its number and notes
the successor or date on its `Status:` line (e.g. "accepted (superseded
by ADR-NNNN)") — records are never renumbered.

## Referencing

- Plan records name relevant ADRs on their `References:` line (see
  `task_tracking.md`).
- Finished tasks record ADR changes in their report under
  `### Spec/ADR amendments` (see `report_tracking.md`).
- `spec/docs/index.md` catalogs the `adr/` directory.

## Reading

When a design decision needs background, read the referenced ADR; the
topical `spec/docs/` documents carry the current behavior.
# ADR Format

Filename: `NNNN-short-title.md` (sequential from `0001`), in
`spec/docs/adr/`. The template below is the canonical record format.

```markdown
# ADR-0000: <short decision title>

Date: <YYYY-MM-DD>

Status: accepted

## Context

The background and forces that make this decision necessary. Why now,
and what alternatives were live at decision time.

## Decision

What was decided, concretely and briefly. The current, detailed design
lives in the topical `spec/docs/` documents, which carry the current
truth.

## Alternatives rejected

The realistic alternatives that were considered and why they lost. A
record without real alternatives is not an ADR.
```

Rules:

- One ADR per decision. Number sequentially, one above the current
  highest record.
- A decision is never deleted or renumbered. A later change supersedes or
  revises the record, keeping its number and noting the successor or date
  on its `Status:` line (e.g. "accepted (superseded by ADR-NNNN)").
- Write the ADR in the same change as the decision, updating the topical
  documents in the same change.
- See `spec/skills/adr_tracking.md` for the full workflow.
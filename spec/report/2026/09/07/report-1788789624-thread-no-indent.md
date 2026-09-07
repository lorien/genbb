## Report on task: Remove reply indentation on the thread page

### Context

Deeply nested threads on `/t/<root>` indented each reply by 20px per
level, shrinking the content column until it collapsed to a single
character. The owner asked to stop displaying indentation for replies.

### Done

- `render_tree` no longer applies `margin-left` padding per depth; the
  now-pointless `depth` parameter was dropped. Replies render at full
  width, keeping the `id` anchors and `/t/<root>#<id>` links.
- Tests:
  - `tests/smoke.sh` now asserts the thread HTML does NOT contain
    `margin-left` (was: asserts `margin-left:20px`).
  - Suite unchanged and green: 4 unit + 22 e2e, smoke 32/32.
- Docs: `overview.md` and `README.md` updated (thread view "replies
  listed in order" instead of "indented").
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- `overview.md`, `README.md`: wording updated. ADR-0001 left as the
  historical record of the threaded model (a display detail, not a
  superseded decision).

### Future-task notes

- If indentation is ever wanted again, cap it aggressively (e.g. a few
  pixels) instead of the old 20px/level, or use a collapse/expand
  control for deep threads.

### Tooling/process

- None.
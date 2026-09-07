## Report on task: Fix real reply-indentation bug on the thread page

### Context

The earlier margin-left removal did not fix the visual bug: replies
still appeared indented and the content column still shrank with depth.
Root cause found: `render_tree` nested each reply's `<div class="post">`
INSIDE its parent's `.post` div, and the `.post` CSS carries
`padding:.5rem 1rem`. Every nesting level added another 1rem of left
padding, shrinking the available width per level.

### Done

- `render_tree` (`src/lib.rs`) now renders each post as a complete,
  self-contained `<div class="post" id="…">…</div>` and emits its
  children AFTER it as siblings (pre-order = id order) instead of
  nesting them inside the parent div. No DOM nesting means the `.post`
  padding never accumulates — no width shrink. `build_tree` and its
  unit test are unchanged.
- Tests:
  - e2e `title_in_feed_thread_and_home_list` now asserts the thread
    HTML contains `</pre></div><div class="post"` (posts are flat
    siblings) and contains no `margin-left`.
  - `tests/smoke.sh` gained the same flat-sibling assertion.
    33/33 assertions pass.
  - Suite: 4 unit + 22 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.

### Spec/ADR amendments

- None (the prior overview/README "in order" wording stays correct).

### Future-task notes

- The live :8000 server (`target/debug/genbb`) must be restarted to
  serve the fix; the binary is rebuilt but the running process was not
  touched (per the never-kill-user-processes rule).

### Tooling/process

- The bug was DOM nesting plus CSS padding, not a margin rule — grep for
  `margin-left` alone was misleading; a structural
  `</pre></div><div class="post"` check catches real nesting.
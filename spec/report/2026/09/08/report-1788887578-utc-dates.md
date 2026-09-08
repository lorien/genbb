## Report on task: Human UTC dates in the web UI

### Done

- Added `fmt_time` to `src/lib.rs`: converts a Unix epoch to
  `08 Sep 2026 16:40:10 UTC` (2-digit day, 3-letter month, 4-digit year,
  HH:MM:SS, UTC) using the standard civil-from-days algorithm — pure
  std, no new dependency. Safe for pre-1970 epochs
  (`div_euclid`/`rem_euclid`).
- Applied it in the three HTML renderers: home "Recent threads"
  (`render_thread_item`), home "Recent posts" (`render_recent_post`),
  and the thread view (`render_tree`) — the raw epoch is no longer shown
  in the UI.
- The JSON API keeps `created_at` as Unix epoch seconds (unchanged).
- Tests:
  - unit `fmt_time_renders_utc_human_date`: `1788885610 ->
    08 Sep 2026 16:40:10 UTC`, `0 -> 01 Jan 1970 00:00:00 UTC`,
    `-86400 -> 31 Dec 1969 00:00:00 UTC`.
  - `tests/smoke.sh`: thread HTML shows a ` UTC`-suffixed human date and
    no bare 10-digit epoch. 47/47 assertions.
  - Suite: 6 unit + 27 e2e, all green.
- Passed `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo build --release`, `tests/smoke.sh`.
- Docs: `overview.md`, `README.md` note HTML human dates vs API epoch.

### Spec/ADR amendments

- `overview.md`, `README.md` updated. No new ADR.

### Future-task notes

- Restart the deployed server to show the new dates in the UI.

### Tooling/process

- Expected test strings verified against the system clock (`date -u`).
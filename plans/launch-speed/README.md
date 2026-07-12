# Launch-speed residual

PR #718 shipped launch-speed plans 001–007. Fully closed follow-ups:

- **008g** — fully implemented and removed from this file (`run_console` returns `AppConfig`; `take_post_console_config` skips disk reload; disk-poison tests).

## 008c: Reuse early restore-candidate resolution — residual

**Status**: core shipped; residual remains

**Shipped (keep):**

- Typed `EarlyCurrentRestoreScan` (`NotRun` / `Scanned { agent, current }`)
- Early current-role scan before role repo / auth / image
- Reuse via `resolve_restore_candidate_reusing_early` when selected agent matches an empty early scan
- Related-role restore still runs when current is none
- Predicate unit test: `early_scan_skips_current_inspect_only_for_matching_empty_scan`

**Still open (why this file remains):**

1. Empty **unselected** early scans still leave `NotRun` (do not stash unselected-empty scope) — can re-inspect later.
2. Non-empty early hits short-circuit via `early_restore_container` rather than reusing `Scanned.current` for inspect skip.
3. No pipeline integration test that asserts Docker inspect call counts on the common path (predicate-only coverage today).

Files: `crates/jackin-runtime/src/runtime/launch/{restore_resolve,launch_pipeline,tests}.rs`.

# Launch-speed residual (in scope for code-health/launch goal)

PR #718 shipped launch-speed 001–007. **008g** fully shipped (`run_console` returns `AppConfig`; `take_post_console_config` skips disk reload).

Open tracking for the goal prompt: [GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md](../GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md) Wave 0.

## 008c: Reuse early restore-candidate resolution — residual

**Status**: core shipped; residual open

### Shipped

- Typed `EarlyCurrentRestoreScan` (`NotRun` / `Scanned { agent, current }`) in `restore_resolve.rs`
- Early current-role scan in `launch_pipeline.rs` before role repo / auth / image
- `resolve_restore_candidate_reusing_early` reuses selected empty scan (skip second inspect)
- Related-role restore still runs when current is none
- Predicate unit test: `early_scan_skips_current_inspect_only_for_matching_empty_scan`

### Still required (Wave 0)

1. Stash **unselected-empty** early scans so a later matching agent can skip re-inspect when safe
2. Reuse typed non-empty early hits via `Scanned.current` where safe (not only `early_restore_container` short-circuit)
3. Integration test: common path does not double Docker `inspect` on current-role candidates
4. Keep rejection diagnostics + timing events

### Files

- `crates/jackin-runtime/src/runtime/launch/restore_resolve.rs`
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`
- `crates/jackin-runtime/src/runtime/launch/tests.rs`

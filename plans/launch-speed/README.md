# Launch-speed tracking (code-health/launch goal)

PR #718 shipped launch-speed 001–007. **008g** fully shipped (`run_console` returns `AppConfig`; `take_post_console_config` skips disk reload).

**Program status:** all in-scope launch-speed residuals closed on PR #759 (no open residual wording below).

## 008c: Reuse early restore-candidate resolution — DONE

**Status**: DONE

### Shipped

- Typed `EarlyCurrentRestoreScan` (`NotRun` / `Scanned { agent, current }` / `ScannedUnselectedEmpty`) in `restore_resolve.rs`
- Early current-role scan in `launch_pipeline.rs` before role repo / auth / image
- `resolve_restore_candidate_reusing_early` reuses:
  - selected empty scan (skip second inspect)
  - typed non-empty `Scanned.current` (no re-inspect)
  - `ScannedUnselectedEmpty` when unselected early scan finds no `is_restore_candidate` manifests for the role
- Related-role restore still runs when current is none
- Rejection diagnostics + timing events preserved on live inspect paths
- Tests:
  - `early_scan_skips_current_inspect_only_for_matching_empty_scan`
  - `early_empty_scan_avoids_second_current_role_inspect`
  - `early_nonempty_scan_reuses_typed_current_without_reinspect`
  - `unselected_empty_early_scan_skips_later_agent_current_inspect`
  - `common_path_single_current_inspect_with_early_then_reuse` (FakeDocker inspect count)

### Files

- `crates/jackin-runtime/src/runtime/launch/restore_resolve.rs`
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`
- `crates/jackin-runtime/src/runtime/launch/tests.rs`

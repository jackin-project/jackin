# Launch-speed deferred items (closed on PR #759)

The launch-speed implementation batch shipped in PR #718. Completed execution plans 001-007 and the completed parts of 008 were removed from `plans/`. The two deferred Plan 008 follow-ups below closed on `chore/rust-code-health-roadmap` (PR #759).

## 008c: Reuse early restore-candidate resolution

**Status**: DONE

**Shipped:**

- Typed `EarlyCurrentRestoreScan` (`NotRun` / `Scanned { agent_scope, resolution }`) in `restore_resolve.rs`.
- Early current-role scan recorded before role repo / auth / image work.
- `resolve_restore_candidate_with_early` reuses the early scan when final agent matches (or unselected scope subsumes), skipping a second current-role Docker inspect on the common path.
- Related-role restore still runs when early current-role is none; rejection diagnostics and timing events preserved.
- Regression test: `early_scan_skips_current_inspect_only_for_matching_empty_scan`.

## 008g: Skip console config reload when console made no config changes

**Status**: DONE

**Shipped:**

- `run_console` returns owned `AppConfig` after the console session.
- `take_post_console_config` uses that in-memory config and skips a redundant disk `AppConfig::load_or_init` on the common path.
- Tests: `no_op_console_skips_disk_reload_for_post_console_config`, `saved_console_config_feeds_post_console_launch_path`.

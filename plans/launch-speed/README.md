# Launch-speed remaining follow-ups

The launch-speed implementation batch shipped in PR #718. Completed execution plans 001-007 and the completed parts of 008 were removed from `plans/`; this file keeps only the two deferred Plan 008 items that still need design/code work.

## 008c: Reuse early restore-candidate resolution

**Status**: DONE

Launch currently performs an early current-role restore scan before role repo, auth, and image work so it can start or recreate an existing current-role instance without paying the full launch pipeline. Later, after role repo validation and final agent selection, `resolve_restore_candidate` can scan the current-role candidates again before checking related restore candidates.

The duplicate scan is safe but wasteful: it can repeat manifest filtering and Docker `inspect` calls on the common path.

What remains:

- Carry a typed early restore result instead of only `early_restore_container` / `early_restore_agent`.
- Record whether the early result was computed for a selected agent or for the unselected all-agent path.
- Reuse the early result only when the final selected agent matches, or when the unselected result is proven to subsume that agent-specific lookup.
- Keep related-role restore prompting intact; early current-role results must not suppress the related-candidate branch when it is still needed.
- Preserve launch-plan rejection diagnostics and timing events.
- Add regression tests proving the common path avoids a second current-role Docker inspect while multi-agent and related-candidate paths keep existing behavior.

## 008g: Skip console config reload when console made no config changes

**Status**: DONE — `run_console` returns `AppConfig`; `take_post_console_config` skips disk reload; tests `no_op_console_skips_disk_reload_for_post_console_config` + `saved_console_config_feeds_post_console_launch_path`

`jackin console` currently reloads `AppConfig` from disk after the console returns a launch/prewarm/action outcome. That reload is correct because the operator may have saved Settings or Workspace edits while inside the console, and the following launch must use the saved config.

The reload is wasted when the operator only selected a launch target and made no config changes.

What remains:

- Extend the console return path to report whether config was saved during the console run, or return the updated config model directly.
- Mark the console state dirty only after a successful config save.
- Keep the current conservative reload behavior for any path that cannot prove config was unchanged.
- Update `handle_console` to skip `AppConfig::load_or_init` only for no-change console exits.
- Add tests for both cases: no-op console launch skips reload; settings/workspace save still makes the following launch use the saved config.

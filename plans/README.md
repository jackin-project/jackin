# Implementation Plans

Execution plans for in-flight workstreams. Each subfolder holds its own plans + README; completed
plans are removed when they ship.

## Active folders

- **complexity-debt/** — `too_many_lines` / `cognitive_complexity` suppression burndown (opened as
  plan 025 in the 2026-07-03 deep audit; PR #713 shipped the first slice, the rest stays open).
- **launch-speed/** — deferred launch-pipeline performance items from PR #718.
- **tui-review/** — review follow-ups from PR #721.
- **agent-status/** — agent runtime status authority program.

## Historical note

The 2026-07-03 deep audit (`improve` skill, against commit `46511939d`) produced 54 advisor plans
(001-054). PR #713 (`feat(workspace): execute first wave of advisor improvement plans`) shipped them:
the fully-done plans were removed and the one with remaining work (complexity-suppression burndown,
025) moved into `complexity-debt/`. The original status table, finding→plan traceability, and audit
coverage notes are preserved in git history at the pre-cleanup commit.

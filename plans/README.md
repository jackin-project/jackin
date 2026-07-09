# Implementation Plans

Execution plans for in-flight workstreams. Each subfolder holds its own plans + README; completed plans are removed when they ship.

## Active folders

- **code-health/** — routine code-health first wave from the 2026-07-09 deep `improve` audit (against `a4761957d`): perf/correctness/tests plans plus routine deferred findings. Source of truth for open routine code-health work under the [codebase-health roadmap item](../docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx).
- **launch-speed/** — deferred launch-pipeline performance items from PR #718.
- **tui-review/** — review follow-ups from PR #721.
- **agent-status/** — agent runtime status authority program.

## Historical note

The 2026-07-03 deep audit (`improve` skill, against commit `46511939d`) produced 54 advisor plans (001-054). PR #713 (`feat(workspace): execute first wave of advisor improvement plans`) shipped them; the fully-done plans were removed, and the remaining complexity-suppression notes were later retired because their counts drifted. The current routine code-health source of truth is [Codebase health: Rust strictness, structure, and AI reviewability](/roadmap/codebase-health-enforcement/); it requires a fresh suppression inventory before execution rather than preserving stale plan counts.

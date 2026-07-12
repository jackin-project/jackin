# Implementation Plans

Execution plans for in-flight workstreams. Each subfolder holds its own plans + README; completed plans are removed when they ship.

## Active folders

Close-out on PR #759 (`chore/rust-code-health-roadmap`) finished every numbered plan status as **DONE** and drained the residual ledger to **CLOSED** / **CLOSED-as-pinned** (zero bare DEFER). Folders remain as evidence + residual next-triggers, not open TODO queues. Program prompt: [GOAL-CLOSE-ALL-REMAINING.md](GOAL-CLOSE-ALL-REMAINING.md).

- **code-health/** — plans 003–069 DONE; residual ledger authoritative for multi-PR pins. Roadmap: [codebase-health-enforcement](../docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx).
- **launch-speed/** — 008c + 008g **DONE** (early restore reuse; post-console config skip reload).
- **tui-review/** — 001 **DONE** (failure-dialog scroll hit geometry).
- **agent-status/** — 001–011 **DONE** (local signed packs + Notification enrich production-ready; live remote publish / full blocked goldens / live Codex reader pinned as product follow-ups).

## Historical note

The 2026-07-03 deep audit (`improve` skill, against commit `46511939d`) produced 54 advisor plans (001-054). PR #713 (`feat(workspace): execute first wave of advisor improvement plans`) shipped them; the fully-done plans were removed, and the remaining complexity-suppression notes were later retired because their counts drifted. The current routine code-health source of truth is [Codebase health: Rust strictness, structure, and AI reviewability](/roadmap/codebase-health-enforcement/); it requires a fresh suppression inventory before execution rather than preserving stale plan counts.

# Codebase health — ledger burn-down record

> **Status: closed.** This executor plan no longer owns any open work. Keep new or forgotten work on the roadmap item itself, not in this closed per-slice plan.

This file used to hold the mechanical burn-down plan for the codebase-health exception ledgers on PR #664. The live tree reached the planned ledger finish line:

- `test-layout-allowlist.toml` is empty (`files = [ ]`).
- `file-size-budget.toml` has no `[[production]]` or `[[test]]` grandfather entries.
- `clippy.toml` has `too-many-lines-threshold = 150`, `cognitive-complexity-threshold = 60`, `excessive-nesting-threshold = 5`, and `too-many-arguments-threshold = 7`.
- The file-size cap is `production_cap = 2000`; the original 1500L target was relaxed for hot-path files where further splitting would need a separate performance-risk decision.

All completed per-file split maps, stale allowlist rows, stale budget rows, and checked-off slice instructions were removed from this plan because they are no longer actionable. The remaining unfinished codebase-health work is tracked only in the roadmap item:

- <RepoFile path="docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx">docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx</RepoFile>

Do not reopen this plan for new slices. If another concrete follow-up appears, add it to the roadmap item first and create a new narrow executor plan only when the follow-up is ready to be executed.

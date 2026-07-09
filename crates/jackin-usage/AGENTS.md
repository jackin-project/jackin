# AGENTS.md — jackin-usage

Usage/pricing/telemetry + token monitors for the `jackin-capsule` daemon.

## Rules (this crate)

- Shared logging tier is rooted here (`clog!`/`cdebug!`, re-exported via `jackin-diagnostics`) so both binaries share one tier — do not introduce a parallel logging path.
- Borrow, don't clone, usage views: account materialization serializes from borrowed views/iterators, not full clones.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

# AGENTS.md — jackin-usage

Usage/pricing/telemetry + token monitors for the `jackin-capsule` daemon.

## Hard rules (this crate)

- **Tier & dependencies:** infrastructure (capsule-side accounting). Allowed inward deps: `jackin-core`, `jackin-protocol`, `jackin-diagnostics`. No dependency on `jackin-capsule` (circular), `jackin-tui`, `jackin-console`, or any presentation crate.
- **Keep `README.md` current:** update it when structure, public API, the token monitor, or the store change (see `crates/AGENTS.md`).
- **Shared logging tier lives here.** `clog!`/`cdebug!` logging infrastructure is rooted here (re-exported via `jackin-diagnostics`) so both binaries share one tier; do not introduce a parallel logging path.
- **Borrow, don't clone, usage views.** Account materialization serializes from borrowed views/iterators, not full clones (perf item in code-health roadmap).

## What lives here vs elsewhere

- This crate owns: token monitoring, usage/pricing accounting, telemetry + store, shared logging tier.
- Host-side run diagnostics / `debug_log!` live in `jackin-diagnostics`. The daemon that consumes this lives in `jackin-capsule`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

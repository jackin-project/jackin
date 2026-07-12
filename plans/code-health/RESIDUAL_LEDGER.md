# Residual ledger — substantial unfinished work only

Authoritative list of **large multi-PR** code-health follow-ups still open after PR #759 close-out.

Removed from this ledger (not tracked as plan work anymore):

- Fully shipped CLOSED rows (evidence in git history)
- Optional micro residuals (zero-copy scrollback row, db/docker metrics demotion, launch-speed 008c inspect-count polish)
- Intentional product/protocol pins with no code work requested (usage accounts-only surface, apple-container not shipping, Hello fail-closed)

| ID | Why still open (substantial) | Next trigger |
|----|------------------------------|--------------|
| **R-launch-typestate** / **R-typestate-general** | Multi-PR LaunchCore typestate extract (~1.3k LOC phase machine) | Dedicated design PR |
| **R-033-suite-a** | Full `run_launch_core` failure-path characterization needs LaunchCore fixture | After LaunchCore extract |
| **R-014-launch-pipeline-bench** | Full FakeDocker LaunchCore pipeline bench (micro `launch_attach` only today) | Same LaunchCore extract PR |
| **R-daemon-decomp** | Capsule daemon module/port rewrite (specs only shipped) | Per-worklist PR |
| **R-daemon-char-remainder** | Remaining daemon behavioral characterization surfaces | After daemon ports |
| **R-sim-turmoil** | Turmoil/proptest sim lane blocked on daemon ports | After daemon decomp slice |
| **R-038-WorkspaceLabel** | Dual-semantics `materialize_workspace` path labels vs config stems + TUI/CLI display strings need `WorkspaceLabel` design | WorkspaceLabel design PR |
| **R-edit-model-convergence** | Full console settings/editor edit-model merge (view-models only shipped) | Console redesign PR |
| **R-allow-attributes-deny** | Mass bare-`#[allow]` burn-down then flip `allow_attributes*` to deny | bare-allow floor → 0 |
| **R-missing-docs-cascade** | `#![deny(missing_docs)]` cascade beyond protocol (one pure crate per PR) | Next pure-crate PR |
| **R-047-maintainability-promote** | Promote remaining 7 maintainability lints off residual-`allow` (census done; burn-down not) | Dedicated lint promote PR |
| **R-perf-platform** | Wire `[[perf]]` / dhat budgets into ratchet; optional iai-callgrind when CI image supports valgrind | After stable benches + CI image |

Disposition: every row is **OPEN-substantial** (future PR), not silent DEFER and not intentional “won’t do.”

Counts: **12** substantial residuals (some rows group related IDs).

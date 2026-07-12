# Residual ledger — code-health unfinished work

Authoritative unfinished multi-PR list for goal
[GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md](../GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md).

**Launch-speed** is tracked separately: [../launch-speed/README.md](../launch-speed/README.md) (**008c DONE**).

**Agent-status** is out of scope for that goal (deferred).

Not tracked here (intentional / optional / shipped):

- Fully shipped CLOSED evidence (git history)
- Optional micro: zero-copy scrollback row, db/docker metrics demotion
- Intentional pins: usage accounts-only surface, apple-container not shipping, Hello fail-closed

| ID | Wave | Why still open | Next trigger |
|----|------|----------------|--------------|
| **R-047-maintainability-promote** | 1 | Seven maintainability lints still residual-`allow` | Re-measure + promote or measured-allow comments |
| **R-allow-attributes-deny** | 1 | Bare-`#[allow]` floor ≠ 0; cannot deny `allow_attributes*` yet | Burn-down → floor 0 → deny |
| **R-missing-docs-cascade** | 1 | Only protocol has `#![deny(missing_docs)]` | Cascade pure crates one-PR-each |
| **R-038-WorkspaceLabel** | 2 | `materialize_workspace` still `&str`; path-label vs config-stem dual-semantics | WorkspaceLabel design + type boundaries |
| **R-launch-typestate** / **R-typestate-general** | 3 | Monolithic `run_launch_core` (~1350 LOC); no phase typestate | LaunchCore extract PR |
| **R-033-suite-a** | 3 | No full `run_launch_core` failure-path fixture (B+C only) | After LaunchCore seams |
| **R-014-launch-pipeline-bench** | 3 | Only `launch_attach` micro-bench; no FakeDocker pipeline bench | Same LaunchCore extract |
| **R-daemon-decomp** | 4 | Specs only; daemon control still monolithic | Per-worklist module/port extract |
| **R-daemon-char-remainder** | 4 | Partial characterization surfaces | After daemon ports |
| **R-sim-turmoil** | 4 | No turmoil/proptest sim harness | After daemon decomp |
| **R-edit-model-convergence** | 5 | View-models only; settings/editor not fully merged | Console redesign PR |
| **R-perf-platform** | 6 | No `[[perf]]` / dhat ratchet family; no iai-callgrind | After stable benches + CI image |

Disposition: **OPEN** until CLOSED in-tree (prefer implement). Operator pin only for hard external blockers.

Counts: **12** code-health residuals (launch-speed 008c closed outside this file).


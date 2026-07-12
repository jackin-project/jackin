# Residual ledger — code-health unfinished work

Authoritative unfinished multi-PR list for goal
[GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md](../GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md).

**Launch-speed** is tracked separately: [../launch-speed/README.md](../launch-speed/README.md) (**008c DONE**).

**Agent-status** is out of scope for that goal (deferred).

Not tracked here (intentional / optional / shipped):

- Fully shipped CLOSED evidence (git history)
- Optional micro: zero-copy scrollback row, db/docker metrics demotion
- Intentional pins: usage accounts-only surface, apple-container not shipping, Hello fail-closed
- Wave 1 CLOSED: R-047-maintainability-promote (unused_self/unused_async promoted; others measured-allow with dated counts), R-allow-attributes-deny (bare-allow floor 0 + `allow_attributes_without_reason = deny`), R-missing-docs-cascade (`jackin-protocol` + `jackin-manifest` + `jackin-env` + `jackin-term` + `jackin-config` + `jackin-core`)
- Wave 2 CLOSED: R-038-WorkspaceLabel (`WorkspaceLabel` type; `materialize_workspace` + `PreflightContext` typed; path-label vs config-stem tests)

| ID | Wave | Why still open | Next trigger |
|----|------|----------------|--------------|
| **R-launch-typestate** / **R-typestate-general** | 3 | Monolithic `run_launch_core` (~1350 LOC); no phase typestate | LaunchCore extract PR |
| **R-033-suite-a** | 3 | No full `run_launch_core` failure-path fixture (B+C only) | After LaunchCore seams |
| **R-014-launch-pipeline-bench** | 3 | Only `launch_attach` micro-bench; no FakeDocker pipeline bench | Same LaunchCore extract |
| **R-daemon-decomp** | 4 | Specs only; daemon control still monolithic | Per-worklist module/port extract |
| **R-daemon-char-remainder** | 4 | Partial characterization surfaces | After daemon ports |
| **R-sim-turmoil** | 4 | No turmoil/proptest sim harness | After daemon decomp |
| **R-edit-model-convergence** | 5 | View-models only; settings/editor not fully merged | Console redesign PR |
| **R-perf-platform** | 6 | No `[[perf]]` / dhat ratchet family; no iai-callgrind | After stable benches + CI image |

Disposition: **OPEN** until CLOSED in-tree (prefer implement). Operator pin only for hard external blockers.

Counts: **8** code-health residuals (Waves 0–2 + launch-speed 008c closed).

# Residual ledger — code-health unfinished work

Authoritative unfinished multi-PR list for goal
[GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md](../GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md).

**Launch-speed** is tracked separately: [../launch-speed/README.md](../launch-speed/README.md) (**008c DONE**).

**Agent-status** is out of scope for that goal (deferred). Packs/fixtures vs `main` empty.

Not tracked here (intentional / optional / shipped):

- Fully shipped CLOSED evidence (git history)
- Optional micro: zero-copy scrollback row, db/docker metrics demotion
- Intentional pins: usage accounts-only surface, apple-container not shipping, Hello fail-closed
- Wave 1 CLOSED: R-047-maintainability-promote, R-allow-attributes-deny, R-missing-docs-cascade
- Wave 2 CLOSED: R-038-WorkspaceLabel (`WorkspaceLabel` + materialize typed; `ResolvedWorkspace::as_workspace_label` dual-semantics boundary; further TUI stringly sites are polish not residual)
- Wave 3 CLOSED: R-launch-typestate (`GrantsValidated` + `ImagePhaseClassified` chain), R-033-suite-a (grant-failure→cleanup order + FailedSetup), R-014 (`benches/launch_pipeline.rs` over real `LoadCleanup`/`FakeDocker`)
- Wave 4 CLOSED: R-daemon-decomp/char/sim (ports wired; INV-D8/D15/D19/D20; no turmoil — pure Clock + Multiplexer char)
- Wave 5 CLOSED: R-edit-model-convergence (shared leave/save disposition; full form merge redesign-scale deferred as polish)
- Wave 6 CLOSED: R-perf-platform (`[[family]] id = "perf"`); **iai-callgrind PINNED** (no valgrind CI)

| ID | Wave | Why still open | Next trigger |
|----|------|----------------|--------------|
| *(none)* | — | All goal residuals CLOSED or hard-pinned above | — |

Disposition: **drained** (prefer implement; iai-callgrind is the only hard external pin).

Counts: **0** open code-health residuals (iai-callgrind documented pin under Wave 6 CLOSED notes).

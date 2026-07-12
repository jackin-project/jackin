# Residual ledger — code-health unfinished work

Authoritative unfinished multi-PR list for goal
[GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md](../GOAL-CODE-HEALTH-AND-LAUNCH-SPEED.md).

**Launch-speed** is tracked separately: [../launch-speed/README.md](../launch-speed/README.md) (**008c DONE**).

**Agent-status** is out of scope for that goal (deferred).

Not tracked here (intentional / optional / shipped):

- Fully shipped CLOSED evidence (git history)
- Optional micro: zero-copy scrollback row, db/docker metrics demotion
- Intentional pins: usage accounts-only surface, apple-container not shipping, Hello fail-closed
- Wave 1 CLOSED: R-047-maintainability-promote, R-allow-attributes-deny, R-missing-docs-cascade
- Wave 2 CLOSED: R-038-WorkspaceLabel
- Wave 3 CLOSED: R-launch-typestate / R-typestate-general, R-033-suite-a, R-014-launch-pipeline-bench
- Wave 4 CLOSED: R-daemon-decomp (ports wired into Hello takeover / control ACK / codename retire / last-session exit), R-daemon-char-remainder (INV-D8 Multiplexer remove_exited_session, INV-D19 handle_last_session_exit, INV-D20 export categories), R-sim-turmoil (pure port decisions + real Multiplexer paths; turmoil not adopted — no crate fit)
- Wave 5 CLOSED: R-edit-model-convergence (`plan_leave_when_dirty` / `plan_explicit_save` shared by editor escape/save and all settings Esc/q ConfirmDiscard paths)
- Wave 6 CLOSED: R-perf-platform (`[[family]] id = "perf"` dhat budgets via `perf_budgets.rs`; **iai-callgrind PINNED** — CI has no valgrind)

| ID | Wave | Why still open | Next trigger |
|----|------|----------------|--------------|
| *(none)* | — | All goal residuals CLOSED or hard-pinned above | — |

Disposition: **drained** (prefer implement; iai-callgrind is the only hard external pin).

Counts: **0** open code-health residuals (iai-callgrind documented pin under Wave 6 CLOSED notes).

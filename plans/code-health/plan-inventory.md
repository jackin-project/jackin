# Plan inventory — residual only

Deep source verification (2026-07-12, five parallel audits on PR #759). Fully-done plans removed from `plans/code-health/`.

| Plan | Class | Residual |
|------|-------|----------|
| 014 | residual | R-014-launch-pipeline-bench |
| 023 | residual | R-023-usage-scope, R-023-apple-container |
| 026 | residual | R-026-borrowed-row |
| 030 | residual | R-edit-model-convergence |
| 033 | residual | R-033-suite-a |
| 038 | residual | R-038-env-console-tail |
| 042 | residual | R-042-db-docker-metrics |
| 045 | residual | R-045-hello-skew |
| 047 | residual | 7× maintainability residual-allow |
| 058 | residual | R-038 (doctor slice only) |
| 064 | residual | R-038 (materialize dual-semantics) |

Removed as **FULLY_IMPLEMENTED** (not listed): 003–013, 015–022, 024–025, 027–029, 031–032, 034–037, 039–041, 043–044, 046, 048–057, 059–063, 065–069.

Ledger pins with **no** residual plan file (architecture / ops only): R-launch-typestate, R-daemon-*, R-typestate-general, R-sim-turmoil, R-iai-callgrind, R-perf-budgets, R-dhat-budgets-ratchet, R-build-time-budget, R-self-tightening, R-health-history-jsonl, R-agent-hygiene, R-allow-attributes-deny, R-missing-docs-cascade.

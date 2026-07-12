# Implementation Plans

Only **residual** plan work remains under `plans/`. Fully implemented plans were removed after deep source verification on PR #759 (`chore/rust-code-health-roadmap`, 2026-07-12).

## How to read this tree

| Folder | What remains | SoT for multi-PR pins |
|--------|--------------|----------------------|
| **code-health/** | Plan files with honest residual only + [RESIDUAL_LEDGER.md](code-health/RESIDUAL_LEDGER.md) | Residual ledger |
| **agent-status/** | Live goldens / pack rewrite / authority / remote packs | Plan files themselves |
| **launch-speed/** | 008c early-restore residual (micro-optimization + test depth) | README |

Removed folders / fully-done plan sets (evidence lives in git history + code):

- **tui-review/** — 001 scroll hit geometry fully shipped
- **agent-status** 001–004, 008, 011 — structural layers fully shipped
- **code-health** ~55 plans — primary Done criteria met with no plan-scoped residual

## Residual open work (summary)

1. **Agent-status product bar** — full per-agent live goldens + pack rewrite (005/007); grok blocked live (006); live Notification validation (009b); live Codex app-server reader (009a); remote pack publish (010).
2. **Code-health multi-PR pins** — see residual ledger (LaunchCore typestate, daemon decomp, WorkspaceLabel, lint promote, perf budgets, …).
3. **Launch-speed 008c** — unselected-empty scan reuse + inspect-count integration test depth.

Program close-out notes: [GOAL-CLOSE-ALL-REMAINING.md](GOAL-CLOSE-ALL-REMAINING.md) (historical; program complete except residuals above).

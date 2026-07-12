# Code-health verification evidence

Branch: `chore/rust-code-health-roadmap` (PR #759).

## Deep residual audit (2026-07-12)

Five parallel source audits classified every numbered plan and residual row:

| Bucket | Result |
|--------|--------|
| FULLY_IMPLEMENTED_REMOVABLE | ~55 code-health plans + tui-review 001 + agent-status 001–004/008/011 + launch-speed 008g — **plan files removed** |
| IMPLEMENTED_WITH_RESIDUAL | Kept as residual plan files (see README) |
| PARTIAL / MISSING | **0** hollow DONE claims without residual honesty |

### Residual ledger after prune

- **23** CLOSED-as-pinned rows retained
- **0** bare DEFER
- **8** fully CLOSED evidence rows **pruned** (host turso, materialize bench, export-volume, map-check, complexity 58, snapshot helpers, thiserror mid-tranches, repo-links gen)

### Package probes re-run during close-out (still valid)

| Area | Evidence |
|------|----------|
| Ratchet SoT | `ratchet.toml`; no production legacy budget TOMLs |
| Metrics 042 | `metrics::tests` with `--features otlp` |
| Launch 008c/008g | `EarlyCurrentRestoreScan`; `take_post_console_config` tests |
| Failure scroll | `scrolled_failure_copy_hit_and_overlay_follow_failure_scroll` |
| Agent-status | Notification enrich; production `verify_signed_bundle`; grok pack bake |
| `lint --strict` | green at close-out tip |

## What remains unfinished

See [RESIDUAL_LEDGER.md](RESIDUAL_LEDGER.md) + residual plan files in this folder + `plans/agent-status/` + `plans/launch-speed/README.md` (008c only).

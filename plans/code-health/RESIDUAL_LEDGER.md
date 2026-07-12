# Residual ledger (pinned unfinished only)

Authoritative list of **not-yet-done** multi-PR / product / ops residuals after deep source verification on PR #759.

Fully **CLOSED** in-tree residuals (host turso, materialize bench, export-volume ratchet, map-check, complexity 58, snapshot helpers, thiserror mid-tranches, repo-links gen) were **removed** from this ledger — evidence is in git history and code. Do not re-add CLOSED rows without a regression.

Disposition values here:

| Value | Meaning |
|-------|---------|
| **CLOSED-as-pinned** | Intentional multi-PR / product / safety pin — unfinished for a future PR, not silent debt |

Bare **DEFER** is forbidden.

| ID | Origin | Disposition | Why still open | Next trigger |
|----|--------|-------------|----------------|--------------|
| R-014-launch-pipeline-bench | plan 014 | **CLOSED-as-pinned** | Full FakeDocker LaunchCore harness multi-crate; only `launch_attach` micro-bench | LaunchCore extract PR |
| R-023-usage-scope | plan 023 | **CLOSED-as-pinned** | Accounts-only usage CLI is intentional product surface | Product reintroduces workspace usage commands |
| R-023-apple-container | plan 023 | **CLOSED-as-pinned** | Backend not shipping this program | When apple-container backend lands |
| R-033-suite-a | plan 033 | **CLOSED-as-pinned** | Suites B+C shipped; suite A needs `run_launch_core` fixture | LaunchCore decomposition PR |
| R-038-env-console-tail | plan 038 / 058–064 | **CLOSED-as-pinned** | Typed frontier advanced; dual-semantics `materialize_workspace` path labels + TUI/CLI display strings need WorkspaceLabel | WorkspaceLabel design PR |
| R-026-borrowed-row | plan 026 | **CLOSED-as-pinned** | Range API shipped; zero-copy owned-row accessor optional | Perf incident on mouse-event scrollback |
| R-042-db-docker-metrics | plan 042 | **CLOSED-as-pinned** | 9 instruments + volume tests shipped; db/docker firehose not demoted | Metrics volume review |
| R-045-hello-skew | plan 045 | **CLOSED-as-pinned** | Hello short-payload fail-closed by design | Only if protocol softens Hello |
| R-allow-attributes-deny | Phase 1 | **CLOSED-as-pinned** | Ratchet caps bare-allow; deny flip blocked until floor 0 | bare-allow floor 0 |
| R-missing-docs-cascade | Phase 1 | **CLOSED-as-pinned** | Protocol pattern shipped; other crates one-PR-each | Next pure-crate PR |
| R-launch-typestate | Phase 2 | **CLOSED-as-pinned** | Multi-PR LaunchCore typestate (~1.3k LOC) | Dedicated design PR |
| R-daemon-decomp | Phase 2 | **CLOSED-as-pinned** | Specs + partial char only; full rewrite multi-PR | Per-worklist PR |
| R-daemon-char-remainder | Phase 2 | **CLOSED-as-pinned** | Partial surfaces; needs daemon ports | After daemon ports |
| R-typestate-general | Phase 2 | **CLOSED-as-pinned** | Same blocker as R-launch-typestate | Same as R-launch-typestate |
| R-edit-model-convergence | Phase 2 | **CLOSED-as-pinned** | View-models shipped; full merge is console redesign | Console redesign PR |
| R-sim-turmoil | Phase 3 | **CLOSED-as-pinned** | Needs daemon ports first | After daemon decomp slice |
| R-iai-callgrind | Phase 4 | **CLOSED-as-pinned** | Needs iai/valgrind in CI image | CI image iai support |
| R-perf-budgets | Phase 4 | **CLOSED-as-pinned** | Ratchet engine present; no `[[perf]]` family | Wire after stable benches |
| R-dhat-budgets-ratchet | Phase 4 | **CLOSED-as-pinned** | dhat literals stay in-source | Same PR as R-perf-budgets |
| R-build-time-budget | Phase 6 | **CLOSED-as-pinned** | Measurement lane shipped; numeric budget needs baselines | After baselines mature |
| R-self-tightening | Phase 7 | **CLOSED-as-pinned** | Engine shipped; bot needs GH app policy | Operator bot design |
| R-health-history-jsonl | Phase 7 | **CLOSED-as-pinned** | Health schema exists; history sink is ops | Ops sink path decision |
| R-agent-hygiene | Phase 7 | **CLOSED-as-pinned** | Gates exist; agent loop is productization | Product agent-hygiene loop |

## Related residual plan files (plan-scoped narrative)

| Plan file | Residual |
|-----------|----------|
| [014](014-hot-path-bench-coverage.md) | R-014-launch-pipeline-bench |
| [023](023-docs-command-drift-gate.md) | R-023-* product pins |
| [026](026-scrollback-range-snapshot.md) | R-026-borrowed-row |
| [030](030-console-view-model-structs.md) | R-edit-model-convergence |
| [033](033-characterization-launch-displace-pty.md) | R-033-suite-a |
| [038](038-workspace-name-newtype.md) / [058](058-residual-complexity-env-snapshot.md) / [064](064-workspace-name-auth-error-token.md) | R-038-env-console-tail |
| [042](042-high-frequency-metrics.md) | R-042-db-docker-metrics |
| [045](045-protocol-env-corpus-closure.md) | R-045-hello-skew |
| [047](047-maintainability-lint-census.md) | All 7 maintainability lints still residual-allow (promote wave) |

## Counts

- **23** pinned residuals (unfinished / intentional product pins)
- **0** bare DEFER
- **0** CLOSED rows (pruned after verification)

## Verification note (2026-07-12 deep pass)

Five parallel source audits (agent-status, launch-speed+tui, code-health 003–030, 031–055, 056–069+ledger) found:

- No residual wrongly marked CLOSED
- No pinned residual that was secretly fully shipped
- Fully-done plan files removed from `plans/` (primary Done criteria met with nothing left to improve in-plan)

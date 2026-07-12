# Code-health residual plans

Fully implemented plans **003–013, 015–022, 024–025, 027–029, 031–032, 034–037, 039–041, 043–044, 046, 048–057, 059–063, 065–069** were removed after deep source verification on PR #759. Primary Done criteria are met in tree with nothing left to improve *from those plan files*.

**Authoritative unfinished multi-PR list:** [RESIDUAL_LEDGER.md](RESIDUAL_LEDGER.md) (23 pinned rows, 0 bare DEFER).

## Residual plan files kept

| Plan | Residual reason |
|------|-----------------|
| [014](014-hot-path-bench-coverage.md) | Full LaunchCore pipeline bench pinned (micro-benches shipped) |
| [023](023-docs-command-drift-gate.md) | Drift gate shipped; usage-scope + apple-container product pins |
| [026](026-scrollback-range-snapshot.md) | Range API shipped; zero-copy borrowed row optional |
| [030](030-console-view-model-structs.md) | View-models shipped; full edit-model convergence = redesign |
| [033](033-characterization-launch-displace-pty.md) | Suites B+C shipped; suite A LaunchCore fixture pinned |
| [038](038-workspace-name-newtype.md) | Spine shipped; WorkspaceLabel dual-semantics tail |
| [042](042-high-frequency-metrics.md) | 9 instruments + volume tests; db/docker demotion optional |
| [045](045-protocol-env-corpus-closure.md) | Corpus shipped; Hello short-payload fail-closed by design |
| [047](047-maintainability-lint-census.md) | Census done; all 7 still residual-allow (no promote wins) |
| [058](058-residual-complexity-env-snapshot.md) | Complexity+snapshot shipped; doctor WN only advances R-038 |
| [064](064-workspace-name-auth-error-token.md) | Auth/token WN shipped; materialize dual-semantics under R-038 |

## Meta

| File | Role |
|------|------|
| [RESIDUAL_LEDGER.md](RESIDUAL_LEDGER.md) | Pinned unfinished only |
| [VERIFICATION.md](VERIFICATION.md) | Re-verify evidence |
| [plan-inventory.md](plan-inventory.md) | Residual inventory |

Roadmap page: [codebase-health-enforcement](../../docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx).

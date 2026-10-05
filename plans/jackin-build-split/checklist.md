# Jackin Build Split

- Status: IN PROGRESS
- Record date: 2026-10-05
- Scope: Initial evidence record.

## Starting state

- Host: `bastion`, Debian 13.7, x86_64, kernel 6.12.94.
- Repository: `https://github.com/jackin-project/jackin.git`.
- Base: `main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`.
- Task worktree: `/root/Projects/tailrocks/jackin-project/jackin-refactor-build-split`.
- Task branch: `refactor/build-split`.
- The task worktree was clean at the base SHA.
- The original `main` worktree had one unrelated `mise.lock` modification.
- The original lock hash was `6be630be77daa3073ec74340cd6a113b969ee9117a69b8d48ebe5d5501df764c`.
- The task worktree lock hash was `ac3b0998f110538c5bb34f2653afaf7f2713d7f9f026dde1bb3f50845cef06ef`.
- The task worktree lock blob was `c78d5f9ff2b48c70d488fa9a4cfbf5f750ef11ce`.
- Documentation work preserves both lock states. No lock edit is in scope.

## Prerequisites and gates

| Item | Status | Evidence or reason |
|---|---|---|
| Coordinator model requirement | FAIL | Task section 2.1 supersedes older instructions. Root runs Sol/medium, not Luna/max. |
| Work agent model assignment | IN PROGRESS | Tools assign work agents Luna/max. Runtime confirmation is pending. |
| Branch and PR inventory | IN PROGRESS | Fetch passed. Full diff review and dispositions remain pending. See [branches](branches.md). |
| Crate and build analysis | IN PROGRESS | See [crate plan](crate-plan.md) and [build results](build-results.md). |
| Generator and CI coverage | IN PROGRESS | See [CI coverage](ci-coverage.md). |
| Debian account route | NOT RUN | No Jackin binary or default configuration exists on this host. |
| Security review | IN PROGRESS | See [reviews](reviews.md). |
| Build and test measurements | NOT RUN | This documentation checkpoint prohibits builds and tests. |
| Live role or account requests | NOT RUN | This documentation checkpoint prohibits live requests. |
| Implementation acceptance review | NOT RUN | No implementation change exists. |
| PR creation or merge | NOT RUN | This checkpoint is documentation-only. |

## Active owners

| Owner | Area | State |
|---|---|---|
| `branches` | Branch and PR inventory | Inventory complete; full diff review pending |
| `build_baseline` | Build measurements | IN PROGRESS; measurements not run |
| `crate_design` | Crate and dependency analysis | IN PROGRESS |
| `architect_contract` | Architect manifest contract | IN PROGRESS |
| `velnor_recon` / `jackin_generator_config` | Generator and CI coverage | IN PROGRESS |
| `codex_schema_runtime` | Codex schema and runtime settings | IN PROGRESS |
| `unprivileged_exec_design` | Execution boundary | IN PROGRESS |
| `preflight_security_review` | Preliminary security review | Initial review complete; follow-up pending |
| `debian_codex_route` | Debian account route | Static review complete; runtime route NOT RUN |

## Records

- [Branch and PR findings](branches.md)
- [Crate and build plan](crate-plan.md)
- [Build results](build-results.md)
- [CI coverage](ci-coverage.md)
- [Debian results](debian-results.md)
- [Security and Codex review](reviews.md)
- [Initial report](final-report.md)

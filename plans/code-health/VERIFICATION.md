# Code-health re-verification evidence

Branch: `chore/rust-code-health-roadmap` (PR #759).  
Captured by the long-running goal executor. Scratch copies also under the implementer scratch dir.

## Branch lock

- Local branch tracks `origin/chore/rust-code-health-roadmap`.
- All plan implementation commits land on this branch only (no `exec-plan-*` work sinks).


## Goal re-verify (2026-07-12 post-069)

- Tip: `621fa5530`+ on `chore/rust-code-health-roadmap` (thiserror mid-tranches CLOSED via 065–069; WorkspaceName residual 064).
- Plans **003–069** DONE; inventory 65 plans, 0 in_tree fails; matrix **SEQ** open = 0.
- R-thiserror-mid-tranches **CLOSED** (instance∥isolation∥docker∥image∥config).
- R-038 partial remains DEFER with measured dual-semantics + TUI/host CLI tail.
- Design/product DEFERs unchanged (daemon, launch typestate, perf budgets, etc.).
- `lint --strict` green; workspace `clippy -D warnings` green after needless_return sweep.
- `ci --fast` red only for documented executor-env waivers (manager_flow Docker-missing disk persist + RUSTSEC-2026-0204); captures under `/tmp/grok-goal-codehealth/reverify/`.

## Plan ledger

Plans **003–069** DONE on this branch. Wave-8 residual program + WorkspaceName frontier slices 055–063. Isolation list/drift typed (063).

Every `plans/code-health/*.md` plan **003–063** is **DONE** in `plans/code-health/README.md` with implementation present on this branch (not vanished exec SHAs).

- Plan **014** materialize bench CLOSED by [057](057-residual-wave-r1-bench-ratchet-map.md); launch-pipeline still R-014-launch-pipeline-bench DEFER.
- Plan **054** closed the plan-011 residual `assertions_on_result_states` adoption.
- Plan **055** closed named residual footnotes (028/049 in tree; 023/033/038 DEFER measured).
- Plan **056** converted every coverage-matrix **SEQ** to **DEFER** + residual-ledger rows (zero open SEQ).
- Plan **057** closed R-014-materialize-bench, R-export-volume-ratchet, R-map-metadata-gate.

## Residual ledger

Authoritative residual dispositions: [RESIDUAL_LEDGER.md](RESIDUAL_LEDGER.md).

Closed on this branch:

| Residual | Evidence |
|---|---|
| R-028-host-turso | Host usage store uses `jackin_usage::store_backend`; no host `turso` dep |
| R-049-repo-links-gen | `docs.yml` `repo-link-check` runs `gen-crate-pages.ts` before repo-links |

All other named residuals and former SEQ themes are **DEFER(measured)** in that ledger (not silent notes).

## In-tree spine probes (greppable)

| Deliverable | Evidence |
|---|---|
| Phase 0 dashboard | `crates/jackin-xtask/src/health.rs`, `code-health-baseline.toml` |
| Suppressions ratchet | `crates/jackin-xtask/src/suppressions.rs`, `suppression-budget.toml` |
| Unified ratchet | `crates/jackin-xtask/src/ratchet.rs`, `ratchet.toml`, `DEFECT_LEDGER.md` |
| Tier-graph arch | `crates/jackin-xtask/src/arch.rs` `TIERS` |
| Test-support crate | `crates/jackin-test-support/` |
| dylint scaffold | `crates/jackin-lints/` |
| Clock seam | `crates/jackin-core/src/clock.rs` |
| WorkspaceName | `crates/jackin-core` `WorkspaceName` |
| Telemetry 018–044 | diagnostics observability + conformance |
| Docs gates 015/023/050 | xtask docs brand/specs/command-drift/readme-freshness |
| Host turso chokepoint | `jackin_usage::store_backend` + host `store.rs` |
| Residual program | `055`/`056` + `RESIDUAL_LEDGER.md` |

## Roadmap Phase 0–8 disposition

Phases are covered by DONE plans 003–069. Former **SEQ** matrix rows are **DEFER** with measured reasons in the residual ledger (plan 056). No open SEQ debt remains in the coverage matrix.

| Residual theme | Disposition |
|---|---|
| Daemon full decomposition | DEFER R-daemon-decomp |
| Launch typestate/phase contracts | DEFER R-launch-typestate |
| thiserror long-tail crates | **CLOSED** R-thiserror-mid-tranches (065–069) |
| Complexity threshold lowering | DEFER R-complexity-threshold |
| allow_attributes* deny | DEFER R-allow-attributes-deny |
| missing_docs crate cascade | DEFER R-missing-docs-cascade |
| Self-tightening ratchet PR bot | DEFER R-self-tightening |
| Health-history JSONL data branch | DEFER R-health-history-jsonl |
| Golden agent tasks | DEFER (spend framing; roadmap self-scopes) |
| AFIT Send story | DEFER (design decision) |
| Sealed trait port taxonomy | DEFER (ports intentional; no lint signal yet) |
| loom/kani | DECIDED out of scope |
| public-API snapshot tooling | DECIDED Skip |
| non_exhaustive sweep | DECIDED keep exhaustive matching |

## Named plan residuals (criterion 3)

| Residual | Disposition |
|---|---|
| PERF-benches-missing (014) | DEFER R-014-* via 055 |
| 028 host turso | **CLOSED** via 055 / e1eacdf44 |
| 049 link-check generator | **CLOSED** via 055 / e1eacdf44 |
| 023 operator flags | DEFER R-023-* via 055 |
| 038 env/console WorkspaceName tail | DEFER R-038-env-console-tail via 055 |
| 033 suite A full LaunchCore fixture | DEFER R-033-suite-a via 055 |
| 054 assertions lint | complete (denied) |

## Commands used for re-verify

```sh
git branch --show-current   # chore/rust-code-health-roadmap
rg 'SEQ\(' plans/code-health/README.md   # expect 0
rg 'use turso::' crates/jackin           # expect 0
cargo check -p jackin -p jackin-usage --benches
cargo run -p jackin-xtask -- docs map-check
cargo run -p jackin-xtask -- lint --strict
cargo run -p jackin-xtask -- lint suppressions
cargo run -p jackin-xtask -- lint ratchet
cargo run -p jackin-xtask -- health --format json
cargo xtask ci --fast   # only documented executor-env waivers red
```

Full `cargo xtask ci --fast` may still hit the documented executor-env waiver class (no Docker; capsule-exported `JACKIN_*`; RUSTSEC via turso). See plans README reconcile log and scratch `waivers.md`.

## Parallel dispatch (post-program residual work)

Future residual execution uses [DISPATCH.md](DISPATCH.md): T0 package verify per
worker, T1 `lint --strict` + T2 `ci --fast` once per merge wave, residual waves
R1–R4 from the residual ledger. Do not re-serialize independent crate work.

## Latest re-verify (goal pass)

- Tip: `a776e275d` on `chore/rust-code-health-roadmap`
- Plans **003–069** DONE; inventory `plan-inventory.md` zero in_tree fails (incl. 051)
- Matrix `SEQ(` count: **0**
- Host turso: clean (`store_backend`)
- `docs map-check`: OK (27 crates)
- `lint --strict`: OK
- `cargo fmt --check`: OK
- `cargo clippy -D warnings`: OK
- `ci --fast` red only:
  - 4× manager_flow disk-persist (no Docker)
  - RUSTSEC-2026-0204 (audit + deny)
- Residual ledger: DEFER only for multi-PR/design/product items; R-038 partial with measured remaining TUI/restore/materialize string sites


## Close-out re-verify (goal pass 2026-07-12)

- Tip: `chore/rust-code-health-roadmap` PR #759 close-out wave
- Claim gaps: 017 ratchet single SoT (legacy budget TOMLs removed); 042 metrics counter assertions; 047 maintainability lints re-measured residual-allow (large_futures/assigning_clones/match_same_arms/drop_non_drop/unused_self/unused_async stay `allow`; needless_pass_by_value allow intentional)
- tui-review 001: failure_scroll threaded into hit-test + OSC8
- launch-speed 008c/008g: early restore scan reuse + run_console returns AppConfig
- agent-status: Notification payload enrich; grok baked; packs/fixtures updated
- Residual ledger: zero bare DEFER rows (all CLOSED or CLOSED-as-pinned)
- Gates: see `/tmp/grok-goal-4d943cf7c64d/implementer/` captures

## ci --fast close-out (HEAD dc39b47c0+)

- `fmt` + `clippy -D warnings` + `lint --strict`: green
- `nextest`: red only 4× `manager_flow` disk-persist tests (Docker daemon missing — documented executor-env waiver)
- `cargo audit` + `cargo deny advisories`: red only RUSTSEC-2026-0204 (crossbeam-epoch via turso — documented waiver)
- Capture: implementer scratch `ci-fast.log`

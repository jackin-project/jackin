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
- R-038 env/console tail **CLOSED-as-pinned** (dual-semantics path labels + remaining TUI/CLI string sites; next-trigger WorkspaceLabel design).
- Multi-PR/product residuals **CLOSED-as-pinned** in RESIDUAL_LEDGER (daemon, launch typestate, perf budgets, etc.) — zero bare DEFER.
- `lint --strict` green; workspace `clippy -D warnings` green after needless_return sweep.
- `ci --fast` red only for documented executor-env waivers (manager_flow Docker-missing disk persist + RUSTSEC-2026-0204); captures under `/tmp/grok-goal-codehealth/reverify/`.

## Full restart inventory (2026-07-12)

Re-ran `plans/GOAL-CLOSE-ALL-REMAINING.md` acceptance from a clean inventory:

| Check | Result |
|-------|--------|
| Open status table rows (`TODO` / `IN PROGRESS` / `BLOCKED` plan rows) | **none** (legend text only) |
| Bare `**DEFER**` in RESIDUAL_LEDGER | **0** (10 CLOSED + 26 CLOSED-as-pinned) |
| A1 ratchet SoT | legacy budget TOMLs gone; shims → `ratchet.toml` |
| A2 metrics volume | `metrics::tests` (feature `otlp`) counter deltas + capsule `cdebug!` contract green |
| A3 plan 047 | honest residual-allow comments (not false promote) |
| D1/D2 launch-speed | `EarlyCurrentRestoreScan` + `take_post_console_config` tests green |
| E tui-review | `scrolled_failure_copy_hit_and_overlay_follow_failure_scroll` green |
| C agent-status | Notification enrich + pack/signed-bundle tests green; grok baked |
| `lint --strict` | green |

Docs honesty refresh: `plans/launch-speed/README.md`, `plans/GOAL-CLOSE-ALL-REMAINING.md`, `plans/README.md` no longer claim open deferred work.

## Plan ledger

Plans **003–069** DONE on this branch. Wave-8 residual program + WorkspaceName frontier slices 055–063. Isolation list/drift typed (063).

Every `plans/code-health/*.md` plan **003–063** is **DONE** in `plans/code-health/README.md` with implementation present on this branch (not vanished exec SHAs).

- Plan **014** materialize bench CLOSED by [057](057-residual-wave-r1-bench-ratchet-map.md); launch-pipeline **CLOSED-as-pinned** R-014-launch-pipeline-bench.
- Plan **054** closed the plan-011 residual `assertions_on_result_states` adoption.
- Plan **055** closed named residual footnotes (028/049 CLOSED; 023/033/038 later CLOSED-as-pinned on close-out).
- Plan **056** converted every coverage-matrix **SEQ** to residual-ledger rows (zero open SEQ); close-out pinned remaining multi-PR rows.
- Plan **057** closed R-014-materialize-bench, R-export-volume-ratchet, R-map-metadata-gate.

## Residual ledger

Authoritative residual dispositions: [RESIDUAL_LEDGER.md](RESIDUAL_LEDGER.md).

Closed on this branch:

| Residual | Evidence |
|---|---|
| R-028-host-turso | Host usage store uses `jackin_usage::store_backend`; no host `turso` dep |
| R-049-repo-links-gen | `docs.yml` `repo-link-check` runs `gen-crate-pages.ts` before repo-links |

All other named residuals and former SEQ themes are **CLOSED** or **CLOSED-as-pinned** in that ledger (not silent notes; zero bare DEFER).

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

Phases are covered by DONE plans 003–069. Former **SEQ** matrix rows live in the residual ledger as **CLOSED** or **CLOSED-as-pinned** (plan 056 + close-out). No open SEQ debt and no bare ledger DEFER remain.

| Residual theme | Disposition |
|---|---|
| Daemon full decomposition | **CLOSED-as-pinned** R-daemon-decomp |
| Launch typestate/phase contracts | **CLOSED-as-pinned** R-launch-typestate |
| thiserror long-tail crates | **CLOSED** R-thiserror-mid-tranches (065–069) |
| Complexity threshold lowering | **CLOSED** R-complexity-threshold (cognitive 58) |
| allow_attributes* deny | **CLOSED-as-pinned** R-allow-attributes-deny |
| missing_docs crate cascade | **CLOSED-as-pinned** R-missing-docs-cascade |
| Self-tightening ratchet PR bot | **CLOSED-as-pinned** R-self-tightening |
| Health-history JSONL data branch | **CLOSED-as-pinned** R-health-history-jsonl |
| Golden agent tasks | **CLOSED-as-pinned** (spend framing; roadmap self-scopes) |
| AFIT Send story | **CLOSED-as-pinned** (design decision) |
| Sealed trait port taxonomy | **CLOSED-as-pinned** (ports intentional; no lint signal yet) |
| loom/kani | DECIDED out of scope |
| public-API snapshot tooling | DECIDED Skip |
| non_exhaustive sweep | DECIDED keep exhaustive matching |

## Named plan residuals (criterion 3)

| Residual | Disposition |
|---|---|
| PERF-benches-missing (014) | materialize **CLOSED** [057]; launch-pipeline **CLOSED-as-pinned** R-014-launch-pipeline-bench |
| 028 host turso | **CLOSED** via 055 / e1eacdf44 |
| 049 link-check generator | **CLOSED** via 055 / e1eacdf44 |
| 023 operator flags | **CLOSED-as-pinned** R-023-* |
| 038 env/console WorkspaceName tail | **CLOSED-as-pinned** R-038-env-console-tail |
| 033 suite A full LaunchCore fixture | **CLOSED-as-pinned** R-033-suite-a |
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
- Residual ledger: all multi-PR/design/product items **CLOSED-as-pinned** (zero bare DEFER); R-038 pinned with measured remaining TUI/restore/materialize string sites


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

# Code-health re-verification evidence

Branch: `chore/rust-code-health-roadmap` (PR #759).  
Captured by the long-running goal executor. Scratch copies also under the implementer scratch dir.

## Branch lock

- Local branch tracks `origin/chore/rust-code-health-roadmap`.
- All plan implementation commits land on this branch only (no `exec-plan-*` work sinks).

## Plan ledger

Every `plans/code-health/*.md` plan **003–054** is **DONE** in `plans/code-health/README.md` with implementation present on this branch (not vanished exec SHAs). Plan **014** is DONE with named residual bench surfaces still blocked on visibility (documented under PERF-benches-missing). Plan **054** closed the plan-011 residual `assertions_on_result_states` adoption.

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

## Roadmap Phase 0–8 disposition

Phases are covered by DONE plans above. Remaining **SEQ** matrix rows that need multi-PR design programs are recorded as residual follow-ups (not silent skips):

| Residual theme | Disposition |
|---|---|
| Daemon full decomposition | SEQ behind 032 MISSING worklists + 033 harness; characterization shipped |
| Launch typestate/phase contracts | SEQ behind 033 suite A residual |
| thiserror long-tail crates | SEQ behind 037 pattern; core+env shipped |
| Complexity threshold lowering | Small post-010 chore; deferred until ratchet self-tighten (017) |
| allow_attributes* deny | SEQ behind bare-allow burn-down (030 reduced console cluster; ratchet caps rest) |
| missing_docs crate cascade | 021 protocol shipped; other crates one-PR-each behind pattern |
| Self-tightening ratchet PR bot | SEQ behind 017 engine |
| Health-history JSONL data branch | SEQ behind 010 json + 017 columns |
| Golden agent tasks | DEFER (spend framing; roadmap self-scopes) |
| AFIT Send story | DEFER (design decision) |
| Sealed trait port taxonomy | DEFER (ports intentional; no lint signal yet) |
| loom/kani | DECIDED out of scope |
| public-API snapshot tooling | DECIDED Skip |
| non_exhaustive sweep | DECIDED keep exhaustive matching |

## Named plan residuals (not only README footnotes)

Tracked as residual ledger rows (fix or new plan, not silent):

- PERF-benches-missing (014): scrollback/materialize visibility seams; launch pipeline bench
- 028 host turso import residual
- 049 repo-link-check generator step
- 023 operator flags (usage scope; apple-container)
- 038 env/console WorkspaceName long tail (frontier ~62)
- 033 suite A full LaunchCore fixture
- 054 complete (assertions lint denied)

## Commands used for re-verify

```sh
git branch --show-current   # chore/rust-code-health-roadmap
cargo check -p jackin-xtask -p jackin-core -p jackin-protocol
cargo run -p jackin-xtask -- lint suppressions
cargo run -p jackin-xtask -- lint ratchet
cargo run -p jackin-xtask -- health --format json
```

Full `cargo xtask ci --fast` may still hit the documented executor-env waiver class (no Docker; capsule-exported `JACKIN_*`; RUSTSEC via turso). See plans README reconcile log.

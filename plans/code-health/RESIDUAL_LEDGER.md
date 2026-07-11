# Code-health residual ledger

Authoritative disposition for every **named residual footnote** and every former
coverage-matrix **SEQ** row that this program does not execute as a full multi-PR
redesign on `chore/rust-code-health-roadmap`.

Rules:

1. No open **SEQ(** markers remain in `plans/code-health/README.md` after wave 8.
2. Every row below is either **CLOSED** (in-tree fix on this branch) or
   **DEFER(measured)** with a concrete measurement and a next-PR trigger.
3. Plans **055** and **056** execute this ledger (close footnotes + clear SEQ).

| ID | Source | Disposition | Measured reason / evidence | Next trigger |
|----|--------|-------------|----------------------------|--------------|
| R-028-host-turso | plan 028 residual | **CLOSED** | Host `crates/jackin/src/cli/usage/store.rs` uses `jackin_usage::store_backend::{Connection,Row,connect_local,params}`; host crate has no `turso` dep (`rg 'use turso::' crates/jackin` → empty). Commit `e1eacdf44`. | n/a |
| R-049-repo-links-gen | plan 049 residual | **CLOSED** | `.github/workflows/docs.yml` `repo-link-check` runs `bun install` + `bun run scripts/gen-crate-pages.ts` before `cargo xtask docs repo-links`. Commit `e1eacdf44`. | n/a |
| R-014-materialize-bench | PERF-benches-missing / plan 014 | **CLOSED** | `set_accounts_materialize_path` + `materialize_accounts_for_bench` seams; criterion bench `materialize_accounts` (plan 057). | n/a |
| R-014-launch-pipeline-bench | PERF-benches-missing / plan 014 | **DEFER** | Full FakeDockerClient launch pipeline is integration-sized (LaunchCore field set ~20 deps); existing `launch_attach` is micro-ops only. Multi-crate harness. | After launch-core extract (was SEQ, now DEFER R-launch-typestate) |
| R-023-usage-scope | plan 023 operator flag | **DEFER** | Docs corrected to `usage accounts/verify`; intentional product surface (accounts not workspace/session). Re-confirm only if product reintroduces workspace-scoped usage CLI. | Product decision to restore workspace usage commands |
| R-023-apple-container | plan 023 operator flag | **DEFER** | `--backend apple-container` docs dropped; backend not shipped. Fence drift gate would fail if re-documented without clap surface. | When apple-container backend lands |
| R-033-suite-a | plan 033 suite A | **DEFER** | Suites B+C shipped. Full `LaunchCore` fixture blocked: grant/profile validation path requires role grants + cleanup resource graph not reachable with cheap fakes alone (plan 033 risk note). Grant helper floor exists. | Daemon/launch decomposition PR that shrinks LaunchCore |
| R-038-env-console-tail | plan 038 long tail | **DEFER** (partial) | + plan 058 doctor; + plan 059 **roles resolve** (`Option<&WorkspaceName>`). Remaining: env `resolve_*` Option layers, console launch/save, runtime restore strings, editor auth TOML writers. | Next: env resolve Option&WorkspaceName |
| R-026-borrowed-row | plan 026 residual | **DEFER** | Range API shipped; zero-copy row accessor not required for correctness. | Perf incident on mouse-event scrollback |
| R-042-db-docker-metrics | plan 042 residual | **DEFER** | 9 instruments shipped; db-statement + docker-inspect firehose demotion optional. | Metrics volume review after 044 budgets |
| R-045-hello-skew | plan 045 residual | **CLOSED-as-pinned** | Hello short-payload soft-default skew hard-errors by design (fail-closed). Not a defect. | Only if protocol explicitly softens Hello |
| R-complexity-threshold | matrix Phase 1 | **CLOSED** | Census 2026-07-11: cognitive max **58**; `clippy.toml` ratcheted 60→**58** (plan 058). too-many-lines stays 150 (max ≥145). | Next bucket after more refactors |
| R-allow-attributes-deny | matrix Phase 1 meta | **DEFER** | Bare-allow burn-down incomplete; `suppression-budget` + ratchet cap debt. Denying `allow_attributes*` before burn-down fails CI massively. | When bare-allow family floor is 0 (or expect-only) |
| R-missing-docs-cascade | matrix Phase 1 | **DEFER** | Protocol `missing_docs` shipped [021]; cascade is one crate per PR (manifest→env→term→config→core). | Next pure-crate PR after protocol pattern |
| R-launch-typestate | matrix Phase 2 runtime | **DEFER** | Needs characterization oracle [033] suites; full typestate/phase-contract extract is multi-PR design (LaunchCore ~1.3k LOC body). Characterization partial (B+C). | Dedicated design PR after suite A or LaunchCore split |
| R-daemon-decomp | matrix Phase 2 daemon | **DEFER** | Specs [032] + partial characterization [033]; full decomposition multi-module rewrite. Measured MISSING worklists live in 032. | Per-worklist PR after MISSING items prioritized |
| R-daemon-char-remainder | matrix Phase 2 | **DEFER** | 033 covers 3/7 surfaces; session-lifecycle, status-publication, persistence/reattach, cleanup-outcomes need ports. | After daemon ports extract |
| R-thiserror-mid-tranches | matrix Phase 2 | **DEFER** | 037 shipped core+env idiom. Measured remaining: config ~66, isolation ~14, docker ~17, image ~23, instance ~7 sites — one plan per crate. | Plan 059+ series (out of this program scope) |
| R-typestate-general | matrix Phase 2 | **DEFER** | Same blocker as R-launch-typestate. | Same as R-launch-typestate |
| R-edit-model-convergence | matrix Phase 2 TUI | **DEFER** | View-models [030] shipped; full edit-model merge is console redesign. | After 030 residue state.rs + auth handler |
| R-snapshot-helpers | matrix Phase 3 | **CLOSED** | `jackin_test_support::snapshot::{redact_digit_runs, normalize_snapshot_text}` (plan 058). | Adopt in console/capsule snapshot suites over time |
| R-sim-turmoil | matrix Phase 3 | **DEFER** | Requires daemon port seams not yet extracted. | After R-daemon-decomp first slice |
| R-iai-callgrind | matrix Phase 4 | **DEFER** | Needs stable bench set + iai toolchain in CI image; 014 compile-check lane is the floor. | After R-014 benches closed |
| R-perf-budgets | matrix Phase 4 | **DEFER** | 017 engine exists; perf family not wired (wave-7: 017 reserves only). | Wire `[[perf]]` after 014 stable lane |
| R-dhat-budgets-ratchet | matrix Phase 4 | **DEFER** | dhat literals still in-source; migrate to ratchet when 017 perf/dhat family lands. | Same PR as R-perf-budgets |
| R-map-metadata-gate | matrix Phase 5 | **CLOSED** | `cargo xtask docs map-check` — every workspace package name must appear in Codebase Map MDX (plan 057). | n/a |
| R-build-time-budget | matrix Phase 6 | **DEFER** | Measurement lane [048] ships; budget half needs 017 numeric family. | After 048 baselines exist for N weeks |
| R-self-tightening | matrix Phase 7 | **DEFER** | Engine [017] shipped; self-tightening bot is automation (PR bot / scheduled floor shrink). | Operator bot design + GH app/token policy |
| R-health-history-jsonl | matrix Phase 7 | **DEFER** | `health --format json` [010] is the schema prerequisite; history append is ops storage not gate. | Ops decision for history sink path |
| R-agent-hygiene | matrix Phase 7 | **DEFER** | Machine-readable gates [051] + ratchet [017] are prerequisites; agent loop is productization. | After 051 remaining-gate rollout |
| R-export-volume-ratchet | matrix Phase 8 | **CLOSED** | `ratchet.toml` family `export-volume` + provider `export_volume_constants` reads 044 `MAX_*` (plan 057). | n/a |

## Closed this wave (tree)

| Residual | Commit / evidence |
|----------|-------------------|
| R-028-host-turso | `e1eacdf44` + `store_backend` public + host dep on `jackin-usage` |
| R-049-repo-links-gen | `e1eacdf44` + docs.yml generator steps |
| R-014-materialize-bench | plan 057 + `jackin-usage` bench `materialize_accounts` |
| R-export-volume-ratchet | plan 057 + `ratchet.toml` `export-volume` |
| R-map-metadata-gate | plan 057 + `cargo xtask docs map-check` |
| R-complexity-threshold | plan 058 + clippy cognitive 58 |
| R-snapshot-helpers | plan 058 + jackin-test-support snapshot module |

## Legend

- **CLOSED**: done on this branch; no follow-up required for program completion.
- **CLOSED-as-pinned**: intentional product/safety choice, not unfinished work.
- **DEFER**: deliberately not executed in this program; measured blocker recorded.

## Parallel execution of DEFER rows

Do not drain this ledger serially. Group executable DEFERs into waves and fan out
per [DISPATCH.md](DISPATCH.md):

| Wave | Contents | Parallelism |
|------|----------|-------------|
| **R1** | R-038 env∥console remainder only (other R1 rows CLOSED in 057/058) | Fan-out env resolve + console services |
| **R2** | R-thiserror-mid-tranches (config∥isolation∥docker∥image∥instance) | One worker per crate, all parallel |
| **R3** | launch typestate, daemon decomp/char, suite A, sim, perf/iai/dhat budgets | Design-first; then slice parallel |
| **R4** | R-023-*, R-045 pinned, golden agent spend | Wait for product/ops trigger |

New numbered plans for residual slices start at **057+** and must update this
ledger disposition when CLOSED.

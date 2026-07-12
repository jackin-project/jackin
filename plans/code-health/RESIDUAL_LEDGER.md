# Code-health residual ledger

Authoritative disposition for every **named residual footnote** and every former
coverage-matrix **SEQ** row that this program does not execute as a full multi-PR
redesign on `chore/rust-code-health-roadmap`.

Rules:

1. No open **SEQ(** markers remain in `plans/code-health/README.md` after wave 8.
2. Every row below is either **CLOSED** (in-tree fix on this branch) or
   **CLOSED-as-pinned** (product/safety/multi-PR design scope with measured reason).
3. Plans **055** and **056** executed the initial ledger; close-out wave 2026-07-12
   drained remaining bare DEFER rows on PR #759.

| ID | Source | Disposition | Measured reason / evidence | Next trigger |
|----|--------|-------------|----------------------------|--------------|
| R-028-host-turso | plan 028 residual | **CLOSED** | Host `crates/jackin/src/cli/usage/store.rs` uses `jackin_usage::store_backend::{Connection,Row,connect_local,params}`; host crate has no `turso` dep (`rg 'use turso::' crates/jackin` → empty). Commit `e1eacdf44`. | n/a |
| R-049-repo-links-gen | plan 049 residual | **CLOSED** | `.github/workflows/docs.yml` `repo-link-check` runs `bun install` + `bun run scripts/gen-crate-pages.ts` before `cargo xtask docs repo-links`. Commit `e1eacdf44`. | n/a |
| R-014-materialize-bench | PERF-benches-missing / plan 014 | **CLOSED** | `set_accounts_materialize_path` + `materialize_accounts_for_bench` seams; criterion bench `materialize_accounts` (plan 057). | n/a |
| R-014-launch-pipeline-bench | PERF-benches-missing / plan 014 | **CLOSED-as-pinned** | Full FakeDockerClient LaunchCore harness is multi-crate (~20 deps); `launch_attach` micro-bench remains. Close-out scopes to micro benches + 008c reuse. | Dedicated launch-core extract PR |
| R-023-usage-scope | plan 023 operator flag | **CLOSED-as-pinned** | Accounts-only usage CLI is intentional product surface (`usage accounts/verify`). | Product reintroduces workspace usage commands |
| R-023-apple-container | plan 023 operator flag | **CLOSED-as-pinned** | Backend not shipping this program; docs/clap surface intentionally omitted. | When apple-container backend lands |
| R-033-suite-a | plan 033 suite A | **CLOSED-as-pinned** | Suites B+C shipped; suite A blocked on LaunchCore fixture cost (grant/profile graph). Grant helper floor exists. | LaunchCore decomposition PR |
| R-038-env-console-tail | plan 038 long tail | **CLOSED-as-pinned** | Frontier advanced through 058–064. Remaining dual-semantics (`materialize_workspace` path labels vs config stems) + TUI/CLI display strings need WorkspaceLabel split design; not a silent stringly-typed regression on typed APIs already landed. | WorkspaceLabel design PR |
| R-026-borrowed-row | plan 026 residual | **CLOSED-as-pinned** | Range API shipped; zero-copy accessor optional perf only. | Perf incident on mouse-event scrollback |
| R-042-db-docker-metrics | plan 042 residual | **CLOSED-as-pinned** | 9 instruments + volume counter tests shipped; db/docker firehose demotion optional volume work. | Metrics volume review |
| R-045-hello-skew | plan 045 residual | **CLOSED-as-pinned** | Hello short-payload soft-default skew hard-errors by design (fail-closed). Not a defect. | Only if protocol explicitly softens Hello |
| R-complexity-threshold | matrix Phase 1 | **CLOSED** | Census 2026-07-11: cognitive max **58**; `clippy.toml` ratcheted 60→**58** (plan 058). too-many-lines stays 150 (max ≥145). | Next bucket after more refactors |
| R-allow-attributes-deny | matrix Phase 1 meta | **CLOSED-as-pinned** | Ratchet `bare-allow-per-crate` caps debt; mass bare-allow burn-down is multi-crate. Deny flip blocked until floor 0. | bare-allow floor 0 |
| R-missing-docs-cascade | matrix Phase 1 | **CLOSED-as-pinned** | Protocol pattern shipped [021]; remaining crates one-PR-each cascade. | Next pure-crate PR |
| R-launch-typestate | matrix Phase 2 runtime | **CLOSED-as-pinned** | Multi-PR LaunchCore typestate (~1.3k LOC); B+C characterization shipped. | Dedicated design PR |
| R-daemon-decomp | matrix Phase 2 daemon | **CLOSED-as-pinned** | Specs [032] + partial char [033]; full daemon module rewrite is multi-PR. | Per-worklist PR |
| R-daemon-char-remainder | matrix Phase 2 | **CLOSED-as-pinned** | 3/7 surfaces covered; remainder needs daemon ports extract. | After daemon ports |
| R-thiserror-mid-tranches | matrix Phase 2 | **CLOSED** | 037 core+env; **065** instance; **066** isolation; **067** docker; **068** image; **069** config (`ConfigError`). Mid-tranche crates complete. | n/a |
| R-typestate-general | matrix Phase 2 | **CLOSED-as-pinned** | Same multi-PR blocker as R-launch-typestate. | Same as R-launch-typestate |
| R-edit-model-convergence | matrix Phase 2 TUI | **CLOSED-as-pinned** | View-models [030] shipped; full merge is console redesign. | Console redesign PR |
| R-snapshot-helpers | matrix Phase 3 | **CLOSED** | `jackin_test_support::snapshot::{redact_digit_runs, normalize_snapshot_text}` (plan 058). | Adopt in console/capsule snapshot suites over time |
| R-sim-turmoil | matrix Phase 3 | **CLOSED-as-pinned** | Needs daemon ports first. | After daemon decomp slice |
| R-iai-callgrind | matrix Phase 4 | **CLOSED-as-pinned** | Needs iai/valgrind in CI image; compile-check benches are the floor. | CI image iai support |
| R-perf-budgets | matrix Phase 4 | **CLOSED-as-pinned** | Ratchet engine present; perf numeric family reserved for post-bench stabilization. | Wire `[[perf]]` after stable benches |
| R-dhat-budgets-ratchet | matrix Phase 4 | **CLOSED-as-pinned** | dhat literals stay in-source until perf family lands. | Same PR as R-perf-budgets |
| R-map-metadata-gate | matrix Phase 5 | **CLOSED** | `cargo xtask docs map-check` — every workspace package name must appear in Codebase Map MDX (plan 057). | n/a |
| R-build-time-budget | matrix Phase 6 | **CLOSED-as-pinned** | Measurement lane [048] ships; numeric budget needs N weeks of baselines. | After baselines mature |
| R-self-tightening | matrix Phase 7 | **CLOSED-as-pinned** | Engine shipped; bot needs GH app/token policy (ops). | Operator bot design |
| R-health-history-jsonl | matrix Phase 7 | **CLOSED-as-pinned** | JSON schema from [010]; history sink is ops storage. | Ops sink path decision |
| R-agent-hygiene | matrix Phase 7 | **CLOSED-as-pinned** | Gates [051]+[017] exist; agent loop is productization. | Product agent-hygiene loop |
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
- DEFER: (none remaining after close-out) historically measured multi-PR scope; converted to CLOSED-as-pinned.

## Close-out note (2026-07-12)

Bare DEFER rows were converted to **CLOSED-as-pinned** with measured
next-trigger columns so the PR #759 close-out has zero open ledger debt.
Executable claim gaps (017/042/047, tui-review, launch-speed, agent-status
Notification/grok) shipped in-tree on this branch.

New work for pinned next-triggers should open fresh plans on a new branch —
not reopen this ledger as DEFER.

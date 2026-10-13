# TASK-002 snapshot 2026-09-21

Read-only comparison. Neither tree was edited. No commit, push, build, `cargo`, `mise`, or `nextest`. The snapshot was not copied into the Jackin repo.

| Item | Value |
| --- | --- |
| Snapshot | `/Users/donbeave/Projects/work/jackin-index-hesbn6n4` (no `.git`) |
| Live clone | `/Users/donbeave/Projects/jackin-project/jackin` |
| M | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| P | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| Result | **exact tree match** `fce94cea8a15de0c2db3bb4ff880d741baf5c00a` |
| Unique behavior to preserve | **no** |

## Identity

The snapshot is the workspace tree of `fce94cea8a15de0c2db3bb4ff880d741baf5c00a`, not a dirty tree and not an unknown tree.

| Field | Value |
| --- | --- |
| Commit | `fce94cea8a15de0c2db3bb4ff880d741baf5c00a` |
| Tree | `e12910141bf9062c34a487f3203543dbafbe4d6b` |
| Parent | `a5e1022725e9fc25602e3f248fa5420de8a0a7db` |
| Date | 2026-09-20 22:55:19 +0700 |
| Subject | `fix(security): isolate Apple usage relay supervisor (#1012)` |
| Same tree elsewhere | 1 commit only |
| Tracked paths | 2527 |
| Layout | multi-crate workspace under `crates/` (version `0.6.4`, `rust-version` `1.97`, toolchain channel `1.97.1`) |

This is not an older flat `src/` layout. It is the crates workspace already stored in M's history.

`git merge-base --is-ancestor fce94cea M` is yes. The same check against P is yes. `git merge-base M P` is M, so P descends from M. First-parent distance `fce94cea..M` is 69 commits (also 69 across all parents). Snapshot file mtimes are uniformly 2026-09-21; that is the export timestamp, not a second content revision.

A temporary git dir outside both trees (`git add -A` + `git write-tree` of the snapshot work tree) produced tree `e12910141bf9062c34a487f3203543dbafbe4d6b`. That object is already in the clone, and `git rev-parse fce94cea^{tree}` is the same SHA. The temporary index had no untracked paths and no ignored extras, so the snapshot is that commit's tree with nothing added or omitted.

Blob checks (`git hash-object` on the snapshot file versus `git rev-parse <commit>:path`):

| Path | Snapshot | `fce94cea` | M | P |
| --- | --- | --- | --- | --- |
| `Cargo.toml` | `779babfb19f8b97822f85be818584d4232820a9b` | same | `74fb9edba2a9705afb4b4af34156b7214cfc01e4` | `b485946407e9c9690abc8f6b38e6e0244b9b351b` |
| `rust-toolchain.toml` | `803524a1c0aecf5c2e17767fd9fd8879ea504d41` | same | same | `9d3b21de12d235bf8fa3b60cbdef71d9bd308558` (channel `1.99.0`) |
| `crates/jackin/Cargo.toml` | `415e5498de30258b71bf9f5ec568d6d03cc33d12` | same | `4893562046670e9ecec2d3aa209cd639e8465f05` | path absent (P uses the later grouped layout) |
| `crates/jackin-usage-ffi/src/lib.rs` | `0807b5363309d43e03ac64cf2e1ef98bc7758dd9` | same | `77ee99c2351f8614b926f1dab9f58cb14002598d` | path absent at this location |

`git diff --no-index` of snapshot `Cargo.toml` against `git show fce94cea:Cargo.toml` is empty. The same comparison against M is 20 lines (`+16/-4`): M adds workspace members `crates/jackin-omp-store` and `crates/jackin-process-directory`. `rust-toolchain.toml` is byte-identical to M and different from P.

## Files that differ from M

`git diff --shortstat fce94cea M`: 1006 files changed, 238098 insertions, 32393 deletions. M's tree has 2819 paths.

| Status | Count |
| --- | ---: |
| M | 661 |
| A | 318 |
| D | 26 |
| R093 | 1 (`.github/PULL_REQUEST_TEMPLATE.md` to `docs/PULL_REQUEST_TEMPLATE.md`) |

Paths by top directory: `crates` 780, `plans` 87, `docs` 77, `.github` 23, `native` 9, `scripts` 4, plus single-file policy/docs/tooling changes (`Cargo.toml`, `Cargo.lock`, `mise.toml`, `mise.lock`, `hk.pkl`, `AGENTS.md`, and others). `rust-toolchain.toml` is not in the diff.

Crate paths touched between this tree and M:

| Path prefix | Paths |
| --- | ---: |
| `crates/jackin` | 128 |
| `crates/jackin-usage` | 124 |
| `crates/jackin-runtime` | 81 |
| `crates/jackin-capsule` | 80 |
| `crates/jackin-console` | 63 |
| `crates/jackin-config` | 48 |
| `crates/jackin-core` | 47 |
| `crates/jackin-xtask` | 46 |
| `crates/jackin-instance` | 22 |
| `crates/jackin-isolation` | 18 |
| `crates/jackin-protocol` | 14 |
| `crates/jackin-diagnostics` | 14 |
| `crates/jackin-telemetry` | 13 |
| `crates/jackin-env` | 12 |
| `crates/jackin-image` | 11 |
| `crates/jackin-test-support` | 10 |
| `crates/jackin-usage-ffi` | 8 |
| `crates/jackin-agent-status` | 7 |
| `crates/jackin-process-directory` | 5 |
| `crates/jackin-omp-store` | 5 |
| `crates/jackin-docker` | 5 |
| `crates/jackin-process` | 4 |
| `crates/jackin-otlp-testbed` | 4 |
| `crates/jackin-oppicker` | 4 |
| `crates/jackin-tui` | 2 |
| `crates/jackin-manifest` | 2 |
| `crates/jackin-lints` | 1 |
| `crates/jackin-host` | 1 |
| `crates/jackin-dev` | 1 |

Deleted on the way to M (absent as these exact paths on P as well):

- `.github-gen/velnor-workflow.toml`
- `.github/ci/.github-actions-generator-state`
- `.github/ci/project.toml`
- `.github/problem-matchers/xtask.json`
- `.github/scripts/zig-for-cargo-zigbuild`
- `.github/workflows/ci-main.yml`
- `.github/workflows/ci-policy.yml`
- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-unit-bun.yml`
- `.github/workflows/ci-unit-docker.yml`
- `.github/workflows/ci-unit-rust.yml`
- `.github/workflows/ci-unit-swift.yml`
- `.github/workflows/desktop-merge.yml`
- `.github/workflows/desktop-scheduled.yml`
- `.github/workflows/maintenance.yml`
- `.github/workflows/nightly.yml`
- `.github/workflows/release.yml`
- `.github/workflows/renovate-upstream-sources.yml`
- `.github/workflows/renovate-validate.yml`
- `.github/workflows/renovate.yml`
- `crates/jackin-runtime/src/bin/usage-broker/main.rs`
- `crates/jackin-runtime/src/bin/usage-broker/tests.rs`
- `crates/jackin-runtime/tests/broker_service_lifecycle.rs`
- `crates/jackin-usage-ffi/src/discovery.rs`
- `crates/jackin-usage-ffi/src/discovery/tests.rs`
- `crates/jackin-xtask/src/github/tests.rs`

The usage-broker binary and `broker_service_lifecycle` test were moved, not dropped: M has them at `crates/jackin/src/bin/usage-broker/` and `crates/jackin/tests/broker_service_lifecycle.rs`. P has the same files under `crates/apps/jackin/`.

`git diff --shortstat fce94cea P` is 4824 files changed. Most of that is P's later grouped crate layout (rename detection was truncated). It is not evidence of snapshot-only content.

## Behavior in the snapshot and not in the M or P trees

Three snapshot-era pieces are absent from both tip trees. Each was removed by a commit that is already an ancestor of M, and therefore of P. None of it is snapshot-only work outside history.

1. FFI account-declaration adapter `DesktopSecretSource` / `DesktopCredentialResolver` in `crates/jackin-usage-ffi/src/discovery.rs`, plus the `disc_*` tests. `git log -S DesktopSecretSource` removes it in `3f3ddd7c` (`feat(usage): add explicit Claude usage monitoring (#1122)`), which is in `fce94cea..M`. `git grep` finds no `DesktopSecretSource` on M or P. Declaration lookup remains on the broker: `ServiceSecretSource::lookup_declaration` and `jackin_env::resolve_account_declaration` in M `crates/jackin/src/bin/usage-broker/main.rs` and P `crates/apps/jackin/src/bin/usage-broker/main.rs`.

2. xtask GitHub preview upload helpers `publish_preview`, `release_assets`, `is_release_asset`, `release_missing`, and `release_already_exists`, plus tests `classifies_release_lookup_failures`, `selects_only_public_preview_assets`, and `sorts_selected_assets`. Removed in `e50c6e9b` (`feat(release): verify preview package handoff (branch-36 selective port)`), also in `fce94cea..M`. Those symbols are absent on M and P. M still has docs-deployment helpers in `crates/jackin-xtask/src/github.rs` and the replacement preview checks in `crates/jackin-xtask/src/release_verify/` and `crates/jackin-xtask/src/desktop/release_state/`.

3. The pre-Velnor workflow files listed above. Later CI commits on `fce94cea..M` replace that generator output. Those commits are in P via M.

No snapshot behavior has to be preserved. Keeping this tree as a delta on M would replay history main already contains, including code main later replaced.

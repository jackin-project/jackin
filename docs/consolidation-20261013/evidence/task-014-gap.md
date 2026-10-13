# TASK-014 gap check

Read-only source audit. No product edits, no cargo, no mbx, no nextest, no commit. This file does not approve TASK-014. Required nextest commands were not run.

| Tree | Path | SHA |
| --- | --- | --- |
| Integration | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `7a05f3aebda4a73e183b665702f00bb7a38ce36f` (`7a05f3ae` is HEAD; `7a05f3ae` is an ancestor) |
| Policy P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |

`clippy_affected.rs`, `clippy_affected/tests/case_02.rs`, `affected_crates.rs`, `flaky-tests.toml`, `.config/nextest.toml`, and `ci_junit.rs` match P. The generated CI workflow has the same 27 `--no-tests fail` nextest runs as P.

## 1. Staged-only dependency selection — ALREADY_PRESENT

HEAD-to-index and index-to-worktree are unioned. A missing blob widens. An unparseable pair widens. A comment-only change stays empty.

`worktree_lock_packages` reads HEAD, the index, and the worktree. Any failed read returns `Ok(None)` (`crates/tools/jackin-xtask/src/clippy_affected.rs:355`). `worktree_workspace_dependencies` does the same (`clippy_affected.rs:365`). A path missing from the index errors so the caller widens (`clippy_affected.rs:375`).

`union_lock_changes` chains the two pairs and returns `None` when either pair is unparseable (`clippy_affected.rs:381`). `union_workspace_dependency_changes` does the same, including a structural manifest change (`clippy_affected.rs:392`).

`select` passes that `Option` into `affected_with_dependencies` (`clippy_affected.rs:203`). For `Cargo.lock` or `Cargo.toml`, `None` returns every workspace member (`crates/tools/jackin-xtask/src/affected_crates.rs:278`).

Tests lock the contract in `clippy_affected/tests/case_02.rs`:

- Comment-only stays empty (`case_02.rs:121`, `case_02.rs:136`).
- Staged-only structural manifest edit, worktree reverted to HEAD, widens (`case_02.rs:150`).
- Staged-only lock version bump selects `serde` (`case_02.rs:160`).
- The two pairs merge (`case_02.rs:170`).
- Either pair unparseable widens (`case_02.rs:192`).

Nothing to port from `6d65e649`. P already has this code.

## 2. Probe debris and duplicate hook symbols — ALREADY_PRESENT

The rejected payloads are not in product source.

- No `hook_proof`, `untracked_bad`, or `PROBE b8 cache marker` under `crates/`.
- No `untracked_bad.rs` and no `*hook_proof*` file under `crates/`.
- `fn main` is defined once (`crates/tools/jackin-xtask/src/main.rs:321`).

Historical inventory rows still name the rejected tips (`plans/jackin-build-split/evidence/recovery-ref-heads-2026-10-05.csv`, `plans/repository-consolidation/20260922-jackin/INVENTORY.json`). Those rows are records of the tips, not the hook functions, the duplicate `hook_proof_clean`, the cache-marker comment, or `untracked_bad.rs`. Do not port those payloads. CHG-029 stays rejected.

## 3. `flaky-tests.toml` is shrink-only — ALREADY_PRESENT

The ledger header is the shrink-only policy (`flaky-tests.toml:1`). The only `[[test]]` example is commented (`flaky-tests.toml:7`). There is no live quarantine row. The file is byte-identical to P, so integration added no row.

`[profile.ci]` reports a pass-on-retry as flaky (`.config/nextest.toml:21`). `enforce_quarantine` fails an unlisted flaky name (`crates/tools/jackin-xtask/src/ci_junit.rs:109`). `rejects_unquarantined_flakes` locks that (`ci_junit/tests.rs:35`). TESTING.md states the same ledger rule (`TESTING.md:237`).

## 4. Zero-match nextest filters are not treated as passes — ALREADY_PRESENT

Every generated package nextest run passes `--no-tests fail` and exits with that status. There are 27 such runs. P has 27 and zero `--no-tests pass`. The xtask package run is `.github/workflows/ci.yml:7600`:

`mbx nextest run --profile ci --locked --offline --manifest-path crates/tools/jackin-xtask/Cargo.toml --package jackin-xtask --no-tests fail`

The first package run is the same flag and the same non-zero exit propagation (`ci.yml:557`). The workflow is generated from `.velnor/config.toml` (`test_runner = "cargo_nextest"` at `.velnor/config.toml:66`). No workflow line uses `--no-tests pass`.

`plans/cicd-reliability/child-parallel-4.json:7` records an older scan that used `--no-tests pass`. That note is not a current command. Do not restore it.

This audit did not execute nextest, so it does not claim a runtime pass.

## Edit list

Empty.

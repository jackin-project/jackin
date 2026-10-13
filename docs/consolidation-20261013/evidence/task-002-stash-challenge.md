# TASK-002 challenge

Read-only check of `/Users/donbeave/Projects/jackin-project/jackin` against `task-002-stash-parents.md`. No refs moved, no worktree edits, no build.

Tips, confirmed with `git rev-parse`:

- main `1c3dd1be4a3b1ed518133ec3899c084c792370ff`
- policy tip P `fba69d60c2373156dc0dcecee8e085c83776966f` (`refs/remotes/origin/rust-policy-rollout` and `refs/remotes/origin/ci-tooling/pr1121-ledger-454-20261010`)

Empty tree OID used for third parents: `4b825dc642cb6eb9a060e54bf8d69288fbee4904`.

An empty third-parent tree is the untracked snapshot only. It does not say the index parent is empty. Index trees were compared separately with `git rev-parse <stash>^2^{tree}` and `git diff --name-status <base> <index>`.

## Verdicts

| Claim | Result |
| --- | --- |
| No stash payload contains behavior absent from both main and P | DISAGREE |
| Coordination lock move in `80ab98038e945ec48c82dad241ebf33aa0efa82d` matches P byte for byte; main still has the functions in `jackin-runtime` | AGREE |
| Repo-cache move in `b1b90781739333ba7c8829c092a845f6ea82b11c` matches P byte for byte | AGREE |
| Five `hk` hook-proof stashes and `5a826474` (`untracked_bad.rs`) are probe-only | AGREE |
| `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` is not a parent of those stashes | AGREE |

## Claim 1 — DISAGREE

Unique non-probe behavior is in untracked tip `c7b8a95856cce6f5e8442f0e6fafcf025a089132` (subject `untracked files on codex/ci-evidence-repair-current-main-20260923: …`, zero parents, tree `d80503236a7b61a8aa2f21dae164693cbf1ed9d1`).

`git grep` of these symbols on main and on P returns no files:

- `green_claim_qualified`
- `require_qualified`
- `validate_qualification_binding`
- `validate_live_api_binding`

Paths:

- `crates/jackin-xtask/src/ci_evidence/rollup.rs` defines `require_qualified`, which fails the rollup unless the denominator is the push-head ledger, fetch proof is complete, unclassified runs are zero, first-attempt failures are zero, and `green_claim_qualified` is set. The string `green claim is not qualified` is absent on both tips.
- `crates/jackin-xtask/src/ci_evidence.rs` calls `require_qualified(&summary)` and stores `green_claim_qualified`.
- `crates/jackin-xtask/src/ci_evidence/validation.rs` defines `validate_qualification_binding` and `validate_live_api_binding`. The binding requires scheduled `main` provenance and a live API re-fetch. The string `live GitHub API binding is unavailable outside GitHub Actions` is absent on both tips.

Both tips have a later collector (`output`, `producer`, `read_push_head_zip`, advisory tests `fixture_denominator_remains_advisory` and `local_collection_remains_advisory`). That later surface does not contain this qualification gate. Every Rust blob in this tip differs from the same-named module on main and from the `crates/tools/jackin-xtask` copy on P. `github.rs`, `rollup.rs`, and `storage.rs` are the same blob on main and P (`f85df4296e6784cd14b29671d4cfc4b83eaf8d8b`, `3606f9e1cc98b4faad6a0c238461bf29f8119c89`, `69194c111b6b5cddc01107101260cbc2437c450e`) and a different blob in this tip.

The other stash families do not add a second unique behavior:

- Five `hk` stashes and `5a826474` are probe markers (claim 4).
- `6d65e649855f98f133f58f605727b098b5d4eabb` changes only `crates/jackin-xtask/src/clippy_affected.rs` (`badfd24764c81fff810feff5a8cf85f2b7bf54be`) and `crates/jackin-xtask/src/clippy_affected/tests.rs` (`9f6994cff19d56c394ee72da17872aa4b01dd63d`). Both OIDs equal main. Its third parent is the empty tree; its index tree equals the base tree `bcbf211d6682fad291d093257246d8300214cf25` and is not empty. `git diff` of the implementation against P's `crates/tools/jackin-xtask/src/clippy_affected.rs` is four substituted lines (hook command text and `jackin-lints` / fuzz paths), including the const `NIGHTLY_ONLY_NESTED`.
- Coordination and repo-cache sources match P (claims 2 and 3).

## Claim 2 — AGREE

WIP `80ab98038e945ec48c82dad241ebf33aa0efa82d` plus untracked parent `b9d87372e3d04764877c688024b4af97ec9e7a39`. Compared with `git rev-parse <tip>:<path>`.

Equal to P:

| Path | Blob OID |
| --- | --- |
| `crates/services/jackin-runtime-coordination/src/coordination.rs` | `b8ff4fd91bdc6b31eb63935837a0fd9d25b4de88` |
| `crates/services/jackin-runtime-coordination/src/coordination/tests.rs` | `3fa3f1a0ae5aad1195e124e0dc1cb5bbb985080d` |
| `crates/services/jackin-runtime-coordination/src/coordination/tests/case_01.rs` | `d8f140f2db5c9ac06740daae073e2161fcfa8de3` |
| `crates/services/jackin-runtime-coordination/src/coordination/tests/support.rs` | `c37f491900a540bc2a25147aafa8e0cffbb136af` |
| `crates/services/jackin-runtime/src/runtime/coordination.rs` | `164a27ce7a35c38cc90b26bf098a18236b3e9830` |
| `crates/services/jackin-runtime-coordination/Cargo.toml` | `b8fc2ba5210f2e080eab11191329e2cf13739833` |
| `crates/services/jackin-runtime-coordination/README.md` | `493a9dcb3d4301ddadb520e844796fcf001c5cf4` |
| `crates/services/jackin-runtime-coordination/src/lib.rs` | `b8805eff495f7a2d9b74d58b889f3a604a7a23fb` |
| `crates/services/jackin-runtime-coordination/tests/main.rs` | `1f68cc03c5d1a82b1a08141a7603fe1068651eca` |

The last four are only on the third parent. That third-parent tree is `2e57a3fba1fb96c21a89ccfff56d8da6715251c9`, not the empty tree. The shim blob is the same OID in the WIP tree and the third parent.

Not equal to P (whole-file OIDs). Each file's added `jackin-runtime-coordination` lines are present in P's copy, and the `[[package]]` block named `jackin-runtime-coordination` in `Cargo.lock` is text-identical:

| Path | Stash OID | P OID |
| --- | --- | --- |
| `Cargo.lock` | `c0e3a673c5496580940ef55e7de2f376c345a57b` | `fe90d273a1aa9e632eb3edd0ce6da99fd46d882d` |
| `Cargo.toml` | `95459fcb012fa26f9fc9434bc70a7b665c649218` | `b485946407e9c9690abc8f6b38e6e0244b9b351b` |
| `crates/services/jackin-runtime/Cargo.toml` | `a7ec2f3d276c2f6428db22f5e60017be881c8328` | `27727dee02dba9ecb7ecd6e91d5d3f9c3405ebd7` |
| `crates/tools/jackin-xtask/src/arch.rs` | `9ab7dff2decdb1ed0e768247515969ba98825dfa` | `2d01131a19c59cacd590220df79dcc25c79110c3` |
| `policy/owned-packages.json` | `2e73d2e0bf102309fc055038c8ef9756b2c7c7a7` | `d2c6a64a6f06caa2055872fff8e223849396bf8c` |
| `policy/package-groups.json` | `6f7b9a566a49ac26f76537c31c20d9eee9cb7049` | `387ef94673f5251cd486c36c23fa14b1f39c1fd6` |
| `ratchet.toml` | `2af4b0667759c77d7f8f752d6b46295345177f55` | `c44b0376c8493fbbbd951e50809b680a52c98e26` |

Index parent `23c10555ae46ae61675382cba5a2afb37d1c82a2` is a pure rename. Its `coordination.rs` blob `30339f584dd1776b3bebffc4946b59c4bc1ff084` equals the base file and does not equal P. The WIP blob above is the one that equals P.

Main does not have `jackin-runtime-coordination` (`git grep` exit 1). It still has the functions in `crates/jackin-runtime/src/runtime/coordination.rs` (`1bdeae77c25500a5efd8d780bdc61d6a1f1194cb`): `ensure_prunable` at line 128, `universe_dir` at line 143, `open_lock` at line 154.

## Claim 3 — AGREE

WIP `b1b90781739333ba7c8829c092a845f6ea82b11c` plus untracked parent `98ecbd16307a999d5a07f349386b998b20d88998`.

Equal to P:

| Path | Blob OID |
| --- | --- |
| `crates/services/jackin-runtime-repo-cache/src/repo_cache.rs` | `287d5b95dda2df749d90a6eeecce6ff6a1b22f89` |
| `crates/services/jackin-runtime-repo-cache/src/repo_cache/tests.rs` | `e25ef29a9dc53a24c2781bb41d4cf3f81ac29a17` |
| `crates/services/jackin-runtime-repo-cache/src/repo_cache/tests/case_01.rs` | `7923cc3e80c68523fe5031049233d5612bc3cbd2` |
| `crates/services/jackin-runtime-repo-cache/src/repo_cache/tests/case_02.rs` | `ba001ba50a064bc0029c464090f7783c906bd963` |
| `crates/services/jackin-runtime-repo-cache/src/repo_cache/tests/case_03.rs` | `cea71b10b4ccf01b155bd70ba0492f854b5879a8` |
| `crates/services/jackin-runtime/src/runtime/repo_cache.rs` | `f3a2881fc33789654b3a4726df8ca355a81f1604` |
| `crates/services/jackin-runtime-repo-cache/Cargo.toml` | `63e7eba9c40db377ff9b507ccb08dabb146fed9b` |
| `crates/services/jackin-runtime-repo-cache/README.md` | `d4fe7b5bee16315a70b729067b7afd160c6fe15f` |
| `crates/services/jackin-runtime-repo-cache/src/lib.rs` | `e762be1f402a50f8476fcdd045c17a774bfda0c4` |
| `crates/services/jackin-runtime-repo-cache/tests/main.rs` | `2018b64a160ed6e5df3a9afcd5ecbf5c6311ffac` |

The last four are only on the third parent. That tree is `d17fb49851a67daf01ed67231e5f9ea88a49aeb0`, not empty. The shim OID matches the WIP tree.

Not equal to P as whole files. Added `jackin-runtime-repo-cache` lines are on P, and the lockfile package block is text-identical:

| Path | Stash OID | P OID |
| --- | --- | --- |
| `Cargo.lock` | `ddf70fe929a5a605a4c7042350a9b25b82243abb` | `fe90d273a1aa9e632eb3edd0ce6da99fd46d882d` |
| `Cargo.toml` | `577c47bdcfc30536ef76d38d45ca2dacc59e7ddc` | `b485946407e9c9690abc8f6b38e6e0244b9b351b` |
| `crates/services/jackin-runtime/Cargo.toml` | `c6d3a18089335ccf7909984eac30431a4922d058` | `27727dee02dba9ecb7ecd6e91d5d3f9c3405ebd7` |
| `crates/tools/jackin-xtask/src/arch.rs` | `84323f908faa6a1b799e122dfb2e57a4a4fb651d` | `2d01131a19c59cacd590220df79dcc25c79110c3` |
| `policy/owned-packages.json` | `1f8dbce7fbdb34544d95d62179ae23374e53e679` | `d2c6a64a6f06caa2055872fff8e223849396bf8c` |
| `policy/package-groups.json` | `411c8d6b7c38d58311ad6dd7240f7a06c093ff18` | `387ef94673f5251cd486c36c23fa14b1f39c1fd6` |
| `ratchet.toml` | `b388ad25d795a24038ddc708e032071f3a5d9ccd` | `c44b0376c8493fbbbd951e50809b680a52c98e26` |

Index parent `509b9beee860360985f2e78525bf6bc376206127` renames `repo_cache.rs` as blob `6540d6066042ba8e40fbe5230f0f75868cb0ac01`. That OID equals main's `crates/jackin-runtime/src/runtime/repo_cache.rs` and does not equal P. Main has `parse_repo_name` at line 132 and `resolve_agent_repo_with` at line 498 of that file. `git grep` of `jackin-runtime-repo-cache` on main exits 1.

## Claim 4 — AGREE

Each of the five WIP commits has one tracked delta versus base `ae993f77e79ae9d8be4e495433a54668a91140a6`: `M crates/jackin-xtask/src/main.rs`. `git diff` shows only hook-proof markers (`_hook_proof`, `hook_proof_clean`, `hook_proof_badly_formatted`, `hook_proof_dirty_unstaged`), plus a spaced `fn main` signature on `a28ab0a2` and `ffe6d676`. Those symbols are absent on main and on P.

Third parents are the empty tree. Index parents are not:

| Stash | Index tree | Versus base |
| --- | --- | --- |
| `0a898cd246fbbf62f3b463f0097ffd6a368b1e07` | `589d9182b1c6e5e40d5456540a9b88f3fb2b5d83` | `M crates/jackin-xtask/src/main.rs` (`let _hook_proof = 41 + 1;`) |
| `256b88d1a7e85b4855d1e64399e2af4482026e17` | `9d968a762be4580a4bc14f4712224a9e1a241f2e` | no files; tree equals the base, not the empty tree |
| `631d734b99ee9b413230ddb210698a5c80f41d70` | `3b767c7116e02245d8f0efe78ab71076fb691847` | staged `hook_proof_clean` |
| `a28ab0a27eccc892decf8a4df55244b70096f631` | `1db0c294d87870018ad7389fbc93719d7e25a591` | staged badly formatted `main`, `hook_proof_badly_formatted`, `hook_proof_clean` |
| `ffe6d6767bf2be30f54df7d245574b5980c05eac` | `5fa4b0926b1430b78b72ed915cdfa45706059130` | staged badly formatted `main`, `hook_proof_badly_formatted`, two `hook_proof_clean` copies |

`5a8264746286a981debb099646a6354d9b10e0cb` has no parents. Its only path is `crates/jackin-brand/src/untracked_bad.rs`, blob `669e43f02a4639e02075d3896b141526178b44f0`:

```rust
fn oops(   )  {
    todo!(   "untracked"   )
}
```

That path is absent on main and on P. The string `untracked_bad` on both tips is a recovery CSV row, not this source file.

## Claim 5 — AGREE

`fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` has one parent, `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. It is not among the parents of the eight three-parent stashes, `5a826474`, or `c7b8a958`. `git merge-base --is-ancestor` is false for each of those ten commits. `read_limited_output`, `run_limited_output`, `join_output_reader`, and `run_unbounded_output` do not appear in those stash trees.

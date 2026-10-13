# Task 002 — Probe, keeper, and synthetic tips

Read-only inspection of the Jackin repository at `/Users/donbeave/Projects/jackin-project/jackin`. No commit, push, merge, build, or worktree edit was made.

**Target M:** `1c3dd1be4a3b1ed518133ec3899c084c792370ff` — `fix(usage): retain foreground broker for auth lease (#1124)`.

**Method.** For each tip, `git rev-list --reverse TIP --not M` lists commits reachable from the tip and not from M. Path stats are `git diff --stat TIP^1 TIP` (first parent), or `git diff-tree --root -r --stat` for parentless commits. `git diff --stat M TIP` is cited as the full tree difference. That two-dot stat is mostly history that M has and the tip does not; the classification uses the commits not reachable from M.

**Result.** 16 tips are debris-only. 0 tips contain a unique useful test or behavior that is absent from M. No SHA must be preserved.

Inspected debris is rejected below. These objects are not rejected:

- `ae993f77e79ae9d8be4e495433a54668a91140a6` (`feat(hooks): enforce fmt and Clippy pre-commit via hk`) is reachable from the stash-shaped tips and is not an ancestor of M. The same parent, `d0d4ee09810eb1817f87a908580dc85fa161ae8e`, is on M, and M contains the landed commit `799deeef907a890771f868a348e30192ecc2560a` (`feat(hooks): enforce fmt and Clippy pre-commit via hk (#1015)`). Every test function name in that commit's `clippy_affected/tests.rs` and `affected_crates/tests.rs` is present on M; M has eight additional test names in those files. Discarding the tips does not drop those tests.
- Stash index and untracked parents are listed with the tip that reaches them. For B004, this note does not close parent-tree review; another pass owns those trees. Their inspected first-parent or root deltas are recorded so they are not treated as unseen.
- Conflict text in B003 names `2990df17e25f30afca84804d9c402abc1ce00231`. That object is not in this repository and is not a parent of the tip. B008's subject names `e57f46b`, which is also absent. Neither is an ancestor to reject.

## Identical patch groups

| Group | Outcome | Tips | Lost test if discarded |
| --- | --- | --- | --- |
| A | Identical docs-only PROBE-A comment. Same parent `ebf5d5cdcc0639611de138b5e1fb5b5d540f6804`, same tree `f78a18218864f6b9ef60ce150db6bc4271fc9442`, same patch-id `14f296d6b9b15eff5004cf358b8e50ccecf796ec`. | B006, B023 | none |
| B | Identical one-line brand comment, patch-id `cca2185b1086fade24d91b1999cc84f273ce2185`. B011 and B017 share tree `d2550d1ccc139cf3b7f0359fa09bb8748fdf7aa4`. B007 and B019 share tree `5f2a40317c6ea9ba91d996b6b333b3cee5bc1ffd`. B016 has the same file patch on a different base, tree `090f597eab22e2b1c1489cda71c88ebbadcfb325`. | B007, B011, B016, B017, B019 | none |
| C | Same brand comment plus a trailing blank line on `Cargo.lock`. | B022 | none |
| D | Distinct `hook_proof*` insertions in `crates/jackin-xtask/src/main.rs` on top of `ae993f77`. None of these symbols exist on M. | B004, B009, B013, B014, B018, B024 | none |
| E | Orphan root whose only blob is malformed `crates/jackin-brand/src/untracked_bad.rs`. | B008 | none |
| F | Synthetic unresolved conflict snapshot. Not buildable. Comparison parent only. | B003 | none (see B003) |

## Per tip

### B003 `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`

Synthetic conflict tree. One parent, `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, which is on M. The commit message says that parent is a comparison baseline, not recovered ancestry, and that the snapshot is not buildable.

Commits not on M:

| SHA | Subject | Path stat vs first parent |
| --- | --- | --- |
| `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` | WIP: preserve unresolved Jackin snapshot | 25 files, +9556 / −21. Largest paths: `crates/jackin-xtask/src/ci_evidence/tests.rs` (+2152), `validation.rs` (+1327), `ci_evidence.rs` (+1058), `github.rs` (+922), `ledger.rs` (+683), plus `crates/jackin-process/src/lib.rs` and `tests.rs`. |

`git diff --stat M TIP`: **569 files changed, 26066 insertions(+), 97092 deletions(-).**

Conflict markers are in `crates/jackin-process/src/lib.rs` and `crates/jackin-process/src/tests.rs`. Helpers that exist only in this tree (`run_unbounded_output`, `read_output`, `join_output_reader`, `run_limited_output`, `read_limited_output`) sit inside those conflict hunks.

`ci_evidence/tests.rs` has 56 `fn` names on both this tree and M; none are only on the tip. Process tests added before the conflict (`timeout_reaps_the_direct_child`, `bounded_timeout_covers_a_blocked_stdin_write`, `timeout_covers_stdin_writer_after_direct_child_exits`, `bounded_capture_accepts_exact_limit`, `bounded_capture_stops_oversized_stdout_and_stderr_promptly`, `bounded_capture_rejects_non_capture_routing`, `sync_spawn_rejects_unenforceable_output_limits`) are weaker duplicates of tests already on M: `timeout_covers_a_blocked_stdin_write_and_reaps_child`, `timeout_owns_descendants_after_direct_child_exits`, `bounded_capture_accepts_exact_independent_limits`, `oversized_streams_fail_promptly_and_reap_child`, and `limits_require_capture_and_exec_ownership`. `async_spawn_rejects_unenforceable_output_limits` is the conflict's incoming side and is already covered by `limits_require_capture_and_exec_ownership` on M.

`desktop_merge_and_scheduled_contracts_preserve_cadence_concurrency_and_gates` is a clean addition in `crates/jackin-xtask/src/ci/tests.rs` and is absent from M. M's `ci/tests.rs` records that this contract test was removed with the post-#1110 CI surface (`6c389d38e` deleted `desktop-merge.yml`, `desktop-scheduled.yml`, `ci-main.yml`, `ci-pr.yml`, and `ci-unit-swift.yml`). The surviving mise task-graph test on M is `desktop::tests::cadence_tasks_define_the_canonical_graph`. Discarding the tip does not drop that test.

**Classification:** proof/probe debris (non-buildable synthetic conflict tree). Reject the whole tree. Do not preserve `fea3ade`.

### B004 `0a898cd246fbbf62f3b463f0097ffd6a368b1e07`

Stash-shaped commit (`On (no branch): hk`), three parents. Commit result only; parent trees are listed and left open.

Commits not on M:

| SHA | Subject | Path stat vs first parent or root |
| --- | --- | --- |
| `ae993f77e79ae9d8be4e495433a54668a91140a6` | feat(hooks): enforce fmt and Clippy pre-commit via hk | 10 files, +1747 / −31: `.cargo/config.toml`, `CONTRIBUTING.md`, `affected_crates.rs`, `affected_crates/tests.rs`, `clippy_affected.rs`, `clippy_affected/tests.rs`, `main.rs`, `hk.pkl`, `mise.lock`, `mise.toml` |
| `6ce0178e3fe9c64e5511b25e21c83e9b7404f392` | untracked files on (no branch): ae993f77 … | empty tree `4b825dc642cb6eb9a060e54bf8d69288fbee4904`, 0 files |
| `315752debc7aa391bc291bfb720fabba546dd2bd` | index on (no branch): ae993f77 … | `crates/jackin-xtask/src/main.rs`, +1 |
| `0a898cd246fbbf62f3b463f0097ffd6a368b1e07` | On (no branch): hk | `crates/jackin-xtask/src/main.rs`, +3 |

`git diff --stat M TIP`: **999 files changed, 32263 insertions(+), 235942 deletions(-).**

The commit result adds `let _hook_proof = 41 + 1;` and a deliberately unformatted `hook_proof_dirty_unstaged`. The index parent's inspected delta is only the assignment. Neither symbol is a test, and neither exists on M.

**Classification:** proof/probe debris for the commit result. Not rejected here: `ae993f77` (already on M as `799deeef`) and the index/untracked parents (other review owns those trees). No test lost.

### B006 `1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d`

Group A. Parent `ebf5d5cd` is on M.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d` | probe: stale-green commit A (docs-only, DO NOT MERGE) | `plans/cicd-reliability/FAILURE_LEDGER.md`, +2 |

`git diff --stat M TIP`: **775 files changed, 37933 insertions(+), 174764 deletions(-).**

The unique delta is an HTML comment labeled PROBE-A that says not to merge. Same tree and patch as B023.

**Classification:** proof/probe debris. No test lost.

### B007 `5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e`

Group B. Parents below the probe commits are on M (`c90bf147381962fa668c6c6ff24cbb95bb9a47a9`).

| SHA | Subject | Path stat |
| --- | --- | --- |
| `663e982111923bbce7bdedf523f7aa0dd571c56d` | ❌ PROBE b8-A brand comment — DO NOT MERGE | `crates/jackin-brand/src/lib.rs`, +1 |
| `5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e` | ❌ PROBE b8-B empty commit — DO NOT MERGE | empty; tree equals `663e982` |

`git diff --stat M TIP`: **774 files changed, 37887 insertions(+), 174472 deletions(-).**

The comment is `// PROBE b8 cache marker A — comment-only, no behavior change.` The tip adds no blob. Result tree matches B019.

**Classification:** proof/probe debris. No test lost.

### B008 `5a8264746286a981debb099646a6354d9b10e0cb`

Group E. Parentless root. Subject names `e57f46b`, which is not an object here.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `5a8264746286a981debb099646a6354d9b10e0cb` | untracked files on (no branch): e57f46b test: partial staging byte-identical | `crates/jackin-brand/src/untracked_bad.rs`, +3 |

`git diff --stat M TIP`: **2820 files changed, 3 insertions(+), 717911 deletions(-).**

The only file is a malformed `fn oops` with a `todo!`. It is not a `#[test]`, and `untracked_bad` does not occur on M.

**Classification:** proof/probe debris. No test lost. `e57f46b` is not rejected.

### B009 `5c4aa5eba5aaea2f2bfa2339f91066567a8e9979`

Group D. Not a stash.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `ae993f77e79ae9d8be4e495433a54668a91140a6` | feat(hooks): enforce fmt and Clippy pre-commit via hk | 10 files, +1747 / −31 (same as B004) |
| `5c4aa5eba5aaea2f2bfa2339f91066567a8e9979` | proof: unstaged defect must not block | `crates/jackin-xtask/src/main.rs`, +1 |

`git diff --stat M TIP`: **999 files changed, 32261 insertions(+), 235942 deletions(-).**

The tip's own line is `let _hook_proof = 41 + 1;`. No function and no test.

**Classification:** proof/probe debris. `ae993f77` is superseded on M, not rejected as debris. No test lost.

### B011 `07beb1c705ee8f813d2c8f5a438291cc031c9d1a`

Group B. Parent `799f57749c6ed8fbfd726a1bf25150c44c26bb5a` is on M.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `07beb1c705ee8f813d2c8f5a438291cc031c9d1a` | ❌ PROBE b8-A brand comment — DO NOT MERGE | `crates/jackin-brand/src/lib.rs`, +1 |

`git diff --stat M TIP`: **772 files changed, 38430 insertions(+), 173927 deletions(-).**

Same comment patch and same result tree as B017.

**Classification:** proof/probe debris. No test lost.

### B013 `256b88d1a7e85b4855d1e64399e2af4482026e17`

Group D. Stash-shaped.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `ae993f77e79ae9d8be4e495433a54668a91140a6` | feat(hooks): enforce fmt and Clippy pre-commit via hk | 10 files, +1747 / −31 |
| `cc37aa609785d7f41c733c8a90b99ebd4be728b6` | untracked files on (no branch): ae993f77 … | empty tree, 0 files |
| `69a8375845165b3978164ee73968d6016f09721a` | index on (no branch): ae993f77 … | no diff vs `ae993f77`; trees match `9d968a762be4580a4bc14f4712224a9e1a241f2e` |
| `256b88d1a7e85b4855d1e64399e2af4482026e17` | On (no branch): hk | `crates/jackin-xtask/src/main.rs`, +2 |

`git diff --stat M TIP`: **999 files changed, 32262 insertions(+), 235942 deletions(-).**

The commit result adds only the unformatted `hook_proof_dirty_unstaged`. The index has no staged delta.

**Classification:** proof/probe debris. `ae993f77` superseded on M. No test lost.

### B014 `631d734b99ee9b413230ddb210698a5c80f41d70`

Group D. Stash-shaped.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `ae993f77e79ae9d8be4e495433a54668a91140a6` | feat(hooks): enforce fmt and Clippy pre-commit via hk | 10 files, +1747 / −31 |
| `db54d4432c25779f71a0b9f6c61095ea8cd92b6c` | untracked files on (no branch): ae993f77 … | empty tree, 0 files |
| `51b0714b9dae27af69bcfa705c4199d59d4861e8` | index on (no branch): ae993f77 … | `crates/jackin-xtask/src/main.rs`, +6 |
| `631d734b99ee9b413230ddb210698a5c80f41d70` | On (no branch): hk | `crates/jackin-xtask/src/main.rs`, +8 |

`git diff --stat M TIP`: **999 files changed, 32268 insertions(+), 235942 deletions(-).**

The commit result adds one `hook_proof_clean` plus `hook_proof_dirty_unstaged`. The index delta is the clean function only. One definition of `hook_proof_clean` is valid Rust and still has no product behavior; it is not the duplicate-symbol case.

**Classification:** proof/probe debris. `ae993f77` superseded on M. No test lost.

### B016 `66227a0033a6f8b917e684de1776753e33978590`

Group B. Base below the probe commits, `82ff0593a8364195667563ecae5af453d2f3c873`, is on M.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `b9c8be174cfdedce17d263777d6d5865888ac025` | ❌ PROBE b8-A brand comment — DO NOT MERGE | `crates/jackin-brand/src/lib.rs`, +1 |
| `66227a0033a6f8b917e684de1776753e33978590` | ❌ PROBE b8-B empty commit — DO NOT MERGE | empty; tree equals `b9c8be17` |

`git diff --stat M TIP`: **771 files changed, 38430 insertions(+), 173860 deletions(-).**

Same brand-comment patch-id as the rest of group B. Different result tree because the base differs.

**Classification:** proof/probe debris. No test lost.

### B017 `77508d454bfe9a903043586db2c8c5faa1f9e960`

Group B. Merge of the B007 probe chain into `799f57749c6ed8fbfd726a1bf25150c44c26bb5a` (on M).

| SHA | Subject | Path stat vs first parent |
| --- | --- | --- |
| `663e982111923bbce7bdedf523f7aa0dd571c56d` | ❌ PROBE b8-A brand comment — DO NOT MERGE | `crates/jackin-brand/src/lib.rs`, +1 |
| `5fa968476cd0e95e5442964876365ea426b283f3` | ❌ PROBE b8-B empty commit — DO NOT MERGE | empty; tree equals `663e982` |
| `77508d454bfe9a903043586db2c8c5faa1f9e960` | Merge `5fa96847` into `799f5774` | `crates/jackin-brand/src/lib.rs`, +1 |

`git diff --stat M TIP`: **772 files changed, 38430 insertions(+), 173927 deletions(-).**

Result tree equals B011. The merge adds only the cache-marker comment.

**Classification:** proof/probe debris. No test lost.

### B018 `a28ab0a27eccc892decf8a4df55244b70096f631`

Group D. Stash-shaped.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `ae993f77e79ae9d8be4e495433a54668a91140a6` | feat(hooks): enforce fmt and Clippy pre-commit via hk | 10 files, +1747 / −31 |
| `483ae73cb45888c3f45225f18f0d875c4fa57b15` | untracked files on (no branch): ae993f77 … | empty tree, 0 files |
| `99f1c36e14d21fab6a190274e38f04f97bb9659a` | index on (no branch): ae993f77 … | `crates/jackin-xtask/src/main.rs`, +8 / −1 |
| `a28ab0a27eccc892decf8a4df55244b70096f631` | On (no branch): hk | `crates/jackin-xtask/src/main.rs`, +10 / −1 |

`git diff --stat M TIP`: **999 files changed, 32270 insertions(+), 235943 deletions(-).**

The commit result reformats `main` to `fn  main(  )` and adds `hook_proof_badly_formatted`, one `hook_proof_clean`, and `hook_proof_dirty_unstaged`. The index delta is the malformed `main`, the badly formatted function, and one clean function. Still a single `hook_proof_clean`.

**Classification:** proof/probe debris. `ae993f77` superseded on M. No test lost.

### B019 `b930e3658c671ca21ab7c25ea1a40349bb69284a`

Group B. Merge into `c90bf147381962fa668c6c6ff24cbb95bb9a47a9` (on M).

| SHA | Subject | Path stat vs first parent |
| --- | --- | --- |
| `0bcfc8426fdd0d8ef09248804a414e872fac5949` | ❌ PROBE b8-A brand comment — DO NOT MERGE | `crates/jackin-brand/src/lib.rs`, +1 |
| `503c4d260e724168ae48f7be98d5a278f6fc3eaf` | ❌ PROBE b8-B empty commit — DO NOT MERGE | empty; tree equals `0bcfc842` |
| `b930e3658c671ca21ab7c25ea1a40349bb69284a` | Merge `503c4d26` into `c90bf147` | `crates/jackin-brand/src/lib.rs`, +1 |

`git diff --stat M TIP`: **774 files changed, 37887 insertions(+), 174472 deletions(-).**

Result tree equals B007. Same comment-only outcome.

**Classification:** proof/probe debris. No test lost.

### B022 `df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb`

Group C. Merge into `87521a953823d7fa448fc5fc9343896762f94ec1` (on M).

| SHA | Subject | Path stat vs first parent |
| --- | --- | --- |
| `d0f676ca1dd09c9516b2cfb80bd5723da926fc61` | ❌ PROBE b8-A brand comment — DO NOT MERGE | `crates/jackin-brand/src/lib.rs`, +1 |
| `f75b82327d1f65848e6d5d0c59ed9d1ae4aee97c` | ❌ PROBE b8-B empty commit — DO NOT MERGE | empty; tree equals `d0f676ca` |
| `ee817c09ba9700c2f01266a4480e66fc7f93fd73` | ❌ PROBE b8-C Cargo.lock newline touch — DO NOT MERGE | `Cargo.lock`, +1 (trailing blank line) |
| `df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb` | Merge `ee817c09` into `87521a95` | `Cargo.lock` +1 and `crates/jackin-brand/src/lib.rs` +1 |

`git diff --stat M TIP`: **771 files changed, 38430 insertions(+), 173859 deletions(-).**

Brand comment patch-id matches group B. The extra blob is a lockfile newline, not a dependency change and not a test.

**Classification:** proof/probe debris. No test lost.

### B023 `e978dda9643079998f342380ccd123562139858a`

Group A. Same parent, tree, and patch-id as B006. Commit timestamps differ (18:49:25 vs 18:50:03 +0700).

| SHA | Subject | Path stat |
| --- | --- | --- |
| `e978dda9643079998f342380ccd123562139858a` | probe: stale-green commit A (docs-only, DO NOT MERGE) | `plans/cicd-reliability/FAILURE_LEDGER.md`, +2 |

`git diff --stat M TIP`: **775 files changed, 37933 insertions(+), 174764 deletions(-).**

**Classification:** proof/probe debris. Identical outcome to B006. No test lost.

### B024 `ffe6d6767bf2be30f54df7d245574b5980c05eac`

Group D. Stash-shaped. This is the duplicate-symbol tip.

| SHA | Subject | Path stat |
| --- | --- | --- |
| `ae993f77e79ae9d8be4e495433a54668a91140a6` | feat(hooks): enforce fmt and Clippy pre-commit via hk | 10 files, +1747 / −31 |
| `ca46bba6d3c3548e5286721db9d8e289bdf26c15` | untracked files on (no branch): ae993f77 … | empty tree, 0 files |
| `ccd3ba955dca0b15d673f6b39b7650b7ec3ece18` | index on (no branch): ae993f77 … | `crates/jackin-xtask/src/main.rs`, +14 / −1 |
| `ffe6d6767bf2be30f54df7d245574b5980c05eac` | On (no branch): hk | `crates/jackin-xtask/src/main.rs`, +16 / −1 |

`git diff --stat M TIP`: **999 files changed, 32276 insertions(+), 235943 deletions(-).**

The commit result reformats `main`, adds `hook_proof_badly_formatted` and `hook_proof_dirty_unstaged`, and defines `hook_proof_clean` twice (the second copy also has `#[allow(dead_code)]`). The index delta already contains both `hook_proof_clean` definitions. That duplicate item is not valid Rust and is not a test. The symbol is absent from M.

**Classification:** duplicate invalid hook symbol. Reject the inspected `main.rs` payload. `ae993f77` stays superseded on M, not rejected as debris. No test lost.

## Counts

- Tips inspected: 16.
- Debris-only: 16. Fifteen are proof or probe debris. B024 `ffe6d6767bf2be30f54df7d245574b5980c05eac` is the duplicate invalid hook symbol, which is still not a useful test.
- Tips with a unique useful test or behavior absent from M: 0.
- SHAs that must be preserved: none.

Coordinator reconciliation: B004 parent trees were left open here and are classified in `task-002-stash-parents.md` as hook-proof probe payloads on base `ae993f77e79ae9d8be4e495433a54668a91140a6`. That base is not debris; its tests are already on main. A separate challenge of the stash byte-for-byte claims is still in progress. This reject covers the inspected probe deltas only.

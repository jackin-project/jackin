# TASK-002: Stash-shaped commits and their parents

Read-only inspection of `/Users/donbeave/Projects/jackin-project/jackin`. No refs were moved and no worktree file was edited. Names and status only; no file bodies, credentials, or PTY bytes.

Comparison tips:

- main `1c3dd1be4a3b1ed518133ec3899c084c792370ff`
- policy tip P `fba69d60c2373156dc0dcecee8e085c83776966f` (`refs/remotes/origin/rust-policy-rollout`, also `refs/remotes/origin/ci-tooling/pr1121-ledger-454-20261010`)
- synthetic aid `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` (not a parent of any stash below)

Collector `commit_metadata` in `/Users/donbeave/Projects/jackin-project/jackin-evidence-20261013/inventory.json` records 8 commits with 3 parents. A scan of every commit reachable from `refs/heads` and `refs/remotes/origin` (81 refs, 1888 commits) finds those same 8 and no other 3-parent commit. Stash-shaped subjects (`WIP on`, `On …`, `index on`, `untracked files on`) add the index commits, the untracked commits, and two untracked-only tips. None of these tips are `refs/heads`; every tip is `refs/remotes/origin`.

## How roles were assigned

Git stash layout, confirmed from subjects and trees:

| Parent slot | Role | What the tree is |
| --- | --- | --- |
| 1 | base | HEAD at stash time. Ordinary subject. |
| 2 | index | Staged snapshot. Subject `index on …`. Its only parent is the base. |
| 3 | untracked | Untracked snapshot. Subject `untracked files on …`. No parents. |

The WIP / `On …` commit tree is the tracked worktree, not the untracked files. An empty third-parent tree is `4b825dc642cb6eb9a060e54bf8d69288fbee4904`. Diffing the WIP commit against that empty tree lists the whole tracked tree as added. That listing is the WIP tree, not an untracked payload. The untracked payload is the third commit's own tree.

## Object count

30 distinct commits in the stash closure:

- 8 three-parent stash tips
- 8 index parents
- 8 untracked parents of those tips
- 4 distinct base commits
- 2 extra untracked-only tips (`5a826474…`, `c7b8a958…`)

`fea3ade` is a 31st object read only as a comparison aid.

Alias refs are not extra objects. `80ab98038e945ec48c82dad241ebf33aa0efa82d` and `b1b90781739333ba7c8829c092a845f6ea82b11c` each have 15 `refs/remotes/origin` names under `rescue/local/20261011-110605-42b7822b/`. Every keeper root below has one origin ref.

## Verdict

No stash payload contains behavior that is missing from both main and P.

| Payload | Class | Versus main and P |
| --- | --- | --- |
| Five `hk` hook-proof stashes on `ae993f77` | Probe only | `hook_proof_*` markers are on neither tip |
| `5a826474` `untracked_bad.rs` | Probe only | Path is on neither tip |
| `6d65e649` clippy affected index | Real lock-selection change | `clippy_affected.rs` and its tests are byte-identical to main. P has the same union helpers; the four-line drift is path comments and `crates/tools/jackin-lints` |
| `80ab9803` coordination extraction | Real runtime lock/namespace move | Extracted sources match P byte for byte. Main still has the functions in `crates/jackin-runtime/src/runtime/coordination.rs`. The package name is absent on main |
| `b1b90781` repo-cache extraction | Real runtime cache move | Extracted sources match P byte for byte. Main still has the functions in `crates/jackin-runtime/src/runtime/repo_cache.rs`. The package name is absent on main |
| `c7b8a958` untracked CI evidence | Earlier CI-evidence draft | Same module names differ from main and P. Main and P already contain the later collector (`output`, `producer`, workflow contract, API paging). Not a lock, cache, usage, or runtime change |

`fea3ade` separately contains bounded process-output helpers (`read_limited_output`, `run_limited_output`, `join_output_reader`) that are absent on main and on P. That commit is not stash ancestry.

---

## Family A — hook-proof probes on `ae993f77`

Shared base for `0a898cd`, `256b88d1`, `631d734b`, `a28ab0a2`, and `ffe6d676`.

### Base `ae993f77e79ae9d8be4e495433a54668a91140a6`

- Subject: `feat(hooks): enforce fmt and Clippy pre-commit via hk`
- Tree: `9d968a762be4580a4bc14f4712224a9e1a241f2e`
- Parent: `d0d4ee09810eb1817f87a908580dc85fa161ae8e` (ordinary history, not a stash parent role)
- Own delta versus that parent:

| Status | Path |
| --- | --- |
| M | `.cargo/config.toml` |
| M | `CONTRIBUTING.md` |
| M | `crates/jackin-xtask/src/affected_crates.rs` |
| M | `crates/jackin-xtask/src/affected_crates/tests.rs` |
| A | `crates/jackin-xtask/src/clippy_affected.rs` |
| A | `crates/jackin-xtask/src/clippy_affected/tests.rs` |
| M | `crates/jackin-xtask/src/main.rs` |
| A | `hk.pkl` |
| M | `mise.lock` |
| M | `mise.toml` |

This commit introduces the hk pre-commit hook. It does not contain the `hook_proof_*` markers. Those markers are only in the stash index and WIP trees below. Classification of this base: real hook wiring, already history, not probe debris.

### Shared shape of the five WIP commits

Each WIP commit's only tracked delta versus the base is `M crates/jackin-xtask/src/main.rs`. Each third parent is an empty tree, so versus the untracked parent the name-status is 2535 paths, all `A`, matching the WIP tree (top levels: `crates` 1647, `docs` 601, `native` 149, `plans` 26, `.github` 21, and smaller roots). That is not untracked content.

The `main.rs` edits are dead markers: a local `_hook_proof` in `main`, and functions `hook_proof_clean`, `hook_proof_badly_formatted`, and `hook_proof_dirty_unstaged`. They do not change lock, cache, usage, or runtime behavior. None of those symbols exist on main or P.

### `0a898cd246fbbf62f3b463f0097ffd6a368b1e07`

Keeper root. Subject: `On (no branch): hk`. Tree: `245c306e70fdbf20f6ad43c3f215d6ec0ebfbeea`. Committer time `2026-09-21T05:56:01+07:00`. One origin keeper ref.

| Role | SHA | Subject | Delta of this WIP versus that parent |
| --- | --- | --- | --- |
| base | `ae993f77e79ae9d8be4e495433a54668a91140a6` | `feat(hooks): enforce fmt and Clippy pre-commit via hk` | `M crates/jackin-xtask/src/main.rs` |
| index | `315752debc7aa391bc291bfb720fabba546dd2bd` | `index on (no branch): ae993f77 feat(hooks): enforce fmt and Clippy pre-commit via hk` | `M crates/jackin-xtask/src/main.rs` |
| untracked | `6ce0178e3fe9c64e5511b25e21c83e9b7404f392` | `untracked files on (no branch): ae993f77 feat(hooks): enforce fmt and Clippy pre-commit via hk` | empty tree; 2535 `A` (whole WIP tree) |

Index tree `589d9182b1c6e5e40d5456540a9b88f3fb2b5d83` differs from the base. Index versus base is only `M crates/jackin-xtask/src/main.rs`: staged `let _hook_proof = 41 + 1;` inside `main`. The WIP tree adds the unstaged `hook_proof_dirty_unstaged` function. Probe only.

Index object parents: base `ae993f77…` only. Untracked object parents: none. Untracked versus root: no files.

### `256b88d1a7e85b4855d1e64399e2af4482026e17`

Subject: `On (no branch): hk`. Tree: `46044e6cab0e915ad89fa6d75a0308a418ae83cc`. `2026-09-21T05:55:51+07:00`.

| Role | SHA | Delta of WIP versus parent |
| --- | --- | --- |
| base | `ae993f77e79ae9d8be4e495433a54668a91140a6` | `M crates/jackin-xtask/src/main.rs` |
| index | `69a8375845165b3978164ee73968d6016f09721a` | `M crates/jackin-xtask/src/main.rs` |
| untracked | `cc37aa609785d7f41c733c8a90b99ebd4be728b6` | empty tree; 2535 `A` |

Index tree equals the base tree. Index versus base: no files. The whole payload is unstaged `hook_proof_dirty_unstaged`. Probe only. Untracked versus root: no files.

### `631d734b99ee9b413230ddb210698a5c80f41d70`

Subject: `On (no branch): hk`. Tree: `0a106faaad00f89fd44794eee5cb0b7ec4eb2cc4`. `2026-09-21T05:55:22+07:00`.

| Role | SHA | Delta of WIP versus parent |
| --- | --- | --- |
| base | `ae993f77e79ae9d8be4e495433a54668a91140a6` | `M crates/jackin-xtask/src/main.rs` |
| index | `51b0714b9dae27af69bcfa705c4199d59d4861e8` | `M crates/jackin-xtask/src/main.rs` |
| untracked | `db54d4432c25779f71a0b9f6c61095ea8cd92b6c` | empty tree; 2535 `A` |

Index tree `3b767c7116e02245d8f0efe78ab71076fb691847`. Index versus base: `M crates/jackin-xtask/src/main.rs` with a formatted `hook_proof_clean` function. WIP adds unstaged `hook_proof_dirty_unstaged`. Probe only. Untracked versus root: no files.

### `a28ab0a27eccc892decf8a4df55244b70096f631`

Subject: `On (no branch): hk`. Tree: `1b7cc6290bf217a82b0f70d9d2a08b181372a2e7`. `2026-09-21T05:54:42+07:00`.

| Role | SHA | Delta of WIP versus parent |
| --- | --- | --- |
| base | `ae993f77e79ae9d8be4e495433a54668a91140a6` | `M crates/jackin-xtask/src/main.rs` |
| index | `99f1c36e14d21fab6a190274e38f04f97bb9659a` | `M crates/jackin-xtask/src/main.rs` |
| untracked | `483ae73cb45888c3f45225f18f0d875c4fa57b15` | empty tree; 2535 `A` |

Index tree `1db0c294d87870018ad7389fbc93719d7e25a591`. Index versus base stages a badly formatted `main` signature, `hook_proof_badly_formatted`, and `hook_proof_clean`. WIP adds unstaged `hook_proof_dirty_unstaged`. Probe only. Untracked versus root: no files.

### `ffe6d6767bf2be30f54df7d245574b5980c05eac`

Subject: `On (no branch): hk`. Tree: `04a69175159e2feda4ef4784d37214850e87e457`. `2026-09-21T05:55:00+07:00`.

| Role | SHA | Delta of WIP versus parent |
| --- | --- | --- |
| base | `ae993f77e79ae9d8be4e495433a54668a91140a6` | `M crates/jackin-xtask/src/main.rs` |
| index | `ccd3ba955dca0b15d673f6b39b7650b7ec3ece18` | `M crates/jackin-xtask/src/main.rs` |
| untracked | `ca46bba6d3c3548e5286721db9d8e289bdf26c15` | empty tree; 2535 `A` |

Index tree `5fa4b0926b1430b78b72ed915cdfa45706059130`. Index versus base stages the badly formatted `main`, `hook_proof_badly_formatted`, and two copies of `hook_proof_clean` (one with `allow(dead_code)`). WIP adds unstaged `hook_proof_dirty_unstaged`. Probe only. Untracked versus root: no files.

---

## Family B — clippy affected index on `65e9dfbd`

### Base `65e9dfbdba1066c29fa3a3ea88a7930cb833152b`

- Subject: `docs(ci): probe C completes §14 bullet 1 (#1029)`
- Tree: `bcbf211d6682fad291d093257246d8300214cf25`
- Parent: `ebf5d5cdcc0639611de138b5e1fb5b5d540f6804`
- Own delta: `M plans/cicd-reliability/EXECUTION.md`

Docs-only probe checklist edit. Not the clippy behavior below.

### Index `0624d32807427aca808eecfa812233154c9a517b`

- Subject: `index on fix/clippy-affected-index: 65e9dfbd docs(ci): probe C completes §14 bullet 1 (#1029)`
- Tree: `bcbf211d6682fad291d093257246d8300214cf25` (same as base)
- Parent: `65e9dfbdba1066c29fa3a3ea88a7930cb833152b`
- Delta versus base: no files

### Untracked `9dcdd23211facd8fbf82f9031b1e9e6dc4b3595c`

- Subject: `untracked files on fix/clippy-affected-index: 65e9dfbd docs(ci): probe C completes §14 bullet 1 (#1029)`
- Tree: empty
- Parents: none
- Delta versus root: no files

### WIP `6d65e649855f98f133f58f605727b098b5d4eabb`

Subject: `On fix/clippy-affected-index: hk`. Tree: `cc23204fc79d60dec4b48a986f10023c009137ea`. `2026-09-21T19:11:16+07:00`. One origin keeper ref.

| Role | SHA | Delta of WIP versus parent |
| --- | --- | --- |
| base | `65e9dfbdba1066c29fa3a3ea88a7930cb833152b` | `M crates/jackin-xtask/src/clippy_affected.rs`, `M crates/jackin-xtask/src/clippy_affected/tests.rs` |
| index | `0624d32807427aca808eecfa812233154c9a517b` | same two paths (index tree equals base) |
| untracked | `9dcdd23211facd8fbf82f9031b1e9e6dc4b3595c` | empty tree; 2612 `A` (whole WIP tree: `crates` 1718, `docs` 603, `native` 149, `plans` 31, and smaller roots) |

The subject says `hk`, but the trees do not contain hook-proof markers. The unstaged payload changes lock and workspace-manifest selection:

- `worktree_lock_packages` and `worktree_workspace_dependencies` read HEAD, the index blob (`git show :path`), and the worktree.
- `union_lock_changes` unions HEAD↔index and index↔worktree package diffs. An unparseable pair widens to the full workspace (`None`).
- `union_workspace_dependency_changes` does the same for workspace dependencies. A staged-only structural edit widens even when the worktree matches HEAD.
- Tests cover comment-only diffs, staged-only lock bumps, merged pairs, and unparseable input.

Classification: real lock/affected-index behavior. The stash message is the probe series' `hk`, and the base commit is a probe checklist doc. The code payload is the selection change.

Versus main: both files are byte-identical (`clippy_affected.rs` `badfd24764c81fff810feff5a8cf85f2b7bf54be`, tests `9f6994cff19d56c394ee72da17872aa4b01dd63d`).

Versus P: same helpers live at `crates/tools/jackin-xtask/src/clippy_affected.rs` and `…/tests/case_02.rs`. The implementation diff is four lines: hook command text (`mise run` versus `cargo xtask`) and nested-package paths (`crates/jackin-lints` versus `crates/tools/jackin-lints`, `crates/*/fuzz` versus `fuzz/*`). Not a unique selection rule.

---

## Family C — runtime coordination stash

### Base `b017883d18e38c15e759c62dbaf75663f4cabc46`

- Subject: `S7 split 43: runtime-host-daemon extraction (M5)`
- Tree: `ddba95a6daf864974dcf4ce9d9eb41ae76efb624`
- Parent: `ea5ea5af44f60b4ebcf670393b61348f205e3f50`
- Own delta versus that parent (the host-daemon split this stash sits on, not the stash payload):

| Status | Path |
| --- | --- |
| M | `Cargo.lock` |
| M | `Cargo.toml` |
| A | `crates/services/jackin-runtime-host-daemon/Cargo.toml` |
| A | `crates/services/jackin-runtime-host-daemon/README.md` |
| A | `crates/services/jackin-runtime-host-daemon/src/host_daemon.rs` |
| R100 | `crates/services/jackin-runtime/src/host_daemon/tests.rs` → `crates/services/jackin-runtime-host-daemon/src/host_daemon/tests.rs` |
| R100 | `…/host_daemon/tests/case_01.rs` → `…/jackin-runtime-host-daemon/src/host_daemon/tests/case_01.rs` |
| R100 | `…/host_daemon/tests/case_02.rs` → `…/jackin-runtime-host-daemon/src/host_daemon/tests/case_02.rs` |
| R100 | `…/host_daemon/tests/support.rs` → `…/jackin-runtime-host-daemon/src/host_daemon/tests/support.rs` |
| A | `crates/services/jackin-runtime-host-daemon/src/lib.rs` |
| A | `crates/services/jackin-runtime-host-daemon/tests/main.rs` |
| M | `crates/services/jackin-runtime/Cargo.toml` |
| M | `crates/services/jackin-runtime/src/host_daemon.rs` |
| M | `crates/tools/jackin-xtask/src/arch.rs` |
| M | `docs/content/development/rust-policy-rollout.mdx` |
| M | `policy/owned-packages.json` |
| M | `policy/package-groups.json` |
| M | `ratchet.toml` |

P has `crates/services/jackin-runtime-host-daemon/` (9 files). Main does not. This base is policy-rollout history, not unique stash residue.

### Index `23c10555ae46ae61675382cba5a2afb37d1c82a2`

- Subject: `index on rust-policy-rollout: b017883d S7 split 43: runtime-host-daemon extraction (M5)`
- Tree: `cbaaac7c1f85ed0b91c81f1b74ba78efe6b7eb5c`
- Parent: `b017883d18e38c15e759c62dbaf75663f4cabc46`
- Delta versus base: pure renames, 0 lines

| Status | Path |
| --- | --- |
| R100 | `crates/services/jackin-runtime/src/runtime/coordination.rs` → `crates/services/jackin-runtime-coordination/src/coordination.rs` |
| R100 | `…/runtime/coordination/tests.rs` → `…/jackin-runtime-coordination/src/coordination/tests.rs` |
| R100 | `…/runtime/coordination/tests/case_01.rs` → `…/coordination/tests/case_01.rs` |
| R100 | `…/runtime/coordination/tests/support.rs` → `…/coordination/tests/support.rs` |

### Untracked `b9d87372e3d04764877c688024b4af97ec9e7a39`

- Subject: `untracked files on rust-policy-rollout: b017883d S7 split 43: runtime-host-daemon extraction (M5)`
- Tree: `2e57a3fba1fb96c21a89ccfff56d8da6715251c9`
- Parents: none
- Delta versus root:

| Status | Path |
| --- | --- |
| A | `crates/services/jackin-runtime-coordination/Cargo.toml` |
| A | `crates/services/jackin-runtime-coordination/README.md` |
| A | `crates/services/jackin-runtime-coordination/src/lib.rs` |
| A | `crates/services/jackin-runtime-coordination/tests/main.rs` |
| A | `crates/services/jackin-runtime/src/runtime/coordination.rs` |

The shim `runtime/coordination.rs` is the same blob in the WIP tree (`164a27ce7a35`). The other four paths are untracked-only; they are absent from the WIP tracked tree. Those four blobs match P.

### WIP `80ab98038e945ec48c82dad241ebf33aa0efa82d`

Subject: `WIP on rust-policy-rollout: b017883d S7 split 43: runtime-host-daemon extraction (M5)`. Tree: `5ca3f740e5f28923d359ba974d074d8f0ec970ed`. `2026-10-07T17:19:14+07:00`. 15 origin alias refs, 0 heads.

| Role | SHA |
| --- | --- |
| base | `b017883d18e38c15e759c62dbaf75663f4cabc46` |
| index | `23c10555ae46ae61675382cba5a2afb37d1c82a2` |
| untracked | `b9d87372e3d04764877c688024b4af97ec9e7a39` |

Versus base (tracked worktree, including the staged rename):

| Status | Path |
| --- | --- |
| M | `Cargo.lock` |
| M | `Cargo.toml` |
| A | `crates/services/jackin-runtime-coordination/src/coordination.rs` |
| R100 | `…/runtime/coordination/tests.rs` → `…/jackin-runtime-coordination/src/coordination/tests.rs` |
| R100 | `…/tests/case_01.rs` → `…/coordination/tests/case_01.rs` |
| R082 | `…/tests/support.rs` → `…/coordination/tests/support.rs` |
| M | `crates/services/jackin-runtime/Cargo.toml` |
| M | `crates/services/jackin-runtime/src/runtime/coordination.rs` |
| M | `crates/tools/jackin-xtask/src/arch.rs` |
| M | `policy/owned-packages.json` |
| M | `policy/package-groups.json` |
| M | `ratchet.toml` |

Versus index (unstaged tracked edits after the pure rename):

| Status | Path |
| --- | --- |
| M | `Cargo.lock` |
| M | `Cargo.toml` |
| M | `crates/services/jackin-runtime-coordination/src/coordination.rs` |
| M | `crates/services/jackin-runtime-coordination/src/coordination/tests/support.rs` |
| M | `crates/services/jackin-runtime/Cargo.toml` |
| A | `crates/services/jackin-runtime/src/runtime/coordination.rs` |
| M | `crates/tools/jackin-xtask/src/arch.rs` |
| M | `policy/owned-packages.json` |
| M | `policy/package-groups.json` |
| M | `ratchet.toml` |

Behavior of that unstaged edit: namespace and lock helpers (`root`, `ensure_prunable`, `ensure_prunable_async`, `universe_dir`, `open_lock`, `open_in_namespace`, `open_directory_in_namespace`, `open_state_in_namespace`) change from `pub(crate)` to `pub`. The runtime path becomes a re-export shim (`pub(crate) use jackin_runtime_coordination::coordination::*`). `arch.rs` registers tier `("jackin-runtime-coordination", 1)`. Policy JSON and `ratchet.toml` gain that package and its README bound. The test support file drops a local `expect(clippy::unwrap_used)` and relies on `allow-unwrap-in-tests`.

Versus the untracked parent, name-status is 4343 paths (`A` 4339, `D` 2, `R053` 1, `R060` 1). That size is the tracked tree compared with a five-file tree. The two `D` paths are the untracked-only `README.md` and `src/lib.rs`. The rename hits are similarity matches of the untracked `tests/main.rs` and `Cargo.toml` onto unrelated tracked files (`jackin-docker` tests, `jackin-usage-coordinator` Cargo.toml). They are not moves of those crates.

Classification: real runtime lock/namespace extraction. Combined tracked WIP plus untracked crate scaffolding is byte-identical to P for `coordination.rs`, tests, support, `lib.rs`, `Cargo.toml`, `README.md`, `tests/main.rs`, and the runtime shim. P also has the package registration. Main still has `fn open_lock`, `fn universe_dir`, and `fn ensure_prunable` in `crates/jackin-runtime/src/runtime/coordination.rs` and has zero hits for `jackin-runtime-coordination`. Main's monolithic file is larger because its tests stay inline; the stash ends that file with `#[cfg(test)] mod tests;`. No lock rule in the stash is missing from both tips.

---

## Family D — runtime repo-cache stash

### Base `894ea46b709076ccc263dfb4576af03fb966e721`

- Subject: `S7 split 47: runtime-session-control extraction (M9)`
- Tree: `394beeea175c9fba346a0e6d7cc3ca47a514f0cc`
- Parent: `1ac4d32dfb0ec20522d5e0984a0712ae16cf288a`
- Own delta versus that parent:

| Status | Path |
| --- | --- |
| M | `Cargo.lock` |
| M | `Cargo.toml` |
| A | `crates/services/jackin-runtime-session-control/Cargo.toml` |
| A | `crates/services/jackin-runtime-session-control/README.md` |
| A | `crates/services/jackin-runtime-session-control/src/lib.rs` |
| A | `crates/services/jackin-runtime-session-control/src/session_control.rs` |
| R100 | `crates/services/jackin-runtime/src/runtime/session_control/tests.rs` → `…/jackin-runtime-session-control/src/session_control/tests.rs` |
| R100 | `…/tests/case_01.rs` → `…/session_control/tests/case_01.rs` |
| R100 | `…/tests/support.rs` → `…/session_control/tests/support.rs` |
| A | `crates/services/jackin-runtime-session-control/tests/main.rs` |
| M | `crates/services/jackin-runtime/Cargo.toml` |
| M | `crates/services/jackin-runtime/src/runtime/session_control.rs` |
| M | `crates/tools/jackin-xtask/src/arch.rs` |
| M | `docs/content/development/rust-policy-rollout.mdx` |
| M | `policy/owned-packages.json` |
| M | `policy/package-groups.json` |
| M | `ratchet.toml` |

P has `crates/services/jackin-runtime-session-control/` (8 files). Main does not. Base history, not stash residue.

### Index `509b9beee860360985f2e78525bf6bc376206127`

- Subject: `index on rust-policy-rollout: 894ea46b S7 split 47: runtime-session-control extraction (M9)`
- Tree: `284e88c26a9de7bd2bda7b2f5d24f3bd807a273e`
- Parent: `894ea46b709076ccc263dfb4576af03fb966e721`
- Delta versus base: pure renames, 0 lines

| Status | Path |
| --- | --- |
| R100 | `crates/services/jackin-runtime/src/runtime/repo_cache.rs` → `crates/services/jackin-runtime-repo-cache/src/repo_cache.rs` |
| R100 | `…/repo_cache/tests.rs` → `…/jackin-runtime-repo-cache/src/repo_cache/tests.rs` |
| R100 | `…/tests/case_01.rs` → `…/repo_cache/tests/case_01.rs` |
| R100 | `…/tests/case_02.rs` → `…/repo_cache/tests/case_02.rs` |
| R100 | `…/tests/case_03.rs` → `…/repo_cache/tests/case_03.rs` |

### Untracked `98ecbd16307a999d5a07f349386b998b20d88998`

- Subject: `untracked files on rust-policy-rollout: 894ea46b S7 split 47: runtime-session-control extraction (M9)`
- Tree: `d17fb49851a67daf01ed67231e5f9ea88a49aeb0`
- Parents: none
- Delta versus root:

| Status | Path |
| --- | --- |
| A | `crates/services/jackin-runtime-repo-cache/Cargo.toml` |
| A | `crates/services/jackin-runtime-repo-cache/README.md` |
| A | `crates/services/jackin-runtime-repo-cache/src/lib.rs` |
| A | `crates/services/jackin-runtime-repo-cache/tests/main.rs` |
| A | `crates/services/jackin-runtime/src/runtime/repo_cache.rs` |

The shim matches the WIP blob (`f3a2881fc337`). The other four match P and are absent from the WIP tracked tree.

### WIP `b1b90781739333ba7c8829c092a845f6ea82b11c`

Subject: `WIP on rust-policy-rollout: 894ea46b S7 split 47: runtime-session-control extraction (M9)`. Tree: `3b8d3617f725390b29b18207d86596811bd7b7f2`. `2026-10-07T17:45:35+07:00`. 15 origin alias refs, 0 heads.

| Role | SHA |
| --- | --- |
| base | `894ea46b709076ccc263dfb4576af03fb966e721` |
| index | `509b9beee860360985f2e78525bf6bc376206127` |
| untracked | `98ecbd16307a999d5a07f349386b998b20d88998` |

Versus base:

| Status | Path |
| --- | --- |
| M | `Cargo.lock` |
| M | `Cargo.toml` |
| A | `crates/services/jackin-runtime-repo-cache/src/repo_cache.rs` |
| R100 | `…/runtime/repo_cache/tests.rs` → `…/jackin-runtime-repo-cache/src/repo_cache/tests.rs` |
| R100 | `…/tests/case_01.rs` → `…/repo_cache/tests/case_01.rs` |
| R100 | `…/tests/case_02.rs` → `…/repo_cache/tests/case_02.rs` |
| R100 | `…/tests/case_03.rs` → `…/repo_cache/tests/case_03.rs` |
| M | `crates/services/jackin-runtime/Cargo.toml` |
| M | `crates/services/jackin-runtime/src/runtime/repo_cache.rs` |
| M | `crates/tools/jackin-xtask/src/arch.rs` |
| M | `policy/owned-packages.json` |
| M | `policy/package-groups.json` |
| M | `ratchet.toml` |

Versus index:

| Status | Path |
| --- | --- |
| M | `Cargo.lock` |
| M | `Cargo.toml` |
| M | `crates/services/jackin-runtime-repo-cache/src/repo_cache.rs` |
| M | `crates/services/jackin-runtime/Cargo.toml` |
| A | `crates/services/jackin-runtime/src/runtime/repo_cache.rs` |
| M | `crates/tools/jackin-xtask/src/arch.rs` |
| M | `policy/owned-packages.json` |
| M | `policy/package-groups.json` |
| M | `ratchet.toml` |

Behavior of the unstaged edit: repo resolution helpers become public (`parse_repo_name`, `repo_matches`, `git_branch`, `RepoResolveOptions` and its builders, `resolve_agent_repo_with`). Imports move from `crate::` / `super::` to `jackin_instance::runtime_slug` and `jackin_runtime_identity::identity::try_capture`. `GitInteractivity` and `RepoResolveOptions` gain `Debug`. Builders gain `must_use`. The old path becomes `pub use jackin_runtime_repo_cache::repo_cache::*`. `arch.rs` registers tier `("jackin-runtime-repo-cache", 4)`. Policy JSON and `ratchet.toml` gain the package and its README bound.

Versus the untracked parent: 4363 paths (`A` 4359, `D` 2, `R053` 1, `R055` 1). Same sparse-tree effect. `D` is untracked-only `README.md` and `src/lib.rs`. The rename hits pair untracked `tests/main.rs` and `Cargo.toml` with unrelated tracked files (`jackin-docker` tests, `jackin-runtime-session-control` Cargo.toml).

Classification: real runtime repo-cache extraction. Source blobs match P (`repo_cache.rs`, three case tests, `tests.rs`, `lib.rs`, `Cargo.toml`, `README.md`, `tests/main.rs`, shim). Main still has `parse_repo_name` and `resolve_agent_repo_with` in `crates/jackin-runtime/src/runtime/repo_cache.rs` and has zero hits for `jackin-runtime-repo-cache`. The main-versus-stash diff is visibility, derives, `must_use`, and import paths, not a second cache algorithm. No cache rule in the stash is missing from both tips.

---

## Other stash-shaped tips

### `5a8264746286a981debb099646a6354d9b10e0cb`

- Subject: `untracked files on (no branch): e57f46b test: partial staging byte-identical`
- Tree: `5630e0d127924911a89357abd4292fd26f31d47c`
- Parents: none
- `2026-09-21T05:14:53+07:00`. One origin keeper ref.
- Delta versus root: `A crates/jackin-brand/src/untracked_bad.rs`

The file is a badly formatted `todo!` stub named as untracked proof. The subject names `e57f46b`, but that commit is not a parent. Probe only. Absent on main and P.

### `c7b8a95856cce6f5e8442f0e6fafcf025a089132`

- Subject: `untracked files on codex/ci-evidence-repair-current-main-20260923: a65f5d75c fix(usage): route Claude API keys away from OAuth adapter (#1092)`
- Tree: `d80503236a7b61a8aa2f21dae164693cbf1ed9d1`
- Parents: none
- `2026-09-23T07:44:20+08:00`. One origin keeper ref.
- The subject names `a65f5d75c`; that commit is not a parent.

Delta versus root:

| Status | Path |
| --- | --- |
| A | `.github/workflows/ci-evidence.yml` |
| A | `.github/workflows/ci-push-head-ledger.yml` |
| A | `crates/jackin-xtask/src/ci_evidence.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/github.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/ledger.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/rollup.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/storage.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/tests.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/validation.rs` |

Workflows are the CI evidence and push-head ledger jobs (about 40 lines each). They are not present as `.github/workflows/` on main or P. Main and P keep related copies as fixtures under the xtask `ci_evidence/fixtures/` tree. The workflow text is a few lines off those fixtures.

The Rust modules are an earlier draft of the same collector. Function names present here and absent as those names on main: `normalize_name`, `read_push_head_artifact`, `require_qualified`, `validate_live_api_binding`, `validate_qualification_binding`. Main (and P, under `crates/tools/jackin-xtask`) instead has the later surface: `output`, `producer`, API paging, workflow-contract checks, and `read_push_head_zip` / `render_markdown`. Several sibling blobs that match between main and P (`github.rs`, `rollup.rs`, `storage.rs`) do not match this tree.

Classification: superseded CI-evidence snapshot, not probe-marker debris, and not a lock, cache, usage, or runtime behavior change. The usage commit in the subject is context for the branch name, not a parent and not a file in this tree.

---

## Synthetic aid `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`

Not a parent of any stash above. One parent: `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. Subject: `WIP: preserve unresolved Jackin snapshot`. Tree: `abc5b11ca1b2aa0699947d54539cb291691189e4`. `2026-10-06T03:43:38+02:00`. Origin ref: `refs/remotes/origin/archive/cleanup-20261006/jackin-origin/recovery/cleanup-20261006/jackin-unreferenced-tree-abc5b11ca1b2/tip-fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`.

Delta versus its parent (25 paths):

| Status | Path |
| --- | --- |
| A | `.github-gen/velnor-workflow.toml` |
| A | `.github/ci/.github-actions-generator-state` |
| A | `.github/ci/project.toml` |
| A | `.github/workflows/ci-evidence.yml` |
| A | `.github/workflows/ci-push-head-ledger.yml` |
| A | `.xcode-version` |
| M | `Cargo.lock` |
| M | `Cargo.toml` |
| M | `crates/jackin-process/Cargo.toml` |
| M | `crates/jackin-process/src/lib.rs` |
| M | `crates/jackin-process/src/tests.rs` |
| M | `crates/jackin-xtask/Cargo.toml` |
| M | `crates/jackin-xtask/src/ci/tests.rs` |
| A | `crates/jackin-xtask/src/ci_evidence.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/github.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/ledger.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/output.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/producer.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/rollup.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/storage.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/tests.rs` |
| A | `crates/jackin-xtask/src/ci_evidence/validation.rs` |
| M | `crates/jackin-xtask/src/cmd.rs` |
| M | `crates/jackin-xtask/src/main.rs` |
| M | `mise.toml` |

Relative to the stashes: this tree includes `ci_evidence/output.rs` and `producer.rs`, which `c7b8a958` does not. Its `ci_evidence.rs` blob equals main (`54b8496715ef`); `c7b8a958`'s does not. Its workflow files differ from `c7b8a958` by a handful of lines. `ci-evidence.yml` in this commit matches main's fixture blob.

`crates/jackin-process/src/lib.rs` here is not main's blob. Functions present here and absent on both main and P: `join_output_reader`, `read_limited_output`, `read_output`, `run_limited_output`, `run_unbounded_output`. `kill_and_reap` is already on main (`crates/jackin-process/src/lib.rs`) and on P (`crates/core/jackin-process/src/lib.rs`). Tests present only in this snapshot include the bounded-capture and timeout-reap cases. That output-limit behavior is in this synthetic snapshot only. It is not recovered from the stash parents.

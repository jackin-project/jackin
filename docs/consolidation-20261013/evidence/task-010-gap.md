# TASK-010 gap audit

Read-only. Product code was not edited. `cargo` and `mbx` were not run. This audit does not approve TASK-010.

## Trees

| Tree | Path | HEAD |
| --- | --- | --- |
| Integration | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `738e2cfa95731b8ee59e705f52e38c802c12a3a8` |
| Policy P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| Main M | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |

History reads used `/Users/donbeave/Projects/jackin-project/jackin`.

`git diff fba69d60 HEAD` for `jackin-runtime-coordination`, `jackin-runtime-repo-cache`, and `jackin-runtime-cleanup-absent-guards` is empty (`diff --raw` line count 0). Directory compare of those three leaves is identical. Line numbers below are the integration tree; they are the same lines in P.

Source blobs:

- `coordination.rs` `b8ff4fd91bdc6b31eb63935837a0fd9d25b4de88` at P, integration, `80ab98038e`, `bff77099`, and `45e83468`.
- Coordination tests `case_01.rs` `d8f140f2db5c9ac06740daae073e2161fcfa8de3` at P and `80ab98038e`.
- `absent_guards.rs` `b3d4e1ae2619145e3a8e03bc33b9e58178bcd553` at P, integration, `bff77099`, and `45e83468`.
- `repo_cache.rs` `287d5b95dda2df749d90a6eeecce6ff6a1b22f89` at P, integration, `bff77099`, and `45e83468`.

Stash `80ab98038e` parents: `b017883d` (HEAD), `23c10555` (index), `b9d87372` (untracked). Untracked `Cargo.toml` and `src/lib.rs` for `jackin-runtime-coordination` match P. The stash tip does not contain the repo-cache crate. Parent `b017883d:crates/services/jackin-runtime/src/runtime/repo_cache.rs` differs from the P leaf only by import paths and `pub` versus `pub(super)`. `repo_matches` is unchanged. M has the same `repo_matches` body at `crates/jackin-runtime/src/runtime/repo_cache.rs:146`.

## Verdicts

### Binding

| Item | Verdict | Where |
| --- | --- | --- |
| `lease_path_matches_open_file` keeps `nlink != 0`, both regular files, path not a symlink, device and inode match | ALREADY_PRESENT | `crates/services/jackin-usage-broker/src/leader.rs:237-250` |
| Cleanup `path_matches_file` delegates to that predicate | ALREADY_PRESENT | `crates/services/jackin-usage-broker/src/leader.rs:252-254` |
| Do not purge active or owned resources | ALREADY_PRESENT | `crates/services/jackin-runtime-cleanup-absent-guards/src/absent_guards.rs:32-49`; held and idle coordination inodes stay at `crates/services/jackin-runtime/src/runtime/cleanup/tests/case_04.rs:145-182` |
| Stable coordination inodes | ALREADY_PRESENT | `crates/services/jackin-runtime-coordination/src/coordination.rs:141-150`, `:174`, `:306` |
| Lock ownership and reentrant last-owner release | ALREADY_PRESENT | `crates/services/jackin-runtime-coordination/src/coordination.rs:153-156`; `crates/services/jackin-runtime-repo-cache/src/repo_cache.rs:52-117` |
| Reject unsafe paths, symlinks, and owners | ALREADY_PRESENT | `coordination.rs:158-170`, `:213-252`, `:288-336` |
| Repository-cache identity matches the selected contract | ALREADY_PRESENT | `crates/services/jackin-runtime-repo-cache/src/repo_cache.rs:132-150` |
| Final purge owner is `jackin-runtime-cleanup-absent-guards` | ALREADY_PRESENT | `crates/services/jackin-runtime-cleanup-absent-guards/src/lib.rs:4`; `absent_guards.rs:16` |

The lease predicate is outside the three leaves. Integration already has it. P's `path_matches_file` at `leader.rs:238-244` checks symlink, device, and inode only. Copying that weaker predicate back is **REJECT**. TASK-010 does not edit `jackin-usage-broker`.

### Safety the task names

| Item | Verdict | Where |
| --- | --- | --- |
| Prunable-root overlap rejection | ALREADY_PRESENT | `coordination.rs:102-110` `reject_overlap`; `:116-123` `root`; `:128-129` `ensure_prunable`. Tests `coordination/tests/case_01.rs:220-270`. Callers `cleanup/purge.rs:76-80`, `cleanup/absent.rs:31`, `jackin-runtime-cleanup-prune-home/src/prune_home.rs:24` |
| Parent and symlink traversal | ALREADY_PRESENT | Missing `..` tails `coordination.rs:54-61`. Non-normalized components `:213-220`. `O_NOFOLLOW` `:199`, `:306`. Dangling symlink `:39-50`. Namespace symlink `:88-93`. Tests `case_01.rs:39-58`, `:142-172`, `:194-216`, `:256-269` |
| Unsafe ownership, permissions, and link counts | ALREADY_PRESENT | Parent uid and mode `coordination.rs:224-229`. Private directories `:247-252`. State uid, mode `0o077`, and `st_nlink != 1` `:288-299` and `:325-336`. Mode fixture `case_01.rs:274-291` |
| Non-regular inode substitution | ALREADY_PRESENT | `S_IFREG` `coordination.rs:290`. `is_file` `:327`. Pre/post `(st_dev, st_ino)` `:331`. FIFO fixture `case_01.rs:293-301`. Shared inode `case_01.rs:62-93` |
| Lock release does not unlink persistent coordination inodes | ALREADY_PRESENT | `coordination.rs:153-156` and open flags at `:306` (no `O_TRUNC`, no unlink in this crate). Reopen keeps the inode `case_01.rs:194-205`. `retained.lock` remains `case_01.rs:252`. Prune keeps held and idle inodes `cleanup/tests/case_04.rs:145-182`. Purge comment `cleanup/purge.rs:99` |
| `RepoLock` reentrant same-process acquisition | ALREADY_PRESENT | `repo_cache.rs:105-111` upgrades the live `Weak`. Test `repo_cache/tests/case_03.rs:69-79` |
| Per-path gating | ALREADY_PRESENT | `repo_cache.rs:77-103`. Test `case_03.rs:91-98` |
| Last-handle release | ALREADY_PRESENT | `Arc<File>` `repo_cache.rs:63-74`, `:115-117`. Last drop then a fresh acquire `case_03.rs:81-87` |
| `ensure_role_resources_absent_for_purge` remains the purge entry | ALREADY_PRESENT | `absent_guards.rs:16-24`. Hub re-export `cleanup/absent.rs:21`. Docker backend `runtime/backend.rs:75-81` |

`NotFound` is the only Docker state that allows purge. Running, paused, restarting, created, removing, dead, stopped, and `InspectUnavailable` refuse (`absent_guards.rs:32-49`). Apple purge uses the same refusal for a still-listed container at `runtime/apple_container.rs:387-402`, inlined in P and integration. The s61 body in `45e83468:crates/services/jackin-runtime-apple-container-ensure-absent/src/ensure_absent.rs` is that same check. Restoring the intermediate `jackin-runtime-apple-container-ensure-absent` crate is **REJECT**.

### Repository identity review

`parse_repo_name` (`repo_cache.rs:132-144`) keeps `owner/repo` and drops the host. `repo_matches` (`:146-150`) compares those strings, and falls back to trimmed full-string equality when parsing fails. The configured URL is passed through `normalize_github_url` (`:387-394`) before the match at `:584`. The cross-protocol fixture is `repo_cache/tests/case_01.rs:58-69` (GitHub HTTPS and `git@github.com`).

That comparison is the contract in P, in M, and in stash parent `b017883d`. Integration matches P. Same `owner/repo` on two hosts compares equal. CHG-013 records that as an identity question, not a confirmed exploit from inspection alone. There is no leaf drift to repair. A host-strict rewrite is not a missing behavior relative to P. **ALREADY_PRESENT**. No PORT.

### Blocking pool

`acquire_repo_lock` (`repo_cache.rs:120-128`) runs `acquire_repo_lock_blocking` on `joined_blocking`, which is `tokio::task::spawn_blocking` (`jackin-telemetry/src/spawn.rs:429-438`). **ALREADY_PRESENT**. A separate cancellation protocol is not present in P and is not a leaf gap.

## Not a product edit

Dedicated hard-link and foreign-uid fixtures are not in `case_01.rs`. The predicates are `st_nlink != 1` and `uid == euid` at `coordination.rs:291-293` and `:327-330`. P and integration already contain that behavior.

Fault-injection tests named by the task procedure (cancellation, interrupted purge, cross-host identity) are additions beyond the P contract. They are not a behavior missing from the integration leaves.

Required commands were not run:

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin-runtime-coordination -p jackin-runtime-repo-cache
mise exec -- mbx +1.99.0 nextest run -p jackin-runtime-cleanup-absent-guards
mise exec -- mbx +1.99.0 clippy -p jackin-runtime-coordination -p jackin-runtime-repo-cache --all-targets -- -D warnings
```

Status of those commands: the nextest command ran later at integration `ef5ca1df`. The three leaves were unchanged by that commit. Command: `env -u RUSTUP_TOOLCHAIN mise exec -- mbx +1.99.0 nextest run -p jackin-runtime-coordination -p jackin-runtime-repo-cache -p jackin-runtime-cleanup-absent-guards`. Exit 0. 38 passed, 0 skipped. `jackin-runtime-cleanup-absent-guards` has no `#[test]` in the leaf; its refusal is covered by hub cleanup tests named in this audit. Clippy for the two packages remains NOT_RUN. This note does not approve TASK-010.

## Edit list

Empty.

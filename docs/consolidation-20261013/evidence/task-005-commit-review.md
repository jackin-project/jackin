# TASK-005 commit review

Verdict: **ACCEPT** for `9e208685c8961f5fa1209d8d97daec7c16b4ad40` and `a1f0916c4764514ac4702bf1b27098ff8111e305` only.

This review does not approve the rest of `integration/branch-consolidation-20261013`, a push, or TASK-005 completion.

Independent read-only review. The reviewer did not author these commits. No files in the primary clone or the worktree were edited. Cargo was not run. Behavior in check 6 is from the committed source.

| Repo | Path | Role |
| --- | --- | --- |
| Primary clone | `/Users/donbeave/Projects/jackin-project/jackin` | Check 7 |
| Worktree | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | Commit objects |

Reviewed history:

```text
1c3dd1be4a3b1ed518133ec3899c084c792370ff  main
9e208685c8961f5fa1209d8d97daec7c16b4ad40  adopt policy layout snapshot from fba69d60
a1f0916c4764514ac4702bf1b27098ff8111e305  fix(usage): recheck lease identity and FFI output paths
```

## Checks

### 1. Parent of `9e208685` — pass

`9e208685c8961f5fa1209d8d97daec7c16b4ad40` has one parent: `1c3dd1be4a3b1ed518133ec3899c084c792370ff`.

`git rev-list --parents -n 1 9e208685` and `git rev-parse 9e208685^` both returned that hash. It is the same commit as `main` in both the worktree and the primary clone.

### 2. Tree of `9e208685` equals tree of `fba69d60` — pass

Both commits have tree `a6d4ea7c1ccddc4da0c7ecb9c12dddf239d2a9c9`.

`git diff-tree 9e208685c8961f5fa1209d8d97daec7c16b4ad40 fba69d60c2373156dc0dcecee8e085c83776966f` produced no output.

`fba69d60` itself has parent `885a63d68a0bc76d54a93fec231f63630d5429b8` and subject `docs: record Velnor d35 lock audit`.

### 3. Ancestry — pass

`fba69d60c2373156dc0dcecee8e085c83776966f` is not an ancestor of `9e208685`. `git merge-base --is-ancestor fba69d60 9e208685` exited 1. Their merge-base is `1c3dd1be4a3b1ed518133ec3899c084c792370ff`, which is `main`, not `fba69d60`.

`main` is an ancestor of `9e208685`. `git merge-base --is-ancestor main 9e208685` exited 0. `main` is the parent recorded in check 1.

### 4. `a1f0916c` file set — pass

Parent of `a1f0916c4764514ac4702bf1b27098ff8111e305` is `9e208685c8961f5fa1209d8d97daec7c16b4ad40`.

`git diff-tree -r --name-status` and `--numstat` show only these modifications:

| Path | Numstat |
| --- | --- |
| `crates/adapters/jackin-usage-ffi/boltffi.toml` | 2 insertions, 2 deletions |
| `crates/services/jackin-usage-broker/src/leader.rs` | 23 insertions, 13 deletions |

No other path is added, deleted, or renamed in that commit.

### 5. boltffi outputs — pass

In `a1f0916c`, `crates/adapters/jackin-usage-ffi/boltffi.toml` sets:

- line 27: `output = "../../../target/xcframework"`
- line 36: `output = "../../../native/Sources/JackinUsageBindings"`

The same file says boltffi is invoked with this crate as the working directory and that those output paths are relative to the crate directory (`crates/adapters/jackin-usage-ffi`). Three `..` segments resolve at the repository root. The parent snapshot still had `../../target/xcframework` and `../../native/Sources/JackinUsageBindings`.

### 6. Lease identity and unknown bytes — pass

Source is `a1f0916c:crates/services/jackin-usage-broker/src/leader.rs`. `BrokerLease` in `crates/services/jackin-usage-broker-wire/src/lease.rs` is a concrete serde struct (`instance_id`, `process_id`, `protocol_version`, `build_id`, `renewed_at_epoch`), not an untagged value that accepts a raw process id.

`lease_path_matches_open_file` (lines 237–250) returns true only when both metadata lookups succeed and all of these hold:

- open-file `nlink() != 0`
- open-file `is_file()`
- `symlink_metadata(path)` `is_file()`
- that path metadata is not a symlink
- path device equals open-file device
- path inode equals open-file inode

Failed metadata lookups return false. `nlink` is read from the open file. The path must name that same inode.

`claim_existing_lease` calls `lease_path_matches_open_file` at line 114 and returns `Ok(None)` on failure. `read_lease_bytes` is the next statement, at line 117.

Non-JSON bytes fail `serde_json::from_slice::<BrokerLease>`. The `else` branch at lines 131–134 sets `replace` to `false`. Lines 136–138 then return `Ok(None)` before `write_lease` at line 139. The previous dead-pid `kill` replacement is gone from this commit. Unknown non-JSON lease bytes are not replaced.

`path_matches_file` now calls the same helper. Its callers in this file pass the lease path. Socket removal still uses `is_socket()` in `unlink_owned_socket_path` (lines 222–234).

### 7. Primary clone — pass

At the start and end of this review, `/Users/donbeave/Projects/jackin-project/jackin` was:

- `HEAD` branch: `main`
- `HEAD` commit: `1c3dd1be4a3b1ed518133ec3899c084c792370ff`
- `git status --porcelain=v1 -uall`: empty

`git status -sb` at the start was `## main...origin/main` with no ahead/behind marker and no dirty paths.

## Outside this verdict

`9e208685` records remaining gaps in its message: the qualification gate, 34 dropped tests, minisign and hyperfine lock rows, and the candidate-specific CI failures measured on `fba69d60`. `a1f0916c` addresses the lease inode recheck and the boltffi relative outputs named in that message. Those other gaps stay open.

During this read, the worktree tip moved. The first status showed `HEAD` `a1f0916c4764514ac4702bf1b27098ff8111e305` and an unstaged change to `crates/core/jackin-protocol/src/attach.rs`. The last status showed a clean worktree at `04cd2b9fd4d15243b905c725fd2dc0e337bfc707`. Commits after the reviewed pair, not covered here:

- `5619ba88b0fe177c472962a111004eb6001f6b92` `fix(protocol): assert nonempty export bytes with assert_ne`
- `04cd2b9fd4d15243b905c725fd2dc0e337bfc707` `fix(usage): hide macOS Claude keychain helpers on Linux`

This review did not create those commits and does not accept them.

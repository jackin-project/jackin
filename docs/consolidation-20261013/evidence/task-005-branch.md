# TASK-005 branch

Created 2026-10-13. Primary clone `/Users/donbeave/Projects/jackin-project/jackin` stayed on `main` at `1c3dd1be4a3b1ed518133ec3899c084c792370ff`. Nothing was pushed.

| Item | Value |
| --- | --- |
| Worktree | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` |
| Branch | `integration/branch-consolidation-20261013` |
| `BASE_SHA` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Layout commit | `9e208685c8961f5fa1209d8d97daec7c16b4ad40` |
| Layout parent | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Layout tree | `a6d4ea7c1ccddc4da0c7ecb9c12dddf239d2a9c9` |
| Policy tip | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| Policy tree | `a6d4ea7c1ccddc4da0c7ecb9c12dddf239d2a9c9` |
| Follow-up commit | `a1f0916c4764514ac4702bf1b27098ff8111e305` |
| Clippy assert commit | `5619ba88b0fe177c472962a111004eb6001f6b92` |
| Linux keychain cfg commit | `04cd2b9fd4d15243b905c725fd2dc0e337bfc707` |
| Remote | `origin/integration/branch-consolidation-20261013` at `04cd2b9fd4d15243b905c725fd2dc0e337bfc707` |

The layout commit's tree matches the policy tip. The 366 policy commits are not ancestors. The branch was not created by merging `fba69d60`.

`a1f0916c` rewrites `crates/adapters/jackin-usage-ffi/boltffi.toml` outputs to `../../../target/xcframework` and `../../../native/Sources/JackinUsageBindings`, and ports `lease_path_matches_open_file` into `claim_existing_lease`. Unknown lease bytes are not replaced when they parse as a dead process id.

Still open on this branch: the qualification gate, the 34 dropped tests, minisign and hyperfine lock rows, and the candidate-specific CI failures measured on the policy tip. This file does not mark TASK-005 done.

# Project handoff — branch consolidation 2026-10-13

This file is the resume point for `integration/branch-consolidation-20261013`. It does not approve a merge to `main`.

## Status

The integration branch is a reviewable snapshot of the policy tree plus reviewed behavior ports. It is **not** a merge of the 366 policy commits and **not** a fast-forward of `main`. Protected `main` was not updated. No pull request was opened.

| Item | Value |
| --- | --- |
| Remote | `https://github.com/jackin-project/jackin.git` |
| Integration branch | `integration/branch-consolidation-20261013` |
| Integration tip at handoff start | `7a05f3aebda4a73e183b665702f00bb7a38ce36f` |
| Protected `main` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Policy candidate P | `fba69d60c2373156dc0dcecee8e085c83776966f` (`rust-policy-rollout`, `ci-tooling/pr1121-ledger-454-20261010`) |
| Usage repair U | `2e4a62da4bc4406764162a267163389a5fb513a7` (`fix/claude-usage-live-repair`) |
| Tooling T | `9ef617d014df15c6d967501bcbbdcb3689ed453c` (`chore/rust-mise-mbx-modernize`) |
| Layout | Policy tree `a6d4ea7c1ccddc4da0c7ecb9c12dddf239d2a9c9`, parent still `main` |

Overall progress: inventory and architecture source units are accepted. Behavior ports through the items below are on this branch. Integrated validation, the independent loss audit, and any protected squash merge are **not** done.

## Binding decisions

- Monitor schema stays **4**. Broker wire stays **v8**. Account state stays **3**. Do not stamp U's schema 6 or wire v9. A schema newer than 4 is refused and left on disk. A non-v8 broker header is a protocol mismatch, not a migration. Do not add `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION`.
- `should_exit_idle` stays the conjunction of elapsed idle, coordinator idle, no active monitor, and no authenticated foreground guard. Do not restore `ClaudeCollectorLiveness`.
- `ServiceStopped` and an authenticated serve exit clear `experimental_collector_source` before workers join.
- A negative Codex `used_percent` stays unset. It must not become 100% remaining or `0% used`. Zero remains an observed zero. Over-cap values clamp only the bar.
- `lease_path_matches_open_file` requires `nlink != 0`, both sides regular files, the path not a symlink, and equal device and inode. Do not port U v9 lease recovery.
- Related-role rebuild options copy the caller's model and effort. The launch pipeline still skips that arm when either value is set (`current_role_reuse_is_compatible`).
- Abandoned-lock reclaim syncs the parent directory after unlinking the lock and before unlinking the journal candidate.
- First CI ledger stays visible and unqualified. Green claims require a push-head boundary.
- Do not hand-edit `.github/workflows/ci.yml`. It is generated. Copying T's workflow would install older pins and flatten crate paths. Checked-in `ci.yml` still installs Rust 1.98.1, MBX 1.21.0, and Mise 2026.9.18. The toolchain files want Rust 1.99.0, MBX 1.23.0, and Mise at least 2026.10.7. Exact semantic cache keys are absent; do not delete `restore-keys` as a substitute. Warm one-minute CI was not measured.
- Do not restore global sccache, permissive WAL, untrimmed `load --model`, in-process broker fallback, probe debris, or the conflicted `fea3ade` tree.
- U live provider acceptance is unproved. Do not call live providers. Do not read Keychain or real credentials.
- In-process broker fallback stays deferred. Native CI/merge/scheduled stay blocked while `/Applications/Xcode_26.6.app` is absent. Installed Xcode on the last machine was 27.0. A run on Xcode 27 is not the pinned gate. Do not sign or notarize.
- Official coverage matrix `goal-references/02-change-coverage-matrix.md` was not rewritten. Do not mark OPEN rows superseded just to finish.

## Completed on this branch

Parent of the first integration commit is `main` at `1c3dd1be`. Then:

- `9e208685` policy layout snapshot from `fba69d60` (not a 366-commit merge)
- `a1f0916c` lease identity recheck and FFI output paths
- Protocol, Linux `mode_t`, Zeroize, Hyperfine 1.20.0, deny exceptions, XCFramework-before-xcodegen
- Restored usage tests, docs citations, TESTING.md paths, MBX-only compiler-cache text
- `86693632` first ledger visible and unqualified
- `544ae930` negative Codex percent left unset
- `738e2cfa` collector source cleared on foreground stop
- `ef5ca1df` model and effort kept on related restore options
- `d5dd92a8` schema 6 and wire v9 refusal tests
- `30e0c1d4` lock directory synced before the journal witness is dropped
- `7a05f3ae` chaos kill accepts an already-dead container

`jackin-isolation` nextest: 189 passed. Coordination and repo-cache nextest: 38 passed. `jackin-runtime-cleanup-absent-guards` has no tests inside the leaf. Focused broker and Codex tests for the ports above passed.

Linux capsule release zigbuild for `aarch64-unknown-linux-gnu.2.17` succeeded. That is a Linux binary, not a Docker end-to-end pass and not the macOS 26 gate.

## Unfinished

1. Docker end-to-end on the last host (macOS 27.0.1 build 26A434, OrbStack) is not the pinned macOS 26 + OrbStack gate. Full run: 24 tests, 17 passed, 7 failed. After `python:3.14-alpine` was present and `JACKIN_USAGE_BROKER_BIN` pointed at `target/debug/jackin-usage-broker`, five of those seven passed (`load_options_launch` and the four docker usage-broker capsule tests).
2. Still failing:
   - `jackin_load_agent_smith_can_reach_its_dind_daemon_with_proxy_env`: Capsule transport ACK failed with connection reset, then the Java Testcontainers smoke did not pass.
   - `usage_broker_killed_owner_recovers_once_without_a_herd`: child `wait_until` barrier timed out. Raising the recovery waits from 10s to 30s still failed and was reverted.
3. Generated `ci.yml` pins were not regenerated. `xtask ci --only policy` was not run on this head. One-minute warm CI was not measured.
4. Independent review of commits after `a1f0916c` is incomplete. Later fix review rejected some commits that were subsequently patched (`59049bc1`, `274665c0`). Commits from `86693632` through `7a05f3ae` still need a reviewer who did not write them.
5. Tasks 014, 016, 017, 018, and 020 are not closed. Task 019 is blocked: do not open or merge a pull request to `main` unless a later instruction specifically authorizes the protected squash workflow.
6. Local audit notes under the evidence directory named in "Local-only items" were not imported into this repository.

## Next steps

1. Clone this branch. Do not start from `main` if the goal is to continue the consolidation.
2. Re-read this file and confirm `git rev-parse HEAD` matches the pushed integration tip.
3. Diagnose the two remaining end-to-end failures before changing product code. Do not weaken assertions to hide a five-minute retry floor or a transport reset.
4. Regenerate `.github/workflows/ci.yml` only with the real Velnor Actions generator. If that command is not in the repo, leave the file unchanged and record BLOCKED.
5. Run `xtask ci --only policy` once, with no second Jackin cargo compile at the same time.
6. Have a different reviewer accept or reject `86693632..HEAD`.
7. Stop at this branch unless a squash pull request into protected `main` is explicitly authorized. Required remote checks include DCO and `Required`. Sign-off must be `Signed-off-by: Alexey Zhokhov <alexey@zhokhov.com>`.

## Environment

Last host: arm64, macOS ProductVersion 27.0.1, build 26A434, OrbStack. Rust override `RUSTUP_TOOLCHAIN=1.98.1` must be unset so `rust-toolchain.toml` selects 1.99.0. Mise on that host was 2026.10.8. `mr-boxington` 1.23.0. One heavy `cargo`/`mbx` compile at a time.

Commands use `mise exec -- mbx +1.99.0 ...` from the repo root. Prefix is illustrative; the checkout's own `mise` tasks are the source of truth.

```sh
git clone https://github.com/jackin-project/jackin.git
cd jackin
git fetch origin integration/branch-consolidation-20261013
git switch -c integration/branch-consolidation-20261013 --track origin/integration/branch-consolidation-20261013
env -u RUSTUP_TOOLCHAIN mise exec -- mbx +1.99.0 --version
```

Environment variable **names** only. Do not copy secret values into the shell history or into git.

- `RUSTUP_TOOLCHAIN` — unset it.
- `JACKIN_HOME_DIR` — unset during end-to-end tests. If set, `jackin load` uses that home and can stop on a re-clone prompt.
- `JACKIN_CAPSULE_BIN` — absolute path to the Linux `jackin-capsule` release binary when running docker end-to-end.
- `JACKIN_USAGE_BROKER_BIN` — absolute path to `target/debug/jackin-usage-broker` when a test process looks for a sibling broker under `target/debug/deps`.
- `JACKIN_E2E_CONSTRUCT_IMAGE` — optional override. Default used by the tests is `projectjackin/construct:trixie`.

Build the Linux capsule without writing the operator home:

```sh
env -u RUSTUP_TOOLCHAIN mise exec -- mbx zigbuild --profile release -p jackin-capsule --target aarch64-unknown-linux-gnu.2.17
```

Docker end-to-end, after that binary exists:

```sh
env -u RUSTUP_TOOLCHAIN -u JACKIN_HOME_DIR \
  JACKIN_CAPSULE_BIN="$PWD/target/aarch64-unknown-linux-gnu/release/jackin-capsule" \
  JACKIN_USAGE_BROKER_BIN="$PWD/target/debug/jackin-usage-broker" \
  mise exec -- mbx +1.99.0 nextest run -p jackin --features e2e --profile docker-e2e --locked --no-fail-fast
```

The image `python:3.14-alpine` must be present locally or the docker usage-broker capsule tests fail immediately. Build the broker bin before `load_options_launch`:

```sh
env -u RUSTUP_TOOLCHAIN mise exec -- mbx +1.99.0 build --bin jackin-usage-broker --bin jackin
```

## Local-only items

These were not pushed and must not be copied blindly:

- Planning overlay `/Users/donbeave/Projects/merge/jackin` is not a git checkout. Do not `git init` it.
- Audit notes and end-to-end homes under `/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013/`. Homes may contain local state. Do not commit them.
- Detached baseline worktrees `wt-baseline-M`, `wt-baseline-P`, `wt-baseline-U`, and `wt-baseline-T` only pin the SHAs above. They have no unique commits.
- `target/` build output, including the Linux capsule binary. Rebuild it on the next machine.
- No stashes were present on the primary clone at handoff.

## Safety

Do not force-push. Do not rewrite history. Do not update git config. Do not commit `.env`, keychain exports, or credential files. Do not call live usage providers during resume.

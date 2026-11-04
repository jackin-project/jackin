# Tests that fail only inside a git hook

Branch: `wt/hook-env`, from `47fd9447f`. Commits: `821b61852` (test helpers and two tool git calls), `df685e46d` (hk.pkl scans).

## Symptom

The `pre-push` hook runs `mise run ci:fast`. Inside it, `jackin-openshell::gateway contract::a_gateway_that_finds_a_missing_file_and_fails_later_fails_the_size_check` and `contract::a_leftover_profile_and_provider_from_an_aborted_run_do_not_fail_the_core_group` stop at `crates/jackin-openshell/tests/gateway/contract.rs:43` with `NotAGitWorkTree(HostPath(".../T/.tmpXXXX"))`. Run outside a hook they pass.

## Which variables leaked

The hook environment was captured with the same hook configuration as the clone (`hook.hk-pre-push.command = mise x -- hk run pre-push --from-hook`), hk 2.11.0, git 2.54.0, a step that writes `env`.

| Push from | `GIT_` variables in the hook |
|---|---|
| main checkout | `GIT_EXEC_PATH`, `GIT_PREFIX` (empty, or `sub/` from a subdirectory) |
| linked worktree, plain script hook | `GIT_DIR` (`<common>/.git/worktrees/<name>`), `GIT_EXEC_PATH`, `GIT_PREFIX` |
| linked worktree, hk hook | the same three, plus `GIT_WORK_TREE` (the worktree; hk adds it) |

The failure needs a linked worktree. `GIT_EDITOR=true` also showed up; it came from the shell that ran the probe, not from the hook.

Each variable was then set alone, in front of the 17 `contract::` tests of `jackin-openshell` (a decoy linked worktree in the scratchpad, narrow nextest run):

| Set alone | Result |
|---|---|
| none | 17 passed |
| `GIT_DIR` | 17 failed. `git init <tmp>` re-initialises `GIT_DIR` and creates nothing in `<tmp>`: `NotAGitWorkTree` at `contract.rs:43` |
| `GIT_WORK_TREE` | 17 failed. `git init` exits non-zero without `GIT_DIR`: `assertion failed: status.success()` at `contract.rs:37` |
| `GIT_PREFIX` | 17 passed |
| `GIT_EXEC_PATH` | 17 passed |

The hook sets both `GIT_DIR` and `GIT_WORK_TREE`, so it hits line 43. `GIT_INDEX_FILE` is set by git for a partial `git commit` (pre-commit). It was not observed in a pre-push hook.

## Root cause

A test helper ran `git` in a temporary directory and left `GIT_DIR` and `GIT_WORK_TREE` in the child environment. Git trusts those two variables over the current directory, so every such call acted on the pushing repository. First case: `crates/jackin-openshell/tests/gateway/contract.rs:30`, `git init <tmp>`.

The fast gate stops at the first failures, so a hook run shows two or four tests. With `--no-fail-fast` and the hook environment set, the whole suite had 118 failures out of 1210: `jackin-openshell::gateway`, `jackin-testkit::gateway transfer` and `contract`, `jackin-xtask::repo_gates` and `jackin-xtask::e2e`. They are `NotAGitWorkTree`, `git init` and `git add -A` exit 128, `could not set 'core.repositoryformatversion'`, and `index.lock` collisions between parallel tests.

The tests also wrote into the pushing repository. In the decoy repository the run created `refs/heads/jackin/jackin-main/main` and similar refs (the export tests), and a hook run that failed on the two tests left `core.worktree` in the pusher's `.git/config`:

```text
> core.worktree=<scratch>/gate/wt-pre
```

## Why the structure allowed it

Eleven call sites in test code spawned `git` and removed nothing (they set only `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_NOSYSTEM`, or nothing). The testkit `fixtures.rs` helper and the product `host_git` removed the three names `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE`, which is not what D08 says: `GIT_PREFIX`, `GIT_COMMON_DIR`, `GIT_OBJECT_DIRECTORY`, `GIT_CONFIG_COUNT` and the rest stayed in the child. Other helpers cleared the whole environment. There was no single owner of "a git process in a test", so each helper made its own choice. Cargo and nextest hand a test the environment of the hook, so a test is hermetic only if each spawn makes it so.

Two non-test calls had the same shape:

- `crates/jackin-openshell/src/contract.rs` `host_git` (product code, the `contract` feature) removed three names. This is a D08 breach: the other `GIT_` variables reached git. It now removes all of them.
- `crates/xtask/src/repo.rs` `read_files` ran `git ls-files` for an explicit root and inherited `GIT_DIR`. xtask is repository tooling, not jackin product, so D08 does not govern it, but its tests pass a scratch root, so it cannot be hermetic without this. It now removes all `GIT_` variables.

The jackin product paths were already correct: the process runner removes the `GIT_` prefix (`crates/jackin-session/src/project/git.rs:91`, `version.rs:98`, `crates/jackin-session/src/host.rs` `command_for`).

## Fix

One builder: `jackin_testkit::git_command()` in `crates/jackin-testkit/src/git.rs`. It starts `/usr/bin/git`, removes every environment variable that starts with `GIT_`, then sets `GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_NOSYSTEM=1`.

| File | Change |
|---|---|
| `crates/jackin-testkit/src/git.rs` (new), `lib.rs` | the builder |
| `crates/jackin-testkit/src/fixtures.rs` | `git()` uses the builder (it removed three names) |
| `crates/jackin-testkit/tests/gateway/support.rs` | `git()` uses the builder |
| `crates/jackin-openshell/tests/gateway/contract.rs` | the `git init` of `fixture()` |
| `crates/jackin-session/tests/plan/roles.rs` | the `git update-index` call |
| `crates/jackin-core/tests/ids.rs` | `git_accepts` |
| `crates/xtask/tests/repo_gates.rs` | `Fixture::git` |
| `crates/xtask/tests/e2e/scenario.rs`, `g_cleanup.rs`, `scenario/host.rs`, `scenario/report.rs` (two), `e_scratch.rs` | the scenario git calls |
| `crates/jackin-openshell/src/contract.rs` | `host_git` removes every `GIT_` variable |
| `crates/xtask/src/repo.rs` | `read_files` removes every `GIT_` variable |
| `crates/jackin-core/Cargo.toml`, `crates/jackin-openshell/Cargo.toml`, `Cargo.lock` | dev-dependency on `jackin-testkit` (allowed by the `LAYERS` rows) |

Already hermetic by `env_clear` and an explicit allowlist, left as they are: `jackin-openshell/tests/gateway/transfer.rs` (two), `jackin-session/tests/plan/roles_origin.rs`, `project/oracle.rs`, `support/trap.rs`, `export/thin.rs`, `jackin-testkit/tests/gateway/transfer.rs`, `jackin-testkit/src/exec.rs` (`HostProgram::command`), and `xtask` `RealHost` (`env_clear` plus the `KEEP` list).

### The task layer does not scrub

`ci:fast`, `xtask ci` and the nextest config do not remove `GIT_` variables, on purpose. A scrub there would hide the next unhardened helper from the only place that shows it (a hook), and would not protect a developer who runs `nextest` directly or from an editor. nextest cannot remove a variable; it can only set one.

## The two scans in hk.pkl

### Pre-push: scan the pushed commits

Before: `betterleaks git . --redact` scanned every local ref, so an unpushed local branch with an old fixture leak failed any push. After: `betterleaks git . --redact --log-opts="HEAD --not --remotes"`, the commits reachable from `HEAD` that no remote-tracking ref holds.

hk 2.11.0 reads the pre-push stdin itself and gives its steps neither the ref lines nor an environment variable (probe: a step with `interactive = true` read an empty stdin, and `env` held no `HK_` or ref variable). The step cannot get the `<local sha> <remote sha>` pairs, so the range is `HEAD`.

Limit: a push of a ref other than `HEAD` (`git push origin other`, `--all`, `--tags`) is not scanned by this step. CI's secrets scan covers `HEAD`, and `hk run security-audit` scans all history. Closing the gap needs the stdin: a native `pre-push` hook (not hk) that passes the lines to `betterleaks`.

Fail-closed: a range that cannot be resolved fails. `--log-opts="no-such-ref --not --remotes"` exits 1. An empty range scans 0 bytes and passes.

### Pre-commit: scan the index

Before: `gitleaks dir .` (the hk builtin) read the working tree with everything untracked or ignored in it. After:

```text
tree=$(mktemp -d) && trap 'rm -rf "$tree"' EXIT && git checkout-index --all --prefix="$tree/" && cd "$tree" && gitleaks dir --redact --verbose --no-banner .
```

The scan covers the tracked files at their staged content (the index, which includes the staged change). `target/` and untracked build output are never read. A file under `target/` that is tracked, by `git add -f`, is in the index, so it is still found. The scan runs with the exported tree as the current directory, so the path rules in `.gitleaks.toml` match the same relative paths as before.

Finding on the stated cause: in a linked worktree where `target` is a symlink to the mbx cache, `gitleaks dir .` 8.30.1 did not read it. A planted token behind the symlink was not found, with or without `--follow-symlinks`. The failure reproduces with a real `target/` directory, as in the main clone, and the index scan fixes that case.

## Evidence

Scratch only: a scratch bare remote and scratch clones under the session scratchpad. Nothing was pushed to a real remote.

Hook run before the fix (`pre` at `47fd9447f`, pushed from a linked worktree):

```text
FAIL jackin-openshell::gateway contract::a_gateway_that_finds_a_missing_file_and_fails_later_fails_the_size_check
FAIL jackin-openshell::gateway contract::a_leftover_profile_and_provider_from_an_aborted_run_do_not_fail_the_core_group
Summary 369/1210 tests run: 367 passed, 2 failed
error: failed to push some refs                      push exit 1
```

Hook run after the fix (`post` at `df685e46d`, same setup): `betterleaks-history` passed, `xtask-ci` ran fmt, clippy, nextest (`1210 tests run: 1210 passed`), deny, shear, comments, layers, pins, containers, errors and reuse, `push exit 0`, and the remote holds `refs/heads/post df685e46`. The pusher's refs and config were unchanged except the remote-tracking ref of the push.

Whole suite with the hook environment set by hand (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_PREFIX`, `GIT_EXEC_PATH` pointing at a decoy linked worktree), `nextest run --workspace --all-features --locked --no-fail-fast`:

| | Result |
|---|---|
| before | 1210 tests run: 1092 passed, 118 failed |
| after | 1210 tests run: 1210 passed; the decoy repository's refs, config, HEAD and status hash is identical before and after |

Normal run: `mise exec -- mbx +1.99.1 xtask ci --fast` exits 0 (1210 passed).

Pre-push scan, scratch remote, hook with `HK_SKIP_STEPS=xtask-ci` (the xtask step is proved above):

| Case | Result |
|---|---|
| old command on a clone with an unpushed local branch that holds a planted token | exit 1, `leaks found: 2` |
| new range command on the same clone | exit 0, `no leaks found` |
| push of clean work from a linked worktree | exit 0 |
| push of a commit with a planted token | exit 1, `leaks found: 2`, remote has no ref for it |
| push where an early commit adds the token and the last removes it | exit 1 |
| unresolvable range | exit 1 |
| next clean push with the leaking local branch still present | exit 0 |

Pre-commit scan, scratch clone, real `git commit` with the hk pre-commit hook (the other steps skipped):

| Case | Old step | New step |
|---|---|---|
| ignored `target/debug/out.txt` with a token, `target/` a real directory | exit 1 | commit exit 0 |
| `git add -f target/forced.txt` with a token | not run | commit exit 1, `File: target/forced.txt` |
| token staged in an ordinary file | not run | commit exit 1 |
| `target` a symlink to a directory with a token | exit 0 | commit exit 0 |

The two commits of this branch ran the new pre-commit step in a real linked-worktree hook and passed.

## Test plan for `test-reviewer`

No test was written. Plan for one: behavior, a test git process ignores every inherited `GIT_` variable.

- Positive: a test binary re-runs itself as a child with `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_PREFIX` and `GIT_OBJECT_DIRECTORY` set to a decoy repository. In the child, `git_command()` runs `git init <tmp>` and `git rev-parse --git-dir`. It must print `<tmp>/.git`, and the decoy must be unchanged.
- Negative: the same child with a plain `Command::new("/usr/bin/git")` must act on the decoy, so the test fails on the old shape.
- No other test covers it: the 118 failures only appear under a hook, and a normal run never sets the variables.

## Not verified

- A partial `git commit` (pre-commit with `GIT_INDEX_FILE`) was not run against the test suite. The builder removes the variable, and the new pre-commit step ran in real linked-worktree commits.
- Linux. The builder uses `/usr/bin/git`, the path the other helpers use. The Linux CI job was not run here.
- The pre-push gap for a ref other than `HEAD`, above.

## Suggestions

- A repository gate in `xtask ci` that rejects `Command::new` with `git` in test code outside `jackin-testkit/src/git.rs`, so the next helper cannot go around the builder.
- `mise.toml` `check:secrets-scan` still runs `gitleaks dir .`, `betterleaks dir .` and a full-history Kingfisher scan from the working tree, so it reads a real `target/`. It is a manual task and CI runs it before a build; moving it to the index export would remove the same trap.

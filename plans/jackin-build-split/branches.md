# Branch and PR Findings

- Status: IN PROGRESS
- Snapshot date: 2026-10-05

## Task branch

- Repository: `https://github.com/jackin-project/jackin.git`.
- Base branch and SHA: `main`, `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`.
- Task worktree: `/root/Projects/tailrocks/jackin-project/jackin-refactor-build-split`.
- Task branch: `refactor/build-split`.
- Initial task worktree state: clean at the base SHA.
- No PR was opened for this task during the documentation checkpoint.

## Preserved original worktree state

- The original worktree is `/root/Projects/tailrocks/jackin-project/jackin` on `main`.
- It had one unrelated, unstaged `mise.lock` modification at task start.
- Its initial `mise.lock` SHA256 was `6be630be77daa3073ec74340cd6a113b969ee9117a69b8d48ebe5d5501df764c`.
- The initial tracked blob was `c78d5f9ff2b48c70d488fa9a4cfbf5f750ef11ce`.
- The task worktree copy has SHA256 `ac3b0998f110538c5bb34f2653afaf7f2713d7f9f026dde1bb3f50845cef06ef`.
- This work changes documentation files only.

## Partial branch and PR snapshot

The branch owner reported five branch refs, two open PRs, and no fork heads before this task branch was pushed. That API inventory matched the then-fetched branch and PR SHAs.

The fresh fetch below ran after this task branch was pushed. It updated six local remote branch refs. No GitHub API query followed it. Treat the earlier branch count and PR status as a prior snapshot.

| Ref | Local ref SHA and delta | Earlier PR snapshot |
|---|---|---|
| `origin/main` | `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` | Base at inventory time |
| `origin/codex/credential-routing-recovery-20260930`, PR [#1111](https://github.com/jackin-project/jackin/pull/1111) | `3a28c199f17da335ecd9abd8dd67ebf1aecc0421`; 38 ahead, 0 behind; 95 files; `+8,106/-994` | Draft; 31 checks succeeded, one skipped; no reviews or comments reported |
| `origin/codex/ci-evidence-ledger-20260929`, PR [#1108](https://github.com/jackin-project/jackin/pull/1108) | `2990df17e25f30afca84804d9c402abc1ce00231`; 7 ahead, 2 behind; 29 files; `+8,229/-57` | Draft; 47 checks, five failed; no reviews or comments reported |
| `origin/codex/account-usage-capsule-consolidation-20261004` | `18bc09e9536d9b662876d2fb4205357a829caa9a`; descends from #1111 by 11 commits; 124 files; `+11,777/-1,889` | No PR or checks reported; 49 commits ahead total; 209 files; `+19,880/-2,880` |
| `origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/staged-index-jackin-1044-current-0437497d7f22` | `0437497d7f22fbdb7c8aad986be932704d707fe9`; stale parent `c52e912...`; 32 behind, 1 ahead | Seven-file merge-base delta: `+206/-134`; earlier review said not a merge candidate |
| `origin/refactor/build-split` | `34c32ca31e58b5e3dac71e88892778568f6f70d1` | This documentation branch; no PR opened |

The PR check and comment results are historical. The latest check and feedback state remain NOT VERIFIED.

### PR #1108 check detail

These results refer to earlier head `2990df17e25f30afca84804d9c402abc1ce00231`. Exact-head Sol review is complete; fixes remain pending.

| Check | Result |
|---|---|
| Generated-tree policy | FAIL: collapsed Swift/Apple job members disagree on the Xcode pin. Adding `.xcode-version` did not resolve it. [Failed job](https://github.com/jackin-project/jackin/actions/runs/36701333261/job/109841216417) |
| `jackin-xtask` Clippy | FAIL: `crates/jackin-xtask/src/ci_evidence/ledger.rs:339` uses `Duration::from_secs(180)`; the lint requires `Duration::from_mins`. Tests and doctests were skipped. [Failed job](https://github.com/jackin-project/jackin/actions/runs/36701335714/job/109841803013) |
| Rust dependency policy | FAIL: `zlib-rs 0.6.8` uses Zlib, which the Apache-2.0/MIT-only base policy rejects without an operator ruling. [Failed job](https://github.com/jackin-project/jackin/actions/runs/36701335714/job/109841803175) |
| `ci-required` | Collected 41 of 41 artifacts. It reports the Clippy and `cargo-deny` failures. |
| Control Required | Mirrors the failed required-check result. |
| Advisory collector | `ci_evidence/github.rs:867-921` labels a generic workflow failure Product. `ci-evidence.yml:22,38-45` lets dispatch errors continue and missing files warn, so the collector can report green without an artifact. |

The collector issue is advisory evidence. It does not clear the failing required checks. Record final disposition after exact-head Sol review.

### Independent Sol review disposition

The independent source review covered exact PR head `2990df17e25f30afca84804d9c402abc1ce00231`. It does not approve the branch for merge.

| Group | Disposition | Evidence and action |
|---|---|---|
| Obsolete generator contract | REJECT | The PR reintroduces `.github-gen/velnor-workflow.toml` and `.github/ci/project.toml`, changes `.github/ci/.github-actions-generator-state`, and deletes `.velnor/config.toml` and current `.github/workflows/ci.yml`. Replace these contracts through current generator ownership. |
| Old workflow and Xcode registration | REJECT | Reject the PR's generated collector workflow registration and `.xcode-version` restoration. Port useful checks through current workflow ownership. |
| Bounded child and owned-process behavior | ALREADY PRESENT | Current main `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` has bounded capture and `OwnedChild` in `crates/jackin-process/src/lib.rs:175-181,320-450`. Do not port duplicate process code. |
| Hook and Clippy fixes | ALREADY PRESENT | Hook predicate and three async-trait lint changes match current main in `crates/jackin-capsule/src/agent_status/hook_installer.rs`, `crates/jackin-capsule/src/exit_assess.rs`, and `crates/jackin-xtask/src/ci/tests.rs`. |
| Evidence collector and ledger | REPLACE | Port useful collection, provenance, immutable-attempt, atomic-output, and fixture behavior to current workflows and generator contracts. Preserve attribution and current ownership. |
| Bounded helper | SELECT | Select only a bounded helper that remains useful. Adapt it to main's process API and current owner. |
| Collector permission and failure reporting | REPLACE | Fix the collector and its validator. Do not broaden global workflow permissions. |
| Xcode pin intent and dependencies | REPLACE | Recompute dependencies and lockfile for the replacement. Preserve license policy and use supported native workflow ownership. |
| Workflow display labels | REPLACE | Preserve useful labels in current generated output. |

GitHub reported PR #1108 as conflicting and dirty against current main. Its latest reported run used merge SHA `65ba707` from older base `310e644`. That run does not validate integration with current main.

#### Stdin regression

In the PR source, `crates/jackin-process/src/lib.rs:489-555` waits for a bounded-output child without closing unused child stdin. The unbounded path closes it at `:355-358`. A child such as `cat` can wait for input indefinitely.

Add a one-second regression test. Preserve child exit and signal results. Do not mark the test as run; builds and tests remain outside this documentation task.

#### Artifact permission

The PR collector downloads artifact ZIPs at `crates/jackin-xtask/src/ci_evidence/ledger.rs:336-343`. The generated [.github/workflows/ci-evidence.yml](https://github.com/jackin-project/jackin/blob/2990df17e25f30afca84804d9c402abc1ce00231/.github/workflows/ci-evidence.yml) grants only `contents: read` at lines 10-11. The validator at `crates/jackin-xtask/src/ci_evidence/validation.rs:941-948` rejects additional permission keys.

GitHub's [artifact download endpoint](https://docs.github.com/en/rest/actions/artifacts#download-an-artifact) requires Actions repository permission `read` for fine-grained tokens. No 403 was observed. Add `actions: read` only to the collector workflow and update its validator. Do not change global permissions.

Both collector workflows expose `workflow_dispatch`, but `ci_evidence.rs:444-459` and `producer.rs:16-19` reject dispatch. Their jobs use `continue-on-error`, and artifact upload warns when files are missing. The workflows are absent from current main's default-branch Actions list, so no dispatch run confirmed this behavior.

Remove the unsupported dispatch trigger or emit an explicit inapplicable result. Make missing artifacts an observer failure. Do not make this advisory collector a required merge check.

The classifier in `ci_evidence/github.rs:867-921` labels generic failed conclusions as Product. Such conclusions do not prove product causation. Add an unknown or data-quality category, or provide causal evidence.

#### Task invocation gate

Jackin `mise.toml` build, test, and lint root tasks still call `cargo xtask`. The PR adds `ci-evidence` and `ci-push-head-ledger` tasks that also call `cargo xtask` at `mise.toml:91-97`. Velnor's `VerificationTask` uses `mise run` only for proven non-Rust tasks.

The Rust-MBX variant and Jackin Mise-wrapper integration remain pending design and activation review. No invocation bypass fix is claimed.

`git cherry` found no patch-equivalent changes in the earlier inventory. Final diff review and dispositions remain IN PROGRESS.

## Broad inventory refresh

The coordinator reports a refresh at `2026-10-05T02:20:13Z` with 1,157 refs and 1,103 PR records. This report contains no per-ref SHA list or final feedback state. Final PR status remains NOT VERIFIED and needs a fresh query.

## Fetch record

- Command time: `2026-10-05T02:29:52+02:00`.
- Command result: exit code `0`; stdout was `ok fetched`.
- Evidence pointer: this subsection records the command result. The following ref snapshot records the local refs.
- A separate shell transcript was not saved.
- The earlier API comparison came from the branch owner. This refresh did not query the API.

```sh
git fetch --no-prune --no-tags --no-write-fetch-head origin '+refs/heads/*:refs/remotes/origin/*' '+refs/pull/1111/head:refs/codex-inspection/pr/1111/head' '+refs/pull/1108/head:refs/codex-inspection/pr/1108/head'
```

The ref snapshot command was `git for-each-ref --format='%(objectname) %(refname)' refs/remotes/origin refs/codex-inspection/pr`.

```text
2990df17e25f30afca84804d9c402abc1ce00231 refs/codex-inspection/pr/1108/head
3a28c199f17da335ecd9abd8dd67ebf1aecc0421 refs/codex-inspection/pr/1111/head
0aa821a088e1bacf3d4d85a4c9faaa67faa85132 refs/remotes/origin/HEAD
18bc09e9536d9b662876d2fb4205357a829caa9a refs/remotes/origin/codex/account-usage-capsule-consolidation-20261004
2990df17e25f30afca84804d9c402abc1ce00231 refs/remotes/origin/codex/ci-evidence-ledger-20260929
3a28c199f17da335ecd9abd8dd67ebf1aecc0421 refs/remotes/origin/codex/credential-routing-recovery-20260930
0aa821a088e1bacf3d4d85a4c9faaa67faa85132 refs/remotes/origin/main
0437497d7f22fbdb7c8aad986be932704d707fe9 refs/remotes/origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/staged-index-jackin-1044-current-0437497d7f22
34c32ca31e58b5e3dac71e88892778568f6f70d1 refs/remotes/origin/refactor/build-split
```

Fetch status: PASS for ref synchronization only. Do not treat it as API verification, diff approval, or merge approval.

## Owner

`branches` completed inventory. It owns full diff review and dispositions. Keep this section IN PROGRESS until that work finishes.

See the [checklist](checklist.md) and [reviews](reviews.md).

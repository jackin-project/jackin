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

`git cherry` found no patch-equivalent changes in the earlier inventory. Final diff review and dispositions remain IN PROGRESS.

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

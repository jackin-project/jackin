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

The branch worker fetched current refs. GitHub API SHAs matched all fetched refs. Inventory found five branches, two open PRs, and no fork heads.

| Ref | Fetched head and delta | Checks and review state |
|---|---|---|
| `codex/credential-routing-recovery-20260930`, PR [#1111](https://github.com/jackin-project/jackin/pull/1111) | `3a28c199f17da335ecd9abd8dd67ebf1aecc0421`; draft; 38 ahead, 0 behind; 95 files; `+8,106/-994` | 31 checks passed, one skipped; no reviews or comments |
| `codex/ci-evidence-ledger-20260929`, PR [#1108](https://github.com/jackin-project/jackin/pull/1108) | `2990df17e25f30afca84804d9c402abc1ce00231`; draft; 7 ahead, 2 behind; 29 files; `+8,229/-57`; worker reported a dirty worktree | 47 checks, five failed; no reviews or comments |
| `codex/account-usage-capsule-consolidation-20261004` | `18bc09e9536d9b662876d2fb4205357a829caa9a`; descends from #1111 by 11 commits; 124 files; `+11,777/-1,889` | No PR or checks; 49 commits ahead total; 209 files; `+19,880/-2,880` |
| Synthetic recovery ref | `0437497d7f22fbdb7c8aad986be932704d707fe9`; stale parent `c52e912...`; 32 behind, 1 ahead | Seven-file merge-base delta: `+206/-134`; not a merge candidate |

`git cherry` found no patch-equivalent changes. The independent crosscheck first reported no remote fetch; the later fetch passed.

Fetch result: PASS for ref synchronization only.

The command used `--no-prune`, `--no-tags`, and explicit branch and PR refspecs. Final diff review and dispositions remain IN PROGRESS.

## Owner

`branches` completed inventory. It owns full diff review and dispositions. Keep this section IN PROGRESS until that work finishes.

See the [checklist](checklist.md) and [reviews](reviews.md).

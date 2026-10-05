# Stop-preservation delivery — 2026-10-05

The immediate STOP was honored. No implementation, tests, reviews, research, or merges were performed after it; actions below only preserve task state. All listed task branches matched their remote refs after checkpointing. No task branch was deleted.

## Jackin Git inventory

| Worktree | Branch | HEAD after checkpoint | State |
|---|---|---|---|
| `jackin` | `main` | `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` | Preserved; unrelated dirty `mise.lock` and `.local/` excluded and untouched. |
| `jackin-pr1111-omp-clippy` | `codex/credential-routing-recovery-20260930` | `1314e89f4b028d0adfb6c9e4ec7b619154c6a8bd` | WIP checkpoint includes partial OMP migration and partial causal universe fixture; clean worktree; tests/build NOT RUN. |
| `jackin-refactor-build-split` | `refactor/build-split` | `9e6ed667a858f3c6d50c34d93300b009121c6c39` | WIP migration-fixture and evidence-note checkpoint; clean. |
| `jackin-refactor-build-split-c72-docs` | `docs/build-split-c72-record` | `3cb98f2a634c1e660e501489d7383329569db7a3` | WIP record checkpoint; clean. |
| `jackin-refactor-build-split-dco` | `refactor/build-split-dco` | `5e9d24e85545686f0803b39540e71148a9c5fe41` | WIP record checkpoint; clean. |
| `jackin-refactor-build-split-dco-signed` | `refactor/build-split-dco-dco-signed` | `a2c0b1d6562041c561e0e95305a73fd1043fecd3` | Clean before this preservation delivery commit. |
| `jackin-refactor-build-split-docs-3b` | `docs/build-split-evidence-3b` | `114effca63bb5c4ce83f8bd56d80187e800cd5d8` | Clean local checkpoint branch pushed at the same OID. |
| `jackin-refactor-build-split-docs-current` | `docs/build-split-evidence-41265` | `b1e08b880223594cceccf3ea44322d44e3cdf984` | WIP record checkpoint; clean. |

The WIP checkpoint commits are DCO-signed. `git worktree list` showed these eight attached worktrees and no detached worktree. `git stash list` was empty. `git log --branches --not --remotes` was empty after pushes. The new WIP-run statuses were captured: 37361809177 (head 1314e89), 37361988706 (9e6ed66), 37362091324 (5e9d24e), and 37361408511 (a2c0b1d) completed with failure; 37361090342 (17151a9) was cancelled; 37359023616 (37b51ce) failed on `clippy::needless_continue`, fixed in 17151a9. No task-branch run remained queued or in progress at inventory time. Failure causes for WIP runs were not investigated.

The separate Velnor owner reports branch `feat/macos-native-build-task-20261005` checkpoint `34a8b19f5a4359e8a19e6ae0fe396aa955c02aff` / tree `715df5dfbb4b3e808f468cc826fe0c1ed6c7a668`, remote-equal and clean; its NativeImage/WorkflowTask implementation is incomplete and untested. Architect reports its R14c task source is preserved in remote/task-owned artifacts; its repository Git remains under its owner's control.

## Preserved helper code

The archives below contain only task-authored scripts, fixtures, tests, and diffs. A bounded scan of those exact candidate code files found no GitHub-token, AWS-key, private-key, or OpenAI-key patterns. Logs, markers, source trees/archives, Cargo caches, account/database files, raw session logs, binaries, runtime images, and rootfs bytes are excluded.

| Archive | Included files / source bytes | SHA-256 | Manifest SHA-256 |
|---|---:|---|---|
| `stop-preservation/mbx-24b8-code.tar` | 54 / 1,047,942 | `a32292743063ec1f50c3adb899e1d0c5f2531525928a4d04e962dff22a19afb5` | `387a5e88cec1b26448d9942f217655fd309f386b46ef7058ac929cecaf6abf6c` |
| `stop-preservation/velnor-preview-code.tar` | 84 / 1,824,162 | `6de7c10a17a9b663edc8a470fec9b12d01373ca271e5f87b84902c006e4d7100` | `4286ab956e17abede9a43021b9fda3cc489f97bca88a1d2145eb244a12ac21a3` |
| `stop-preservation/architect-runner-code.tar` | 60 / 1,548,976 | `0144a308e523b38c5d1caaf229cb7c24d4af6818ce0bb57244a09332c4904c43` | `13c8d0ef512f186f09afd93709331b11aba71b58ca282d5fb2284151a0b78e94` |
| `stop-preservation/architect-r14c-code.tar` | 1 / 67,419 | `7cf36e2ea6b12439097ec2e64d491054d22fdd535b6038e48b47042e9fdb7dda` | `68380d0f4d1dc22b8f58217e56456488168497ab7584897d2cd7c51d351bb998` |
| `stop-preservation/jackin-source-export-code.tar` | 6 / 101,054 | `07199ce25a7c5458885445833b59d2624b02e75841a2f937554cbd3ad829f203` | `8fc86c848a79b2f1a6c630d48345c720ee72f56471775dc7efd142a2591a99c6` |

R14c's image build and networkless runtime result remain a separate private record: image `sha256:a4d75e707095add28cf3f95bfde8c888f746eb9757ee50da112f962cd88b695f`, build exit 0 in 181.785s, `IMAGE_RUNNER_PASS`, runtime exit 0 in 4.222s. This does not establish ARM, Codex, auth, or app compilation. MBX v14 source-marker recovery/verification is historical 24b8 evidence only; no current-source compile/test passed. The 1314e89 OMP work is deliberately incomplete and has no compile/test result.

## Cleanup state

At this record cut, no temporary files, source snapshots, rootfs/runtime directories, or worktrees had been deleted. The main worktree's `mise.lock` and `.local/` remain untouched. Private runtime/log/source evidence stays outside the archive; it was not copied into Git. No test/build/review/merge is claimed by this delivery.

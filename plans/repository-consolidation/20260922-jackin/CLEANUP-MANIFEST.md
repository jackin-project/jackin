# Jackin cleanup manifest

Audit: `20260922-jackin`.

This manifest is the deletion gate. Every target is identified by exact path/ref and object ID, with dirty state captured before removal. Recovery root: `/Users/donbeave/.codex-chainargos2/jackin-recovery-20260922-rzwau1`.

## Retained targets

- Canonical checkout: `/Users/donbeave/Projects/jackin-project/jackin`. Preserve; switch from the starting feature branch to synchronized local `main` only after cleanup.
- Audit integration worktree: `/private/tmp/jackin-repository-consolidation-20260922`. Retain until audit PR merge; then remove exact path.
- Unrelated repositories: marketplace, agent-smith, github-terraform, and Velnor checkouts. No mutation.
- Recovery root and all recovery artifacts. Retain intentionally after cleanup.

## Linked worktrees

Pre-cleanup source snapshots:

- `worktrees-precleanup-landed-20260922.txt`
- `worktree-state-precleanup-landed-20260922.tsv`
- `worktree-path-state-precleanup-landed-20260922.tsv`
- `ignored-nontarget-precleanup-final-20260922.tsv` (unchanged; rechecked below)

`dirty=1` means tracked or untracked user state; all seven dirty product worktrees have separate staged/unstaged/index/untracked captures under `dirty-worktrees/`. Ignored files were inspected. `jackin-config-reference` contains 34 non-target generated documentation files; they are preserved under `ignored-worktrees/config-reference/` with per-file SHA-256 values. Other ignored content is build output under `target/`. `prunable=1` identifies a missing registered path whose administrative record may be pruned only after the exact row is rechecked.

| path | HEAD | branch | prunable | exists | dirty | tracked | untracked | ignored | action |
|---|---|---|---:|---:|---:|---:|---:|---:|---|
| /Users/donbeave/Projects/jackin-project/jackin | 2a318440ce2e02a15a76530812f375ee4998a0f3 | feat/multi-account-support | 0 | 1 | 0 | 0 | 0 | 1 | RETAIN canonical checkout |
| /private/tmp/jackin-1002-audit-tqEcJN | 0a66b7a919ad69a8ee5d2e723369d6a86cdefb93 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-audit.tTFsrT | a73743ab5cc39b24d39173c1393d713734d8f856 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-programmatic | e4e15a56557fdccdd0054a3d3e67addd6a1d71b1 | fix/pr1002-programmatic-template | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-swift-ci | 278cdbe517164564fc7206f37becff0e2b92b0b2 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-swift-ci-0dc | 33d39a46e1c6a4c1093d819033a5fab3e42752f3 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-swift-ci-repair | 81b8c0d2155722a4b97581c8da734ec3e00cd538 | codex/pr1002-swift-ci-repair | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-sync.ZKJeLQ | 1d0b749c7d2f09bc2b590d97130beb404ee48d05 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-tombstone-canonical-20260920 | 1430398af38e4a6571fa99b32c65a2627778763e | codex/pr1002-tombstone-canonical-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-1002-wrapper-semantics | d1fc67cbbdf358191c7a27b1354c44088516b9c2 | codex/pr1002-wrapper-semantics | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-actionlint-1004.HS8GkQ | 073ebbc514028733b3ad844e7c96a9a6ce973968 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-amp-launch-regression.WdYqRI | 843b83061f5dff2095a555065814b7950e046bc8 | codex/fix-amp-launch-profile-account-mounts-regression-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-auth-tree-atomicity | 098b4410c8021b4173506c29f9e27c355a3bfe44 | codex/fix-auth-tree-atomicity-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-broker-discovery-repair-20260920 | 7e92df16b0b7e8aa90f15314e7d0a13210774f01 | codex/repair-broker-discovery-error-state-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-broker-lifecycle-d7ed629 | d2e1aa28432cce07a941edd0e49aa8ffc1e86da9 | codex/fix-broker-lifecycle-d7ed629-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-capsule-clippy-1002 | c83d8e7ac77f7c73ece0da820942eee52911a986 | fix/1002-capsule-clippy-doc-lints | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-capsule-e2e-gap | e6189de31cbb6363dbb6bff0059ab7f1b91e2086 | codex/capsule-e2e-lifecycle-1002 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-capsule-lint-20260920 | a9382676f2f2e4c2e057c9a3c5c9284966b1a2ea | codex/capsule-lint-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-config-editor-transaction | 0644ca393d7836528bc2f58ca46186ace754901e |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-console-identity-20260922 | 110ea3a385aecfab49cd438955feeb3833e2745c | chore/console-identity-20260922 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /private/tmp/jackin-credential-boundary-current-20260920 | 0d181e88e6178bfe10bbdc37caaba1c3e938ecaf | codex/repair-credential-boundary-current-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-editor-refactor-20260920 | ef4df9a77e76340033b98cb707783e6eb671cfff | codex/refactor-editor-zshrc-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-fix-services-clippy | 83edb2d0f80faa1602e25530ab106e8e615c6896 | codex/fix-services-clippy-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-keychain-consent-20260922 | e90a37980249d4031d4d7155d697d6ebcb2a321b | fix/keychain-consent-diagnosis-20260922 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /private/tmp/jackin-launch-cleanup-evidence-20260922 | 67579b0685a2e0381c74a132afb07a88d1a02ea8 | fix/launch-cleanup-evidence-20260922 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /private/tmp/jackin-main-baseline-gate | 41796158b1e45535ae4e74d5ff048cb5bb4e0488 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-mbx-test | e4d1e4ce2dd186b033ebab3f9b37d1b4176d272b |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1001 | 884a2a38fa78ecd3c9f83148702ddb290ec8700a |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002 | 4216b039452b9b9bd5bde10b356bebe0323138fe | codex/pr1002-config-boundaries | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-94a | 94a7428796ccfe5d94f8aef76a558babad7f98c4 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-audit | 0a66b7a919ad69a8ee5d2e723369d6a86cdefb93 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-audit-176dcc | 176dcc0632f977a78d58443bde1b3ceb40304606 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-audit-current.DseX84 | f080e622f1681369f5bdbe1ce418cbaba9b57ea7 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-audit-f080 | d2e4049db65059babf9cdc43b178a4a501f88bd5 | review/pr1002-audit-f080 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-audit.X2jujY | 19e9f00e08a1587262d44dd057308183dcaf122d |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-cache | 94a7428796ccfe5d94f8aef76a558babad7f98c4 | fix/pr1002-xdg-cache-root | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-ci-fixes | 663d425e083b2ff866d2cd341efba0bb43443fe0 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-codex-model | 919fae2a7b12d050906dfe3b3b356a86a92b11b8 | agent/pr1002-codex-model | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-config-removal | dd15f5346df10f3eb38292461a846a21cbf355cb | agent/pr1002-config-removal | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-config-v1alpha11 | ec8eae66889cec6078b5ef568893caa12f5f6a05 | codex/pr1002-config-v1alpha11 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-credential-boundary-20260920 | ffe17e221b3c2a261a7f3857b0feec316812198e | fix/pr1002-credential-boundary-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-credentials | 67020f6c951206217a1405ca6f9b0c21ae3800db | agent/pr1002-credentials | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-discovery-xdg | 10218734536a9176c8be2382e2612ce4e9c0205b | agent/pr1002-discovery-xdg | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-effective-model | 7e9b76d9f53384f16d7363cf58b7483689966727 | codex/pr1002-effective-model | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-evidence | ebf74abe5ed89cebfc604b21e613d32120969e3c | audit/pr1002-evidence | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-private-config-atomicity-20260920 | 03c197495409f8beef40203c0a2566cf30c6c617 | codex/pr1002-private-config-atomicity-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-review | 0a66b7a919ad69a8ee5d2e723369d6a86cdefb93 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-trust | 70e11ddff051a6cda6e416f86021c51c9059f74a |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-usage-base | e5e0d681613e8bdb7493d25c575857a3893a67b8 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-usage-isolation | 1bdfeb8834a79fbdf02ed7f12a4694b6e1c56ee7 | fix/pr1002-usage-isolation | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-usage-lifecycle | 15caf638bc9a5d2e61827d9745843760b83e0cbe | codex/pr1002-usage-lifecycle | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-usage-scope | 70dc5515badefd26a6ca98771c739d873af1731e | agent/pr1002-usage-scope | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1002-zshrc | 6f61a5779b9b83c86031e5675dfc4ed9bd7720e0 | codex/pr1002-zshrc-canonical | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1003-290 | 290899797c8abf533e9b24089393c4771b7615d3 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1003-494c2bc | 494c2bc227c0d4a72e7f82f5ab7b6333b4b0a58f |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1003-boxington-cache | b7cc9d6f5b3b411c74cf801a5bc6330b03db8651 | codex/pr1003-boxington-cache | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1003-e5 | e5cf95556ea59d703c35f94125b9caa40800e173 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1003-final-gate | b7cc9d6f5b3b411c74cf801a5bc6330b03db8651 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1003-final-gate-ec8e | ec8eae66889cec6078b5ef568893caa12f5f6a05 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1004-final-preview-publish | 1087356cce21a12528657a9d163951e37dddae7f | codex/pr1004-final-preview-publish | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1004-generator-drift | a0e18c458d8c1b6cb5101ee597830f49c8712255 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1004-review | d87dbab1013d17136b5efce53394b315f619c685 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1004-review.TwZN4i | 4b1c2cea2d47bed627cbd9b6a7c98384520c3db4 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1005-evidence-repair | f38cb9ea9fef0ed245a204da7c4396e4afe47562 | codex/pr1005-evidence-repair-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1007-runtime-refresh | 2cf631ba5a1d3a7e0d5bdf051f95b6c8899a1fb8 | codex/ci-performance-campaign | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1008-review | 9033f95d61400a8649313726f55bb6727d873946 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr1063-adapt-20260922 | f25a6fa22ee5646b40c507b0c4c83b03249d7195 | fix/capsule-manifest-errors-20260922 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /private/tmp/jackin-pr963 | 4c6e71a12e6220f3fea4f02f7357f8556de5438e | sync/pr963 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-pr975 | 36b29fb75b62e65c760d3877b5e0b2bbdd82b37f | renovate/jdx-mr-boxington-action-1.x | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-preview-audit-f080 | f080e622f1681369f5bdbe1ce418cbaba9b57ea7 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-preview-audit.07I1gX | 41796158b1e45535ae4e74d5ff048cb5bb4e0488 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-preview-sorted-iteration-20260922 | d19b0602e0b2e6145720d2d06b54758c01fe5429 | fix/preview-sorted-iteration-20260922 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /private/tmp/jackin-preview-verification-safety-20260922 | 3ad15ba3f0ade7907effa42952cea59c068de405 | chore/preview-verification-safety-20260922 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /private/tmp/jackin-private-publication-atomicity-20260920 | 07c9dd72da6d7ae40c5dc5349e6f7ca4d1846fb6 | codex/fix-private-publication-atomicity-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-private-publication-repair-20260920 | 07c9dd72da6d7ae40c5dc5349e6f7ca4d1846fb6 | codex/repair-private-publication-final-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-repository-consolidation-20260922 | fd14ac6a7e0642842e66eeeb31d4b589d07148e1 | chore/repository-consolidation-20260922 | 0 | 1 | 1 | 0 | 12 | 1 | RETAIN audit worktree until audit PR merge |
| /private/tmp/jackin-schema2-candidate-073 | 073ebbc514028733b3ad844e7c96a9a6ce973968 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-schema2-candidate-validation | d87dbab1013d17136b5efce53394b315f619c685 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-schema2-draft | 41796158b1e45535ae4e74d5ff048cb5bb4e0488 |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-security-audit.uGMhdL | d7ed629cbc7464fbcbd95483ed447e81ea37cf3b |  | 1 | 0 | 0 | 0 | 0 | 0 | PRUNE exact stale administrative record after recovery |
| /private/tmp/jackin-usage-presentation-20260922 | a9ba4568e853a68a2e8b1a7d10700d7681f006fd | chore/usage-presentation-20260922 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /private/tmp/jackin-velnor-probe | 278cdbe517164564fc7206f37becff0e2b92b0b2 |  | 1 | 0 | 0 | 0 | 0 | 0 | RETAIN unrelated Velnor registration; no filesystem mutation |
| /private/tmp/tailrocks-velnor-source-4fa | 04da35e41e8ce806ff7d7f59ebba7b204b15cb95 | codex/velnor-legacy-rolling-tag-repair-20260920 | 1 | 0 | 0 | 0 | 0 | 0 | RETAIN unrelated Velnor registration; no filesystem mutation |
| /private/tmp/velnor-pin-4fa | 4fa7a3a85f141a6bb95bc9bdf0eef9e3ddde165d |  | 1 | 0 | 0 | 0 | 0 | 0 | RETAIN unrelated Velnor registration; no filesystem mutation |
| /private/tmp/velnor-pr958-fix-generated-whitespace | 43494d4b4d37b9b5dc099fa7bcabdb93483b854a | codex/fix-generated-whitespace | 1 | 0 | 0 | 0 | 0 | 0 | RETAIN unrelated Velnor registration; no filesystem mutation |
| /private/tmp/velnor-promote-generated-tree-970-971 | dbdebc91bbdafd4d3fd5017c7d0be76173a94c9b | codex/velnor-promote-generated-tree-970-971 | 1 | 0 | 0 | 0 | 0 | 0 | RETAIN unrelated Velnor registration; no filesystem mutation |
| /Users/donbeave/Projects/jackin-project/jackin-1002-credential-isolation | b6cb87028ab80c0d058b53f187610909d06f40ff | fix/1002-credential-isolation | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-admission-fix | 30da1fb9ca66414589c3f56a7fa0e72730890326 | codex/pr1002-admission-fail-closed | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-amp-launch-profile-fix | 3218706cf2b992ceeb20ca2d0ea280d4c6eae3d9 | fix/amp-launch-profile-account-mounts-3218706 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-apple-usage-relay-peer-isolation-20260920 | b65a2962f25ff055f84b8afa5bcfab7506ba1cb5 | codex/fix-apple-usage-relay-peer-isolation-20260920 | 0 | 1 | 1 | 8 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-auth-source-identity-repair-20260920 | 0070b5723d0333d8ba3f47f10d66d46ac4fdf39e | codex/repair-auth-source-identity-20260920 | 0 | 1 | 1 | 6 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-auth-tree-races | 0070b5723d0333d8ba3f47f10d66d46ac4fdf39e | codex/fix-auth-tree-races-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-broker-lifecycle-repair-20260920 | 48b4d69bdee7450c4c6d0eace0e3e97ee6a862b4 | codex/repair-broker-lifecycle-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-config-reference | a9dc330d9098f77ce53a9280562e318053644485 | fix/config-reference-public-schema-20260920 | 0 | 1 | 0 | 0 | 0 | 33 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-console-identity | d3e5f38b37c3437db26343c3e339892dea818e73 | codex/fix-console-account-identity-20260920 | 0 | 1 | 1 | 4 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-credential-atomic | fed40341ee9a2e558a5e1e3e4bc85e58a5daa30a | codex/pr1002-credential-atomicity | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-credential-revision | df90bb6c2788e5426104fe705c4f15ef0ea5c8ad | codex/pr1002-credential-revision-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-diagnostics-meter-install | 27c7a3b01be829f00a02c69c2fc9911112209995 | codex/diagnostics-meter-install-root-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-doc-runtime-schema-repair-20260920 | d3e5f38b37c3437db26343c3e339892dea818e73 |  | 0 | 1 | 0 | 0 | 0 | 33 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-docs-schema-followup-20260920 | f47f4e2c45d66ef3ad30ed99cd7d4e725a715892 | codex/docs-schema-inventory-followup-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-launch-lease-repair | 2be2c7fa303d569f84820cc56c2d3f33dcc0dad3 | codex/repair-launch-lease-blockers-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-launch-security-repair-20260920 | 2be2c7fa303d569f84820cc56c2d3f33dcc0dad3 | codex/repair-launch-security-20260920 | 0 | 1 | 1 | 4 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-meter-install-fix | 0ee3bf6d6ab295baa09074ca25502fbe4b4a1b3a | codex/fix-meter-install-error-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-opencode-workstream | 533f82ef29a8dbed40be229fac95c37ba4372d62 | codex/pr1002-opencode-source-bound | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-openrouter-usage-20260920 | ebc4421444d553f99139c90ebef99c4a88e1684a | codex/openrouter-usage-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-pr1004-package-provenance | 4b1c2cea2d47bed627cbd9b6a7c98384520c3db4 | codex/pr1004-package-provenance | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-pr1006-config-atomicity | 9033f95d61400a8649313726f55bb6727d873946 | codex/pr1006-corrective-fix | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-pr1009-p1-shutdown-20260920 | 7787f1b48f1757f304210640ea16a51743b86fd2 | codex/pr1009-p1-shutdown-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-preview-verify | 073ebbc514028733b3ad844e7c96a9a6ce973968 | codex/verify-preview-package | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-release-integration | 9edc88247c95a93f7ad4fd30c331be9e3faad94f | codex/release-integration-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-restore-identity-expansion-20260920 | 7fbbc2067cb258a83e50b617abb1a739e28d517e | codex/fix-restore-identity-expansion-20260920 | 0 | 1 | 1 | 3 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-runtime-generation-lease | 5f356db4233eb5649326aa708dfdebcafe3ca3ff | codex/fix-runtime-generation-lease-races-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-schema-docs-20260920 | 44f429abe6e0d0f1e20bc28b284a7d373e18f77b | fix/schema-docs-profile-selector-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-schema-docs-combined-20260920 | dd90de85570088f2dbc0f79c1147d4c6410b0cb7 | codex/combined-schema-docs-321-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-schema-inventory-repair-20260920 | d73179d82df7583baf96155f35b8cbef658034fd | codex/schema-inventory-repair-20260920 | 0 | 1 | 0 | 0 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-secret-transport-20260920 | 7fbbc2067cb258a83e50b617abb1a739e28d517e | codex/fix-secret-transport-20260920 | 0 | 1 | 0 | 0 | 0 | 0 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-usage-credential-routing-20260920 | 7fbbc2067cb258a83e50b617abb1a739e28d517e | codex/repair-usage-credential-routing-20260920 | 0 | 1 | 1 | 17 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
| /Users/donbeave/Projects/jackin-project/jackin-usage-presentation-repair | 7fbbc2067cb258a83e50b617abb1a739e28d517e | codex/usage-presentation-repair-20260920 | 0 | 1 | 1 | 2 | 0 | 1 | REMOVE exact worktree after recovery/disposition recheck |
## Standalone Jackin clones

| path | repository evidence | HEAD/ref | recovery | action |
|---|---|---|---|---|
| `/Users/donbeave/Projects/jackin-project/jackin-main` | Jackin clone; clean; current `main` at `fd14ac6a7e0642842e66eeeb31d4b589d07148e1`; retained local `fix/usage-broker-fallback` at `ec3dbcf662f1f62eb20a53e7a9e4ae6004c05498` and `goal-handoff/verify-merge-pr1063-cb81b336` at `8e14f13a810898b2a54aebccb336d95fc6686ca5` | captured in `standalone-clones/jackin-main-final.bundle` | bundle to verify before removal | remove exact clone after final recheck |
| `/Users/donbeave/Projects/jackin-project/jackin-schema-fix-20260920` | Jackin clone; clean; `codex/fix-schema-versions-inventory-20260920` at `ba1ad3f7fe8dc19026a3a8dbaf94a47dcdbd3a56` | captured in `standalone-clones/jackin-schema-fix-20260920-final.bundle` | bundle to verify before removal | remove exact clone after final recheck |

## Remote branches

`remote-branches-precleanup-landed-20260922.tsv` is the expected-object-ID deletion input (25 remote heads; #1077 is already auto-deleted after merge). Delete every non-`main` branch only after a live `git ls-remote` recheck equals the recorded OID. Preserve `main`; preserve the audit branch until its PR lands. Record each deletion result in `remote-branch-deletion-results-20260922.tsv`.

## Local branches

`refs-precleanup-landed-20260922.tsv` is the local ref input. It contains 109 local heads, including the unpushed audit branch. After linked worktrees are removed and final recovery is verified, delete only audited non-`main` Jackin heads; retain unrelated Velnor refs. Preserve `main` and the audit branch until its PR lands. Record final local refs in `refs-after-cleanup-20260922.tsv`.

## Final gate

After all exact removals: fetch with prune, repeat filesystem/repository/PR discovery, verify only the canonical Jackin checkout remains in-scope, verify `HEAD == origin/main`, and record the final snapshot in the recovery root. Any changed target stops deletion and is re-audited.

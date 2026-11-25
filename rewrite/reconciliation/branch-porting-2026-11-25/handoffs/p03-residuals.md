# p03-residuals handoff
- Branch fix/p03-residuals, head 1d0ef275b, PR #1326 (ready for review, CI green on f7323cc52; Sonar gate passed).
- Done: R1 honest cancellable lookups (locate, purge/NotManaged/DeleteUnconfirmed, instances_of recheck), R2 second-signal abandon report, R3 Recreate/Revert + create_provider_if_absent (+ object-ID refresh guard), console notice via termrock#604/#605 (pin d5799531), Codex threads all replied and resolved.
- G-FAST EXIT=0 (3002 tests) on f7323cc52. The last G-FAST (gfast8.log, head 3787a4d90 + doc commit 1d0ef275b) was stopped unfinished by this checkpoint: rerun it on the final head.
- Reviews: code rounds 1-7 at high; round 7 APPROVED (one doc nit, fixed in 1d0ef275b).
- Left: rerun G-FAST on head, wait for CI green on new head, update PR body evidence (G-FAST line, reviews), append "1326 p03-residuals" to scratchpad/merge-queue.txt.
- Open threads: none unresolved at last check.

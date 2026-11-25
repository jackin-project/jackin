Branch fix/removal-plan-live-config, head 25ac1dc47cc2c905ca4d03cbcac294102a483c38, draft PR #1330 (https://github.com/jackin-project/jackin/pull/1330).
Done: RemovalPlan/OfflineRemoval live-config guards, edit_checked pre-edit check, 11 new tests plus core store test; red/green captured; jackin-session 798/798 and core+tui+jackin 951/951 passed; independent review APPROVE.
Last push fixes 3 clippy errors in peers.rs tests (CI Lint had failed on them); not yet re-verified by a G-FAST run (the rerun was stopped).
Left: run G-FAST (xtask ci --fast, throttled) or let CI confirm Lint, wait for CI green, gh pr ready 1330, answer any comments, append "1330 removal-guards" to scratchpad merge-queue.txt.

# SUP-091 — MINIMAX RAMP TO 14 (6 jackin + 8 servers), staged

Combined target: 14 concurrent MiniMax-M3.1-Flash workers (jackin 6,
servers 8; combined cap 16). Staged: 4+4 first, then 6+6, then 6+8 if
stable and independent work remains. MiniMax 5h quota 0% used (reset
~1h), weekly 49% used - this window is the priority.

Rules: exact pin minimax/MiniMax-M3.1-Flash thinking high; isolated
worktrees; unique file ownership; no duplicates of running work; joins
<=240s; refill slots same turn as workers complete (waves); merge-ready
work merges immediately - never delay a merge for a percentage; report
running/queued/completed by model each wave. GLM-Flash tests/verify,
MiMo-Flash triage/audit, Pro exceptional only, Kimi zero until operator
confirms reset (Nov 16 21:33).

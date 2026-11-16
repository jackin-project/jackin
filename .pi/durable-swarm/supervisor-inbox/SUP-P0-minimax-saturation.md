# SUP-P0 — MINIMAX SATURATION (P0, supersedes routing in SUP-087/088/089)

Verified by supervisor just now: ZERO Pi workers running in either fleet;
servers clone 0.8.0 PROHIBITS MiniMax-M3.1-Flash (w3-impl disabled);
MiniMax 5h quota 0% used, resets in ~1.4h; weekly 49% used. This is the
underutilization root cause. Execute immediately:

1. MiniMax-M3.1-Flash (w3-impl / w4-audit) is the DEFAULT for all new
   eligible implementation work: target 4 concurrent MiniMax workers in
   this fleet, in successive waves; ~80% of new Flash-suitable coding work.
2. GLM-5.3-Flash (w5-tests) ~12%: independent tests, verification, CI
   failure investigation. MiMo V2.6 Flash (w1-scout) ~8%: analysis,
   triage, audits. Pro/Kimi: genuine escalations only. Kimi stays ZERO
   until the operator confirms its quota reset (Nov 16 21:33).
3. Coordinators: stop doing implementation yourselves. Spawn -> join
   (<=240s) -> collect -> spawn next wave. A completed worker's slot is
   refilled in the same turn; never end a turn with free MiniMax slots
   while backlog exists.
4. Independent worktrees, no two writers on the same files, no duplicate
   tasks. Merge-critical work first; never delay a merge for a percentage.
5. Report per-wave: workers spawned by model, tasks delivered, results.

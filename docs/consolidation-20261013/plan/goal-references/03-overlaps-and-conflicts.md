# 03 — Overlaps and conflicts

> **Planning status: PARTIAL EVIDENCE / IMPLEMENTATION BLOCKED.** All 79 discovered server branches have dossiers and all 50 distinct tips received some inspection. Complete unique-history/semantic closure, the operator's local inventory, Jackin test execution and independent review are not complete. These are hard execution gates, not silently waived work.

## Comparison evidence and limitations

Selected GitHub comparisons establish ancestry/divergence, and tip patches establish concrete candidate behavior. Broad comparison responses are capped at 300 files; some added-file stats are zero despite real nonempty content. Full commit/file/rename coverage and stable patch IDs were not computed against Jackin locally. The collector and TASK-002 close those gaps before any implementation approval.

## Competing implementations and prescribed resolution

| Overlap | Observed evidence | Resolution to verify | Validation |
| --- | --- | --- | --- |
| M foreground lifetime vs policy extraction / fallback | M should_exit_idle guard; P broker split; 33d88b1 fallback | Keep foreground/monitor/passive truth table and exact authority; fallback is gated, not default | TASK-007 + TASK-015, clock/owner-loss/capability tests |
| P layout vs U flat usage repair | P 366 ahead; U 25 ahead; merge base M | Use reviewed P leaf architecture; semantically port U auth/provider/monitor/schema tests, not old flat modules | TASK-004/005 then 007/008/009 |
| P vs T modernization | T has 2 exclusive commits and is 370 behind P | Extract missing tool/native/release improvements; retain coherent selected manifest and lock | TASK-006 range diff + pinned tooling/native tests |
| s7/s61/dirty snapshots vs P | Capability, purge and test-split differences; partial comparisons | Compare bodies/tests and all payloads, resolve final package ownership; no intermediate duplicate crates | TASK-007/010/014 |
| 30 modern stash refs | Two exact tips; first-parent patches expose extraction but not all parent content | Deduplicate object analysis; inspect every parent tree; close every alias row explicitly | TASK-002/010 |
| CI push ledger vs first-parent denominator | 1c42a33,14bd147,c7b8a958 vs 12dea6c5 | Prefer event/producer/artifact-bound denominator; retain useful first-parent negative tests | TASK-006 multi-commit-push, missing-producer/boundary and fixture rejection |
| Global sccache vs task-scoped successor | b1dc105 exports wrapper before installation; 41dd148e constrains scope | Reject global wrapper/backend assumptions; adapt isolation tests to current selected cache design | TASK-006 actual tool execution, installer env and warm-cache proof |
| Native migration candidates | 8fb49688 knowingly unregenerated; 432e0108 complete product contract; 7646c61/0437497 recovered variants | Reject mismatched input/output state; preserve producer-consumer and all task-owned native obligations | TASK-015 full native/UI/OrbStack gates |
| WIP lock rows vs selected pins | musl keys point to GNU archives; added Zig provenance | Regenerate and verify platform compatibility; retain valid signature provenance | TASK-006 locked platform tests |
| CLI reflog vs build-split propagation | d04467f0 parsing/dry-run; archived 42c6f8ed/8dac3d6e path evidence | Keep one end-to-end model/effort implementation; remove unsupported options and duplicate code | TASK-011 parse/dry-run/launch/restore cases |
| Typed provider status vs string heuristics | 6ce20ce1 typed errors and misleading-text tests | Only structured status/retry metadata controls auth/rate policy; display messages stay presentation | TASK-008 typed/text-conflict fixtures |
| Probe tips vs useful underlying work | Malformed hooks, empty commits, comment probes; staged selector fix 6d65e649 | Reject visible production debris but retain isolated staging/stale-head tests and audit ancestry | TASK-002/014 |
| Synthetic conflict tree vs recovered behavior | fea3ade admits unresolved conflicts and synthetic parent | No whole-tree merge; inspect sides/hunks and extract justified missing behavior | TASK-013 merge-marker and behavior audit |
| Historical records vs current source | Old claimed greens, rejected security attempts and unresolved obligations | Preserve dated provenance; verify selected destination and rerun rather than treating text as proof | TASK-003/013/016 |

## Conflict handling protocol

Resolve by documented behavior and source evidence, never blanket ours/theirs. For a conflict, record all source SHAs, paths/symbols, selected behavior, rejected alternative, test and reviewer. Structural moves and shared protocol/manifest changes serialize. A clean Git merge is not a functional test. A rejected commit may still donate valuable tests/docs/fixes. Every changed decision must leave an audit trail.

## Prior consolidation is not an exemption

The October 6 squash at `9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b` records earlier recovery/consolidation work and rejected sccache restoration. Those statements explain why archives may overlap strongly with main, but they do not prove current branch-to-final behavioral equivalence. Inspect aggregate squash deltas and destination tests. Do not interpret preserved synthetic ancestry as proof of an original development line.

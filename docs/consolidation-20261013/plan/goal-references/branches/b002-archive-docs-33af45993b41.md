# B002 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/docs/build-split-evidence-41265/tip-b1e08b880223594cceccf3ea44322d44e3cdf984`  
**Tip:** [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984)  
**Group:** archive-docs  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Build-split evidence inventory and unresolved recovery gates**

- Historical inventory at a67ef88d (abbreviated revision in the inspected record) records 35 commits (22 source/test/config and 13 documentation). This is not the current branch inventory.
- Useful audit seeds include model/effort propagation, CODEX_HOME discovery, OMP database/WAL capture, stored agent identity on restore, unsupported initial-prompt removal, diagnostic redaction and c72 signed-rule validation.
- The source records rejected intermediate redaction implementations and a failed ambient BuildKit-token-removal review; do not import those intermediate implementations as approved security fixes.
- Historical unresolved obligations include ordered-fsync cleanup, migration predecessor/ambiguity fixtures, profile-proof invalidation, projection-only Kimi capture, non-Unix traversal safety, pending session-lease recovery and concurrent-writer OMP snapshot proof.
- These are investigation seeds, not confirmed current defects. Find their final destinations and tests before reopening or marking superseded.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `plans/jackin-build-split`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-015: COMBINE](../02-change-coverage-matrix.md#chg-015), [CHG-016: ADAPT](../02-change-coverage-matrix.md#chg-016), [CHG-018: COMBINE](../02-change-coverage-matrix.md#chg-018), [CHG-031: ADAPT](../02-change-coverage-matrix.md#chg-031)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-015:** Trimmed/empty model, four effort values, invalid effort, defaults/null JSON, actual launch/restore selection. Risk: Accepted-but-ignored user options or restored sessions selecting the wrong agent.
- **CHG-016:** Concurrent writer snapshot, WAL reset suffix, interruption/fsync cleanup, migration golden/ambiguity fixtures, proof invalidation and source discovery. Risk: Reopening fixed work or silently losing still-missing persistence/security behavior.
- **CHG-018:** Per-assertion catalog map, payload inspection, TLS compile/feature cases, rejected endpoint choices. Risk: Security regressions hidden by structural test churn.
- **CHG-031:** Link/source SHA checks, exact current task commands and NOT_RUN/live distinction. Risk: An executable runbook accidentally performs a live cutover or claims unearned readiness.

Mandatory remainder: `REM-002` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-011, TASK-012, TASK-013, TASK-016 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

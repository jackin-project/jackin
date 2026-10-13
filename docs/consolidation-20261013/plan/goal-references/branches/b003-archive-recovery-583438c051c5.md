# B003 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/cleanup-20261006/jackin-unreferenced-tree-abc5b11ca1b2/tip-fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`  
**Tip:** [`fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`](https://github.com/jackin-project/jackin/commit/fea3ade6830912017f2ad80c0c5a9d1acfbde0b7)  
**Group:** archive-recovery  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Unresolved synthetic recovery snapshot**

- Commit explicitly preserves source tree abc5b11ca1b2aa0699947d54539cb291691189e4 and says synthetic parent 0aa821a088e1bacf3d4d85a4c9faaa67faa85132 is a comparison baseline, not recovered ancestry.
- Unresolved conflicts remain in crates/jackin-process/src/lib.rs and tests.rs. The source explicitly disclaims buildability and merge readiness.
- Reject whole-tree integration. Inspect each conflict side and unrelated saved hunk; retain useful behavior through separately reviewed extraction, never through a synthetic-ancestry merge.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/jackin-process/src/lib.rs`
- `crates/jackin-process/src/tests.rs`
- `.github-gen/velnor-workflow.toml`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-030: REJECT](../02-change-coverage-matrix.md#chg-030)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-030:** Per-conflict-side audit, meaningful hunk dispositions and no unintended merge markers. Risk: Broken source and fabricated history entering the integration.

Mandatory remainder: `REM-003` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-013 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`](https://github.com/jackin-project/jackin/commit/fea3ade6830912017f2ad80c0c5a9d1acfbde0b7). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

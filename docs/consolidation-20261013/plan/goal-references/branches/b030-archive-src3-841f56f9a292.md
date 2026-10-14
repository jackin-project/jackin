# B030 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-12dea6c5fd0f/tip-12dea6c5fd0f9d2c594d5ad3159e6493d03352f4`  
**Tip:** [`12dea6c5fd0f9d2c594d5ad3159e6493d03352f4`](https://github.com/jackin-project/jackin/commit/12dea6c5fd0f9d2c594d5ad3159e6493d03352f4)  
**Group:** archive-src3  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Competing first-parent CI evidence denominator**

- Replaces an event/observed-run fallback with first-parent history, marks fixtures unqualified, records conflicting terminal observations and makes unqualified rollup nonzero.
- First-parent history is useful Git evidence but does not prove the actual push-event grouping that triggered workflows. A multi-commit push can produce a workflow only for its head.
- Choose durable push-head/artifact-bound qualification from 14bd1471 for post-merge workflow obligations; retain failure, conflict and fixture-negative tests from this candidate where missing.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/jackin-xtask/src/ci_evidence.rs`
- `.github-gen/velnor-workflow.toml`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-020: SUPERSEDED](../02-change-coverage-matrix.md#chg-020)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-020:** Multi-commit push represented by one workflow head; gaps and fixture inputs stay unqualified. Risk: Counting obligations for commits that were never distinct push heads, or missing true unstarted obligations.

Mandatory remainder: `REM-030` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-006 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `12dea6c5fd0f9d2c594d5ad3159e6493d03352f4`](https://github.com/jackin-project/jackin/commit/12dea6c5fd0f9d2c594d5ad3159e6493d03352f4). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

# B040 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/rescue/local/937c9c5f-a870-43c7-aa58-36a3f5c1b9cf/ed5b0cf1d7a4fdfe4e62eed7/usage-r37-followup`  
**Tip:** [`2abed4ad67d8438ab9eb124b564be6ad5b545847`](https://github.com/jackin-project/jackin/commit/2abed4ad67d8438ab9eb124b564be6ad5b545847)  
**Group:** usage-followup  
**Server protected flag:** false  
**Inspection:** TIP_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Retain state store for revoked-account marker assertion**

- The test corrupt_revoked_account_is_replaced_with_result_free_uncertainty_marker retains the concrete Arc store while passing a cloned trait-object Arc to the coordinator.
- A reasoned clone_on_ref_ptr expectation documents why the retained handle is needed to inspect persisted state after catalog rotation.
- Keep the persistence assertion if missing at the final policy tip; otherwise record exact body/test equivalence. Tip inspection does not close the branch remainder.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/services/jackin-usage-coordinator/src/tests/case_03.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-005: KEEP](../02-change-coverage-matrix.md#chg-005)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-005:** corrupt_revoked_account_is_replaced_with_result_free_uncertainty_marker; inspect actual persisted state after catalog rotation. Risk: A passing coordinator test that no longer examines durable state.

Mandatory remainder: `REM-040` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-007 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `2abed4ad67d8438ab9eb124b564be6ad5b545847`](https://github.com/jackin-project/jackin/commit/2abed4ad67d8438ab9eb124b564be6ad5b545847). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

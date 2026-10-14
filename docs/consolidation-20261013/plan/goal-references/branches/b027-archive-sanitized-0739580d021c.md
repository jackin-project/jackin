# B027 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/source3-sanitized/8f2d72ac88d4f15b1435aae1f508fcb2d7ca5eab/tip-502e0627d3940e2cd68ae0078910f21452d9ccc1`  
**Tip:** [`502e0627d3940e2cd68ae0078910f21452d9ccc1`](https://github.com/jackin-project/jackin/commit/502e0627d3940e2cd68ae0078910f21452d9ccc1)  
**Group:** archive-sanitized  
**Server protected flag:** false  
**Inspection:** TIP_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Per-request diagnostic retry-payload assertions**

- assert_wire_requests now requires one matching event record in every captured log request, in addition to total trace/log/metric request counts.
- The 0/1/2 distribution across three retries must fail even though the flattened total is three. This is stronger than a count-only assertion.
- Preserve this regression contract through diagnostic test moves; do not automatically add duplicate assertions if the canonical helper already enforces it.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/jackin-diagnostics/tests/wire_failure_support/mod.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-017: KEEP](../02-change-coverage-matrix.md#chg-017)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-017:** Negative 0/1/2 distribution and positive 1/1/1; traces/logs/metrics request counts. Risk: Retry-payload corruption concealed by aggregate counts.

Mandatory remainder: `REM-027` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-012 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `502e0627d3940e2cd68ae0078910f21452d9ccc1`](https://github.com/jackin-project/jackin/commit/502e0627d3940e2cd68ae0078910f21452d9ccc1). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

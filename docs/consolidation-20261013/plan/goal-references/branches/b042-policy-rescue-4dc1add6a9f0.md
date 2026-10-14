# B042 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/rescue/local/20261011-110605-42b7822b/c4ea76082d0e4a93c792f46d/rust-policy-rollout-bff77099`  
**Tip:** [`bff77099f8273a4d480a0e430cba66afc4197543`](https://github.com/jackin-project/jackin/commit/bff77099f8273a4d480a0e430cba66afc4197543)  
**Group:** policy-rescue  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Preserve s61 rollout documentation changes**

- Versus policy: two ahead, 169 behind; merge base 95629842235f0d19a89b8e2d09c4e4813867fb5b.
- Tip parent 26cf7e939218e9f16121d3a2fa287cdb15b27832 is an extraction checkpoint. Do not classify documentation-only tip as a documentation-only entire branch.
- Read the ensure_absent_for_purge_with leaf extraction and compare the live purge guard before considering the two-commit branch superseded.

## Relationship and unique commits

Observed comparison base `fba69d60c2373156dc0dcecee8e085c83776966f`: head ahead 2, behind 169, merge base `95629842235f0d19a89b8e2d09c4e4813867fb5b`, status diverged. This is a comparison to a non-main base; main-relative divergence remains uncomputed.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/services/jackin-runtime`
- `crates/services/jackin-runtime-apple-container-ensure-absent`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-014: COMBINE](../02-change-coverage-matrix.md#chg-014)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-014:** Guard outcomes, interrupted purge, live/container ambiguity and unchanged test identities. Risk: Safety checks lost during rename or stale baseline claims accepted as current.

Mandatory remainder: `REM-042` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-010 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `bff77099f8273a4d480a0e430cba66afc4197543`](https://github.com/jackin-project/jackin/commit/bff77099f8273a4d480a0e430cba66afc4197543). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

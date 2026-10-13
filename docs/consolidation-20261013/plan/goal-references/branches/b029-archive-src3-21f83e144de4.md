# B029 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-8fb49688a640/tip-8fb49688a6408627e22118f92d24f47785564979`  
**Tip:** [`8fb49688a6408627e22118f92d24f47785564979`](https://github.com/jackin-project/jackin/commit/8fb49688a6408627e22118f92d24f47785564979)  
**Group:** archive-src3  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Pre-regeneration generic Apple CI inputs**

- The commit explicitly says inputs-only, policy --check expected to fail and DO NOT LAND before matching M1 regeneration. Historical renderer remains 44243ed4917d53ad74a96c64d1c58dbe52c944c1.
- Adds explicit visibility and Xcode inputs, removes the synthetic Swift package path and sets BoltFFI deployment target 26.0.
- Treat as an incomplete intermediate candidate. Compare later 432e0108 and current toolchain; carry native obligations, never the intentionally unmatched input/output state.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `.github-gen/velnor-workflow.toml`
- `.github-gen/visibility.toml`
- `.xcode-version`
- `crates/jackin-usage-ffi/boltffi.toml`
- `mise.toml`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-024: REJECT](../02-change-coverage-matrix.md#chg-024)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-024:** Byte-identical generation from the final reviewed input tree and selected generator product. Risk: Known-invalid CI state entering main.

Mandatory remainder: `REM-029` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-015 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `8fb49688a6408627e22118f92d24f47785564979`](https://github.com/jackin-project/jackin/commit/8fb49688a6408627e22118f92d24f47785564979). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

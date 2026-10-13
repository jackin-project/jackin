# B033 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-d16cd795d4a5/tip-d16cd795d4a539b84ba5f44290663b2363d49350`  
**Tip:** [`d16cd795d4a539b84ba5f44290663b2363d49350`](https://github.com/jackin-project/jackin/commit/d16cd795d4a539b84ba5f44290663b2363d49350)  
**Group:** archive-src3  
**Server protected flag:** false  
**Inspection:** TIP_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Explicit Apple profile and deployment-floor declaration**

- Adds native.apple cargo_profile=desktop-release and deployment_floor=26.0 to generation inputs and updates the input digest.
- The visible patch does not prove downstream producer/consumer adoption. Compare the completed product-edge implementation at 432e0108 and the current native configuration.
- Preserve one consistent native contract across Rust FFI, SwiftPM and XcodeGen; do not duplicate obsolete source-of-truth files.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `.github-gen/velnor-workflow.toml`
- `.github/ci/.github-actions-generator-state`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-023: COMBINE](../02-change-coverage-matrix.md#chg-023)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-023:** Fresh native build, correct producer paths, CI/merge/scheduled commands and macOS OrbStack release lane. Risk: Swift resolves a missing/wrong product or validates fewer behaviors after migration.

Mandatory remainder: `REM-033` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-015 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `d16cd795d4a539b84ba5f44290663b2363d49350`](https://github.com/jackin-project/jackin/commit/d16cd795d4a539b84ba5f44290663b2363d49350). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

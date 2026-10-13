# B025 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/staged-index-jackin-1044-current-0437497d7f22/tip-0437497d7f22fbdb7c8aad986be932704d707fe9`  
**Tip:** [`0437497d7f22fbdb7c8aad986be932704d707fe9`](https://github.com/jackin-project/jackin/commit/0437497d7f22fbdb7c8aad986be932704d707fe9)  
**Group:** archive-staged  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Recovered staged-index Apple CI input state**

- The commit identifies a staged worktree-index capture generated without source hooks; it is a preservation snapshot, not proof of validation.
- Visible changes resemble the generic Apple schema-2 migration, including Velnor 690b3935, native product edges and frozen Renovate inputs.
- Compare tree and per-parent deltas against 7646c61, 8fb49688 and 432e0108. Preserve useful input contracts but regenerate selected output and protect handwritten files.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `.github-gen/velnor-workflow.toml`
- `native/Package.swift`
- `native/project.yml`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-023: COMBINE](../02-change-coverage-matrix.md#chg-023)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-023:** Fresh native build, correct producer paths, CI/merge/scheduled commands and macOS OrbStack release lane. Risk: Swift resolves a missing/wrong product or validates fewer behaviors after migration.

Mandatory remainder: `REM-025` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-015 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `0437497d7f22fbdb7c8aad986be932704d707fe9`](https://github.com/jackin-project/jackin/commit/0437497d7f22fbdb7c8aad986be932704d707fe9). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

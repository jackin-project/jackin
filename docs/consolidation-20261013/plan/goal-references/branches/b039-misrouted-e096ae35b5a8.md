# B039 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/recovery/cleanup-20261006/misrouted-jackin/src-2e29a881/dangling-sccache-successor-41dd148e514d`  
**Tip:** [`41dd148e514d1bf3f84401f0ecbf8e87b58bb261`](https://github.com/jackin-project/jackin/commit/41dd148e514d1bf3f84401f0ecbf8e87b58bb261)  
**Group:** misrouted  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Recovered sccache successor: scope wrapper to Cargo tasks**

- Visible changes remove installer-wide wrapper environment in favor of task-scoped Cargo commands. GHA backend defaults off unless runtime endpoint and token are available; missing sccache disables the wrapper.
- Desktop merge trigger gains pull_request; cancellation is limited to PR runs. The archived renderer does not support merge_group in this primitive.
- Contract tests cover provisioning and installer isolation. The backend smoke returns early when tools are unavailable, so an apparent passing test is not sufficient execution evidence.
- The prior main consolidation record rejected reintroducing old sccache architecture. Extract useful isolation/coverage contracts into the selected current toolchain, not obsolete generated workflow bytes.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `mise.toml`
- `.github/workflows/desktop-merge.yml`
- `crates/jackin-xtask/src/ci/tests.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-022: ADAPT](../02-change-coverage-matrix.md#chg-022)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-022:** Provision-before-use, wrapper absence, endpoint/token availability, actual executed smoke count. Risk: Stale tests forcing obsolete architecture or early-return passes misreported as validation.

Mandatory remainder: `REM-039` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-006 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `41dd148e514d1bf3f84401f0ecbf8e87b58bb261`](https://github.com/jackin-project/jackin/commit/41dd148e514d1bf3f84401f0ecbf8e87b58bb261). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

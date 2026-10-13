# B004 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-0a898cd246fbbf62f3b463f0097ffd6a368b1e07/tip-0a898cd246fbbf62f3b463f0097ffd6a368b1e07`  
**Tip:** [`0a898cd246fbbf62f3b463f0097ffd6a368b1e07`](https://github.com/jackin-project/jackin/commit/0a898cd246fbbf62f3b463f0097ffd6a368b1e07)  
**Group:** archive-keeper  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**On (no branch): hk**

- Despite root in its archive ref name, this is a three-parent stash-shaped commit, not an orphan root.
- First-parent delta only: crates/jackin-xtask/src/main.rs gains a _hook_proof assignment and deliberately unformatted unused hook_proof_dirty_unstaged function.
- Reject these two proof insertions as production changes. This is NOT a rejection of unseen index/untracked parent payloads or historical base behavior.
- Inspect index and untracked trees separately before closing branch coverage.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/jackin-xtask/src/main.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-029: REJECT](../02-change-coverage-matrix.md#chg-029)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-029:** Isolated hook/stale-head/cache regression fixtures; no probe debris in final source. Risk: Accidentally rejecting useful ancestral changes along with a probe tip.

Mandatory remainder: `REM-004` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-014 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `0a898cd246fbbf62f3b463f0097ffd6a368b1e07`](https://github.com/jackin-project/jackin/commit/0a898cd246fbbf62f3b463f0097ffd6a368b1e07). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

# B050 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c0ab9196b766-80ab98038e`  
**Tip:** [`80ab98038e945ec48c82dad241ebf33aa0efa82d`](https://github.com/jackin-project/jackin/commit/80ab98038e945ec48c82dad241ebf33aa0efa82d)  
**Group:** stash-alias  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Stashed runtime-coordination extraction**

- Inspected stash delta adds jackin-runtime-coordination and moves permanent coordination inode handling out of runtime.
- Protect namespace from prunable-root overlap, permission/I/O ambiguity, missing paths containing parent traversal, symlink traversal, non-owned/non-private/non-regular or multiply linked state inodes; lock ownership must not unlink persistent inodes.
- Exact policy manifest already lists this crate, but body, test and third-parent equivalence remain unproved.
- Inspect every stash parent; top-level stash delta alone can omit untracked Cargo.toml/lib.rs or README payloads.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `Cargo.toml`
- `Cargo.lock`
- `crates/services/jackin-runtime-coordination/src/coordination.rs`
- `crates/services/jackin-runtime-coordination/src/coordination/tests.rs`
- `crates/services/jackin-runtime/src/runtime/coordination.rs`
- `crates/tools/jackin-xtask/src/arch.rs`
- `policy/owned-packages.json`
- `policy/package-groups.json`
- `ratchet.toml`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Exact-tip aliases: [B047](b047-stash-alias-53dd4c6affcc.md), [B048](b048-stash-alias-b7b19a9f3034.md), [B052](b052-stash-alias-01522f00a01b.md), [B054](b054-stash-alias-68ff05fe0f69.md), [B056](b056-stash-alias-cbbb9b2a9c6e.md), [B057](b057-stash-alias-09031c14e05c.md), [B059](b059-stash-alias-ebc2e2faed24.md), [B060](b060-stash-alias-15ca898cb2c6.md), [B062](b062-stash-alias-dca0343dfb05.md), [B068](b068-stash-alias-929b5577bb5f.md), [B069](b069-stash-alias-8836a20a65fb.md), [B070](b070-stash-alias-f574e4820623.md), [B072](b072-stash-alias-64003e4d826d.md), [B075](b075-stash-alias-770a8c6e1ecf.md). These share object analysis but retain separate coverage rows.

Related decisions: [CHG-012: COMBINE](../02-change-coverage-matrix.md#chg-012), [CHG-013: COMBINE](../02-change-coverage-matrix.md#chg-013)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-012:** Prunable overlap, parent/symlink traversal, missing/permission ambiguity, wrong owner/mode/type/link count, cleanup and lock release. Risk: Lock identity substitution or pruning active authority.
- **CHG-013:** Nested same-process handles, different paths, blocking-pool acquisition/cancellation, same owner/repo on different hosts. Risk: Wrong cached repository reuse or lock lifetime regressions; not a confirmed exploit from inspection alone.

Mandatory remainder: `REM-050` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-010 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `80ab98038e945ec48c82dad241ebf33aa0efa82d`](https://github.com/jackin-project/jackin/commit/80ab98038e945ec48c82dad241ebf33aa0efa82d). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

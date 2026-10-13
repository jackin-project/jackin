# B020 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-c7b8a95856cce6f5e8442f0e6fafcf025a089132/tip-c7b8a95856cce6f5e8442f0e6fafcf025a089132`  
**Tip:** [`c7b8a95856cce6f5e8442f0e6fafcf025a089132`](https://github.com/jackin-project/jackin/commit/c7b8a95856cce6f5e8442f0e6fafcf025a089132)  
**Group:** archive-keeper  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Untracked CI evidence implementation and workflows**

- Untracked recovery tree contains schema-5 CI evidence code and generated ci-evidence / ci-push-head-ledger workflows.
- The implementation includes durable push-head provenance, retained artifact bytes, workflow identity and generator-state contracts. Read this tree as an untracked payload, not as an ordinary complete project checkout.
- Old generated workflow permissions, cancellation and tool-install ordering need review. A contents-only permission declaration is not proof of sufficient artifact-read permissions.
- Compare against 1c42a33 and 14bd147 and the current policy ledger. Never restore old generated jobs merely to satisfy stale contract tests.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `.github/workflows/ci-evidence.yml`
- `.github/workflows/ci-push-head-ledger.yml`
- `crates/jackin-xtask/src/ci_evidence.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-019: COMBINE](../02-change-coverage-matrix.md#chg-019)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-019:** Missing producer, missing predecessor, wrong before SHA, mismatched workflow/tree/digest, partial status, fixture and conflicting terminal observations. Risk: False-green reliability or stale evidence reused across heads.

Mandatory remainder: `REM-020` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-006 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `c7b8a95856cce6f5e8442f0e6fafcf025a089132`](https://github.com/jackin-project/jackin/commit/c7b8a95856cce6f5e8442f0e6fafcf025a089132). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

# B005 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d/tip-1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d`  
**Tip:** [`1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d`](https://github.com/jackin-project/jackin/commit/1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d)  
**Group:** archive-keeper  
**Server protected flag:** false  
**Inspection:** TIP_PARTIAL_PATCH_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Durable CI evidence ledger, schema 4**

- Cherry-pick attribution names source 0c656c4159c4d6167e8d804a938b6419efa5f6d1.
- The ledger records expected CI/Main and Desktop obligations independently of observed runs so never-started workflows remain missing. Cancellation and infrastructure failure remain non-green.
- Fixture denominators cannot qualify production-green claims. Run/workflow/tree/artifact identities and conflicting observations must remain attributable.
- Compare the schema-5 hardening at 14bd1471 and c7b8a958, and the selected current CI contract. Keep behavior and negative tests without restoring stale workflow names or schema-4 serializers.

## Relationship and unique commits

Main-relative merge base and ahead/behind were not computed. No zero-divergence or containment claim is made.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `Cargo.lock`
- `crates/jackin-xtask/Cargo.toml`
- `crates/jackin-xtask/src/ci_evidence.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-019: COMBINE](../02-change-coverage-matrix.md#chg-019)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-019:** Missing producer, missing predecessor, wrong before SHA, mismatched workflow/tree/digest, partial status, fixture and conflicting terminal observations. Risk: False-green reliability or stale evidence reused across heads.

Mandatory remainder: `REM-005` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-006 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d`](https://github.com/jackin-project/jackin/commit/1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

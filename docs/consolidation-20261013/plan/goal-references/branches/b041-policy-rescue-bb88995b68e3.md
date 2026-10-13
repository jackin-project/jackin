# B041 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/rescue/local/20261011-110605-42b7822b/bdb98a128593444a2c9f2482/rust-policy-rollout-ef238a7e`  
**Tip:** [`ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`](https://github.com/jackin-project/jackin/commit/ef238a7e4720fe1be8e8e295e1c502f2ba529e0b)  
**Group:** policy-rescue  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Recovered test-split import/helper changes**

- Versus policy tip: one ahead, 362 behind; merge base 32d78964698cdd1d97eb9fcb180e410348e46d70.
- Visible comparison changes test case modules, imports and support helpers across adapters/apps/core. Large comparison is capped; not all changed files are proven covered.
- Compare discovered test identities and assertion bodies against the final policy layout. A lint/import fix is useful only if test discovery is preserved.

## Relationship and unique commits

Observed comparison base `fba69d60c2373156dc0dcecee8e085c83776966f`: head ahead 1, behind 362, merge base `32d78964698cdd1d97eb9fcb180e410348e46d70`, status diverged. This is a comparison to a non-main base; main-relative divergence remains uncomputed.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/adapters/jackin-docker/src/docker_client/tests.rs`
- `crates/adapters/jackin-image/src/agent_binary/tests/support.rs`
- `crates/apps/jackin-capsule/src/daemon/tests.rs`
- `crates/core/jackin-config/src/accounts/tests.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-018: COMBINE](../02-change-coverage-matrix.md#chg-018), [CHG-028: COMBINE](../02-change-coverage-matrix.md#chg-028)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-018:** Per-assertion catalog map, payload inspection, TLS compile/feature cases, rejected endpoint choices. Risk: Security regressions hidden by structural test churn.
- **CHG-028:** Per-package/feature/test identity map, body review, actual nonzero selection and platform disposition. Risk: False confidence from inflated counts or undetected orphan modules.

Mandatory remainder: `REM-041` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-012, TASK-014 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`](https://github.com/jackin-project/jackin/commit/ef238a7e4720fe1be8e8e295e1c502f2ba529e0b). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

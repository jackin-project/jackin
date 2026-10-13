# B044 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/rescue/local/20261011-110605-42b7822b/dirty/jackin-s7-usage-d5c53271a9`  
**Tip:** [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48)  
**Group:** dirty  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Recovered s7 usage working state**

- Compared against 3ddbc056: 10 ahead, 1 behind, merge base d32e85563648931f80ffab141bb2650f478c2a60.
- Visible changes split coordinator/state, host accounts/projection/broker/publish, token monitor, provider parsing, and snapshot-store code under the intermediate jackin-usage crate.
- No claim that all these moves are semantically neutral. Compare with final policy leaf crates and live-repair branch; retain auth, freshness, locking and storage behavior.

## Relationship and unique commits

Observed comparison base `3ddbc0563ed95225b0004c393b3928fb599ee066`: head ahead 10, behind 1, merge base `d32e85563648931f80ffab141bb2650f478c2a60`, status diverged. This is a comparison to a non-main base; main-relative divergence remains uncomputed.

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/services/jackin-usage/src/coordinator.rs`
- `crates/services/jackin-usage/src/host/broker.rs`
- `crates/services/jackin-usage/src/host/broker/authorize.rs`
- `crates/services/jackin-usage/src/usage/claude/windows.rs`
- `crates/services/jackin-usage/src/usage_snapshot_store/schema.rs`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-004: COMBINE](../02-change-coverage-matrix.md#chg-004), [CHG-007: ADAPT](../02-change-coverage-matrix.md#chg-007)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-004:** Mixed env/profile, duplicate launch-key binding, mismatched fingerprint/source/account, closed alias-set and zero-call denial tests. Risk: Privilege expansion through permissive reconciliation.
- **CHG-007:** Per-window partial observations, missing used/reset, stale session with fresh weekly, bounded percent and synthetic zero-call fixtures. Risk: Misleading quota availability or fresh-looking stale data.

Mandatory remainder: `REM-044` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-007, TASK-008 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

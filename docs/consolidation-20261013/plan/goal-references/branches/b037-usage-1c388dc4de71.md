# B037 — Branch dossier

[Inventory](../01-branch-inventory.md) · [Coverage matrix](../02-change-coverage-matrix.md) · [Execution tasks](../05-execution-tasks.md)

## Identity and scope

**Exact server ref:** `refs/heads/fix/claude-usage-live-repair`  
**Tip:** [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7)  
**Group:** usage  
**Server protected flag:** false  
**Inspection:** PARTIAL_SEMANTIC_REVIEW  
**Complete unique-history review:** OPEN  
**Independent review:** NOT_RUN (coordinator inspection only)  
**Owner/upstream/local tracking relationship:** not established from the server branch listing.

## Purpose and observed changes

**Claude usage live repair and monitor evidence**

- 25 commits ahead of target, zero behind; diverged from policy tip at target (policy has 366 exclusive commits).
- Changes cover auth/coordinator/broker/protocol, per-window quota evidence and schema migration, synthetic CLI-provider fixtures, and live handoff records.
- Latest handoff explicitly says live acceptance is false. v12 can derive zero used from 100 percent remaining, but that fix has only isolated smoke evidence in the source record, not live verification.
- Source handoff reports schema 5 to 6 migration, wire v9, stable binding and observe-only monitor identity. Source checkpoint db59c4015fb4715e1d39157aaa31f4a1497c823a identifies the v12 candidate.
- Observe-only runnable=false is not task authorization. Do not fabricate a reset timestamp, turn stale quota into fresh evidence, or authorize provider requests through a passive observer.
- Do not copy historical local paths/PIDs into automatic commands. Stopping a live owner, Keychain authentication, opening real migrated state, or provider cutover each requires explicit operator approval.
- Adapt into selected layered crates after symbol mapping; retain test coverage, not stale flat-layout duplicates.

## Relationship and unique commits

Observed comparison base `1c3dd1be4a3b1ed518133ec3899c084c792370ff`: head ahead 25, behind 0, merge base `1c3dd1be4a3b1ed518133ec3899c084c792370ff`, status ahead. 

The pinned tip above and any source/checkpoint SHAs described here are evidence anchors. This dossier does **not** claim that a tip patch equals the branch's entire unique change set. TASK-002 must enumerate the complete unique history, merge resolutions, squash-equivalent behavior and relevant parent trees. Source-record historical test claims are not planning test results.

## Inspected files/components

- `crates/jackin-protocol/src/usage_broker.rs`
- `crates/jackin-protocol/src/usage_monitor.rs`
- `crates/jackin-usage/src/coordinator/policy.rs`
- `crates/jackin-usage/src/host/broker/monitor/projection.rs`
- `crates/jackin-usage/src/host/broker/monitor/storage.rs`
- `crates/jackin-usage/src/host/broker/monitor/tests/fingerprint_migration.rs`
- `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs`
- `crates/jackin-usage/src/usage/claude.rs`
- `crates/jackin-usage/src/usage/claude/lease.rs`
- `crates/jackin/src/bin/usage-broker-fixture.rs`
- `crates/jackin/tests/usage_provider_flow.rs`
- `crates/jackin/tests/usage_provider_flow/auth_outcomes.rs`
- `plans/claude-unattended-monitor/claude-code-handoff.md`

File paths above are source-side evidence, including removed/intermediate paths where applicable. Use the approved TASK-004 map for final child-file ownership rather than assuming all source paths exist at P.

## Overlap and selected behavior

Related decisions: [CHG-006: COMBINE](../02-change-coverage-matrix.md#chg-006), [CHG-007: ADAPT](../02-change-coverage-matrix.md#chg-007), [CHG-008: ADAPT](../02-change-coverage-matrix.md#chg-008), [CHG-009: COMBINE](../02-change-coverage-matrix.md#chg-009), [CHG-031: ADAPT](../02-change-coverage-matrix.md#chg-031)

Recommended integration applies only to the inspected behavior. Never cherry-pick an entire recovery/stash/probe branch on the strength of this dossier. Missing valuable behavior is adapted into the selected architecture with source-attributed tests; identical destinations need proof before SUPERSEDED.

## Verification, risks and closure

No Jackin tests were executed for this branch during planning. No independent agent approved it. The full commit/file/parent set and comparison to the final integration remain open. Broad API responses can truncate patches/files; a file rename or empty stat cannot prove behavioral retention.

Specific validation obligations:

- **CHG-006:** Port 429 text; typed 403 with message mentioning 401; typed 401/429; transport/decode text; absent authoritative retry deadline. Risk: False authorization denial, fabricated rate deadlines or incorrect inference-only classification.
- **CHG-007:** Per-window partial observations, missing used/reset, stale session with fresh weekly, bounded percent and synthetic zero-call fixtures. Risk: Misleading quota availability or fresh-looking stale data.
- **CHG-008:** Observe-only goal_id/runnable output, no launch/auth/provider dispatch, absent/denied capabilities. Risk: Unattended activity against live credentials.
- **CHG-009:** All supported predecessors, interruption, malformed/ambiguous records, fingerprint stability, mixed-version rejection and recovery. Risk: Irreversible state changes or silently incompatible clients.
- **CHG-031:** Link/source SHA checks, exact current task commands and NOT_RUN/live distinction. Risk: An executable runbook accidentally performs a live cutover or claims unearned readiness.

Mandatory remainder: `REM-037` in the [matrix](../02-change-coverage-matrix.md). Close it through TASK-002; use TASK-008, TASK-009, TASK-016 for the approved implementation/validation. Retain original refs and all failed evidence.

- [ ] Complete unique commits and all relevant parent payloads inventoried.
- [ ] Every meaningful behavior/test/config/documentation change classified with destination proof.
- [ ] Actual required tests executed on the correct revision and host.
- [ ] A different reviewer approved the source-to-result mapping.
- [ ] Final loss-prevention audit reconciles this exact ref.

## Evidence provenance

Primary inspected object: [commit `2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7). Inventory comes from paginated GitHub branch reads on 13 October 2026 (Asia/Singapore). [Normalized observation record](../evidence/observations.json) retains findings and retrieval labels; it is curated analysis, not a verbatim API export. See [evidence limitations](../09-evidence-and-limitations.md).

# C17 — Recovery, reconciliation and preservation

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Recover durable workflow truth after daemon, Capsule, backend or storage failure while preserving unfinished work. Coordinate reconciliation in `jackin-workflow`; extend existing runtime/isolation cleanup with mandatory workflow preservation holds.

## 2. Existing foundation and gap

Jackin already has isolation state/finalization/cleanup/safe-removal modules and Capsule Keep/Discard exit choices. Workflows need additional durable ownership and only-copy rules across those paths. A failed run must be reportable even when its sandbox remains retained. [J07](#evidence-and-references), [J14](#evidence-and-references).

## 3. API and data contracts

```text
reconcile_host(controller_epoch) -> ReconciliationReport
reconcile_run(run_id, expected_version) -> RunRecoveryPlan
reconcile_session(binding, pending_commands) -> SessionRecoveryAssessment
preserve_work(resource, reason, evidence) -> PreservationHold
assess_cleanup(resource, proposed_action) -> Allowed(RemovalPermit) | Held(reasons)
release_hold(hold_id, verified_copy_evidence, actor) -> Receipt
recover_failed_run(run_id, selected_attempt, permit) -> Continuation
```

`ReconciliationReport` enumerates adopted, stopped, missing, conflicting and retained resources plus unresolved effects. A hold protects a transitive set of repository objects, uncommitted state, session diagnostics and artifact references, not just a directory name. `CopyEvidence` identifies an exact content/OID set, verified local archive or remote receipt, accessibility/encryption-key status and verification time.

## 4. Required recovery sequence

Acquire exclusive local controller ownership and a new fencing epoch. Open/validate migrations and the ledger, then stop new dispatch while reading outbox states, active instances and Capsule epochs. Match resources only using exact run/attempt/effect ownership. Adopt one matching instance; multiple matches or unexpected digests are conflicts, not an invitation to pick whichever is newest.

For sessions, compare physical epochs and vendor IDs. A host-daemon restart may reconnect to the unchanged original Capsule session. A Capsule/process crash cannot preserve a dead original process; certified vendor resume can restore the conversation in a newly recorded physical session, and replay preserves prior terminal history. Report this distinction explicitly.

Resolve pending commands from Capsule’s journal and certified vendor evidence. Unknown delivery remains operator-required; no blind resend. Inspect test/review workers for exact targets before collecting results. Reconcile remote ref/PR mutations before issuing new external effects. Rebuild scheduler reservations from observed owned resources and durable attempts; stale reservations are not removed until the resource disposition is known.

## 5. Preservation contract

Every workflow implementation repository receives a hold before writable work begins. Cancellation, failure, timeout, ordinary retention expiry, `eject`, `prune`, `purge` and other Jackin cleanup paths must consult workflow holds. Existing manual non-workflow cleanup behavior remains compatible; ownership determines the additional guard. `force` is not a workflow only-copy override.

Unique unpushed/uncommitted work remains retained until a verified independent copy covers it. A remote push of one commit does not cover later untracked files. A local archive is not a recoverable copy if corrupt or its decryption key is unavailable. Under disk pressure, stop new work and ask for preservation action rather than automatically delete held state.

After failure, default preservation quarantines the sandbox: stop unneeded worker activity, revoke credentials/network access and retain volumes/checkpoints/diagnostics. A separately authorized diagnostic attachment may inspect it. Keeping a sandbox does not mean leaving privileged credentials or unbounded network access active indefinitely.

Normal workflow cleanup cannot discard the only copy. A future explicit destructive user operation outside ordinary workflow cleanup would require separate narrowly scoped design and unmistakable confirmation; it is not part of v1 automatic behavior. Jackin cannot prevent an operating-system owner from manually deleting files outside its APIs, and does not claim protection against complete disk loss without an independent backup.

## 6. Failure reporting and retention

Persist `failed` or `cancelled` plus retained-resource/cleanup status immediately when the disposition is known. Cleanup retries are separate effects with their own bounds. Failed deletion cannot turn a failed workflow back into an indefinitely running one. Retained sandbox listings include run identity, reason, exact unbacked content, estimated size and permitted recovery/export actions.

## 7. Build instructions

Implement a reconciliation simulator with host/instance/session/database observations and deterministic decisions. Add workflow labels and controller fencing before real adoption. Add preservation-hold lookups to all existing destructive entry points using existing safe-removal primitives. Implement quarantine and independent-copy verification. Test crash points from the architecture recovery matrix against the real storage/backend stack.

## 8. Dependencies and integration

C04 lifecycle, C06 command journal, C08 checkpoints, C10 ledger, C11 artifacts, C16 remote effects, C22 retention. Recovery never infers a successful implementation from a surviving process or an agent’s latest message.

## 9. Acceptance tests

`REC-01`: host restart adopts original live session without duplicate input. `REC-02`: Capsule restart changes physical epoch and reports resume honestly. `REC-03`: conflicting owned instances stop automatic adoption. `REC-04`: every normal cleanup path respects unpushed/uncommitted holds. `REC-05`: disk pressure preserves rather than deletes unique work. `REC-06`: failure reports despite retained sandbox. `REC-07`: quarantine removes active privileges. `REC-08`: missing/corrupt backup does not count as a copy.

## Evidence and references

- **J05 — [Runtime public integration surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime/src/lib.rs).** Full file. Existing launch, inspect, spawn, attach and cleanup re-exports; reactive daemon feature boundary.
- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J14 — [Git isolation public surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-isolation/src/lib.rs).** Full file. Existing materialization, finalization, cleanup, state and safe-removal boundaries.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.

## Explicit unknown-command resolution

Expose `resolve_unknown_command(command_id, resolution, actor, expected_versions, reason)` through C18. A certified lookup may prove delivery/absence; otherwise operator choices are `observed_received`, `authorize_replacement` or `abandon`. Keep the original uncertain receipt. A replacement is a new command/effect with `replacement_of`, fresh grant/lease checks and an explicit warning that the original may have been processed. Record operator attestation separately from vendor proof.

On restart, reconcile active finalization records and their writer barriers before acceptance/publication. A lost or unprovable barrier invalidates eligibility until a new verified finalization boundary is established; it is not silently assumed intact.

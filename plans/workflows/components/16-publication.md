# C16 — Checkpoint publication and final GitHub publication

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Back up authorized checkpoints and publish accepted results through host-controlled Git/GitHub effects. Keep publisher credentials and remote mutation authority in the host control plane, using existing credential/process integrations.

## 2. Existing foundation and gap

The current isolation and host credential boundaries are reusable. Workflow-specific publication records, exact-revision acceptance guards and reconciliation remain new contracts. Existing research already separates GitHub visibility from execution authority and prohibits unapproved merge behavior. [J04](#evidence-and-references), [J07](#evidence-and-references), [J14](#evidence-and-references).

## 3. API and data contracts

```text
backup_checkpoint(checkpoint, backup_grant, effect_id) -> PublicationHandle
publish_accepted(certificate, PublishSpec, action_permit, effect_id) -> PublicationHandle
reconcile_publication(effect_id, expected_remote_target) -> PublicationObservation
export_artifacts(accepted_or_explicit_diagnostic_scope, export_policy) -> PublicationReceipt
```

`PublishSpec.action` is `push_checkpoint`, `push_accepted`, `create_pr` or `export_artifacts` in v1. Merge is not implicit and is rejected unless a separately implemented, explicitly authorized future operation is enabled. A PR can target the configured base branch without merging into it.

Records include exact repository/remote, source OID, run-specific branch, expected old remote OID, target base branch/OID, required remote checks, certificate/decision IDs, action, effect idempotency key, remote ref/PR ID and reconciled receipt. Credentials are references resolved at dispatch, not fields in the plan or artifact.

## 4. Required functionality

Checkpoint backup is allowed before acceptance only when the frozen plan and current host policy authorize it. Use a run/attempt-specific remote branch such as `jackin/runs/<run-id>/<node-id>/attempt-<n>` with validated IDs. Backup publishes the sealed checkpoint, not a live mutable worker branch. Suspected secrets or unapproved artifact classifications block external backup and keep local preservation holds.

Final publication validates the latest-candidate acceptance certificate immediately before creating the outbox intent and again at dispatch. Check required local/remote checks against the exact OID and trusted check identity. Reconcile target-base changes according to policy; if integration with a new base is required, create a new candidate and reverify. Never replace the accepted OID with whichever branch head happens to exist.

Push only to authorized refs under a non-destructive expected-old/new update contract. Require fast-forward unless creating a new empty run branch; no implicit force push, tag replacement, branch deletion or default-branch mutation. Use a transport that can enforce/reconcile the expected ref update, not an unchecked shell command. A remote divergence causes conflict/operator action. A force-with-lease option is not silently used as a substitute for the no-force policy.

PR creation includes a unique run/effect marker and a compact provenance summary. On a lost response, query that marker and exact repository/head/base before creating another PR. If multiple matching PRs exist, surface a conflict rather than choose arbitrarily. Store provider request/response identity with sanitized evidence. Remote API rate limits and temporary outages use bounded retry after reconciliation.

## 5. Preservation and failure

A local checkpoint becomes remotely backed up only after verifying the remote ref/object receipt for its exact OID. A push request being sent is not proof. A PR URL alone is not proof the latest checkpoint was pushed. Remote deletion or loss can invalidate a copy claim; periodic reconciliation or conservative local holds prevent cleanup based on stale assumptions.

Publication failure does not erase implementation acceptance or unfinished work. Record separate acceptance and publication states. A plan that requires publication cannot report whole-run success until it finishes; an artifact-only plan can finish without network access. Cancellation stops new publication dispatch, but already in-flight effects are reconciled honestly.

## 6. Build instructions

Implement a fake remote with lost-response/duplicate/conflict injection. Add host-side scoped credential resolution and branch target validation. Implement checkpoint push separately from accepted publication. Add expected-ref checks, PR marker lookup and receipt persistence. Wire preservation release only after exact backup verification. Keep arbitrary shell and agent-supplied GitHub mutations outside this port.

## 7. Dependencies and integration

C03 authorizes; C08 supplies sealed OIDs; C10 journals; C15 certifies acceptance; C17 protects unique copies; C22 exports evidence. Any existing Git/GitHub helpers should be extended rather than duplicated after a targeted implementation-time call-site audit.

## 8. Acceptance tests

`PUB-01`: checkpoint backup is distinguishable from acceptance. `PUB-02`: latest accepted OID is the only final source. `PUB-03`: no publishing credential inside workers. `PUB-04`: lost push/PR response reconciles without blind duplication. `PUB-05`: force/delete/default-branch mutations are denied. `PUB-06`: changed remote/base/check identity blocks stale publication. `PUB-07`: unverified backup cannot release the only-copy hold. `PUB-08`: cancellation/policy revocation races are recorded truthfully.

## Evidence and references

- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.
- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J14 — [Git isolation public surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-isolation/src/lib.rs).** Full file. Existing materialization, finalization, cleanup, state and safe-removal boundaries.

## Finalization check at publication

Final accepted publication requires the certificate's active `FinalizationRecord` as well as the latest candidate ordinal. Confirm its writer-fence generation before dispatch; do not publish based on a green snapshot while silently ignoring later unsealed work. Revoking finalization stops new accepted-publication intents; already-dispatched effects remain explicitly reconciled and attributed to the exact published OID.

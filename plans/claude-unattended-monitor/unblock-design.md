# Unblocking Claude Code usage tracking

Historical design proposal from the predecessor implementation. Its diagnosis describes that earlier snapshot, not current shipped behavior. Current contracts and verification are in [bootstrap-contract.md](bootstrap-contract.md), [claude-code-handoff.md](claude-code-handoff.md), and [verification.md](verification.md). Retained for design provenance; do not execute it as a current CLI runbook.

## Verified cause

The existing handoff prevents monitor creation until an operator supplies both
an account binding and a fresh SGD account-total receipt. The implementation
always evaluates the spend guard alongside quota guards. A missing budget is
also unverifiable; omitting `--budget-sgd` does not enable quota-only operation.
Spend receipts are manual operator attestations and expire after 300 seconds.
There is no automatic authoritative SGD billing source in this monitor.

Monitor creation currently persists a goal even when its spend baseline is
absent. A later receipt does not establish that original baseline. Do not start
the real budgeted goal as a readiness probe or rename it to reset attribution.

The account argument is a local partition key. It is not discovered or verified
against Claude credentials. An operator can choose a stable label, but must
confirm its binding; session IDs cannot establish account identity.

## Recommended architecture

1. Separate evidence collection from permission to dispatch. A durable observer
   can ingest and report quota evidence without creating a spend baseline or
   authorizing work. An observation-only state never means runnable.
2. Report tracking, quota, budget and dispatch readiness separately. Keep the
   final dispatch decision authoritative and retain stable machine-readable
   reasons. Doctor reports local service capability, not valid billing or quotas.
3. Persist an explicit budget policy. Verified-SGD policy retains the existing
   unknown-spend pause and SGD40/45/48 thresholds under the SGD50 ceiling.
   A quota-only policy may permit dispatch based on available quota guards only
   after explicit operator approval of the missing SGD protection. It reports
   spend unknown and budget enforcement disabled; it never calls that budget
   verified. Policy approval is a separate, auditable operator action recording
   goal/account scope, previous and new policy, approval time and revision;
   unattended monitor start only consumes that stored policy. Existing goals
   must not silently downgrade their policy, erase
   historical spend, or reset baselines. The previous SGD50 requirement is not
   waived by this proposal. Any operator-authorized change away from strict
   enforcement leaves an explicit coverage gap; enabling strict enforcement
   again must not silently claim complete spend attribution across that gap.
4. Make account binding a separate operator setup record with explicit scope.
   Capture callback session and Claude version. Unbound observations may be
   session-scoped, but must not be merged into an asserted account total.
   Preserve existing statusline output and prepare a settings proposal without
   applying it to the running session.
5. Distinguish callback receipt time, last changed evidence, field freshness,
   and window/model descriptor validity. Identical callbacks or timer reruns
   do not renew provider evidence. Changing a sibling field must not refresh
   every field. Audit the current 300-second expiry of unchanged reset/model
   descriptors; any validity change requires independent tests and a documented
   rule, not a claim that the provider was contacted. Missing/stale required
   usage evidence stays unknown. Never guarantee autonomous freshness or resume.
6. Preserve broker ownership, local-only monitoring, single-flight work,
   persistent deadlines, quota pause latches, ordered action sequences, and
   reset verification. Do not introduce unattended CLI fallback or credential
   access. Optional independent provider observations remain a separate,
   operator-enabled source requiring a verified supported read contract and all
   existing rate-limit/no-dialog protections.
7. Treat waiting for external setup differently from an evidence reset wait.
   Missing binding/policy/adapter requires an operator setup result and a
   checkpoint. Avoid active goal reasoning, repeated completion verification,
   and doctor loops while no monitor exists. A durable active observer should
   keep the local broker alive; a zero-observer broker may still idle-exit.

## Supported data boundary

The [current official statusline documentation](https://code.claude.com/docs/en/statusline)
documents five-hour and seven-day used percentages and reset epochs. Windows
can be independently absent. Local statusline callbacks do not supply a quota
provider observation timestamp or authenticated account identity. Session USD
cost is an estimate, and gateway USD spend is also an estimate. Neither is a
verified SGD account bill. Model-specific limits and extra-usage permission
must remain unknown when not supplied by a supported source.

Quota-only operation therefore requires operator acceptance of its narrower
coverage. It cannot promise an SGD cap or safe automatic extra usage. Hard
billing protection remains an account-side Anthropic spending limit; where
extra-usage state is unobservable, the operator must arrange the account policy
and Jackin must disclose that it cannot independently verify it.

## Durable implementation queue

- [ ] Freeze protocol/CLI contract for observer lifecycle, scope binding,
      readiness dimensions, explicit policy and operator authorization.
      Owner: protocol + CLI agent; coordinate before any shared edits.
- [ ] Implement observer persistence and policy evaluation in broker monitor
      engine. Owner: monitor engine agent after contract freeze.
- [ ] Implement bounded statusline metadata and composition changes.
      Owner: statusline agent; no writes to live Claude settings.
- [ ] Enforce strict budget baseline admission/activation atomically; retain
      historical uncertainty and rollover totals. Owner: spend agent.
- [ ] Migrate projection and usage consumers together; remove superseded
      schema/paths rather than add compatibility aliases. Owner: integration.
- [ ] Independent security and rate-limit reviews. Owners: separate reviewers.
- [ ] Deterministic fake-clock/Keychain/HTTP verification and installed binary
      fixture proof. Owner: verifier; no live provider or Keychain checks.
- [ ] Update handoff only after actual commands/schema and installed pair are
      verified. Operator applies reviewed adapter during an allowed setup window.

## Required additional proofs

Observer start without spend does not authorize dispatch or create a poisoned
goal baseline. Quota-only authorization is explicit, persisted and cannot be
silently selected by an unattended caller for a strict goal. Missing spend stays
unknown in every mode. Strict goal activation without a valid baseline is
atomic and leaves no partial goal. Restart retains binding, policy, action
sequences and evidence ages. Stable callbacks, changing sibling fields, missing
windows, reset transitions and model changes cannot manufacture freshness.
Active observers survive idle/sleep; external setup blockers do not create poll
bursts. Retain all existing threshold, reset, weekly, spend rollover, no-dialog,
deduplication, 401/403/429 and passive-read regressions.

## Deployment boundary

This document changes no code, installed binaries, account policy, service,
Claude settings or running session. Current commands do not implement the
proposed observer or quota-only policy. No fabricated receipt, bypass flag or
new goal ID is an acceptable way to unblock the existing strict goal.

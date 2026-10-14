# 08 — Review and decision log

## Actual planning review status

**No independent agent review was performed.** No delegation/execution tool was available after capability discovery. The coordinator performed repository inspection, synthesis and a separate self-check, but those are not independent opinions. Historical branch documents mentioning reviewers or test passes are source records, not reviews of this planning bundle.

Independent inventory/architecture/plan approval is a hard prerequisite in TASK-001 through TASK-004. Every implementation must be reviewed by an agent/person other than its implementer. A final independent audit is required in TASK-018. The repository's zero required human-approval count does not waive these user-requested requirements.

## Coordinator self-check findings and corrections

| Record | Finding | Disposition in this bundle | Independent status |
| --- | --- | --- | --- |
| PLAN-R01 | No real local checkout; clone failed DNS. | Deliver a clearly labeled planning overlay; local branches, patch IDs, Jackin baselines and source edits remain blocked. | NOT_RUN |
| PLAN-R02 | Branch count and analysis depth are different measures. | Preserve 79 exact refs / 50 distinct tips; give every ref a dossier and an explicit OPEN remainder. Do not equate tip triage with exhaustive history review. | NOT_RUN |
| PLAN-R03 | Large API comparisons cap file lists; missing entries/zero stats are not absence of changes. | Document truncation and require full local Git/all-parent/semantic closure before implementation. | NOT_RUN |
| PLAN-R04 | Squash/cherry-pick/recovery history makes ancestry-only decisions unsafe. | Make previous consolidation evidence a comparison input, not proof of present behavior; distinguish source-reported checks. | NOT_RUN |
| PLAN-R05 | Candidate paths differ from intermediate rescue paths. | Verify P manifest categories: diagnostics/telemetry services, OMP/FFI adapters, purge owner cleanup-absent-guards. Require exact child-symbol/test map before writes. | NOT_RUN |
| PLAN-R06 | M and P testing instructions differ. | Keep original baseline Cargo commands separate from P's pinned MBX/Rust 1.99 and native-local Mise commands. Validate command/test existence at each exact head. | NOT_RUN |
| PLAN-R07 | Source instructions contain push/live/scanner-bypass commands. | Do not follow them as authorization. Explicit no-push, no live credential/provider/state mutation, no protection/scanner bypass. | NOT_RUN |
| PLAN-R08 | Same-tip stash aliases and synthetic merge histories need careful handling. | Deduplicate objects only; retain every alias, all parents, internal merges and useful behavior. Reject only proved debris/state, not unseen ancestry. | NOT_RUN |
| PLAN-R09 | A self-authored plan can look independently reviewed if phases are relabeled. | State reviewer status NOT_RUN everywhere; independent approval remains a gate. | NOT_RUN |
| PLAN-R10 | Old handoff commit ID was available only abbreviated. | Retain the source abbreviation rather than invent a 40-character object ID. All integration sources use verified full tips or explicitly unresolved references. | NOT_RUN |
| PLAN-R11 | Sidecar source binary path and final purge owner needed correction. | Use observed runtime `src/bin/usage-broker/main.rs` source and P cleanup-absent-guards package; do not copy intermediate path assumptions. | NOT_RUN |
| PLAN-R12 | Read-only collector originally checked only worktree/private Git output paths and status stability. | Added common Git directory exclusion and HEAD/ref-stability checks; extended synthetic tests to linked worktrees, multi-parent objects and shallow history. Eight synthetic tests pass. | NOT_RUN |

## Architecture decisions — proposed, not independently approved

The authoritative source-specific records are the [change matrix](02-change-coverage-matrix.md), [conflict analysis](03-overlaps-and-conflicts.md) and [architecture](04-target-architecture.md). There are 33 seed decisions with branches, verified source tips, behavior, alternatives, rationale, task, tests and risks. Every seed currently says `PROPOSED; independent review NOT_RUN`; destination revisions are deliberately null because no integration occurred.

Conditional preference: review/adapt P's layered architecture rather than merging its entire history; preserve M foreground/passive/monitor invariants; adapt U typed evidence/monitor behavior without claiming live acceptance; reconcile T and recovered tooling. Prefer durable push-head/artifact obligations over first-parent history alone, safe task-scoped wrappers over global wrappers, and real native product/coverage preservation over stale generated files. Reject evidenced probe debris and known-invalid conflicted/generated trees while examining their ancestors and parent payloads separately.

No branch owner, code-review approval, final destination commit or successful current test has been invented. Actual author metadata is not treated as ownership. `SUPERSEDED` can describe a rejected design in favor of another design; it is not a claim the chosen code has already landed unless destination evidence proves that.

## Independent plan-review checklist

- [ ] Reconcile all local heads/tracking refs/server advertisements and every source object, including stale/experimental/recovery branches and stash parents.
- [ ] Audit every meaningful change and each OPEN remainder; verify patch-equivalence, squash and behavioral mappings rather than trust tip labels.
- [ ] Challenge chosen architecture, security/lease/data/native/CI contracts and tests; confirm no important behavior is lost.
- [ ] Confirm every task's dependencies, exact ownership, commands, acceptance, rollback and separate reviewer are executable in the actual checkout.
- [ ] Check all links, branch IDs/SHAs and source attributions; artifact validation does not replace semantic review.
- [ ] Review the standalone prompt for authorization boundaries, initial evidence gates and honest blocked reporting.

## Execution review record requirements

For each review, append task/change ID, reviewer identity, exact reviewed commit/tree, source comparisons, findings with severity and code/test evidence, implementer response, fixing commit, re-review and final verdict. A reviewer must inspect the actual diff and tests; repeating an implementation summary is not review. Return significant failures for correction and re-review before integration. No approval by the same implementer.

Resolve PR comments/replies/outdated or unresolved threads using evidence. Accepted findings need the fixing commit and revalidation; rejected findings need a supported rationale. General comments require a disposition too. Re-fetch all feedback at the final head. Publishing replies, resolving threads, pushing and merging remain remote writes requiring the authorization appropriate to that action. A general merge request does not waive unaddressed feedback.

The coordinator alone updates GOAL's authoritative progress ledger from verified task outputs. States are PENDING, IN_PROGRESS, IN_REVIEW, BLOCKED, FAILED, VERIFIED and DONE. During this planning delivery, implementation tasks are not completed; source analysis/test/independent-review/permission gaps remain explicit.

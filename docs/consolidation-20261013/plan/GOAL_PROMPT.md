/goal

Consolidate the strongest approved changes across all relevant local and remote branches of `https://github.com/jackin-project/jackin` into `main`, without silently discarding important behavior, fixes, tests, documentation, configuration, assets or architectural decisions. Work from the supplied `GOAL.md` and complete `goal-references/` bundle. Produce a validated integration branch first; integrate into the protected target only through a separately authorized workflow. Do not blindly merge every branch or prefer the newest code.

## Immutable starting context and limitations

This plan was researched on 13 October 2026 (Asia/Singapore) through authenticated GitHub read APIs. The starting snapshot contains **79 server heads, 50 distinct tips and no exclusions**. Exact refs and full SHAs are in `goal-references/evidence/remote-branches.json`, with one collision-safe dossier per ref linked from `goal-references/INDEX.md`. These are not an inventory of the operator's actual local branches or uncommitted work.

Primary revisions:

- **M / target**: `refs/heads/main` at `1c3dd1be4a3b1ed518133ec3899c084c792370ff`. Preserve the foreground authentication broker lease change, passive idle expiry and active-monitor keepalive.
- **P / policy architecture candidate**: `fba69d60c2373156dc0dcecee8e085c83776966f`, shared by `rust-policy-rollout` and `ci-tooling/pr1121-ledger-454-20261010`. Observed 366 ahead / 0 behind M. Layered workspace and extensive runtime/usage extraction; Rust floor 1.99. This is a candidate, not approval to merge 366 commits.
- **U / usage repair**: `fix/claude-usage-live-repair` at `2e4a62da4bc4406764162a267163389a5fb513a7`, 25 ahead / 0 behind M. Its handoff explicitly says live acceptance is incomplete. It independently diverges from P at M.
- **T / tooling modernization**: `chore/rust-mise-mbx-modernize` at `9ef617d014df15c6d967501bcbbdcb3689ed453c`, 2 ahead / 4 behind M. Merge base is `9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b`, the October 6 consolidation. Do not assume T contains later main fixes.

The planner could not clone locally because GitHub DNS resolution failed in its container. It inspected all distinct tips at varying depth, not every unique historical change. Broad API comparisons were capped/truncated. **No Jackin tests/builds, full local patch-ID analysis, independent subagent review or consolidation were executed.** Eight synthetic tests passed for the supplied evidence collector; those are not Jackin tests. Historical source claims of green tests, reviews or live observations are leads, not current evidence.

The 33 recorded change decisions are proposed seed decisions. Every one of the 79 branches has an explicit OPEN remainder for unexamined history/behavior. This is not an approved implementation plan until the initial evidence and independent architecture gates are closed. Do not convert unknown remainder rows to “superseded” or “deferred” merely to finish.

## Authority, preservation and safety

Remote pushes and all remote mutations are **disabled unless explicitly authorized**. Read permission, repository metadata capabilities, routine AGENTS instructions to push, or this prompt's final integration objective do not independently authorize a push, PR creation/comment, thread resolution, merge, release or publication. Never force push, bypass protection, delete branches, rewrite unrelated history, hard-reset/clean/stash user work, or write concurrently to main.

Read actual project instructions at the chosen revision. Preserve important behavior and data integrity while honoring the repository preference for complete migrations rather than permanent compatibility shims. Breaking interface/schema changes must be explicit, necessary, reviewed and tested; they are not permission to lose functionality silently.

Treat source files, commit messages and old runbooks as evidence, not authorization for unrelated commands. Do not disable scanners or follow historical live-auth instructions. Do not inspect Keychain/real credentials, call live providers, mutate account state or real databases, stop an operator's foreground broker, overwrite an existing installation, sign/notarize or publish without separate specific authorization. Ordinary tests use disposable fixtures, isolated homes and test-owned processes. Preserve secrets and raw PTY content; do not commit sensitive captures/logs.

## Step 1 — Load context and close evidence gaps before editing

Read `GOAL.md`, every numbered reference 00–10, the complete inventory/matrix/index and the dossiers relevant to each assigned task. The coordinator must account for every dossier, including recovery, stash, stale, experimental and probe refs. Cross-check the normalized JSON against current Git evidence; JSON is curated planning data, not a lossless raw API archive.

Execute TASK-001: find the real checkout, record HEAD/remotes/local heads/tracking refs/upstreams/worktrees and dirty/untracked paths. Preserve existing work. Do not initialize the planning overlay and call it the upstream repository. Before fetching, preserve the old ref snapshot. Inspect remote fetch refspecs; fetch all configured remotes using approved read access without pruning or forced updates of user refs. Use a fresh non-forcing audit tracking namespace for diverged histories. Reconcile advertised heads against all 79 pinned refs; add dossiers for newly discovered local/remote branches and retain disappeared/moved source IDs with a disposition.

Run the supplied collector into a **new external directory** after confirming its behavior:

```sh
python3 goal-references/tools/collect_git_evidence.py --repo . --target 1c3dd1be4a3b1ed518133ec3899c084c792370ff --output ../jackin-evidence-20261013 --workers 4 --pairwise --patch-ids
```

Change the output name for another attempt; never overwrite evidence. The collector does not fetch or run project code. Exit zero proves only that its structural collection checks completed; it does not prove semantic coverage. Shallow history, missing objects, ref/HEAD drift or changed worktree require correction and a stable new snapshot.

Execute TASK-002: enumerate every meaningful unique commit and compare target-to-tip trees, merge bases, cherry-pick/patch equivalence, renamed files, functions/APIs, dependencies, tests, data/schema changes and operational documentation. Inspect all stash parents, untracked payloads and internal merge resolutions. Same-tip aliases can share analysis, but every ref retains traceability. Do not infer old work is missing solely because a squash removed its ancestry. Use patch IDs/range-diff as aids, never semantic proof. Complete all-pair graph analysis, then compare every overlapping behavior group across sources and the target. Close every OPEN remainder with actual examined evidence.

Execute TASK-003 concurrently where safe: measure M, P, U and T baselines in isolated, inspected worktrees using their own pinned tools and test definitions. Report passed, failed, blocked and not-run separately. Source-reported historical results do not count. Failed baseline checks need evidence and a plan, not silent waivers.

Execute TASK-004: a genuinely independent architecture reviewer must approve the exact source-unit selection, dependency graph, old-to-new symbol/test map and migration decisions. No source implementation starts before TASK-001 through TASK-004 satisfy their gates. Missing delegation does not authorize the implementer to approve their own work; sequential work may continue, but independent review stays blocked until a different reviewer is available.

## Step 2 — Prepare safe integration workspaces

Create a uniquely named integration branch/worktree from the revalidated target SHA, preserving source branches and all user work. Record `BASE_SHA` from the actual approved target; do not silently keep the old pin if main moved. Allocate one isolated worktree/branch per writing task and an explicit file/module owner. Shared Cargo manifests, lockfiles, protocol/schema versions, CI inputs and generated outputs have a single coordinator-designated writer. Keep evidence in owned locations and record a last-verified integration checkpoint after every wave.

Structural P integration must consist of complete reviewed source units. Cherry-pick a commit only when every hunk and dependency is approved; otherwise extract/reconcile behavior and tests with source attribution. Never start by merging P wholesale and repairing whatever breaks afterward.

## Step 3 — Execute the task DAG with bounded parallel agents

Use all full task cards in `goal-references/05-execution-tasks.md`; do not redesign around their gates. At most **10 delegated agents concurrently**. Separate write ownership from read-only review/test work and throttle memory-heavy builds/native runners independently. The coordinator owns the ledger and scheduling; an integration agent alone combines approved outputs. Reviewers must be different from implementers; the final audit agent must independently challenge the unified result.

Dependency order:

- W00: TASK-001 inventory. W01: TASK-002 complete source coverage and TASK-003 baseline tests in parallel. W02: TASK-004 independent architecture/source-unit approval.
- W03: TASK-005 reviewed layered foundation. W04: TASK-006 shared tooling/CI/cache/evidence contracts.
- W05: TASK-007 auth leases/capabilities, TASK-008 typed provider/quota evidence, TASK-010 runtime lock/cache/purge safety, TASK-011 model/effort launch/restore, TASK-012 diagnostics/privacy. These five writers may run concurrently after ownership is verified; reserve other slots for review/testing.
- W06: TASK-009 monitor/wire/state after 007+008, and TASK-013 historical recovery obligations after 010+011+012.
- W07: TASK-015 native/sidecar/platform integration after 009+010+011+012+013. W08: TASK-014 test discovery/staged-hook preservation after 009+013+015.
- W09: TASK-016 documentation and final generation after 014+015. W10: TASK-017 integrated validation. W11: TASK-018 independent source-loss audit. W12: TASK-019 authorized protected integration. W13: TASK-020 final report. An honest blocked report may be issued earlier without marking the DAG complete.

Every assignment includes its task/change IDs, exact source SHAs, owned files, dependency commits, preserved behaviors, acceptance criteria, verification commands, review role, evidence output and rollback instructions. Agents write separate reports; only the coordinator reconciles shared plans. Return failed work for correction and separate re-review. Do not drop difficult tasks.

## Step 4 — Preserve the selected architecture and behavior contracts

Prefer P's reviewed layered architecture, not its timestamp or commit count. Verify actual destination roots with Cargo metadata. P places apps under `crates/apps`, foundations under `crates/core`, diagnostics/telemetry/runtime/usage under `crates/services`, OMP and FFI under `crates/adapters`, and test/tool packages in their respective categories. The final observed purge owner is `crates/services/jackin-runtime-cleanup-absent-guards`, not an intermediate rescue package name. Map child symbols before editing.

Protect these concrete invariants while integrating the matrix:

1. M's `should_exit_idle` foreground-liveness guard survives splitting. Active monitors keep the service alive; passive idle brokers still expire. Sleep jumps, long idle windows and owner termination must be covered. Account/source/material fingerprint and revocation/uncertainty binding remain exact; unavailable or unauthorized state makes zero provider calls.
2. Carry typed provider status through the full flow. Text mentioning 401, 403 or 429 is not credential or retry authority. Preserve truthful per-window quota provenance, including zero-used windows, without inventing resets. Observe-only monitor results do not create runnable work, task identity or dispatch authorization.
3. Reconcile U schema/wire requirements against the selected actual versions; do not blindly assign its schema-6/wire-v9 numbers. Test migration/refusal/corruption with disposable predecessor data and preserve last-good state. U's live readiness remains unproved until the required separately authorized evidence exists.
4. Preserve stable coordination inodes, lock ownership and reentrant last-owner release; reject unsafe paths/symlinks/owners. Keep repository-cache identity correct and prevent purge of active/owned resources. Examine all repeated stash payloads once per object and every parent; alias count is not independent proof.
5. Preserve `load --model` and `--effort`: trim/reject empty model, enforce the selected effort enum, keep intentional dry-run null/default/existing keys, and carry choices through launch and restore. Confirm equivalent behavior at the final leaf owners instead of restoring obsolete flat paths.
6. Preserve diagnostic redaction and each-request retry checks. A corrupt 0/1/2 payload distribution must fail even if its total matches. Do not leak secrets, PTY bytes or sensitive bodies through test evidence or telemetry.
7. Prefer durable push-head/producer/artifact-bound CI obligations over first-parent history alone or an observed-run denominator. Missing, cancelled, nonterminal, conflicting and failed first attempts remain visible. Cache reuse needs exact semantic inputs; wrappers must exist before use and not leak into tool installation. One-minute warm CI timing is a requirement to measure, not a claimed achievement.
8. Preserve native FFI-before-SwiftPM/XcodeGen ordering, profile/floor, harness counts, UI checks, release stamping and dSYM validation. Keep bundled sidecar packaging. An in-process fallback is DEFERRED until lease/CAS/capability safety is proved; availability alone is insufficient.

Reject proved probe debris, duplicate invalid hook symbols, known-unregenerated pre-M1 states and the conflicted synthetic tree as implementation artifacts. Do not reject their unseen ancestors or useful tests. Preserve stronger current implementations rather than reintroducing old global sccache or generated CI states. Regenerate from reviewed current inputs, preserve handwritten files and never hand-edit ownership digests.

## Step 5 — Require independent review before integration

The reviewer inspects the actual diff against source branches/commits and matrix behaviors, including negative cases, tests, data/API compatibility, security and architecture. Completion by an implementation agent is not approval. Record reviewer identity, exact reviewed commit, findings, fixing commits and re-review. Independently challenge high-risk or ambiguous decisions and verify any “superseded” claim against a concrete destination and tests.

Integrate only accepted outputs, in dependency order, using attributable commits. Resolve conflicts semantically from the architecture/matrix—not blanket “ours/theirs” or Git's automatic result. A clean merge is not evidence that functionality survived. After each meaningful wave, run the affected combined checks and update the checkpoint/coverage ledger.

## Step 6 — Validate continuously and at the final head

Use `goal-references/06-validation-and-test-plan.md`. M's documented baseline uses Cargo/nextest; P's selected command family is:

```sh
mise install
mise exec -- mbx +1.99.0 metadata --no-deps --format-version 1
mise exec -- mbx +1.99.0 nextest list --message-format json
mise exec -- mbx +1.99.0 xtask health --format json
mise exec -- mbx +1.99.0 nextest run
mise exec -- mbx +1.99.0 nextest run --all-features
mise exec -- mbx +1.99.0 xtask ci --fast
mise exec -- mbx +1.99.0 xtask ci
```

Run commands individually and retain each exit code; a later success must not hide a prior failure. Verify each package/test filter at the actual selected head. Required zero-match tests fail. Do not replace pinned tools with ad hoc installations. Execute all selected build/type/strict lint, features/powersets, snapshot, dependency/security, documentation and generated-contract gates. Public examples need ordinary nextest-discoverable regression coverage under the selected documentation policy.

Usage/broker/runtime changes require **Apple Silicon macOS 26 with OrbStack**:

```sh
mise exec -- mbx +1.99.0 xtask ci --e2e
mise exec -- mbx +1.99.0 nextest run -p jackin --features e2e --profile docker-e2e
```

Prepare the Capsule binary/image according to the selected testing guide first. `usage_broker_e2e` must actually execute under `docker-e2e`, with JUnit under `target/nextest/docker-e2e/`. Require 2/20-client single-flight, owner-loss recovery, timeout ownership, Desktop/Capsule generation adoption, capability isolation, distinct-account concurrency, shared rate deadline/failure count and unavailable-state zero-call results. Generic Linux Docker, unit tests or fake counters do not satisfy this gate.

Native P verification:

```sh
mise -C native run ci
mise -C native run merge
mise -C native run scheduled
mise exec -- mbx +1.99.0 xtask desktop test
```

UI tests need a logged-in macOS host. Retain actual per-test counts and failures; do not call unavailable/skipped native checks green. Keep live credentials and operator state outside the test scope. Regenerate CI last and verify deterministic outputs after final documentation/structure changes. Review pending/changed snapshots, conflict markers and the entire final diff. Run `python3 goal-references/tools/validate_bundle.py --root .` for document/ledger structure, understanding it does not validate product semantics.

## Step 7 — Perform a complete loss-prevention audit

Revisit every local/remote source and every meaningful unique change, including tests, docs, config, assets, migrations, APIs and compatibility. For every matrix row, link the final implementation/test destination, a proved equal-or-better replacement, an evidenced rejection, or an explicitly approved deferral with blocker/impact. Close all unexamined remainder rows; do not hide incomplete analysis as a deferral. Essential unresolved behavior blocks completion.

Record full source and destination SHAs, required behavior, alternatives, rationale, dependencies, regression risks, actual tests and independent reviewer. Recheck branches that moved. Compare observed test discovery before/after extraction and review removed tests individually. An inventory count, clean history, passing structural validator or successful merge does not prove no loss.

## Step 8 — Obtain final independent verification

Assign a separate reviewer to the entire integrated tree, source inventory and matrix. Challenge coverage, security/authorization, state transitions, cache evidence, native release path, tests and unproved assumptions. Resolve significant findings and re-run impacted checks. Verify exact-head required CI/reviews, not stale source-head runs. If independent review or a required host is unavailable, report BLOCKED and retain the reviewable branch; do not fabricate reviews or success.

## Step 9 — Integrate only through the authorized target workflow

Refresh main's active ruleset (observed ID 14746904), repository policy and PR feedback. The observed contract requires linear history, squash PR integration, resolved review threads and `DCO` plus `Required` checks. There are no bypass actors. Server approval count zero does not waive independent review here.

With specific authorization for the necessary remote actions, publish the approved integration branch and use the protected squash-PR workflow. Re-fetch all reviews/comments/replies and unresolved or outdated threads at the final head; give accepted and rejected feedback justified dispositions, fixing commits and revalidation. No direct main commits, force push or protection bypass. If authorization/checks are absent, stop at a validated reviewable local branch—or an honestly blocked branch if validation did not pass—and state the exact missing steps. Do not call it integrated into main.

## Step 10 — Track progress, recover safely and report facts

Use states `PENDING`, `IN_PROGRESS`, `IN_REVIEW`, `BLOCKED`, `FAILED`, `VERIFIED`, `DONE`. A task becomes VERIFIED only after acceptance and independent review; DONE additionally records integration and coverage evidence. Preserve first failures/retries and source decisions. The coordinator owns the master progress ledger.

For failures follow `goal-references/07-risks-and-rollback.md`: preserve both conflicting outputs; abort only the actually active merge/cherry-pick in an owned clean task workspace; repair from a last-verified checkpoint or use a reviewed attributable revert. Never reset/clean unrelated work or guess a merge mainline. Use disposable data for migration recovery, not live-state rollback experiments. Stop only verified test-owned processes. A failed final merge leaves the reviewed branch and an exact blocker report.

Final report must state branches discovered/analyzed and evidence depth; exact source/destination revisions; changes/features integrated, combined, superseded, rejected or deferred; conflicts and chosen resolutions; commands actually executed with results/host/counts; independent reviews actually completed; remaining risks/gaps/permissions; final Git/PR/main status; and proof that the coverage matrix is reconciled. If blocked, say which work remains and do not claim all behavior was preserved. Do not promise unperformed background work.

# GOAL — Consolidate Jackin without silent loss

Snapshot: 13 October 2026 (Asia/Singapore). **Planning only; no source consolidation executed.**

> **Planning status: PARTIAL EVIDENCE / IMPLEMENTATION BLOCKED.** All 79 discovered server branches have dossiers and all 50 distinct tips received some inspection. Complete unique-history/semantic closure, the operator's local inventory, Jackin test execution and independent review are not complete. These are hard execution gates, not silently waived work.

## 1. Goal

Consolidate the best approved functional, architectural, security, performance, testing and documentation changes into `main`, preserving important target behavior, resolving competing implementations and eliminating accidental duplication. Produce a validated integration branch first, then use the authorized protected target workflow. Correctness, review and complete change accounting take priority over mechanical Git merging.

## 2. Scope and branch coverage

Repository: `https://github.com/jackin-project/jackin`. Target: `refs/heads/main`, pinned M=`1c3dd1be4a3b1ed518133ec3899c084c792370ff`. There are **79 discovered server heads, 50 distinct tips and no exclusions**. All have linked dossiers with varying inspection depth; complete semantic-history analysis is not claimed. The operator's real local branches/worktrees are unexamined because no local clone was accessible.

[Complete exact-ref inventory](goal-references/01-branch-inventory.md) · [All dossier links](goal-references/INDEX.md) · [Change matrix](goal-references/02-change-coverage-matrix.md) · [Evidence limitations](goal-references/09-evidence-and-limitations.md).

Primary candidates: P=`fba69d60c2373156dc0dcecee8e085c83776966f` (`rust-policy-rollout` and exact-tip CI alias, 366 ahead of M); U=`2e4a62da4bc4406764162a267163389a5fb513a7` (`fix/claude-usage-live-repair`, 25 ahead); T=`9ef617d014df15c6d967501bcbbdcb3689ed453c` (`chore/rust-mise-mbx-modernize`, 2 ahead/4 behind M). P and U diverge at M; T diverges at the October 6 consolidation. Recovery, archive, stash and experimental refs all remain in scope.

Known blockers: full local inventory/history/patch-equivalence, complete commit/file semantics, actual Jackin baselines/platform execution, genuinely independent plan review and remote-write permission. Broad API comparisons are capped, and old source test claims are not current PASS evidence. No implementation starts before TASK-001 through TASK-004 close the relevant evidence gates.

## 3. Success criteria

- [ ] Every in-scope local and remote branch is fully analyzed, with current source tips pinned.
- [ ] Every meaningful unique change and all stash/merge parent payloads have traceable decisions.
- [ ] Overlapping implementations are compared behaviorally; important behavior is not silently discarded.
- [ ] The selected architecture, APIs, data models, toolchain and native products are coherent.
- [ ] All required tests/builds/lints/type/feature/security/native/OrbStack gates pass at the final head.
- [ ] Every implementation task and final source-loss audit have genuinely independent review.
- [ ] All significant review findings and PR feedback are resolved with justified dispositions.
- [ ] The final matrix has no merely unexamined material remainder.
- [ ] The validated result reaches main only through an explicitly authorized protected squash-PR workflow.

## 4. Execution strategy

Prefer the reviewed P layered architecture, not an unreviewed 366-commit merge. Protect main's foreground lease/monitor/passive behavior. Port useful U usage changes semantically into verified leaf owners, reconcile T tooling, and salvage approved archive/stash behavior with complete source attribution. Reject only evidenced probe debris, known-invalid generated states and unsafe design choices; do not reject their unseen ancestors.

Run up to 10 delegated agents with isolated worktrees and explicit ownership. The coordinator owns the master ledger and shared integration branch. Independent review precedes integration; constrained builds/native tests are throttled separately. Shared manifests, locks, protocol changes and generator outputs serialize. No concurrent target writes, direct main commits, branch deletion, force push or remote publication.

Use the workspace commands and full DAG in [05-execution-tasks.md](goal-references/05-execution-tasks.md). Initialize `BASE_SHA` to the validated target and compute `INTEGRATION_SHA` from the frozen integration worktree. A moved source/target invalidates affected decisions/tests and requires re-analysis. Integrate only reviewed, tested source units and validate after each meaningful wave. If subagents are unavailable, work serially but leave independent approval blocked rather than inventing it.

Final main integration is a separately authorized squash PR with DCO/Required and justified resolution of all feedback. The default endpoint is a reviewable local integration branch when permission or gates are missing.

## 5. Detailed execution tasks


<a id="task-001"></a>
### TASK-001 — Freeze all actual local and remote refs without losing work

**Objective:** Close the missing operator-checkout inventory and establish a reproducible source snapshot before any source edit.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** Every pinned SHA in the inventory, plus newly inventoried sources; no unpinned branch-tip substitution.

**Dependencies:** None. **Parallel group:** W00. **Assigned role:** Coordinator + repository-analysis agents. **Initial state:** BLOCKED.

**Target files/components:** `.git (read-only inventory)`, `goal-references/01-branch-inventory.md`, `goal-references/evidence/remote-branches.json`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Read repository AGENTS.md, BRANCHING.md and TESTING.md and the complete planning bundle. Resolve the real checkout; do not initialize a new repository and pretend it is the upstream clone.
2. Capture status, untracked paths, worktrees, remotes, current HEAD, local heads, tracking refs and upstreams before fetching. Preserve all uncommitted work; do not stash, clean, reset or delete on the operator's behalf.
3. With read access, fetch all configured remotes without pruning or force-updating existing user refs. Compare advertised remote heads to the 79 pinned refs. Record stale, moved, added, absent and inaccessible refs, including real local-only branches.
4. Run the supplied collector into a new external evidence directory. The collector itself does not fetch. For protected/non-fast-forward tracking refs, preserve old IDs and use separate audit refs rather than overwriting them.
5. Recheck repository ruleset 14746904 and current PR/check policy through read APIs. Confirm remote writes remain disabled. Expand inventory/dossiers for newly discovered sources; do not silently remove a disappeared branch.
6. Ask an independent inventory reviewer to reconcile advertised heads, local refs and the snapshot. Block implementation until the real local coverage and missing-object issues are resolved.

**Acceptance criteria:**

- [ ] Every server branch, local head and remote-tracking branch has a stable ref-to-SHA record; aliases are retained but analysis is deduplicated by object.
- [ ] Working tree and untracked content remain unchanged; shallow/partial/missing history is reported and repaired through authorized reads.
- [ ] Any movement from the planning snapshot invalidates affected decisions and test evidence before implementation.

**Verification commands / expected results:**

```sh
git status --short; git remote -v; git branch -a -vv; git worktree list --porcelain
git for-each-ref --format="%(refname) %(objectname) %(upstream) %(symref)"
git log --graph --all --decorate --oneline
python3 goal-references/tools/collect_git_evidence.py --repo . --target 1c3dd1be4a3b1ed518133ec3899c084c792370ff --output ../jackin-evidence-20261013 --workers 4 --pairwise --patch-ids
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Fetching may race active development. Do not treat names containing rescue/local as evidence of the operator's local state. Redact credential-bearing remote URLs before retaining logs.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** No source changes are permitted. Preserve incomplete evidence as failed/incomplete, create a new output directory for a retry and never overwrite the captured original refs.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-002"></a>
### TASK-002 — Close every unique commit, stash parent and semantic coverage gap

**Objective:** Turn the 50 inspected-tip observations into a complete meaningful-change matrix; no blanket approval follows from tip inspection.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** Every pinned SHA in the inventory, plus newly inventoried sources; no unpinned branch-tip substitution.

**Dependencies:** TASK-001. **Parallel group:** W01. **Assigned role:** Up to six branch-analysis agents, two cross-comparison agents, security reviewer and test reviewer. **Initial state:** BLOCKED.

**Target files/components:** `goal-references/branches/`, `goal-references/02-change-coverage-matrix.md`, `goal-references/03-overlaps-and-conflicts.md`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Partition the 50 distinct tips into policy/extraction, live usage, modernization, CI/native archives, probes/hooks, and build-split/runtime salvage. Keep each of the 79 exact ref identities linked to its analyzed object; batch new local-only branches too.
2. Use full local commit lists and direct tree diffs, not the capped GitHub comparison file lists. Record all unique commits, renames, additions, deletions, modes, symlinks, assets, migrations, manifests and tests.
3. Compute stable patch IDs for all reachable non-merge commits and compare against target history. For merge commits inspect every parent delta and the actual result; for squash history inspect the aggregate source-to-target behavior, not just ancestry.
4. Inspect index and untracked parent trees of 0a898cd2, 80ab9803, b1b90781 and all other stash-shaped objects. The 30 modern stash aliases are two objects, but neither first-parent delta alone closes untracked contents. Treat the fea3ade synthetic parent as a comparison aid, not true recovered ancestry.
5. Expand each observed source packet into one row per meaningful behavior/test/config change. For each row record exact source SHA/files/symbols, alternatives, disposition, destination, tests, reviewer and blocker. Replace—not conceal—the per-branch open-remainder rows only when their complete commit/file sets reconcile.
6. Cross-review competing ledger denominators, native generator revisions, sccache scopes, typed provider errors, model/effort propagation and usage broker authority. Demonstrate SUPERSEDED with destination body/test evidence; a shared crate name or old consolidation message is insufficient.
7. Probes and malformed hook markers may be rejected only for the inspected changes. Preserve their useful stale-head/staging regression scenarios and review ancestral content independently.
8. Reject implementation handoff while any material branch portion is merely unexamined. A DEFER row records a blocker; it does not waive the preservation objective. Publish the signed/reviewed source-selection manifest for TASK-004.

**Acceptance criteria:**

- [ ] No unexamined remainder remains for an in-scope source before implementation approval, or the execution stays BLOCKED with exact missing SHAs.
- [ ] All 1,225 pairwise relationships among the initial 50 distinct tips are accounted for structurally; overlap families receive symbol/behavior comparisons.
- [ ] Every source packet has full commit/file coverage and an independently reviewed disposition; duplicates and intentional omissions have proof.

**Verification commands / expected results:**

```sh
git rev-list --left-right --count 1c3dd1be4a3b1ed518133ec3899c084c792370ff...fba69d60c2373156dc0dcecee8e085c83776966f
git diff --no-ext-diff --no-textconv --find-renames --stat 1c3dd1be4a3b1ed518133ec3899c084c792370ff 2e4a62da4bc4406764162a267163389a5fb513a7
git log --cherry-pick --left-right --oneline fba69d60c2373156dc0dcecee8e085c83776966f...2e4a62da4bc4406764162a267163389a5fb513a7
git range-diff 1c3dd1be4a3b1ed518133ec3899c084c792370ff..2e4a62da4bc4406764162a267163389a5fb513a7 1c3dd1be4a3b1ed518133ec3899c084c792370ff..fba69d60c2373156dc0dcecee8e085c83776966f
git show --no-patch --format="%H %P %T %s" 0a898cd246fbbf62f3b463f0097ffd6a368b1e07
git ls-tree -r 0a898cd246fbbf62f3b463f0097ffd6a368b1e07^3
python3 goal-references/tools/validate_bundle.py --root .
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Patch IDs can miss renamed/adapted equivalents and cannot validate merge resolutions. Never reject all stash content just because the visible tip is a probe.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Keep the original inventory immutable and append corrected decisions with superseding records. Return questionable classifications to review; no code rollback is needed at this stage.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-003"></a>
### TASK-003 — Measure the target and candidate baselines on isolated hosts

**Objective:** Establish actual baseline failures, test catalogs and tool requirements instead of inheriting historical green claims.

**Source branches:** [B038](goal-references/branches/b038-baseline-f921bd05e68b.md), [B035](goal-references/branches/b035-modernize-9949620d3954.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B037](goal-references/branches/b037-usage-1c388dc4de71.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md).
**Exact source tips:** [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff), [`9ef617d014df15c6d967501bcbbdcb3689ed453c`](https://github.com/jackin-project/jackin/commit/9ef617d014df15c6d967501bcbbdcb3689ed453c), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7)

**Dependencies:** TASK-001. **Parallel group:** W01. **Assigned role:** Testing/quality agents, separate baseline reviewer. **Initial state:** BLOCKED.

**Target files/components:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `TESTING.md`, `.config/nextest.toml`, `flaky-tests.toml`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Create detached or dedicated read/test worktrees at M, P, U and T without touching the operator working tree. Record the toolchain/manifest versions separately for every source; M declares Rust 1.97 while P requires the MBX 1.99 command contract.
2. Inspect installation/build scripts before running them; use pinned Mise tools and isolated caches/data directories. Do not copy scanner-bypass commands, live auth instructions, historical PIDs or user-specific install paths from commit bodies.
3. Capture metadata, nextest-discoverable test identities, feature flags, snapshot catalogs and all known configuration failures. Compare names and assertions, not only total counts.
4. Run the repository-defined safe local gates and record PASS, FAIL, BLOCKED or NOT_RUN per exact SHA. A source document's 718-test or earlier consolidation result is historical evidence only.
5. Run platform-backed baselines only on matching hosts. macOS 26 Apple Silicon with OrbStack is mandatory for the usage lane; native UI needs a logged-in session. Unit tests are not a substitute.
6. Classify each failure as target baseline, candidate baseline, environmental blocker or uncertain. Do not relax lints, skip affected tests or recolor flaky retries merely to obtain a green candidate.

**Acceptance criteria:**

- [ ] Actual baseline records exist for each intended starting revision, with test identities and platform prerequisites.
- [ ] All unexecuted checks retain explicit reasons; zero matching required tests is failure.
- [ ] An independent reviewer has approved which failures are inherited and which must be fixed before integration.

**Verification commands / expected results:**

```sh
mise install
cargo metadata --no-deps --format-version 1; cargo nextest list --message-format json
cargo nextest run; cargo nextest run --all-features; cargo xtask ci --fast; cargo xtask ci
mise exec -- mbx +1.99.0 metadata --no-deps --format-version 1
mise exec -- mbx +1.99.0 nextest list --message-format json
mise exec -- mbx +1.99.0 nextest run --all-features
mise exec -- mbx +1.99.0 xtask ci
mise exec -- mbx +1.99.0 xtask health --format json
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Use the M commands only at M/U/T where their source contract confirms them; use pinned MBX commands at P. Executing task runners may download tools or run fixture processes, so isolate and inspect them first.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Keep failed logs and immutable baseline checkouts. Stop only the test-owned processes; do not stop a live usage broker or alter real credential/state directories.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-004"></a>
### TASK-004 — Approve one layered architecture and exact source-to-path map

**Objective:** Choose semantics and interfaces before moving files or reconciling hundreds of policy commits.

**Source branches:** [B038](goal-references/branches/b038-baseline-f921bd05e68b.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md), [B035](goal-references/branches/b035-modernize-9949620d3954.md), [B037](goal-references/branches/b037-usage-1c388dc4de71.md), [B041](goal-references/branches/b041-policy-rescue-bb88995b68e3.md), [B042](goal-references/branches/b042-policy-rescue-4dc1add6a9f0.md), [B043](goal-references/branches/b043-policy-rescue-cf95d97cce7c.md), [B044](goal-references/branches/b044-dirty-3eaee4abbc92.md), [B045](goal-references/branches/b045-dirty-868886b2213b.md), [B046](goal-references/branches/b046-dirty-fc03ff5ffa1d.md), [B047](goal-references/branches/b047-stash-alias-53dd4c6affcc.md), [B048](goal-references/branches/b048-stash-alias-b7b19a9f3034.md).
**Exact source tips:** [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`9ef617d014df15c6d967501bcbbdcb3689ed453c`](https://github.com/jackin-project/jackin/commit/9ef617d014df15c6d967501bcbbdcb3689ed453c), [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`](https://github.com/jackin-project/jackin/commit/ef238a7e4720fe1be8e8e295e1c502f2ba529e0b), [`bff77099f8273a4d480a0e430cba66afc4197543`](https://github.com/jackin-project/jackin/commit/bff77099f8273a4d480a0e430cba66afc4197543), [`3ddbc0563ed95225b0004c393b3928fb599ee066`](https://github.com/jackin-project/jackin/commit/3ddbc0563ed95225b0004c393b3928fb599ee066), [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48), [`45e834683c8d5ca0fea1c7d3676cb136efa4acb1`](https://github.com/jackin-project/jackin/commit/45e834683c8d5ca0fea1c7d3676cb136efa4acb1), [`b72046bbc9314e93cddf4d8252570dc375935f56`](https://github.com/jackin-project/jackin/commit/b72046bbc9314e93cddf4d8252570dc375935f56), [`80ab98038e945ec48c82dad241ebf33aa0efa82d`](https://github.com/jackin-project/jackin/commit/80ab98038e945ec48c82dad241ebf33aa0efa82d)

**Dependencies:** TASK-002, TASK-003. **Parallel group:** W02. **Assigned role:** Architecture agent, security reviewer and coordinator. **Initial state:** PENDING.

**Target files/components:** `goal-references/04-target-architecture.md`, `goal-references/02-change-coverage-matrix.md`, `Cargo.toml`, `policy/owned-packages.json`, `policy/package-groups.json`, `ratchet.toml`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Use P's adapters/apps/core/services/testing/tools/ui layout as the preferred candidate, not as automatic approval of 366 commits. Check dependency cycles, public API contracts, feature sets and the file-size/policy rationale against actual source.
2. Map each flat M/U/T module to a concrete selected package/path and record where each moved test will be discovered. Include usage broker/coordinator/provider/projection/storage, runtime coordination/repo-cache, CLI launch, diagnostics, OMP and native FFI.
3. Freeze the wire/schema, typed provider error, credential binding, lease and quota-freshness contracts for downstream tasks. Identify breaking changes explicitly; repository policy rejects permanent legacy shims, but that is not permission to lose required behavior.
4. Produce an exact approved source-unit list: whole commits only when all their hunks and dependencies are approved; otherwise list targeted symbols/files and adapted regression tests. Separate structural moves from behavior changes.
5. Approve or reject each architectural alternative with tests: durable push-head CI evidence over first-parent-only obligations; safe task-scoped tooling over installer-wide wrappers; sidecar completeness versus gated in-process fallback.
6. Assign one owner for shared manifests, locks, generated CI inputs and protocol boundary changes. Independent architecture/security review is mandatory before TASK-005 starts.

**Acceptance criteria:**

- [ ] Every implementation task has resolved selected paths and approved source units, including newly found local changes.
- [ ] No old and new implementations remain in parallel by accident; intended breaking changes and data migrations have tests and explicit decisions.
- [ ] All high-risk architecture choices have independent approval and a verifiable rollback plan.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 metadata --no-deps --format-version 1
mise exec -- mbx +1.99.0 xtask health --format json
mise exec -- mbx +1.99.0 xtask ci --only policy
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Crate relocation touches manifests, telemetry schema, feature edges, native product paths and test discovery simultaneously. Do not let multiple writers edit shared ownership/lock files.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** If the preferred layout fails architectural review, leave integration unmodified and revise the explicit mapping/source-unit list. Do not silently switch architecture mid-wave.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-005"></a>
### TASK-005 — Integrate the reviewed policy layout and foundational extractions

**Objective:** Create the integration branch from the pinned target and apply the approved structural/foundation units without a blanket policy merge.

**Source branches:** [B038](goal-references/branches/b038-baseline-f921bd05e68b.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md), [B041](goal-references/branches/b041-policy-rescue-bb88995b68e3.md), [B042](goal-references/branches/b042-policy-rescue-4dc1add6a9f0.md), [B043](goal-references/branches/b043-policy-rescue-cf95d97cce7c.md), [B044](goal-references/branches/b044-dirty-3eaee4abbc92.md), [B045](goal-references/branches/b045-dirty-868886b2213b.md), [B046](goal-references/branches/b046-dirty-fc03ff5ffa1d.md), [B047](goal-references/branches/b047-stash-alias-53dd4c6affcc.md), [B048](goal-references/branches/b048-stash-alias-b7b19a9f3034.md).
**Exact source tips:** [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`](https://github.com/jackin-project/jackin/commit/ef238a7e4720fe1be8e8e295e1c502f2ba529e0b), [`bff77099f8273a4d480a0e430cba66afc4197543`](https://github.com/jackin-project/jackin/commit/bff77099f8273a4d480a0e430cba66afc4197543), [`3ddbc0563ed95225b0004c393b3928fb599ee066`](https://github.com/jackin-project/jackin/commit/3ddbc0563ed95225b0004c393b3928fb599ee066), [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48), [`45e834683c8d5ca0fea1c7d3676cb136efa4acb1`](https://github.com/jackin-project/jackin/commit/45e834683c8d5ca0fea1c7d3676cb136efa4acb1), [`b72046bbc9314e93cddf4d8252570dc375935f56`](https://github.com/jackin-project/jackin/commit/b72046bbc9314e93cddf4d8252570dc375935f56), [`80ab98038e945ec48c82dad241ebf33aa0efa82d`](https://github.com/jackin-project/jackin/commit/80ab98038e945ec48c82dad241ebf33aa0efa82d)

**Dependencies:** TASK-004. **Parallel group:** W03. **Assigned role:** Integration agent with one layout writer and independent test-catalog reviewer. **Initial state:** PENDING.

**Target files/components:** `Cargo.toml`, `Cargo.lock`, `crates/adapters/`, `crates/apps/`, `crates/core/`, `crates/services/`, `crates/testing/`, `crates/tools/`, `crates/ui/`, `policy/owned-packages.json`, `policy/package-groups.json`, `ratchet.toml`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Create integration/branch-consolidation-20261013 from the reviewed target SHA in a separate worktree; use a collision-safe suffix if the name already exists. Never reuse or reset an unrelated branch.
2. Apply TASK-004's exact approved foundational units in dependency order. Use whole cherry-picks only for wholly approved commits; otherwise extract reviewed changes into small source-attributed commits. Do not merge P wholesale or begin from P while claiming it was derived safely from main.
3. Carry the minimum coherent pinned toolchain and manifest changes required to make the selected layout build; TASK-006 owns the rest of toolchain/cache reconciliation. Preserve baseline foreground liveness and all approved runtime behavior during moves.
4. Reconcile test-module splits from ef238a7e/b72046bb with their final policy equivalents. Keep TLS feature guards, connection validation, telemetry redaction and all relevant negative assertions discoverable.
5. Keep old paths only during an uncommitted transition; finish each reviewed structural unit coherently and remove obsolete shims/aliases when its callers have migrated. Do not delete source tests until destination identities are proven.
6. Commit each verified foundation unit with source attribution and update the old-to-new mapping. Run metadata, target tests, policy gates and a test-catalog diff after each meaningful unit; independent review must precede integration into the shared branch.

**Acceptance criteria:**

- [ ] The integration branch descends from the approved target and preserves every approved foundation behavior.
- [ ] Workspace/package/feature graph is coherent; no accidental duplicate crate or orphaned test module remains.
- [ ] Every removed source test has a destination or independently approved exclusion and each foundation unit is reviewable.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 metadata --no-deps --format-version 1
mise exec -- mbx +1.99.0 nextest list --message-format json
mise exec -- mbx +1.99.0 xtask ci --fast
mise exec -- mbx +1.99.0 xtask lint files
mise exec -- mbx +1.99.0 xtask lint agents
git diff --check
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Exclusive ownership of Cargo.toml/Cargo.lock/policy/ratchet and all layout moves. No other source writer starts until this wave is integrated and the path contract is frozen.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Abort only a merge/cherry-pick in the task-owned clean worktree; revert already integrated foundation commits in reverse dependency order. Preserve source refs and baseline snapshots.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-006"></a>
### TASK-006 — Reconcile toolchain, native task entry points and CI evidence/cache contracts

**Objective:** Combine P and T tooling improvements without restoring obsolete workflow architecture or unverified lock mappings.

**Source branches:** [B035](goal-references/branches/b035-modernize-9949620d3954.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md), [B078](goal-references/branches/b078-wip-6b8347c05938.md), [B039](goal-references/branches/b039-misrouted-e096ae35b5a8.md), [B005](goal-references/branches/b005-archive-keeper-efa220a4ecda.md), [B012](goal-references/branches/b012-archive-keeper-27c1d7ad2d95.md), [B020](goal-references/branches/b020-archive-keeper-83afb29df592.md), [B030](goal-references/branches/b030-archive-src3-841f56f9a292.md), [B032](goal-references/branches/b032-archive-src3-70770d79115b.md).
**Exact source tips:** [`9ef617d014df15c6d967501bcbbdcb3689ed453c`](https://github.com/jackin-project/jackin/commit/9ef617d014df15c6d967501bcbbdcb3689ed453c), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`ef40fea11a54ade7911de7622e6e011707241dd4`](https://github.com/jackin-project/jackin/commit/ef40fea11a54ade7911de7622e6e011707241dd4), [`41dd148e514d1bf3f84401f0ecbf8e87b58bb261`](https://github.com/jackin-project/jackin/commit/41dd148e514d1bf3f84401f0ecbf8e87b58bb261), [`1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d`](https://github.com/jackin-project/jackin/commit/1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d), [`14bd1471616056e07c81c59d59eafc0963b16865`](https://github.com/jackin-project/jackin/commit/14bd1471616056e07c81c59d59eafc0963b16865), [`c7b8a95856cce6f5e8442f0e6fafcf025a089132`](https://github.com/jackin-project/jackin/commit/c7b8a95856cce6f5e8442f0e6fafcf025a089132), [`12dea6c5fd0f9d2c594d5ad3159e6493d03352f4`](https://github.com/jackin-project/jackin/commit/12dea6c5fd0f9d2c594d5ad3159e6493d03352f4), [`b1dc105acffb68ba71fff86f9c18dc5ff77de924`](https://github.com/jackin-project/jackin/commit/b1dc105acffb68ba71fff86f9c18dc5ff77de924)

**Dependencies:** TASK-005. **Parallel group:** W04. **Assigned role:** Toolchain/CI implementer with independent supply-chain and evidence reviewers. **Initial state:** PENDING.

**Target files/components:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `mise.toml`, `mise.lock`, `native/mise.toml`, `scripts/release-preview-package`, `scripts/test-mbx-cargo-wrapper.py`, `.github-gen/`, `.github/workflows/`, `crates/tools/jackin-xtask/src/ci_evidence/`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Compare T's two exclusive commits to P using the full local diff and range-diff. Retain missing native Mise/release-preview and dependency changes only after compatibility review; do not overwrite the layered Cargo manifest with a flat one.
2. Use one consistent reviewed Rust/MBX/Mise/tool pin set. Regenerate lockfiles from those inputs; reject blindly copying the WIP musl-to-GNU mappings and preserve verified minisign/checksum provenance.
3. Preserve the durable push-head denominator and schema-5 qualification behavior from 1c42a33/14bd147/c7b8a958 where missing. Reject first-parent-only history as proof of workflow push obligations, but preserve negative/fixture/conflicting-observation tests.
4. Reject installer-wide sccache/GHA activation from b1dc105. Compare successor installer-isolation tests to the selected cache design; do not reintroduce a removed compiler cache solely to make historical tests pass.
5. Validate the documented warm CI contract: offline fan-out, exact semantic forward-dependency keys, reverse-dependency selection, one canonical writer, stale-head rejection, no forbidden downloads/recompiles on an exact warm run and the stated one-minute target. Record cold/failed runs without inflating targets or hiding first attempts.
6. Regenerate from the current reviewed generator inputs after the final input edit. Protect handwritten files and inspect all generated deltas. Reconcile stale TESTING.md command/path links against the actual selected tree, not old ci-pr.yml assumptions.
7. Run configuration, dependency-policy, wrapper, ledger and cache invalidation tests with actual nonzero selections. A skipped smoke due to missing sccache is NOT RUN, not validation.

**Acceptance criteria:**

- [ ] One coherent pinned toolchain, lock and generator contract is independently reviewed and reproducible.
- [ ] Qualified CI evidence is bound to actual push heads, completed producer runs, exact artifacts and workflow/Git identity.
- [ ] Warm performance/selection/cache invalidation gates have measured evidence or remain blocking; no historical success or absent run is relabeled PASS.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 xtask ci --only policy
mise exec -- mbx +1.99.0 nextest run -p jackin-xtask -E 'test(ci_evidence)'
mise exec -- mbx +1.99.0 nextest run -p jackin-xtask -E 'test(affected)'
mise exec -- mbx +1.99.0 nextest run -p jackin-xtask -E 'test(wrapper)'
git diff --check
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** All manifests/locks/workflow inputs/outputs are exclusively owned here. Generator invocations must be read from the current checked-in task contract; this plan does not invent a generator flag.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert the tool-input and regenerated-output unit together. Retain prior approved artifact keys and evidence; never overwrite historical failed CI attempts or apply old binary caches to new semantic contracts.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-007"></a>
### TASK-007 — Preserve foreground auth leases and exact coordinator capability binding

**Objective:** Combine main liveness fixes, U auth repair, s7 capability checks and revoked-account persistence tests.

**Source branches:** [B038](goal-references/branches/b038-baseline-f921bd05e68b.md), [B037](goal-references/branches/b037-usage-1c388dc4de71.md), [B043](goal-references/branches/b043-policy-rescue-cf95d97cce7c.md), [B044](goal-references/branches/b044-dirty-3eaee4abbc92.md), [B040](goal-references/branches/b040-usage-followup-bf245b09ab75.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md).
**Exact source tips:** [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff), [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`3ddbc0563ed95225b0004c393b3928fb599ee066`](https://github.com/jackin-project/jackin/commit/3ddbc0563ed95225b0004c393b3928fb599ee066), [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48), [`2abed4ad67d8438ab9eb124b564be6ad5b545847`](https://github.com/jackin-project/jackin/commit/2abed4ad67d8438ab9eb124b564be6ad5b545847), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f)

**Dependencies:** TASK-006. **Parallel group:** W05. **Assigned role:** Usage authority implementer; separate security reviewer. **Initial state:** PENDING.

**Target files/components:** `crates/services/jackin-usage-broker/`, `crates/services/jackin-usage-coordinator/`, `crates/services/jackin-usage-discovery/`, `crates/services/jackin-usage-credential-resolver/`, `crates/services/jackin-usage-coordinator/src/tests/case_03.rs`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Trace M's should_exit_idle and foreground collector liveness through P's broker split. Preserve the truth table: active foreground lease or active monitor prevents idle exit; idle passive brokers still expire.
2. Adapt U's coordinator/auth behavior into the selected leaf crates rather than restoring flat modules. Preserve non-TTY interaction_required before broker launch/state creation and keep passive observation separate from task authorization.
3. Compare ScopedCapability, ForwardingRequirement, credential_keys_match, binding_matches_proof and authorize_credential_binding_group from 3ddbc056/40823a76. Require exact account/surface/source/material-fingerprint matches and fail closed on ambiguous groups, absent material or multiple candidates.
4. Preserve r37's concrete store handle and persisted result-free uncertainty-marker assertion after revoked-account replacement; verify whether P already carries the exact test before adding it again.
5. Add/retain deterministic clock-jump and beyond-ten-minute lease tests, owner-loss and unavailable-state zero-call assertions. Do not satisfy an auth test by enabling a live provider or relaxing a capability check.
6. Have the independent security reviewer test rejection cases and the tester record nonzero focused test identities. Integrate only after review and targeted validation pass.

**Acceptance criteria:**

- [ ] Foreground auth ownership survives idle/sleep cases without making passive brokers immortal.
- [ ] Credential authorization is bound to exact validated identities and never inferred from error text or monitor presence.
- [ ] Revoked/corrupt state produces a persisted result-free uncertainty marker and no unauthorized provider call.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin-usage-broker
mise exec -- mbx +1.99.0 nextest run -p jackin-usage-coordinator
mise exec -- mbx +1.99.0 nextest run -p jackin-usage-discovery
mise exec -- mbx +1.99.0 nextest run -p jackin-usage-credential-resolver
mise exec -- mbx +1.99.0 clippy -p jackin-usage-broker -p jackin-usage-coordinator --all-targets -- -D warnings
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Protocol shape is frozen in TASK-004. Route new shared protocol requirements to the coordinator; do not edit provider/monitor files concurrently owned by TASK-008/009.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert the bounded auth/coordinator unit and retain failed fixture stores for diagnosis without exposing secrets. Never roll back by killing the operator's foreground owner or downgrading real state.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-008"></a>
### TASK-008 — Unify typed provider errors and Claude per-window quota evidence

**Objective:** Keep authoritative status/freshness metadata through provider parsing, including the 100-percent-remaining edge case.

**Source branches:** [B028](goal-references/branches/b028-archive-src3-95287c049e3e.md), [B037](goal-references/branches/b037-usage-1c388dc4de71.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md), [B044](goal-references/branches/b044-dirty-3eaee4abbc92.md).
**Exact source tips:** [`6ce20ce13cb87c5ab89b44d72698fa0d68aacf39`](https://github.com/jackin-project/jackin/commit/6ce20ce13cb87c5ab89b44d72698fa0d68aacf39), [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48)

**Dependencies:** TASK-006. **Parallel group:** W05. **Assigned role:** Provider implementer; independent error-model reviewer. **Initial state:** PENDING.

**Target files/components:** `crates/services/jackin-usage-provider-core/`, `crates/services/jackin-usage-provider-claude/`, `crates/services/jackin-usage-provider-codex/`, `crates/services/jackin-usage-output/`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Compare typed ProviderError changes from 6ce20ce1 with selected provider-core/Codex/Claude implementations and U's live-repair parsing. Retain typed HTTP status and structured retry metadata until presentation.
2. Preserve the rule that arbitrary strings mentioning 401/403/429 or a port number do not establish auth, scope restriction or a retry deadline. Claude inference-only scope classification requires typed HTTP 403, not a textual substring.
3. Adapt the v12 checkpoint db59c4015fb4715e1d39157aaa31f4a1497c823a for zero used derived from authoritative 100% remaining. Validate bounds and distinguish unknown from zero; do not invent reset timestamps.
4. Keep session and weekly window freshness independent. A fresh weekly result must not make an unobserved/stale session result fresh. Preserve fallback provenance and auth-error versus display-error precedence.
5. Add adversarial parser/error fixtures for misleading text, typed status conflicts, missing reset/used fields, stale cache and partial windows. Measure provider calls with synthetic executors.
6. Independent review must challenge both false-success and false-denial cases; run exact selected provider tests before integration.

**Acceptance criteria:**

- [ ] Typed statuses and authoritative retry data survive all adapters; display strings cannot change control policy.
- [ ] 100% remaining can mean zero used only when the source observation supports it; missing reset/freshness stays unknown.
- [ ] No test or smoke makes a real provider call without separate explicit authorization.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin-usage-provider-core -p jackin-usage-provider-claude -p jackin-usage-provider-codex
mise exec -- mbx +1.99.0 clippy -p jackin-usage-provider-core -p jackin-usage-provider-claude --all-targets -- -D warnings
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** No concurrent coordinator/protocol writes. Shared ProviderError interface changes require TASK-004 amendment and rerunning affected consumers.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert parser/provider changes as one source-attributed unit; preserve failing sanitized fixtures. Do not relabel old live observations or mutate real cache data to force freshness.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-009"></a>
### TASK-009 — Reconcile monitor persistence, wire contracts and observe-only fixtures

**Objective:** Adapt U's monitor/storage/schema work to the chosen architecture and prove non-authorizing observation.

**Source branches:** [B037](goal-references/branches/b037-usage-1c388dc4de71.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md), [B044](goal-references/branches/b044-dirty-3eaee4abbc92.md).
**Exact source tips:** [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`40823a76a3e20b901d5bc410979cee0931fd5c48`](https://github.com/jackin-project/jackin/commit/40823a76a3e20b901d5bc410979cee0931fd5c48)

**Dependencies:** TASK-007, TASK-008. **Parallel group:** W06. **Assigned role:** Monitor/storage implementer, migration reviewer and isolated-fixture test agent. **Initial state:** PENDING.

**Target files/components:** `crates/core/jackin-protocol/`, `crates/services/jackin-usage-broker-wire/`, `crates/services/jackin-usage-snapshot-store/`, `crates/services/jackin-usage-store-backend/`, `crates/services/jackin-usage-token-monitor/`, `crates/services/jackin-usage-projection/`, `crates/apps/jackin/src/bin/usage-broker-fixture.rs`, `crates/apps/jackin/tests/usage_provider_flow/`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Locate the selected counterparts of U usage_broker.rs/usage_monitor.rs, monitor projection/storage, fingerprint_migration tests and provider_diagnostics fixtures. Treat all new destination filenames as resolved only by TASK-004's map.
2. Reconcile schema 5-to-6 and wire-v9 source requirements against P's actual selected versions; do not blindly bump numbers or lower a later version. Document serialization compatibility and every supported predecessor.
3. Use cloned synthetic state to test stable fingerprints/binding, uncertainty markers, interrupted migration, malformed/ambiguous inputs and per-window evidence. Define atomic migration/recovery behavior without opening the operator's real store.
4. Preserve observe-only invariants: goal_id absent, runnable false, no grant/revocation of task authority. A monitor must neither create a credential lease nor authorize provider dispatch.
5. Keep CLI fixture binaries and auth outcome tests isolated with synthetic HOME/config/state and fake broker/credential-shell/proxy markers. Assert zero calls; explicitly distinguish those counters from OS-level egress and native Keychain proof.
6. Produce a reviewed migration and wire-contract report plus failing/passing fixture comparisons. Leave live acceptance and attended auth/cutover BLOCKED unless separately authorized and actually tested.

**Acceptance criteria:**

- [ ] Each schema predecessor and wire compatibility decision has tested handling, not a silent version-number overwrite.
- [ ] Observe-only flows do not gain task authority or accidentally refresh live accounts.
- [ ] Headless fixtures produce reproducible expected outputs and nonzero executed tests; live readiness remains a separate gate.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin-protocol -p jackin-usage-broker-wire -p jackin-usage-snapshot-store -p jackin-usage-token-monitor
mise exec -- mbx +1.99.0 nextest run -p jackin -E 'test(usage_provider_flow)'
mise exec -- mbx +1.99.0 xtask ci --fast
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Exclusive protocol/schema ownership during this wave. Coordinate fixture binary manifest edits with the shared manifest owner; do not race TASK-007/008.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Abort before real data migration. For fixture failure restore the complete fixture snapshot, not selected database files. Never run an old binary on a store migrated beyond its supported schema.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-010"></a>
### TASK-010 — Preserve runtime inode, repository-cache and purge safety

**Objective:** Verify modern stash extractions and s61 purge guards rather than assuming their presence in the manifest proves equivalence.

**Source branches:** [B047](goal-references/branches/b047-stash-alias-53dd4c6affcc.md), [B048](goal-references/branches/b048-stash-alias-b7b19a9f3034.md), [B042](goal-references/branches/b042-policy-rescue-4dc1add6a9f0.md), [B045](goal-references/branches/b045-dirty-868886b2213b.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md).
**Exact source tips:** [`80ab98038e945ec48c82dad241ebf33aa0efa82d`](https://github.com/jackin-project/jackin/commit/80ab98038e945ec48c82dad241ebf33aa0efa82d), [`bff77099f8273a4d480a0e430cba66afc4197543`](https://github.com/jackin-project/jackin/commit/bff77099f8273a4d480a0e430cba66afc4197543), [`45e834683c8d5ca0fea1c7d3676cb136efa4acb1`](https://github.com/jackin-project/jackin/commit/45e834683c8d5ca0fea1c7d3676cb136efa4acb1), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f)

**Dependencies:** TASK-006. **Parallel group:** W05. **Assigned role:** Runtime/storage implementer with filesystem-safety reviewer. **Initial state:** PENDING.

**Target files/components:** `crates/services/jackin-runtime-coordination/`, `crates/services/jackin-runtime-repo-cache/`, `crates/services/jackin-runtime-cleanup-absent-guards/`, `crates/services/jackin-runtime/src/runtime/apple_container.rs`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Compare all parents and body/test payloads of both repeated stash tips to P's runtime-coordination and repo-cache leaf crates. Include untracked manifests/lib files and policy ownership declarations.
2. Preserve permanent coordination inode semantics: reject prunable-root overlap, parent/symlink traversal, unsafe ownership/permissions/link counts and non-regular inode substitution. Lock release must not unlink persistent coordination inodes.
3. Preserve RepoLock reentrant same-process acquisition, per-path gating, last-handle release and blocking-pool semantics. Independently review repo_matches host stripping against canonical repository identity; do not call it safe or exploitable from the name alone.
4. Compare ensure_absent_for_purge_with extraction and its guard behavior at the s61 checkpoint to the final leaf; preserve preconditions and error handling, not only signatures. The pinned P manifest names jackin-runtime-cleanup-absent-guards, not the intermediate ensure-absent crate; map the saved function into that approved final owner.
5. Add fault-injection and concurrency tests for acquisition, cancellation, final-handle release, interrupted purge and cross-host repository identities. Make any observed host-identity defect a tested root-cause fix, not an undocumented behavior change.
6. Only mark the stash changes SUPERSEDED when implementations, tests and all parent payloads have explicit selected destinations or justified exclusions.

**Acceptance criteria:**

- [ ] No persistent inode or lock authority is invalidated by cleanup/reentrant handling.
- [ ] Repository identity and purge operations have unambiguous tests and independently reviewed failure modes.
- [ ] All 30 stash alias refs remain traceable to the two fully reconciled objects.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin-runtime-coordination -p jackin-runtime-repo-cache
mise exec -- mbx +1.99.0 nextest run -p jackin-runtime-cleanup-absent-guards
mise exec -- mbx +1.99.0 clippy -p jackin-runtime-coordination -p jackin-runtime-repo-cache --all-targets -- -D warnings
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Own only the named runtime leaves and purge seam. Shared policy/manifest changes are queued to the coordinator; no broad runtime reformat while TASK-011 is active.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert the bounded leaf change, preserve failed temporary inode/store fixtures and do not delete permanent coordination namespaces or operator repositories.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-011"></a>
### TASK-011 — Preserve model/effort selection through CLI launch and restore

**Objective:** Carry the useful reflog feature through the complete launch path instead of retaining only CLI parsing.

**Source branches:** [B079](goal-references/branches/b079-wip-ebf572949c76.md), [B002](goal-references/branches/b002-archive-docs-33af45993b41.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md), [B038](goal-references/branches/b038-baseline-f921bd05e68b.md).
**Exact source tips:** [`d04467f05f1183008322047b1828c201e551e947`](https://github.com/jackin-project/jackin/commit/d04467f05f1183008322047b1828c201e551e947), [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff)

**Dependencies:** TASK-006. **Parallel group:** W05. **Assigned role:** CLI/runtime launch implementer with independent end-to-end reviewer. **Initial state:** PENDING.

**Target files/components:** `crates/apps/jackin/src/app/load_cmd.rs`, `crates/apps/jackin/src/cli/role.rs`, `crates/services/jackin-runtime/`, `crates/core/jackin-core/`, `crates/apps/jackin-capsule/`, `crates/services/jackin-runtime-launch-load-options/`, `crates/services/jackin-runtime-launch-dry-run/`, `crates/services/jackin-runtime-launch-restore/`, `crates/services/jackin-runtime-launch-programmatic-selection/`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Compare d04467f0 with the canonical descendants of historical 42c6f8ed91571e68c080c2ed7c6ac62a045d6553 and 8dac3d6eb651c4515ffedeefa3f616b64b07e1bf. Keep one approved implementation.
2. Preserve trimmed nonempty model values, low/medium/high/max effort parsing, optional defaults and help. Carry both values through LoadOptions, runtime launch assembly and Capsule/session metadata where the source requires it.
3. Keep dry-run JSON existing keys, adding or preserving model_override/effort semantics with null for absent values. Text output must reflect actual effective selections.
4. Compare restore-time stored agent identity and unsupported initial-prompt handling from the historical evidence record. Do not restore a CLI option that the selected backend cannot honor.
5. Add/retain tests from CLI parse through dry-run, launch and restore, including overrides, invalid inputs and omitted selections. No real launch/provider call is needed for these tests.
6. Constrain runtime/Capsule edits to mapped launch/restore seams; request shared type changes through the coordinator and have another reviewer verify the complete propagation path.

**Acceptance criteria:**

- [ ] Accepted overrides affect the intended launched/restored behavior, not merely help text or parser structs.
- [ ] Old dry-run fields and explicitly approved compatibility behavior are preserved; intentional breaking changes are documented.
- [ ] Invalid model/effort inputs fail deterministically without side effects.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin -E 'test(load)'
mise exec -- mbx +1.99.0 nextest run -p jackin-runtime -E 'test(launch) | test(restore)'
mise exec -- mbx +1.99.0 nextest run -p jackin-capsule -E 'test(model) | test(effort)'
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Exact launch/restore file ownership is assigned by TASK-004; do not edit runtime coordination/repo-cache files owned by TASK-010 or provider parsing owned by TASK-008.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert the entire propagation unit, including tests and wire/type changes, rather than leaving a parsed-but-unused option. Preserve the prior reviewed CLI contract.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-012"></a>
### TASK-012 — Retain diagnostic redaction and per-request retry proofs

**Objective:** Preserve security-sensitive observability behavior and stronger retry assertions through test/module splits.

**Source branches:** [B027](goal-references/branches/b027-archive-sanitized-0739580d021c.md), [B002](goal-references/branches/b002-archive-docs-33af45993b41.md), [B041](goal-references/branches/b041-policy-rescue-bb88995b68e3.md), [B046](goal-references/branches/b046-dirty-fc03ff5ffa1d.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md).
**Exact source tips:** [`502e0627d3940e2cd68ae0078910f21452d9ccc1`](https://github.com/jackin-project/jackin/commit/502e0627d3940e2cd68ae0078910f21452d9ccc1), [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984), [`ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`](https://github.com/jackin-project/jackin/commit/ef238a7e4720fe1be8e8e295e1c502f2ba529e0b), [`b72046bbc9314e93cddf4d8252570dc375935f56`](https://github.com/jackin-project/jackin/commit/b72046bbc9314e93cddf4d8252570dc375935f56), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f)

**Dependencies:** TASK-006. **Parallel group:** W05. **Assigned role:** Diagnostics implementer with independent privacy/security reviewer. **Initial state:** PENDING.

**Target files/components:** `crates/services/jackin-diagnostics/`, `crates/services/jackin-telemetry/`, `crates/testing/jackin-otlp-testbed/`, `crates/adapters/jackin-docker/src/docker_client/tests.rs`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Use TASK-004's exact package map; directory categories in this task are candidates until resolved. Find the selected assert_wire_requests helper and compare the sanitized 502e0627 per-request assertion.
2. Require every retried log request to contain exactly one matching record; add a 0/1/2-distribution negative fixture so flattened totals cannot conceal corruption.
3. From the build-split record, compare rejected intermediate redaction changes to the final selected implementation. Preserve no-secret/no-private-value export assertions across split test modules.
4. Inspect Docker TLS feature guards and rejection of incompatible connection choices from b72046bb. Preserve those tests without restoring obsolete imports or dead modules.
5. Run against a dev-only synthetic OTLP testbed. Do not capture real PTY bytes, credentials or operator data as telemetry; a configured trusted test endpoint must not become a production fallback.
6. An independent reviewer must inspect serialized payloads and adversarial cases, not merely test count or pretty output.

**Acceptance criteria:**

- [ ] Each retry carries the expected payload; count-only false positives are rejected.
- [ ] No live secret or raw PTY content is exported or committed by validation.
- [ ] TLS, connection validation and diagnostic privacy assertions remain nextest-discoverable.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin-diagnostics
mise exec -- mbx +1.99.0 nextest run -p jackin-docker
mise exec -- mbx +1.99.0 clippy -p jackin-diagnostics -p jackin-docker --all-targets -- -D warnings
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Diagnostic package category must be verified from metadata, not guessed. Shared telemetry-schema changes require review of all consumers and cannot be made concurrently by another writer.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert the bounded test/diagnostics unit; securely retain only sanitized failing fixture data. Never repair a test by redacting its assertions or accepting leaked payloads.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-013"></a>
### TASK-013 — Resolve historical build-split recovery and migration obligations

**Objective:** Prove whether archived restore/config/OMP fixes are preserved, and close any confirmed missing behavior.

**Source branches:** [B001](goal-references/branches/b001-archive-docs-437a9e81ba65.md), [B002](goal-references/branches/b002-archive-docs-33af45993b41.md), [B003](goal-references/branches/b003-archive-recovery-583438c051c5.md), [B026](goal-references/branches/b026-archive-sanitized-0ced81950334.md), [B038](goal-references/branches/b038-baseline-f921bd05e68b.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md).
**Exact source tips:** [`3cb98f2a634c1e660e501489d7383329569db7a3`](https://github.com/jackin-project/jackin/commit/3cb98f2a634c1e660e501489d7383329569db7a3), [`b1e08b880223594cceccf3ea44322d44e3cdf984`](https://github.com/jackin-project/jackin/commit/b1e08b880223594cceccf3ea44322d44e3cdf984), [`fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`](https://github.com/jackin-project/jackin/commit/fea3ade6830912017f2ad80c0c5a9d1acfbde0b7), [`2d97bba390864fbdc79aa6566e3394df938408af`](https://github.com/jackin-project/jackin/commit/2d97bba390864fbdc79aa6566e3394df938408af), [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f)

**Dependencies:** TASK-010, TASK-011, TASK-012. **Parallel group:** W06. **Assigned role:** Recovery/migration implementer with independent persistence reviewer. **Initial state:** PENDING.

**Target files/components:** `crates/adapters/jackin-omp-store/`, `crates/core/jackin-config/`, `crates/services/jackin-runtime/`, `crates/core/jackin-process/`, `plans/jackin-build-split/`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Treat B001/B002 dated records as investigation seeds. For each named historical source commit, locate the final selected destination and tests before deciding a defect is still present or already fixed.
2. Verify OMP database/WAL capture, journal reset-suffix handling, ordered cleanup plus parent-directory durability, and stored agent identity restoration. Test concurrent writers rather than claiming an optimistic reread is full backup consistency.
3. Check configuration migration predecessor versions, metadata updates, golden and ambiguous-input fixtures, profile-proof creation/invalidation, CODEX_HOME discovery and projection-only Kimi capture.
4. Inspect the fea3ade unresolved process-conflict snapshot by side/hunk. Reject the non-buildable whole tree and synthetic ancestry; extract only independently justified missing behavior.
5. Where a historical obligation is already satisfied, record exact selected symbols/tests and SUPERSEDED evidence. Where it is genuinely missing, implement a bounded root-cause fix with fault-injection/restart tests. No material item may be silently left unexamined.
6. Keep external Architect/Velnor repository changes as scoped references only. Do not mutate another repository, publish an artifact or rotate real state as part of this task.

**Acceptance criteria:**

- [ ] Every historical obligation has a current evidence-based disposition; no stale report is represented as a present confirmed failure.
- [ ] Restore, cleanup and migration preserve approved behavior under interrupted and ambiguous conditions.
- [ ] No unresolved conflict markers or synthetic recovery tree imports enter the integration branch.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin-omp-store
mise exec -- mbx +1.99.0 nextest run -p jackin-config
mise exec -- mbx +1.99.0 nextest run -p jackin-runtime -E 'test(restore) | test(migration) | test(cleanup)'
git diff --check
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Own only recovery/migration seams mapped in TASK-004 after launch/runtime tasks finish. Missing historical objects remain a blocker, not a reason to invent the fix.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Use complete fixture snapshots for recovery tests. Revert a reviewed code unit on failure; never apply a downgrade to a real migrated database or unlink recovery evidence.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-014"></a>
### TASK-014 — Audit test discovery and retain staged-hook regression contracts

**Objective:** Ensure that test splitting, file moves and probe rejection have not weakened verification.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** Every pinned SHA in the inventory, plus newly inventoried sources; no unpinned branch-tip substitution.

**Dependencies:** TASK-009, TASK-013, TASK-015. **Parallel group:** W08. **Assigned role:** Testing agent and independent test-catalog reviewer. **Initial state:** PENDING.

**Target files/components:** `crates/tools/jackin-xtask/src/clippy_affected.rs`, `crates/tools/jackin-xtask/src/clippy_affected/`, `goal-references/02-change-coverage-matrix.md`, `.config/nextest.toml`, `flaky-tests.toml`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Compare baseline and final nextest identities per package/feature/target, mapping every rename or split. An increased total does not prove preservation of individual assertions.
2. Preserve 6d65e649 staged-only dependency selection: union HEAD-to-index and index-to-worktree, safe widening on missing/unparseable/structural inputs, comment-only no-op behavior.
3. Turn useful hook/stale-head/cache probe intent into isolated temporary-repository fixtures. Exclude malformed production functions, duplicate definitions, untracked_bad.rs and cache-marker comments; never execute a recovered hook script without inspection.
4. Audit tests that return early for absent tools, zero selections, ignored/platform-only tests and retry passes. Required checks need nonzero actual execution or an explicit BLOCKED result.
5. Route any missing assertions back to the owning implementation task for correction, review and rerun. Do not become a second writer across native/provider/runtime modules.
6. Reconcile documentation examples into nextest and run ci-doc-examples. Preserve the shrink-only flaky-test policy; adding a quarantine entry requires independent disposition rather than hiding a regression.

**Acceptance criteria:**

- [ ] Every required source test/assertion has a final identity or independently approved exclusion.
- [ ] Staged-only dependency edits cannot be skipped by affected-crate selection.
- [ ] No proof-only source debris remains, and retries/early returns/zero-match runs are accurately reported.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest list --message-format json
mise exec -- mbx +1.99.0 nextest run -p jackin-xtask -E 'test(affected) | test(manifest_union) | test(lock_union)'
mise exec -- mbx +1.99.0 xtask ci-doc-examples --package jackin-xtask
mise exec -- mbx +1.99.0 xtask ci --only snapshots
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Native/feature owners retain their files; this task audits and returns defects rather than racing broad test rewrites. Central manifests remain coordinator-owned.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert only the hook/selector test unit if necessary and return catalog regressions to their source tasks. Never drop tests or refresh snapshots solely to make a broken implementation pass.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-015"></a>
### TASK-015 — Validate bundled sidecar and native producer-consumer integration

**Objective:** Preserve installable broker behavior and complete Apple/FFI release-path obligations in the unified architecture.

**Source branches:** [B034](goal-references/branches/b034-archive-src3-ac5215e0b69e.md), [B015](goal-references/branches/b015-archive-keeper-dc5b26803593.md), [B025](goal-references/branches/b025-archive-staged-67aa054ffa3d.md), [B029](goal-references/branches/b029-archive-src3-21f83e144de4.md), [B031](goal-references/branches/b031-archive-src3-75be4f6a7848.md), [B033](goal-references/branches/b033-archive-src3-9264ed8676fb.md), [B035](goal-references/branches/b035-modernize-9949620d3954.md), [B036](goal-references/branches/b036-policy-0b27f2016591.md), [B037](goal-references/branches/b037-usage-1c388dc4de71.md), [B077](goal-references/branches/b077-policy-517d17ddf7fc.md).
**Exact source tips:** [`33d88b1cf764f614d3935c11c48decd53386083e`](https://github.com/jackin-project/jackin/commit/33d88b1cf764f614d3935c11c48decd53386083e), [`7646c61bd4261a5908693bdaf0a6ecd9e9643448`](https://github.com/jackin-project/jackin/commit/7646c61bd4261a5908693bdaf0a6ecd9e9643448), [`0437497d7f22fbdb7c8aad986be932704d707fe9`](https://github.com/jackin-project/jackin/commit/0437497d7f22fbdb7c8aad986be932704d707fe9), [`8fb49688a6408627e22118f92d24f47785564979`](https://github.com/jackin-project/jackin/commit/8fb49688a6408627e22118f92d24f47785564979), [`432e0108ce90b64b428c69c27d4a2ac23e559379`](https://github.com/jackin-project/jackin/commit/432e0108ce90b64b428c69c27d4a2ac23e559379), [`d16cd795d4a539b84ba5f44290663b2363d49350`](https://github.com/jackin-project/jackin/commit/d16cd795d4a539b84ba5f44290663b2363d49350), [`9ef617d014df15c6d967501bcbbdcb3689ed453c`](https://github.com/jackin-project/jackin/commit/9ef617d014df15c6d967501bcbbdcb3689ed453c), [`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f), [`2e4a62da4bc4406764162a267163389a5fb513a7`](https://github.com/jackin-project/jackin/commit/2e4a62da4bc4406764162a267163389a5fb513a7)

**Dependencies:** TASK-009, TASK-010, TASK-011, TASK-012, TASK-013. **Parallel group:** W07. **Assigned role:** Native/runtime implementer; independent broker safety and Apple test reviewers. **Initial state:** PENDING.

**Target files/components:** `crates/apps/jackin/Cargo.toml`, `crates/apps/jackin/src/bin/usage_broker.rs`, `crates/services/jackin-runtime/`, `crates/services/jackin-usage-broker/`, `crates/adapters/jackin-usage-ffi/`, `native/mise.toml`, `native/Package.swift`, `native/project.yml`, `native/Scripts/run-ui-tests.sh`, `crates/tools/jackin-xtask/src/desktop/`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Resolve exact P package paths with TASK-004 metadata before writing. Preserve B034's requirement that a jackin installation includes the intended broker sibling binary and shares the correct service entry point.
2. Compare missing/unusable-sidecar fallback against current foreground lease ownership. Select fallback only after proving leader-election/CAS, capability isolation, client-death lifetime and error propagation; otherwise preserve explicit unavailable behavior with an approved rationale and package the sidecar correctly.
3. Choose a complete native product contract from current P/T plus useful 432e0108/d16cd795 obligations. Reject the intentionally unmatched pre-M1 input/output state. Avoid duplicate synthetic Swift units.
4. Keep Rust FFI producer, XCFramework paths, desktop-release profile, macOS deployment floor, SwiftPM and XcodeGen consumers consistent. Regenerate binding/product outputs with reviewed inputs and keep the full native test/format/lint/dSYM/release-stamping/UI-count obligations.
5. Run native ci, merge/UI and scheduled/dead-code lanes on appropriate hosts, then the mandatory Apple Silicon macOS 26 OrbStack usage lane with machine-readable JUnit. Prove 2/20-client single-flight, owner-loss recovery, timeout ownership, generation adoption, capability isolation, per-account concurrency, shared rate deadlines and zero-call unavailable state.
6. Do not sign, publish, install over an operator binary, read Keychain or switch a live broker automatically. Package validation uses a new fixture install prefix; attended live acceptance remains separate.

**Acceptance criteria:**

- [ ] The built CLI and broker package are complete and launch/activation failure semantics are independently approved.
- [ ] Every native consumer receives the correct validated producer artifact; UI and release-path tests actually execute.
- [ ] OrbStack evidence covers all required usage scenarios; Linux or unit-only evidence cannot satisfy this gate.

**Verification commands / expected results:**

```sh
mise -C native run ci
mise -C native run merge
mise -C native run scheduled
mise exec -- mbx +1.99.0 xtask desktop test
mise exec -- mbx +1.99.0 xtask ci --e2e
mise exec -- mbx +1.99.0 nextest run -p jackin --features e2e --profile docker-e2e
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Native FFI/package paths are verified before edits. Shared manifests/generator changes go through the sole owner and require dependent revalidation. UI sessions and OrbStack are scarce host resources; serialize runtime-heavy tests.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert code/product-input changes together and rebuild in a fresh fixture prefix. Stop only task-owned fixtures; never stop/replace a live owner or reuse a migrated real state store.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-016"></a>
### TASK-016 — Reconcile documentation, operating records and final generation inputs

**Objective:** Publish one accurate runbook and source-attribution record, without converting historical claims into current verification.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** Every pinned SHA in the inventory, plus newly inventoried sources; no unpinned branch-tip substitution.

**Dependencies:** TASK-014, TASK-015. **Parallel group:** W09. **Assigned role:** Documentation/evidence owner, independent operator-runbook reviewer. **Initial state:** PENDING.

**Target files/components:** `GOAL.md`, `GOAL_PROMPT.md`, `goal-references/`, `TESTING.md`, `BRANCHING.md`, `plans/claude-unattended-monitor/`, `plans/jackin-build-split/`, `plans/cicd-reliability/`, `native/mise.toml`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Update commands and module links to the final selected tree, retaining separate historical baseline records. Remove obsolete flat-path instructions and old CI workflow assumptions only after proving their replacement.
2. Preserve useful decision history from c72/41265, s61, live monitor and CI evidence records with exact source/destination attribution. Mark prior test runs as historical and unexecuted live actions as NOT RUN.
3. Document intended API/schema changes, observation versus dispatch authority, package/native prerequisites and explicit attended/live authorization boundaries.
4. Reconcile every change matrix row with an integrated SHA and test/review evidence or an explicitly approved rejection/defer. Important blocked changes keep the overall goal BLOCKED; do not erase them to finish the checklist.
5. Keep AGENTS.md lean and respect its existing authority rather than adding the entire plan. Redact secrets and user-specific live paths from committed runbooks.
6. After the final documentation/config edit, request any necessary generator refresh from the sole generator owner and rerun reproducibility/policy gates, since docs may affect scan digests.

**Acceptance criteria:**

- [ ] All internal plan links and source references resolve; every exact branch has a dossier.
- [ ] Commands point to actual current tasks/modules and clearly distinguish live from fixture validation.
- [ ] Historical reports, current results and unrun checks are never conflated.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 xtask roadmap audit
mise exec -- mbx +1.99.0 xtask docs repo-links
mise exec -- mbx +1.99.0 xtask research check
mise exec -- mbx +1.99.0 xtask lint agents
python3 goal-references/tools/validate_bundle.py --root .
git diff --check
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Documentation can change generator scan inputs; do not approve generated-state reproducibility before the final documentation edit. Coordinator alone updates the master task ledger.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Revert only the inaccurate documentation/generated-input unit, preserving the decision log and historical failed evidence. Do not rewrite earlier audit records to conceal mistakes.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-017"></a>
### TASK-017 — Run final integrated build, policy, security and platform gates

**Objective:** Validate the exact composed integration head rather than a collection of individually green task branches.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** Every pinned SHA in the inventory, plus newly inventoried sources; no unpinned branch-tip substitution.

**Dependencies:** TASK-016. **Parallel group:** W10. **Assigned role:** Dedicated test agents, integration coordinator and independent validation reviewer. **Initial state:** PENDING.

**Target files/components:** `Entire integration tree`, `target/nextest/ (ignored test evidence)`, `native/.build/test-results/ (ignored test evidence)`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Freeze the candidate SHA and clean-worktree status. Inspect the full diff versus the pinned/revalidated target and every integrated task; no unreviewed file or unresolved conflict marker may remain.
2. Run default/all-feature nextest, full non-Docker xtask gates, scoped feature powersets, lint/policy/dependency checks, docs/snapshots and release-path builds on the selected pinned toolchain.
3. Run the mandatory OrbStack and native/UI gates at this exact composed head, not an earlier task SHA. Record actual selected and executed tests, host versions, first attempts, retries and all ignored/blocked scenarios.
4. Validate API/wire/schema migration decisions, sidecar packaging, fixture no-egress claims, error authority and full CLI launch/restore flow. Inspect test assertions as well as exit codes.
5. Check warm CI performance and exact-cache-hit behavior against the selected documented contract. Do not inflate timeouts, bypass policy, blanket-accept snapshots, or add broad lint/flake suppressions.
6. Return all new failures to the originating task for correction and independent re-review. Any subsequent code/config change invalidates affected final evidence and requires rerunning the relevant full-wave gates.

**Acceptance criteria:**

- [ ] All required build/test/policy/security/platform gates pass on the same reviewed integration head, with nonzero required tests.
- [ ] No unapproved baseline regression or undocumented skip remains; environmental blockers are explicit and prevent target integration.
- [ ] Validation evidence is independently reviewed and machine-readable where the repository provides it.

**Verification commands / expected results:**

```sh
mise exec -- mbx +1.99.0 nextest run
mise exec -- mbx +1.99.0 nextest run --all-features
mise exec -- mbx +1.99.0 xtask ci
mise exec -- mbx +1.99.0 hack check -p jackin -p jackin-diagnostics -p jackin-capsule -p jackin-agent-status -p jackin-runtime --feature-powerset --all-targets --locked
mise exec -- mbx +1.99.0 xtask ci --e2e
mise -C native run ci
mise -C native run merge
mise -C native run scheduled
git diff --check
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Only the integration agent advances the shared integration branch. Test agents use immutable worktrees and separate target/state directories; serialize constrained Apple hosts.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Leave the candidate branch reviewable but BLOCKED, preserve first failures and revert the offending reviewed unit only when the dependency rollback is understood.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-018"></a>
### TASK-018 — Independently audit every source for lost behavior

**Objective:** Perform the final no-silent-loss audit against all branch, commit, file and decision evidence.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** Every pinned SHA in the inventory, plus newly inventoried sources; no unpinned branch-tip substitution.

**Dependencies:** TASK-017. **Parallel group:** W11. **Assigned role:** Final audit agent not responsible for implementation or prior self-approval. **Initial state:** PENDING.

**Target files/components:** `goal-references/01-branch-inventory.md`, `goal-references/02-change-coverage-matrix.md`, `goal-references/branches/`, `Entire integration diff`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Re-read the complete inventory and every per-source coverage closure, including stale/experimental/archive/local branches and all duplicated refs. Confirm no branch disappeared between snapshot and final review.
2. For each meaningful source change, inspect the destination implementation/tests or the explicit rejection/supersession/defer evidence. Check assets, modes, docs, configs, dependency pins and migrations as well as Rust symbols.
3. Independently challenge high-risk claims: P's 366 commits all reviewed, U live acceptance not falsely claimed, all stash parents examined, synthetic recovery conflicts excluded, typed error authority preserved and native/CI obligations retained.
4. Inspect source-to-final behavioral diffs and test identities. Clean Git history, a shared patch ID, file presence or a green aggregate status is insufficient proof.
5. Classify each finding with severity and exact evidence. Return material issues to correction/review/validation; do not waive them solely to finish the plan.
6. Record auditor identity, exact reviewed head and a reasoned verdict. Without a genuinely independent reviewer, this task remains BLOCKED.

**Acceptance criteria:**

- [ ] Every source-to-destination row is reconciled and every material omission has explicit authorized disposition.
- [ ] All significant audit findings are resolved and verified at the final SHA.
- [ ] The auditor is independent; coordinator self-review is not counted as completion.

**Verification commands / expected results:**

```sh
python3 goal-references/tools/validate_bundle.py --root .
git diff --no-ext-diff --no-textconv --find-renames "$BASE_SHA" "$INTEGRATION_SHA"
git log --oneline "$BASE_SHA".."$INTEGRATION_SHA"
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** No source writes by the auditor. A moved target or branch reopens affected coverage and validation before approval.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Withdraw approval and return the affected task to IN_PROGRESS/FAILED with its original evidence preserved. Do not erase the finding or silently drop its coverage row.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-019"></a>
### TASK-019 — Integrate through the authorized squash-PR workflow

**Objective:** Only after all gates and explicit publication authorization, deliver the validated result to main under repository protections.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff)

**Dependencies:** TASK-018. **Parallel group:** W12. **Assigned role:** Coordinator/integration agent with independent final-head reviewer. **Initial state:** BLOCKED.

**Target files/components:** `Integration branch`, `Target refs/heads/main (only authorized PR merge)`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Default state: remote push/publication is NOT authorized. Keep the validated local integration branch and record BLOCKED_PERMISSION; do not create a remote branch, PR, comment, release or merge until explicit authorization covers it.
2. When authorized, refresh main, required checks, rulesets and all PR feedback. If main moved, incorporate it through the documented reviewed workflow and rerun affected coverage and final validation.
3. Use the repository's squash-only PR mechanism with DCO and Required statuses for the exact final head and all review threads resolved by justified disposition. Zero required GitHub approval count does not waive this plan's independent review.
4. Read all reviews/comments/replies, including outdated threads. Accepted feedback needs the fixing commit and verification; rejected feedback needs evidence. Do not delete or dismiss feedback or bypass protections.
5. Complete only the specifically authorized publication/merge operations. No force push, branch deletion, direct main commit or unrelated history rewrite is allowed.
6. After an authorized merge, record the new main SHA and verify the actual merged-tree CI/required workflows. Local validation alone is not proof that remote main was integrated.

**Acceptance criteria:**

- [ ] Explicit operation-specific authorization is recorded before any remote write.
- [ ] Exact final-head checks and independent review are satisfied and all feedback has a justified disposition.
- [ ] Main integration is reported only after the authorized squash merge is observed; otherwise the integration branch remains reviewable and status BLOCKED.

**Verification commands / expected results:**

```sh
git status --short; git rev-parse HEAD
git log --oneline "$BASE_SHA".."$INTEGRATION_SHA"
Read current ruleset, PR reviews/threads and exact-head required checks through authenticated GET APIs.
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** Current rules may differ from the planning snapshot. Never infer write authorization from token/admin permissions or repository instructions to push frequently.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** If publication or CI fails, stop and preserve the reviewable branch. After a merged regression, prepare a reviewed revert PR only with authorization; never reset or force-push main.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

<a id="task-020"></a>
### TASK-020 — Publish an honest completion or blocked-status report

**Objective:** Provide the final preserved/excluded/blocked change accounting with exact evidence and next action.

**Source branches:** [all 79 pinned refs plus newly discovered local/remote refs](goal-references/01-branch-inventory.md).
**Exact source tips:** Every pinned SHA in the inventory, plus newly inventoried sources; no unpinned branch-tip substitution.

**Dependencies:** TASK-018. **Parallel group:** W13. **Assigned role:** Coordinator, with report review separate from implementation. **Initial state:** PENDING.

**Target files/components:** `GOAL.md`, `goal-references/08-review-and-decision-log.md`, `goal-references/02-change-coverage-matrix.md`. Referenced package roots are verified where noted; TASK-004 must resolve child-file migrations before writes. Shared files have a single coordinator-designated owner.

**Procedure / implementation instructions:**

1. Report all discovered/analyzed branch refs, exact source and final SHAs, integrated features and the source-to-final coverage reconciliation.
2. List KEEP/ADAPT/COMBINE results and evidence for SUPERSEDED/REJECT/DEFER items; distinguish approved exclusions from unexamined or blocked important work.
3. List tests actually executed and their pass/fail/block/not-run results, environments, retries, independent reviewers and unresolved findings.
4. Report TASK-019 accurately: authorized main integration with observed SHA, or a local integration branch blocked by permission/checks. A permission blocker does not justify claiming the goal is complete.
5. Keep GOAL.md states and evidence links aligned. A final report may be delivered while the overall consolidation remains BLOCKED; do not label implementation DONE merely because the plan exists.

**Acceptance criteria:**

- [ ] Every completion statement is supported by source/result evidence.
- [ ] Any remaining blocker includes a concrete owner/action and no invented reviewer or test result.
- [ ] The target integration status is unambiguous.

**Verification commands / expected results:**

```sh
python3 goal-references/tools/validate_bundle.py --root .
git rev-parse HEAD; git status --short
```

Expected: each applicable check succeeds at the reviewed task SHA with a nonzero required test selection; read-only inventory commands produce complete, reconciled evidence. Commands unavailable on the required host are BLOCKED/NOT_RUN, never PASS. Source-specific M commands are used only on their matching baseline; the MBX commands apply to the selected P contract. Final runtime variables `BASE_SHA` and `INTEGRATION_SHA` are initialized in the workspace strategy.

**Potential conflicts:** TASK-020 depends on final audit for a completion report, but the coordinator must still provide a blocked-status report at any earlier stopping point. This exception is not task completion.

**Independent review requirements:** A reviewer other than the implementer must compare the immutable task commit to source evidence and acceptance criteria; coordinator self-review is not independent approval.

**Evidence to record:** Record source and destination SHAs, complete owned diff, test selection and executed counts, exact commands/exit codes, host/tool versions, first failures and retries, reviewer identity/verdict, and all coverage row IDs. Keep secrets and live-state contents out of artifacts.

**Rollback/recovery:** Correct an inaccurate report through an appended correction and updated status; retain the prior evidence rather than rewriting history.

**Completion checklist:**

- [ ] The specific acceptance criteria above are satisfied and source coverage is reconciled.
- [ ] Only owned files changed; intended behavior and relevant tests are preserved.
- [ ] Required validation actually ran and passed, with first failures/retries retained.
- [ ] A different reviewer approved the exact task commit; corrections were re-reviewed.
- [ ] Source attribution, final destination and evidence are recorded before DONE.

## 6. Integration waves

| Wave | Tasks | Concurrency / gate |
| --- | --- | --- |
| W00 | [TASK-001](#task-001) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W01 | [TASK-002](#task-002), [TASK-003](#task-003) | Parallel only with disjoint owned files; independent readers use immutable worktrees. |
| W02 | [TASK-004](#task-004) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W03 | [TASK-005](#task-005) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W04 | [TASK-006](#task-006) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W05 | [TASK-007](#task-007), [TASK-008](#task-008), [TASK-010](#task-010), [TASK-011](#task-011), [TASK-012](#task-012) | Parallel only with disjoint owned files; independent readers use immutable worktrees. |
| W06 | [TASK-009](#task-009), [TASK-013](#task-013) | Parallel only with disjoint owned files; independent readers use immutable worktrees. |
| W07 | [TASK-015](#task-015) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W08 | [TASK-014](#task-014) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W09 | [TASK-016](#task-016) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W10 | [TASK-017](#task-017) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W11 | [TASK-018](#task-018) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W12 | [TASK-019](#task-019) | Sequential gate; downstream tasks wait for reviewed/verified output. |
| W13 | [TASK-020](#task-020) | Sequential gate; downstream tasks wait for reviewed/verified output. |

Within multi-task waves, writers must have disjoint file ownership. Between waves, the coordinator integrates only reviewed immutable task commits and runs targeted plus cross-component checks. Failed work returns to correction and re-review; tasks are never dropped because reconciliation is difficult.

## 7. Final verification

- [ ] All tasks complete, or the overall result explicitly BLOCKED with exact unresolved obligations.
- [ ] Full source-to-result coverage and every unique ref reconciled.
- [ ] Unit, integration, required smoke/E2E, build, lint, feature/type, policy and dependency checks actually pass.
- [ ] macOS 26 Apple Silicon/OrbStack and native UI/release-path tests execute where required.
- [ ] API/wire/schema compatibility and migration predecessor/recovery tests verified.
- [ ] No secret/raw PTY leaks, unresolved source merge markers, orphaned tests or unexplained diff changes.
- [ ] Documentation, snapshots, generated inputs/outputs and operating records reviewed.
- [ ] Independent final audit at the same head as the final verification.
- [ ] Final remote checks/protections/feedback refreshed and explicit authorization recorded before publication.

[Full validation plan](goal-references/06-validation-and-test-plan.md) defines baseline versus selected-toolchain commands, host gates, expected counts/evidence and failure classification. A clean Git history or successful merge never substitutes for behavioral verification.

## 8. Recovery and rollback

Abort an in-progress merge/cherry-pick only in the task-owned worktree. For integrated changes, revert attributable units in reverse dependency order and rerun affected tests; do not reset or rewrite unrelated history. Preserve every original source ref and uncommitted operator file. Resolve competing agent edits in isolated branches, not with blanket ours/theirs.

Use disposable fixture state and complete snapshots for migration recovery; do not downgrade a real migrated store or stop a live broker. A failed final gate leaves the integration branch reviewable and BLOCKED. Any post-merge revert requires its own authorized protected PR. [Detailed risks/recovery](goal-references/07-risks-and-rollback.md).

## 9. Progress tracking

Allowed states: `PENDING`, `IN_PROGRESS`, `IN_REVIEW`, `BLOCKED`, `FAILED`, `VERIFIED`, `DONE`. VERIFIED requires actual acceptance evidence; DONE additionally requires independent approval and attributable integration. The coordinator alone updates this table, while agents submit separate immutable reports. Current rows are planning-stage initial execution states, not claimed implementation progress.

| Task | State | Current reason / completion evidence |
| --- | --- | --- |
| TASK-001 | BLOCKED | No operator checkout; clone DNS failure. |
| TASK-002 | BLOCKED | Complete local history/semantic coverage not available. |
| TASK-003 | BLOCKED | No Jackin baseline/platform test run. |
| TASK-004 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-005 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-006 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-007 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-008 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-009 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-010 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-011 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-012 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-013 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-014 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-015 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-016 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-017 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-018 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |
| TASK-019 | BLOCKED | Remote publication is not authorized. |
| TASK-020 | PENDING | Waiting for reviewed prerequisites; no implementation performed. |

Planning deliverables are separate from implementation completion. Consult [review/decision log](goal-references/08-review-and-decision-log.md) and [artifact integrity report](goal-references/10-artifact-integrity.md) for actual planning checks. No independent review or Jackin test success is inferred from this bundle's existence.

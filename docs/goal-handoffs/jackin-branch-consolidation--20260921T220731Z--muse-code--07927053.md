# GOAL: Consolidate every remote branch of jackin-project/jackin into main (oldest-first)

## A. Identity and pause status

- Handoff ID: `jackin-branch-consolidation--20260921T220731Z--muse-code--07927053`
- Created (UTC): 2026-09-21T22:07:31Z | Last update (UTC): 2026-09-21T22:25:00Z
  (publish metadata update; final head SHA in PR body + §7 receipt).
- Original goal status: `PAUSED_BY_USER`
- Handoff status: `READY` (all §7 checks passed — see publish log below).
- Worker stop status: VERIFIED. `subagent_status(status_filter=running)` returned
  `not_found` after the single running goal worker (CI watcher ordinal 168) was sent a
  stop message and then cancelled via `subagent_cancel` (accepted; terminal
  `cancelled` receipt observed). No other goal-owned agents, watchers, retry loops, or
  background tasks exist. Handoff-only auditors (ordinals 169-172) ran read-only and
  finished; they performed no implementation.
- Runtime pause control: NO agent-accessible goal-pause API exists
  (`update_goal` offers only `complete`/`blocked`, both wrong here). Mitigation: all
  workers stopped, this turn ends in STOP with no continuation scheduled, and a final
  `report_progress` records `PAUSED_BY_USER` + this handoff path so any future
  automatic continuation sees the pause first. `PAUSED_BY_USER` expresses the requested
  disposition; it is not proof of a runtime-level pause primitive.
- Source agent/CLI: Muse Code (CLI) | Session: green-lynx
  (`01a0bfbf-ab66-7a50-b63d-567aa1ea96de`) | Goal ID:
  `goal-f2bb71d1-79a9-4852-be7e-e6fb3d88616b` (98% at pause).
- Repository: `https://github.com/jackin-project/jackin.git` (sanitized; no creds).
- Handoff path (on preservation branch): `docs/goal-handoffs/jackin-branch-consolidation--20260921T220731Z--muse-code--07927053.md`
- Source branch / HEAD at pause: `integrate/branch36-verify-preview` @
  `85a7bd39a0d466b692af39f08ceb2f1fa267b240` (pushed; PR #1066 head, verified match).
- Checkpoint code SHA(s): port commit `c5c4cdad`, fix commit `e30f23bf`, merge commit
  `85a7bd39` (all on the integration branch, all pushed).
- Preservation branch: `goal-handoff/jackin-consolidation-07927053` (TO-CREATE in
  §6 from main `df4671e4` — verified absent pre-publish; pre-publish step:
  create worktree + branch, commit this file, push, verify remote head, open
  draft PR; TASK-CREATED, cleanup candidate at final completion — NOT during pause).
- PR base / observed SHA: `main` @ `df4671e4d9f2860e90a5c71d8d0bd85b23d23291`
  (observed 2026-09-21T22:05Z; re-verified unchanged 22:14Z).
- Handoff PR URL: https://github.com/jackin-project/jackin/pull/1070 (DRAFT,
  base `main`, head `goal-handoff/jackin-consolidation-07927053`; auto-merge
  n/a for drafts; no merge queue entry; CI triggered by push is observational
  only — no repair loop during pause).
- Recovery portability: NOT fully remote-portable. Depends on explicitly listed
  local artifacts in section I: the campaign store
  `/Users/donbeave/Projects/github/_consolidation/jackin/` (1.3 GB: LEDGER.md,
  evidence/, bundles/ — NOT a git repo, no remote) and the session log. All code
  checkpoints are on remote refs; all review verdicts are summarized in the LEDGER
  (local) and in this HANDOFF (remote PR).
- Resume authorization: explicit later user request only (see section J).

## B. Original goal and success contract

RECOVERED (verbatim user objective, secrets redacted — full text in goal record
`goal-f2bb71d1-79a9-4852-be7e-e6fb3d88616b`; operative excerpt):

> Repository: https://github.com/jackin-project/jackin/branches/all — Target branch:
> main. Highest priority: consolidate EVERY remote branch into main oldest-first,
> preserving/improving what supports current direction, rejecting the rest, deleting
> each processed source branch. Final state: verified main with all accepted work,
> resolved PRs, no other remote branches. Authorizes: implementation, commits,
> pushes, PR ops, review replies, closure comments, compliant merges, deletion of
> fully evaluated branches.

RECONSTRUCTED consolidated statement: systematically dispose 41 frozen-queue
branches plus later/concurrent arrivals, oldest-first, via per-branch analysis +
independent challenge + selective fresh-port replacement PRs (or verified delete),
ending with only `main` on the remote, all accepted work integrated and verified,
all PRs merged/closed with rationale, full evidence ledger.

Scope: all `refs/heads/*` except `main` in jackin-project/jackin (incl. no-PR,
draft/closed/merged-PR, and newly arriving branches). Non-goals: other repos;
whole-merges or cherry-picks of stale stacks; root/process docs (LEDGER:48
rejection class); `preview.yml` copies (regen-only); journal/staged-proof designs
rejected in favor of landed #1006/#1046 machinery; unrelated prod ops.
Constraints/preferences (user + campaign): squash-only merges; `Signed-off-by:
Alexey Zhokhov <alexey@zhokhov.com>` only, no Codex trailers; oldest-first, one
active source branch; guarded deletion only
(`--force-with-lease=<ref>:<reviewed-SHA>`); delegate-first via subagents with
independent review; commit often, push regularly; verify recovery bundles before
delete; never weaken checks to merge faster.
Acceptance (all must hold): every in-scope branch disposed oldest-first with
evidence; accepted work on latest main + tests/docs; rejections reasoned and not
reintroduced; PRs merged/closed with comments; checks green on final main; fresh
remote inventory contains ONLY main; nothing accepted left local-only/unpushed;
publishing/ops paths intact; ledger + recovery artifacts durable.
Post-resumption obligation (from this handoff): integrate all required related
goal work, resolve its PRs, then clean up verified-obsolete goal-owned local
worktrees/branches per section E.6 gates. Never blind-merge experiments; never
delete shared/unrelated/unknown resources; respect repo merge policy (squash).

## C. State at the exact interruption point

Last completed action: pushed merge commit `85a7bd39` (merge main `df4671e4` into
`integrate/branch36-verify-preview`, conflict in the generated state file resolved
via pinned-generator regen → scan `98f2cdd66b3d75e4`) and spawned a fresh CI watch
for PR #1066.
In-progress at pause: CI re-watch on #1066 @ `85a7bd39` (cancelled mid-poll by this
pause; last direct observation: 42 pass / 3 pending / 0 fail, MERGEABLE/BLOCKED).
Outcome: unknown whether the 3 pending jobs finished — resumption MUST re-observe.
Exact next intended action: re-observe #1066 (head/main/checks), merge via squash
with `--match-head-commit` on green + existing APPROVE-MERGE, verify main, post
closure comment + close #1004, guarded-delete `codex/verify-preview-package` @
`073ebbc5`, continue with b37.
Partial work / inconsistencies: none in the tree (worktree clean). Open hypothesis:
none blocking. Known external drift at pause: main advanced twice during b36
(`7df4083d` → `df4671e4` via owner-merged #1053); 2 brand-new branches appeared
(`cicd/s2-repromote-80bc420d` #1065, `docs/report-live-results` #1067);
`fix/usage-broker-fallback` gained PR #1063 and moved tip again.
Workers at pause: CI watcher ordinal 168 (CANCELLED, verified); re-review ordinal
165 (FINISHED, APPROVE-MERGE); regen operator 167 (FINISHED); auditors 169-172
(FINISHED, read-only). No remote jobs owned by this goal except PR #1066 CI runs
(triggered by pushes; safe to leave running).

## D. Requirement-by-requirement progress ledger

Format: `ID | Requirement | Status | Evidence | Remaining | Dependencies`.
Branch IDs b1..bN follow the frozen queue + arrivals; full per-branch evidence lives
in LEDGER.md (LOCAL-ONLY path below) and is summarized here.

- R-QUEUE | Build + maintain oldest-first queue | VERIFIED_DONE (at pause) |
  LEDGER queue L15-29 + 21-head ls-remote snapshot in E.3 (2026-09-21T22:05Z) |
  re-inventory at each resume/boundary | none.
- R-B1-B28 | Dispose branches 1-28 (ports #1013/#1020/#1027/#1033/#1040/#1046/
  #1048/#1050 + already-present deletes) | VERIFIED_DONE | LEDGER dispositions 1-28;
  7/7 deletion-sample refs verified absent (audit 172) | none | none.
- R-B29 | Port credential-boundary Group A+D via #1055 | VERIFIED_DONE | main
  `8f0c1209` (#1055 squash); ref absent (verified); LEDGER:29 | none | none.
- R-B30 | Delete already-present f080 | VERIFIED_DONE | lease-held delete; ref absent
  (verified); LEDGER:30 | none | none.
- R-B31 | Delete superseded generation-lease | VERIFIED_DONE | lease-held delete; ref
  absent (verified); LEDGER:31 | none | none.
- R-B32 | Port wire-format test via #1061 | VERIFIED_DONE | main `7df4083d` (#1061
  squash); ref absent (verified); LEDGER:32 | none | none.
- R-B33 | Reject holla-parity (N4 failed execution gate) | VERIFIED_DONE |
  lease-held delete; ref absent (verified); b33 bundle verified; LEDGER:33 | none.
- R-B34 | Delete already-present audit-f080 | VERIFIED_DONE | lease-held delete; ref
  absent (verified); LEDGER:34 | none | none.
- R-B35 | Verify #1003 squash-landed + delete | VERIFIED_DONE | tree-identical
  squash proof; lease-held delete; ref absent (LEDGER:35; ref check at resume) | none.
- R-B36 | Port #1004 packaging core via #1066, close #1004, delete branch |
  IN_PROGRESS | candidate `85a7bd39` pushed; APPROVE-MERGE (delta re-review);
  CI 42 pass/3 pending/0 fail (STALE — re-observe); #1004 still OPEN (verified) |
  merge → close #1004 → guarded-delete @073ebbc5 (section H-1) | CI green.
- R-B37..B41 | Frozen-PR group (audit-evidence, #1006×2, #1007, #1009) | NOT_STARTED
  (b37 recon done, DELETE-likely) | evidence/b37-*-analysis.md | full disposition each.
- R-LATER | Later/concurrent arrivals (≈12 incl. proofs, CI actors, #1062-closed
  branch, #1063-#1067) | NOT_STARTED | inventory E.3 | queue by policy, dispose each.
- R-EXT | Externally merged branches (#1052/#1054/#1056/#1057/#1059/#1053) |
  VERIFIED_DONE | squash commits on main; refs absent (spot-verified; full recheck at
  final gate) | none.
- R-FINAL | Final only-main verification + report | NOT_STARTED | — | section H-6.
- R-CLEAN | Post-integration local cleanup | NOT_STARTED (FORBIDDEN during pause) |
  runbook E.6 | after R-FINAL.

## E. Change and preservation inventory

All code checkpoints are pushed to remote refs (nothing accepted is local-only).
The 1.3 GB campaign store (LEDGER + evidence + bundles) is LOCAL-ONLY (not a git
repo, no remote) — the one nonportable dependency (section I). /tmp scratch
(review notes, diffs, legacy bundles) is wipe-prone; verdicts are preserved in
LEDGER + this HANDOFF; per-branch durable bundles cover recovery.

### E.1. Discovery scope and ownership

Inspected (2026-09-21T22:05-22:10Z): git common dirs C-1
(`/Users/donbeave/Projects/github/jackin`) and C-2 (`.../velnor`) via
`worktree list`; full `ls-remote refs/heads/*` (21 heads, complete); `gh pr list`
open (12, limit 40, complete) + targeted merged/closed PR reads; `/tmp` campaign
scratch (`ls`/`du`); `_consolidation/jackin` store (`du`, listings). No filesystem
scan beyond these goal-connected locations. Coverage uncertainty: LOW for jackin
(all refs + PRs enumerated); velnor worktrees WT-4..WT-7 classified by path/name
only (not inspected inside — excluded as unrelated/shared).
Ownership: `GOAL_EXCLUSIVE` = created by this campaign for this goal;
`GOAL_SHARED` = velnor regen worktree (tool use in a shared repo);
`UNRELATED`/`UNKNOWN` = retained, never touched.

### E.2. Local worktree and clone ledger

- C-1 | jackin clone | `/Users/donbeave/Projects/github/jackin` | origin
  jackin-project/jackin | primary checkout retained | NOT_APPLICABLE (keep).
- WT-1 | C-1 main worktree | same path | branch `main` @ `fce94cea` (STALE, behind
  remote `df4671e4`; never used for campaign edits) | CLEAN, no lock |
  GOAL_SHARED (primary checkout) | KEEP.
- WT-2 | C-1 linked worktree | `/tmp/jackin-int1002` (106 GB incl. build cache) |
  branch `integrate/branch36-verify-preview` @ `85a7bd39` (matches remote + PR #1066
  head — VERIFIED) | CLEAN, no lock, no in-progress merge/rebase | GOAL_EXCLUSIVE |
  INTEGRATE_THEN_REMOVE only after R-FINAL (REQUIRED for resume; wipe-prone path —
  all content pushed, so rebuildable via fresh clone + checkout).
- WT-3 | C-2 linked worktree | `/tmp/velnor-gen-4dec6b9e` | detached @ `4dec6b9e`
  (pinned velnor-workflow generator build) | GOAL_SHARED | KEEP until campaign end
  (future regens need it; rebuildable: `git worktree add --detach` + cargo build).
- WT-4/5/6 | C-2 | `/tmp/pr1040-review`, `/tmp/pr1047-review`, `/tmp/velnor-b07-port`
  | UNKNOWN (other sessions/goals by name) | REVIEW_SHARED — do not touch.
- WT-7 | `/private/var/.../grok-goal-*/velnor-048` | UNRELATED (another agent) | exclude.
- WT-8 | C-2 | `/tmp/velnor-handoff-b04e988e` @ `53bc9f1e`
  [`goal-handoff/velnor-consolidation-b04e988e`] | UNRELATED (another goal's
  handoff worktree) — do not touch.
- Stashes: TO-VERIFY (coordinator runs `git stash list` on C-1/C-2 before publish;
  auditors reported none observed).
- Campaign store (not a worktree): `/Users/donbeave/Projects/github/_consolidation/jackin/`
  — LEDGER.md + evidence/ (analyses, ~40 files) + bundles/ (38 verified bundles,
  1.3 GB) + recovery-20260920.bundle. GOAL_EXCLUSIVE. No remote. KEEP (see I).

### E.3. Local and remote branch ledger (21 campaign heads @ 2026-09-21T22:05Z;
26 live heads incl. 5 foreign task refs @ 22:14Z — see F-01..F-05 below)

IDs B-RL-xx. Tips are short SHAs (full SHAs in auditor evidence + ls-remote log).
Queue key: b36 > b37 > b38 > b39 > b40 > b41 > later/concurrent (PR-date order).

- B-RL-01 `main` @ `df4671e4` — target. PRs merged into it listed in E.4.
- B-RL-02 `integrate/branch36-verify-preview` @ `85a7bd39` — TASK-CREATED,
  GOAL_EXCLUSIVE. Local (WT-2) == remote == PR #1066 head (VERIFIED). Commits:
  `c5c4cdad` port, `e30f23bf` CI fixes, `85a7bd39` main-merge+regen. Disposition:
  auto-delete on squash-merge (verify), else delete at R-FINAL.
- B-RL-03 `codex/verify-preview-package` @ `073ebbc5` — b36 ACTIVE source. PR #1004
  OPEN. Guarded-delete AFTER #1066 merge + #1004 close (lease `073ebbc5`).
- B-RL-04 `audit/pr1002-evidence` @ `f38cb9e` — b37. PR #1005 CLOSED-unmerged
  (2026-09-21T08:00Z, base branch deleted). Recon DELETE-likely.
- B-RL-05 `codex/pr1006-config-migration-atomicity` @ `e4f8d81` — b38. PR #1006 MERGED.
- B-RL-06 `codex/ci-performance-campaign` @ `33b9189` — b39. PR #1007 OPEN draft.
  Carries #1007-deferred content (b33 N7/N8/N9 + schema-2 follow-ups).
- B-RL-07 `codex/pr1006-corrective-fix` @ `9033f95` — b40. PR #1008 MERGED.
- B-RL-08 `codex/fix-meter-install-error-20260920` @ `7787f1b` — b41. PR #1009
  CLOSED-unmerged (base `feat/multi-account-support` deleted; head live @
  `7787f1b4` — reviewer-verified). Branch stays queued.
- Later/concurrent (PR-date order): B-RL-09 `proof/instruction-only-no-work` @
  `09e0d3f` (#1030); B-RL-10 `migrate/apple-ci-generic` @ `6ff54ce` (#1044);
  B-RL-11 `proof/instruction-only-no-work-d1` @ `5a53e63` (#1045 do-not-merge);
  B-RL-12 `proof/instruction-only-no-work-d3` @ `b426225` (#1058 do-not-merge);
  B-RL-13 `proof/positive-control-d3` @ `0587381` (#1060 do-not-merge);
  B-RL-14 `cicd/repromote-major1-9660c9ff` @ `0d369a8` (#1062 CLOSED-unmerged
  21:06Z, reason UNKNOWN — queue by its PR date 19:43Z);
  B-RL-15 `fix/usage-broker-fallback` @ `ec3dbcf6` (#1063, tip moves often —
  moved `2754bd2` → `ec3dbcf6` during pause write-up);
  B-RL-16 `ci/first-attempt-evidence` @ `f140258` (#1064);
  B-RL-17 `cicd/s2-repromote-80bc420d` @ `909a9f5` (#1065 21:05Z, NEW at pause);
  B-RL-18 `docs/report-live-results` @ `0515ad3` (#1067 21:35Z, NEW at pause);
  no-PR (fallback order TBD at resume): B-RL-19 `cicd/integration` @ `8f2d72a`,
  B-RL-20 `cicd/s1-mise-desktop` @ `17bf8ab`, B-RL-21 `rollout/agent-policy` @ `95b5f96`.
- Handoff branch (TO-CREATE in §6 — verified absent pre-publish):
  `goal-handoff/jackin-consolidation-07927053` — TASK-CREATED, GOAL_EXCLUSIVE;
  cleanup candidate at R-FINAL only.
- FOREIGN task refs (observed live during pause; NOT owned by this campaign —
  NEVER touch, delete, or merge; resuming agent reconciles queue impact):
  F-01 `preserve/handoff-1402ca52/integrate-velnor-apple-ci` @ `8fb49688`;
  F-02 `preserve/handoff-1402ca52/jackin-m1rehearsal` @ `2c57f74b`;
  F-03 `preserve/handoff-1402ca52/jackin-scratch` @ `0b02a313`;
  F-04 `goal-handoff/verify-merge-pr1063-cb81b336` @ `c466c9bd`;
  F-05 `goal/handoff-jackin-velnor-20260921t220036z-51368cd2` @ `eca5dc97`.
  Live head count moved 21 → 26 during the pause write-up (5 foreign arrivals +
  tip move `fix/usage-broker-fallback` → `ec3dbcf6`). These belong to other
  agents' concurrent handoff/verify activity (different handoff IDs); they are
  recorded for coverage, not queued for disposal.
- Deleted refs (verified absent, sample): b29/b30/b31/b32/b33/b34 + fix-meter
  siblings per §D; full list in LEDGER. No resurrection observed at pause.

### E.4. Related PR ledger

OPEN (12, all base `main` unless noted): #1004 (b36 source, superseded by #1066 —
FUTURE: comment + close after #1066 merge); #1007 draft (b39, owns deferred
schema-2/CI substance); #1030/#1045/#1058/#1060 (CI proofs, three do-not-merge);
#1044 (Apple CI recipe); #1063 (usage-broker-fallback — tip volatile);
#1064 (first-attempt evidence); #1065 (s2 repromote, NEW); #1066 (b36 replacement —
FUTURE: merge on green); #1067 (post-1053 report, NEW).
CLOSED-unmerged: #1005 (b37, base deleted → unmergeable; branch still queued);
#1062 (branch live, reason UNKNOWN — investigate at its turn, do NOT assume);
#1009 (b41, base deleted → unmergeable; branch still queued).
MERGED (campaign + external, most recent first): #1053→`df4671e4` (external,
carried-failures); #1061→`7df4083d` (b32); #1052 (external, schema-2 CI);
#1056/#1057/#1059 (external); #1055→`8f0c1209` (b29); #1054 (external);
#1003/#1006/#1008 (old, squash-landed); #1013/#1020/#1027/#1033/#1040/#1046/#1048/
#1050 (campaign ports). #1066 checks at pause: 42 pass / 3 pending / 0 fail,
MERGEABLE/BLOCKED (STALE after pause — re-observe).
PR review obligations: #1066 has no blocking human/bot reviews (bot notice only);
older open PRs carry whatever reviews exist — the resuming agent MUST reread all
review comments on a PR before merging/closing it.

### E.5. Integration map and ordered landing plan — FUTURE EXECUTION ONLY

Map: WT-2 → `integrate/branch36-verify-preview` @ `85a7bd39` (remote, same SHA) →
PR #1066 → `main`. Then per-branch: source ref → (replacement PR | direct delete
with lease) → `main`; PRs closed with rationale. Order: H-1 (b36) → b37 → b38 →
b39 (#1007: land deferred CI/schema-2 substance incl. b33 N7/N8/N9) → b40 → b41 →
B-RL-09→18 in PR-date order → B-RL-19/20/21 (order TBD by fallback policy) →
handoff branch deletion → R-FINAL gate. Parallelizable: read-only recon of later
branches; sequential: all target-branch mutations (merges, closes, deletes), one
source branch at a time. Each step needs: pinned SHAs, analysis + independent
challenge, candidate review for ports, green CI on the merged head, guarded
delete, ledger update. Do NOT execute any of this during the pause.

### E.6. Post-integration local cleanup runbook — FUTURE EXECUTION ONLY

Documented now; executes only after explicit resumption + verified R-FINAL.
Candidates (re-observe EVERYTHING before acting; any drift → stop that row):

- WT-2 `/tmp/jackin-int1002` | expect `integrate/branch36-verify-preview` @
  merge-descendant tip | target n/a (worktree) | proof: R-FINAL done + branch
  deleted remotely + `git worktree list` shows it clean | recovery: remote refs +
  bundles | gates: no process using it; clean | action: `git worktree remove
  /tmp/jackin-int1002` (no --force) | Status: PENDING (needs R-FINAL).
- WT-3 `/tmp/velnor-gen-4dec6b9e` | expect detached `4dec6b9e` | proof: no future
  regen needed (R-FINAL done) | recovery: rebuildable from velnor repo + rev |
  gates: C-2 admin clean; no other goal use | action: `git -C <velnor> worktree
  remove /tmp/velnor-gen-4dec6b9e` | Status: PENDING.
- /tmp scratch (`/tmp/b*.txt`, `/tmp/*.diff`, legacy `/tmp/*.bundle`) | action:
  remove ONLY files with durable counterparts (LEDGER verdicts / evidence/bundles);
  verify each first | Status: PENDING.
- RETAIN (never cleanup candidates): C-1 clone + WT-1, campaign store
  `_consolidation/jackin` (durable record), HANDOFF branch until R-FINAL then
  remote-delete with PR, WT-4..WT-7 (not ours), C-2 clone itself.
- Local branches in WT-2/C-1: only `integrate/branch36-verify-preview` (goal);
  delete locally after its remote deletion post-merge (`git branch -d`, verify
  tip SHA first). No other goal branches exist locally.
- Remote cleanup (handoff branch, source branches) follows the ORIGINAL goal
  authorization (guarded deletes after verified disposition), NOT this runbook
  alone. After cleanup: re-enumerate worktrees/branches, write receipt to LEDGER.

## F. Decisions, findings, assumptions, and rejected approaches

- Selective fresh ports (never whole-merge/cherry-pick of stale stacks): every
  ported branch was re-derived onto current main; whole-merges proven catastrophic
  (+7k/-30k-class regressions repeatedly). Standing rule.
- Rejected classes (do not reintroduce without new evidence + independent review):
  root/process docs (LEDGER:48); plans/multi-account/* (LEDGER:48/55, salvaged to
  research mdx); journal persistence (LEDGER:58, #1006 won); staged-proof relay
  (Group C, #1046 won); parallel publish machinery (N3/#1027); DeleteTag migration
  (AdvanceTag won); failing test micros (b33-N4 hang, b36 micro red).
- Deferred (not dropped): preview.yml + package-release declare (generator lacks
  the primitive at pin 4dec6b9e — needs typed prepublish capability first);
  tap-PR updater script (absent both sides); b33 N7/N8/N9 + schema-2 substance
  (owned by queued #1007/b39); per-kind HOME follow-up (deferred, LEDGER).
- Owner rulings honored: typed Velnor package-release capability over
  verify-task convention (#1004 discussion → drove 59da6060 + G1 deferral);
  squash-only; Alexey-only signoff.
- Finding: mise.toml is a velnor-workflow render input — any mise change needs a
  pinned regen of `.github-actions-generator-state` or Policy `generated-tree`
  fails (proven on #1066, run 35657105324). Regen tool: WT-3 (pinned 4dec6b9e).
- Finding: tar must be a workspace dep (cargo-deny workspace-duplicate ban);
  jackin-image now inherits it (fix commit e30f23bf).
- Assumption needing no validation: none blocking. Open question: why #1062 was
  closed unmerged (UNKNOWN — investigate at its turn).

## G. Verification evidence and known failures

- #1066 @ `85a7bd39` (merge head): 42 pass / 3 pending / 0 fail (gh pr checks,
  2026-09-21T22:05Z approx, pre-pause) — INTERRUPTED (watch cancelled by pause;
  STALE on resume).
- #1066 @ `e30f23bf`: Policy PASS (regen verified), DCO PASS; CI/PR run missing at
  watch time (unresolved observation, possibly transitional) — STALE (superseded
  by merge head).
- #1066 @ `c5c4cdad`: 38 pass / 2 fail (rust-policy tar-duplicate + Policy
  generated-tree, both deterministic) / 3 pending — FAIL, root-caused and fixed
  in `e30f23bf` (re-review APPROVE-MERGE).
- Local gates on port: `cargo fmt --check` PASS; `cargo test -p jackin-xtask`
  361/361 PASS; `cargo clippy --workspace --all-targets` clean (1 pre-existing
  future-incompat note); `mbx deny` green after fix (fixer-verified) — PASS at
  e30f23bf; merge head adds only main + regen fingerprint (re-verify cheaply).
- Main `df4671e4` push-CI: success (auditor-verified). No unrelated failures known.

## H. Ordered remaining-work plan (DO NOT EXECUTE during pause)

- H-1 (FIRST TASK): Finish b36. Re-observe #1066 (head? main? checks?—
  verify `85a7bd39` still head, main still `df4671e4`, all checks green,
  MERGEABLE/CLEAN). Merge squash with `--match-head-commit 85a7bd39...`.
  Verify main + test presence. Comment + close #1004 (superseded-by-#1066 text:
  ported core + AdvanceTag + retirement; dropped micro/gen/stamps; deferred
  preview.yml/declare + updater). Guarded-delete `codex/verify-preview-package`
  (lease `073ebbc5`). Update LEDGER. Validation: ref absent; #1004 closed;
  #1066 squash on main.
- H-2: b37 `audit/pr1002-evidence` (recon DELETE-likely; #1005 already closed):
  disposition review (recheck mdx post-#1004-resolution + ledger salvage grep),
  guarded-delete. Then b38 (verify #1006 squash), b39 (#1007: land deferred
  substance), b40 (#1008 verify), b41 (#1009 state check + dispose).
- H-3: Later/concurrent B-RL-09→21 in order (proofs incl. do-not-merge handling,
  #1062-closed branch investigation, volatile-tip branches last-observed-state
  refresh, no-PR trio fallback ordering).
- H-4: Delete handoff branch + close handoff PR after R-FINAL (remote cleanup per
  original authorization, guarded/last).
- H-5: R-FINAL gate (section 10 criteria: only-main inventory, post-merge checks,
  ledger + bundles durable) + completion report.
- H-6: R-CLEAN per E.6 (re-observe every row; stop on drift; write receipt).

## I. Environment and operational recovery

- Host: macOS (darwin, user donbeave). Toolchain: repo-pinned Rust
  (cargo/rustc), `cargo-nextest`, `gh`, `git`, `mise` (repo tasks), pinned
  velnor-workflow @ `4dec6b9e` (WT-3 build; rebuild: detached worktree +
  `cargo build -p velnor-workflow`).
- Repos/clones: C-1 jackin (main op), C-2 velnor (regen tool only).
- Nonportable (local-only) artifacts + retrieval: campaign store
  `/Users/donbeave/Projects/github/_consolidation/jackin/` (LEDGER.md,
  evidence/*analysis.md + /tmp/b*-review.txt originals in /tmp, bundles/ 38
  files incl. b18/b20/b24-b36 + recovery-20260920.bundle) — exists ONLY on this
  host; back it up before any host migration. Session log:
  `/Users/donbeave/.local/share/muse/sessions/2026/09/20/01a0bfbf-ab66-7a50-b63d-567aa1ea96de/session.jsonl`
  (+ per-agent `subagent/` logs) for transcript recovery.
- No credentials are stored in the handoff. `gh`/`git` use the host's existing
  auth (verify with `gh auth status` at resume; re-auth per host policy if stale).
- External state changed by campaign (all intentional, all recorded): merged PRs
  (#1013…#1061), branch deletions (lease-held), PR #1066 open, #1004 open,
  handoff branch + draft PR (this checkpoint). CI runs on #1066 may still be
  in flight at pause — safe to leave; resume re-observes, never assumes.
- No deployments/migrations/infra changes were made (CI-only side effects).

## J. Fresh-agent resume runbook

1. Read this document COMPLETELY, then LEDGER.md
   (`/Users/donbeave/Projects/github/_consolidation/jackin/LEDGER.md`) tail from
   "Dispositions continued (main baseline 7df4083d)", then linked evidence for the
   active branch.
2. Verify host state: same machine? campaign store present? `gh auth status` OK?
   WT-2 present? If WT-2 is gone (wipe-prone /tmp), recreate: fresh
   `git worktree add /tmp/jackin-int1002 integrate/branch36-verify-preview` from
   C-1 after `git fetch origin`. If the store is gone → BLOCKED (see K).
3. Re-observe live remote state FIRST: `git ls-remote` (main + queue heads),
   `gh pr view 1066` (head/checks/mergeState), `gh pr view 1004` (still open?).
   Reconcile with section E; main may have advanced again — follow the
   main-advance procedure (inspect delta, re-verify mergeability, repeat affected
   checks) before merging anything.
4. Resume the ORIGINAL goal at H-1 (first task above), NOT this handoff task.
   Keep all original constraints (squash-only, Alexey-only signoff, oldest-first,
   guarded deletes, independent review, delegate-first).
5. Maintain LEDGER + evidence as the working record; commit progress per branch.
6. After R-FINAL, execute E.5 remaining landings (if any), then E.6 cleanup with
   live re-observation per row.
7. Interpretation rule: this pause lasts until the USER requests resumption. A
   later instruction `/goal Read and resume docs/goal-handoffs/jackin-branch-consolidation--20260921T220731Z--muse-code--07927053.md`
   authorizes continuing the ORIGINAL goal. It does NOT mean pause again,
   regenerate this handoff, or create another handoff PR.

Resume command: `/goal Read and resume docs/goal-handoffs/jackin-branch-consolidation--20260921T220731Z--muse-code--07927053.md`
(first checkout the handoff branch or read it via the PR: `gh pr checkout <handoff-PR#>`).

## K. Blockers, omissions, and independent review

- Blockers: NONE for preservation/publication (all checkpoints pushed; doc is
  local until §6 completes). Resume preconditions: same host (or migrated
  campaign store), working `gh` auth, WT-2 or ability to recreate it.
- Omissions/unknowns: (1) #1062 close reason UNKNOWN; (2) full SHAs abbreviated
  in E.3 (re-pin live at resume anyway); (3) velnor WT-4..WT-8 contents not
  inspected (excluded by ownership); (4) CI on `85a7bd39` left in-flight
  (3 pending) — outcome unknown by design (pause forbids repair loops).
- Stash check: DONE — `git stash list` empty on C-1 and C-2; WT-2 status clean
  (coordinator, pre-publish 22:1xZ).
- Independent review: DONE (reviewer ordinal 173, read-only vs live evidence).
  Initial verdict NEEDS-FIXES; all 5 fixes applied: (1) 3→5 foreign refs added
  (F-01..F-05, never-touch); (2) handoff branch marked TO-CREATE + pre-publish
  step added; (3) #1009 → CLOSED; (4) WT-8 added (unrelated); (5) wording:
  "created" → TO-CREATE, real UTC timestamps, stash recorded. E.6 gates: PASS.
  Secrets: NONE. Fresh-agent recovery: YES (biggest gap — unclassified
  preserve/* refs — now fixed).

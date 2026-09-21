# GOAL: Verify jackin locally (console + capsule usage, agents boot) and merge PR

## A. Identity and pause status

```yaml
handoff_id: verify-jackin-merge-pr1063--20260921T220648Z--muse-code--cb81b336
created_utc: 2026-09-21T22:06:48Z
updated_utc: 2026-09-21T22:35:00Z
audit_outcome: VERIFIED (audit §L; all sources FULL, all gaps repaired, publication re-verified)
original_goal_status: PAUSED_BY_USER
handoff_status: READY
source_cli: Muse Code (session apricot-canopus, id 01a0c4b8-8b97-7570-b7b7-5c1a55026a05)
repository: git@github.com:jackin-project/jackin.git (local: /Users/donbeave/Projects/jackin-project/jackin-main)
handoff_path: docs/goal-handoffs/verify-jackin-merge-pr1063--20260921T220648Z--muse-code--cb81b336.md
source_branch: fix/usage-broker-fallback
source_head_at_pause: 2754bd217a93de841df0b80c6f4ce3b3c1b75168
checkpoint_code_sha: ec3dbcf662f1f62eb20a53e7a9e4ae6004c05498 (fix(usage) keychain fail-fast, pushed to origin/fix/usage-broker-fallback)
preservation_branch: goal-handoff/verify-merge-pr1063-cb81b336
pr_base: main (observed remote head df4671e4d9f2860e90a5c71d8d0bd85b23d23291; PR base record c52e912bd3757c4ca736be288b0e03779144b561)
goal_pr: https://github.com/jackin-project/jackin/pull/1063 (OPEN, non-draft, CONFLICTING)
handoff_pr: https://github.com/jackin-project/jackin/pull/1069 (DRAFT, base fix/usage-broker-fallback, doc-only)
remote_portable: PARTIAL (code + HANDOFF remote; /tmp evidence reports, docker images/containers, keychain/credential state are machine-local, see §E/I)
resume_authorization: explicit later user request only
```

Worker stop status (verified via subagent_status + terminal results): the single running
goal worker (keychain-nohang builder) returned DONE+PASS coincident with the stop
request; its cancel was accepted after its terminal result. All other goal subagents
were already terminal. No goal-owned background loops, watchers, or retry daemons exist.
Runtime goal-control limitation: the available tool supports only complete/blocked.
At handoff creation no runtime pause transition was performed by this agent;
a runtime `terminal_pause` record was subsequently observed (2026-09-21T22:15:53Z,
80%, next: HANDOFF §H item 2). PAUSED_BY_USER is recorded here administratively
either way. `PAUSED_BY_USER` expresses the requested
disposition, not proof of a remote-job stop (no remote jobs were started by this goal
beyond CI runs triggered by pushes).

## B. Original goal and success contract

Verbatim user objective (from goal control record, no secrets present):

> Commit and push and merge that PR. But before doing that verify jackin works
> correctly locally and we can see usage for all coding agents from this mac in
> Jackin Console and also verify jackin can start and coding agent usage displayed
> inside Jackin Capsule and verify Jackin capsule works correctly and coding agents
> are starting correctly (but don't waste tokens in these coding agents, and if you
> test them use the smallest, cheapest model with lowest effort, like haiku low or
> codex luna low).

Consolidated statement (reconstructed summary, not a quote): on branch
`fix/usage-broker-fallback` (PR #1063 — "that PR" resolved: at goal-set no PR
existed; goal workers created #1063, the only PR ever for the branch, 2 min
after goal-set), prove on the user's mac that (1) host/console usage shows
every configured coding agent, (2) a capsule-backed `jackin load` boots, shows
in-capsule usage, and starts coding agents; then commit, push, and merge the PR.
Supersession (still binding): "continue autonomously until finished" is
superseded by the pause for continuation purposes only; all other standing
duties (no-questions, subagents, independent verification, commit discipline)
remain binding, including for handoff work.

Standing amendments (user, this session): commit+push always; use subagents
aggressively; never ask questions, work fully autonomously; commit often on one
branch, push regularly, minimize branches.

Material constraints: [USER] near-zero token spend on coding agents (no live
prompts run so far; cheapest model/lowest effort, e.g. haiku low / codex luna
low, if a test ever needs one); [CLAUDE.md] DCO signoff (`git commit -s`) on
every commit; subagents for implementation/review/verification (aggressive fan-out
per standing directive); repo rules (AGENTS.md): no legacy code/shims, research
project (breaking OK), correctness over ROI, structural fixes preferred, full
pre-merge review protocol. Standing duties preserved: verify important
assumptions/findings independently, re-verify critical decisions, multi-subagent
challenge on significant uncertainty; commit small verified units often on one
branch and push regularly (recoverable/reviewable/bisectable); additional
branches only for a clear technical/workflow reason.

Tagged acceptance criteria ([USER] = verbatim user text; [AGENTS.md]/[CLAUDE.md] =
standing repo rules; [AGENT-DERIVED] = agent operationalization, not user text):
(0) [USER] jackin works correctly locally + jackin starts (host-health: install,
start, doctor-clean modulo pre-existing findings, status/prewarm class checks).
(1) [USER] usage for ALL coding agents from this mac visible in Jackin Console;
[AGENT-DERIVED] operationalized as Fresh host snapshots with quota data for the
8 configured agents discovered via `jackin account list` (codex, claude, amp,
grok, kimi, google, cursor, meta); edge surfaces: opencode present-but-unregistered
→ document only, zai/minimax/openrouter absent → out of scope; [AGENT-DERIVED
FALLBACK] genuinely logged-out providers (proof required, e.g. empty tokens)
may block 8/8 pending human re-login — the unqualified user criterion remains
primary and the fallback must be labeled as such in the verdict.
(2) [USER] capsule-backed `jackin load` boots, daemon healthy, in-capsule usage
visible, coding agents start. (3) [USER + AGENTS.md] PR #1063 merged; [AGENTS.md]
merge protocol honored (all reviews/threads read, dispositions with fix+commit-URL
replies, re-fetch at final head; waiver rule: only explicit PR-specific human
authorization permits ignoring identified feedback — general merge approval is
not a waiver); CI green. (4) [AGENTS.md + CLAUDE.md] relevant tests/clippy/fmt
clean; [CLAUDE.md] DCO signoff on every commit.

Post-resumption obligation (from pause instruction): integrate all required related
goal work, resolve its PRs, and clean up verified-obsolete goal-owned local
worktrees/branches — after explicit resumption and fresh safety checks only. Never
merge unrelated experiments or delete shared resources.

## C. State at the exact interruption point

Last completed action: keychain fail-fast fix finished and verified by worker
(586 jackin-usage tests pass, clippy/fmt clean, live `snapshot --agent claude`
returns in ~7s with honest diagnostic); 6 files left uncommitted. Prior: fmt
(879988ab) + policy re-render (2754bd21) committed and pushed; capsule image rebuilt
with Landlock fix (783e9c448f7e, daemon healthy in probe).

In-progress at pause: nothing (worker had just gone terminal). Exact next intended
action: commit the 6 keychain files + push, then live usage-matrix re-verify and PTY
`jackin load` retry (see §H item 1 — now partially done by this handoff's
preservation commit; first resumption task updated accordingly).

Partially edited files: none beyond the 6 keychain files (complete, verified).
Conflicts/detached/index anomalies: none in goal checkout. Open hypotheses: none
blocking; stale capsule `Error: parsing agent.toml` was environmental (fixed by
image rebuild, proven by hash match + healthy probe).

Workers at pause: keychain-nohang DONE+PASS (terminal); fmt-policy committer DONE;
rebuild2 DONE+PASS; all 4 handoff auditors DONE. No still-running goal workers.
External side effects: pushes to origin/fix/usage-broker-fallback (through 2754bd21);
CI runs on PR #1063 (see §G); docker image rebuilds (local only); one pending macOS
Keychain consent sheet (moot after fail-fast fix is in the used binary).

## D. Requirement-by-requirement progress ledger

| ID | Requirement | Status | Evidence/files/commits | Remaining work | Dependencies |
|----|-------------|--------|------------------------|----------------|--------------|
| R1 | Console/host usage for ALL configured agents | IN_PROGRESS | FAIL at 039ce1b1 (4/8 Fresh: codex/amp/grok/cursor; /tmp/goal-console.md). Root causes: claude+kimi logged out (environmental, /tmp/goal-usagegap.md Q2); google hardcoded Missing (BUG, fixed by b99275f8); meta no-poll mislabeled (fixed by b99275f8); `refreshing` mask (fixed by b99275f8); keychain hang (fixed, uncommitted at pause, /tmp/goal-keychain.md). b99275f8 has NO verification report (gap S7). | Commit keychain fix; rebuild/install HEAD jackin; re-run full usage matrix; claude+kimi need human re-login for full 8/8 | keychain commit |
| R2 | Capsule boots via jackin load, stays up | IN_PROGRESS | FAIL→partial: stale-capsule parse exit-1 (goal-capsule.md); keychain hang pre-container (goal-capsule2.md); image rebuilt with fix (783e9c448f7e, daemon healthy 17s+, /tmp/goal-rebuild2.md PASS); failure-path evidence fix verified live (ver-207 (b) case) | PTY `jackin load` retry with fixed binary + new image | keychain commit, HEAD install |
| R3 | In-capsule usage display + agent boot (zero tokens) | NOT_STARTED | No agent process ever started; bypass probe: `usage accounts → []` exit 0 (plumbing OK, empty cache). No prompts sent anywhere (budget kept) | After R2 boots: daemon status, in-capsule usage, process-alive checks only | R2 |
| R4 | Commit + push (DCO, single branch) | DONE (at pause; future conflict-resolution commits remain) | 8 commits pushed, HEAD==origin==ec3dbcf6 (keychain fix committed+pushed by handoff preservation, §A checkpoint_code_sha) | none now; future commits for §H items 5–6 | none |
| R5 | CI green on PR #1063 | IN_PROGRESS | At 26249aad: 43 pass, 4 fail (policy generated-tree, fmt×2, manifest-fuzz infra 403), 2 Swift pending (/tmp/goal-cifail.md). Fixes pushed (879988ab, 2754bd21). At 2754bd21: DCO+Policy SUCCESS; full workload status UNKNOWN (rollup only 2 entries — re-fetch; CI may still run) | Re-fetch checks; re-run manifest-fuzz infra 403; await green | keychain push retriggers CI |
| R6 | Merge PR #1063 per AGENTS.md protocol | NOT_STARTED | PR OPEN, zero human reviews/threads, 1 bot comment (codex limits). BLOCKER: mergeable=CONFLICTING (remote main advanced to df4671e4; PR base record c52e912b) | Rebase/merge main resolution, re-fetch reviews+checks at final head, approvals, merge | R1–R5, conflict resolution |
| R7 | No regressions (tests/clippy/fmt/DCO) | IN_PROGRESS | Per-fix suites green at commit time (584–586 usage, 344 runtime launch, 36 shell_runner, 16 ffi, 483 jackin lib, clippy clean, fmt clean). Must re-run at final head | Final test pass at merge head | R4 |

Closest to VERIFIED_DONE: failure-path evidence fix (fix-evidence + ver-207 live +
rev-208 SHIP, landed in 039ce1b1) — code-verified, not end-to-end goal-verified.

## E. Change and preservation inventory

Branch `fix/usage-broker-fallback` (main...HEAD at pause): 7 commits, 72 files,
+1706/-437 (per PR ledger; includes version-bump lockstep + policy re-render).

| Commit | Scope | State |
|--------|-------|-------|
| 0268e454 | usage-broker in-process fallback + sidecar shipped with jackin | pushed, verified live |
| d75c0f11 | NotFound cleanup silence + channel-aware capsule errors + 0.6.5-dev + E014 delete | pushed, verified (prewarm exit 0 via preview) |
| 039ce1b1 | capture_combined stderr diagnosis + failed_setup evidence preservation | pushed, verified live + SHIP review |
| b99275f8 | Antigravity discovery+refresh wiring + honest unavailable diagnostics | pushed, IMPLEMENTED_UNVERIFIED (no report — S7) |
| 26249aad | Landlock symlink-target grants (process_isolation.rs) | pushed, verified (21/21 isolation tests incl regression; image hash match) |
| 879988ab | rustfmt 3 files (CI fmt gates) | pushed |
| 2754bd21 | velnor generated-tree re-render (CI policy gate) | pushed; DCO+Policy SUCCESS |
| UNCOMMITTED (6 files, 249+/29-) | keychain fail-fast (skip_authenticated_items + ConsentRequired plumbing + tests) | complete+verified by worker, preserved by handoff commit (see §E.3) |

Keychain fix files: host.rs, host/discovery.rs, host/discovery/tests.rs, usage.rs,
usage/claude.rs, usage/tests.rs (all under crates/jackin-usage). No manifest/lock
changes. Worker evidence: 586 pass, clippy/fmt clean, live 7.2s honest diagnostic.

Machine-local, non-git artifacts (see §E.2/I): docker image 783e9c448f7e (Landlock
fix, verified hash c4846382); preserved containers jk-1p0zwcvr-agentsmith and
jk-tk0mt07z-agentsmith (evidence, keep until merge); vk5h8m6s record+network
(leftover, cleanup candidate);
~20 /tmp/*.md evidence reports (local-only, paths in §G/K); pending Keychain consent
sheet (moot post-fix).

### E.1. Discovery scope and ownership

Inspected (2026-09-21T22:0xZ): `git worktree list --porcelain`, `git status`,
`git stash list`, `git branch -vv`, remote-tracking refs, `ls-remote` for
origin/main + goal branch, `gh pr view/list` (+ REST for PR #1063 comments,
checks, mergeability), and direct-sibling checkouts under
/Users/donbeave/Projects/jackin-project/ (names/branch/HEAD/dirty only). No broad
filesystem scan; no fetch performed (read-only); no prune/GC/cleanup; no stash
pops. Coverage uncertainty: other clones outside the surveyed roots would be
missed (GAP-6); sibling worktree interiors were status-level only.

Ownership: W1/B1/P1063 are GOAL_EXCLUSIVE. All sibling checkouts/worktrees are
UNRELATED (other agents' 20260920 workstreams, different branches/PRs) except
where overlapping files create merge-interest, noted as UNKNOWN-merge only. Zero
stashes exist anywhere surveyed. Detached worktree (doc-schema) is clean.

### E.2. Local worktree and clone ledger

| ID | Path | Type / owner | Branch @ HEAD | State | Disposition |
|----|------|--------------|---------------|-------|-------------|
| W1 | /Users/donbeave/Projects/jackin-project/jackin-main (.git = common dir) | main checkout / this session | fix/usage-broker-fallback @ ec3dbcf6, CLEAN (keychain fix committed+pushed; draft dir shipped via B2/P-HO, never onto B1) | 0 modified, 0 staged, 0 untracked, 0 stashes | KEEP (primary checkout) |
| W2 | handoff worktree: NOT CREATED (doc committed via transient `git checkout -b` + switch-back on clean tree; no extra worktree exists) | n/a | goal-handoff/verify-merge-pr1063-cb81b336 (branch only) | n/a | no action |
| C1..C7 | jackin, jackin-agent-smith, jackin-marketplace, jackin-github-terraform, jackin-schema-fix-20260920, velnor-971-fix, homebrew-tap (+homebrew-tap-provenance-hardening, clean) | standalone clones / other agents | various (see /tmp/ho-worktree.md) | all clean | NOT_APPLICABLE (unrelated) |
| WU-clean | ~25 linked worktrees of jackin/.git (credential, schema, launch, docs workstreams) | linked / other agents | various codex/* branches | all clean | NOT_APPLICABLE (unrelated) |
| WU-dirty | 7 dirty linked worktrees (apple-usage-relay 8 files, auth-source 6, console-identity 4, launch-security 4, restore-identity 3, usage-credential-routing 17, usage-presentation 2) | linked / other agents | various codex/*-20260920 | 44 files uncommitted total | REVIEW_SHARED (do not touch; merge-interest: usage-credential-routing overlaps usage/* files textually) |
| WU-det | jackin-doc-runtime-schema-repair-20260920 | linked, DETACHED @ d3e5f38b | n/a | clean | NOT_APPLICABLE (unrelated; orphan-prone if committed to) |

| WT-tmp | 77 worktrees of jackin/.git under /private/tmp/* (other agents' scratch: mbx-test, schema2-*, velnor-probe, etc.; several detached) | linked / other agents | various | 7 dirty (1+1+11+13+1+1+9=37 files), 0 missing | NOT_APPLICABLE (unrelated; surveyed-and-excluded; do not touch) |

Missing/inaccessible worktrees: none among goal-related; /tmp trees surveyed at
registry level only. Lock/prunable flags: none observed on goal resources.
Multiple /tmp trees are detached (in addition to the clean detached doc-schema
sibling); none is goal-owned.

### E.3. Local and remote branch ledger

| ID | Ref | Tip | Upstream / remote head | vs target | Disposition |
|----|-----|-----|------------------------|-----------|-------------|
| B1 | fix/usage-broker-fallback | ec3dbcf6 (local==origin, verified via ls-remote) | origin/fix/usage-broker-fallback == ec3dbcf6 | ahead 8 of local main 5639cdc3; PR base c52e912b; remote main df4671e4 (advanced) | KEEP; integrate to main via P1063 after conflict resolution |
| B2 | goal-handoff/verify-merge-pr1063-cb81b336 | c466c9bd (local==origin==P-HO head; = B1 + 2 doc commits, doc-only +378 delta, remote blob verified identical) | origin/goal-handoff/verify-merge-pr1063-cb81b336 == c466c9bd | n/a (record branch) | KEEP as record; retain-as-record (never merge to main) |
| B-main | main (local 5639cdc3, STALE) / origin/main remote df4671e4 | df4671e4 remote | n/a | target | KEEP; fetch before any rebase/merge work |

No other local branches. Remote-tracking refs for ~25 unrelated origin/* branches
exist (fetch-stale); unrelated. No stashes, no detached tips, no reflog-only goal
commits. PR #1063 base record (c52e912b) differs from both local main and remote
main — resuming agent must fetch and reconcile before conflict resolution.

### E.4. Related PR ledger

| ID | PR | State | Head → base | Checks / reviews | Future action |
|----|----|-------|-------------|------------------|---------------|
| P1063 | #1063 fix: usage-broker fallback… (https://github.com/jackin-project/jackin/pull/1063) | OPEN non-draft | fix/usage-broker-fallback @ ec3dbcf6 → main @ c52e912b | DCO+Policy SUCCESS at 2754bd21; full workload UNKNOWN at ec3dbcf6 (re-fetch); 0 human reviews; 1 bot comment; mergeable=CONFLICTING | resolve conflicts, green CI, AGENTS.md protocol, merge |
| P-HO | #1069 GOAL handoff (https://github.com/jackin-project/jackin/pull/1069) | OPEN DRAFT | B2 @ c466c9bd → fix/usage-broker-fallback; MERGEABLE; doc retrievable at head ref (blob verified) | n/a (record PR; informational CI only) | retain as record; never merge to main |

Other open PRs (#1070/#1068/#1067/#1066/#1065/#1064/#1060/#1058/#1045/#1044/#1030/
#1007/#1004): all UNRELATED (#1070/#1068 are other-goal doc handoffs to main;
rest are CI/probes/docs workstreams). Recently merged #1061/#1053/#1052 and
closed #1062 are unrelated. No stacked dependencies on P1063; no predecessor/
superseding goal PRs. No PR touches goal branches besides P1063/P-HO.

### E.5. Integration map and ordered landing plan — FUTURE EXECUTION ONLY

Map: `W1(ec3dbcf6,clean) -> B1==origin/B1 -> P1063(open,conflicting) ->
main(df4671e4)`; `B2==origin/B2 -> P-HO(#1069,draft,mergeable,doc-only)` (no W2;
doc committed via transient switch). No worker branches exist (all subagents
shared W1);
no cherry-picks needed. WU-dirty worktrees are other-goal work: exclude from
integration; note textual overlap (usage-credential-routing touches usage/*
like B1) as merge-watch, not merge-scope.

Ordered plan (post-resumption): (1) fetch; reconcile base (c52e912b vs df4671e4
vs local 5639cdc3); (2) resolve P1063 conflicts (method per repo policy; NO
history rewrite of shared refs — prefer merge commit unless repo practice says
rebase; conflicts unexamined — hypothesis: version-bump lockstep + usage files
vs main advances); (3) full validation at resolved head (usage matrix, capsule
boot, test/clippy/fmt suites); (4) CI green + AGENTS.md review protocol; (5)
merge P1063; (6) verify main state post-merge. P-HO stays open-as-record.
Resuming agent must re-read all P1063 threads and compare live heads with
recorded SHAs before landing anything.

### E.6. Post-integration local cleanup runbook — FUTURE EXECUTION ONLY

| Resource | Path/ref | Owner | Recovery if deleted | Proposed action |
|----------|----------|-------|---------------------|-----------------|
| vk5h8m6s record + jk-vk5h8m6s-agentsmith-net | ~/.jackin/data + docker network | goal session | unrecoverable (regenerable by re-launch) | `jackin purge vk5h8m6s` after resume; re-observe first |
| Stale instance records jk-2c990jk8, jk-c1k0ykf7, jk-ppc51hsc (+locks) | ~/.jackin/data | goal session (failed launch attempts) | unrecoverable (regenerable) | `jackin purge <id>` each after resume |
| Hung snapshot probes (I1) | PIDs: none recorded — re-observe via `ps aux \| grep 'usage host snapshot'` | goal session (likely exited; committer found none) | n/a (re-runnable) | kill only if still present and command line matches |
| ver-207 evidence: jk-1p0zwcvr-agentsmith + jk-tk0mt07z-agentsmith + /tmp/jk-ver-207-fv13jB | docker + /tmp | goal session | unrecoverable (docker rm) — keep until P1063 merged | remove only after merge |
| True orphan jk-5ha8pkbh-agentsmith (exited 0, older image) | docker | goal session (observed, pre-existing) | unrecoverable | `jackin purge 5ha8pkbh` after resume |
| W2 handoff worktree | NOT CREATED (doc committed via transient branch switch; tree restored — see §E.3) | n/a | n/a | no action; no worktree to remove |
| B2 handoff branch (local) | goal-handoff/verify-merge-pr1063-cb81b336 | goal session | recoverable via origin/B2 + P-HO | keep as record; `git branch -d` only when record no longer needed |
| W1, B1, P1063 | primary | goal session | n/a | KEEP (no deletion) |
| WU-*/C*/WT-tmp siblings | other agents | other/unknown | n/a | NEVER touch |

Gates (ALL required before any deletion): (1) required changes verified in
target or explicitly superseded with recovery retained; (2) no unpreserved
work/ownership ambiguity on the candidate; (3) no active dependent
(agent/process/PR/recovery path); (4) post-integration validation passed and
HANDOFF + recovery refs reachable independent of the candidate; (5) exact
host/path/ref/tip re-verified live — old snapshots never authorize deletion.
No remote deletions. No bulk/prefix/decoupled-prune deletions. Note:
jk-tk0mt07z-agentsmith network already absent (container remains) — purge only
what exists at re-observation. /tmp note: ~1531 files machine-wide; the handoff's
"~20 reports" is the §G-named goal subset — resuming agent must select by exact
name, not count.

## F. Decisions, findings, assumptions, rejected approaches

Decisions (shipped on B1): in-process broker fallback + sidecar install (missing
binary was root cause); 0.6.5-dev lockstep (bare 0.6.4 misrouted to unpublished
v-tag channel; rejected +sha-as-preview); E014 DELETE not ATTACH (no
construction site; would hide new guidance); NotFound-silent cleanup;
capture_combined docker-logs diagnosis; run_preserving_evidence on both
failed_setup paths (grant path keeps full cleanup); sessions-present NO-FIX
(fail-closed invariant); Antigravity wiring + honest diagnostics; Landlock
symlink grants; keychain skip-authenticated-items fail-fast (UIFail infeasible:
security-framework 3.7 lacks the control, unsafe forbidden — equivalent contract
via documented Skip semantics + presence probe).

Findings: claude+kimi logged out on this mac (empty tokens); meta no-poll is
by-design; stale image capsule caused parse exit-1 (fixed by rebuild, hash
proven); keychain consent hang blocks all discovery headless (fixed in code);
PR base/main drifted (conflicts). Rejected: silent stable→preview fallback
(breaks pinning); auto-build capsule in load (toolchain/minutes, no consent);
fallback inside ensure_usage_broker_process (lacks discovery params);
GUI-click dependency (replaced by code fix, per autonomous directive).
Residual risks / known follow-ups (not goal-blockers unless noted): (a) keychain
fix covers usage-discovery reads only — `jackin-instance read_claude_keychain`
shells `security -w` and CAN still prompt (provisioning-time interactive scope;
needs timeout wrapper or SecItem migration if ever run headless); `keychain_service_exists`
is metadata-only (cannot prompt). (b) Publish-side contract unfulfilled
(OUT OF GOAL SCOPE, belongs to release flow): versioned-tag capsule assets are
still unpublished, so every stable install 404s until release.yml uploads them;
the goal fixed only the dev-channel (-dev lockstep) half. (c) ver-207 race:
detached pre-attach check can race a ~60ms crash (false `active`) — detached
callers must re-verify via status/hardline; apply this rule in §H item 3.
Assumption needing validation: b99275f8 correctness (no report — S7);
re-verify live at resume (folded into §H item 2).

## G. Verification evidence and known failures

PASS (durable): broker fallback live (`jackin usage` exit 0, Fresh/Authoritative);
prewarm exit 0 via preview (empty cache); capture_combined live ((b) case,
stderr surfaced, evidence preserved); 344 runtime + 36 shell_runner + 584–586
usage + 16 ffi + 483 jackin-lib tests at commit times; clippy clean; fmt clean
at 879988ab; DCO+Policy SUCCESS at 2754bd21; image 783e9c448f7e healthy probe;
keychain: 586 pass + clippy/fmt clean + live 7.2s honest diagnostic
(worker-reported, plausible — commit contains claimed test files — but NOT
independently re-run; and the consent branch was never exercised live against
a truly prompt-gated item — unit tests + Apple's documented Skip contract
only, by design to avoid touching real credentials).

FAIL (record): console 4/8 (pre-fix); capsule launch ×2 (pre-fix); EACCES-126
on stale image (pre-rebuild); CI policy+fmt×2 at 26249aad (fixed, re-check
pending); manifest-fuzz infra 403 (needs re-run). INTERRUPTED: PTY load
(SIGTERM, no container); 3 snapshot probes hung (likely exited since).
STALE: all /tmp live reports predate current HEAD (see ho-verify §2).
NOT RUN: post-fix usage matrix; PTY load on new image; final-head suites;
post-keychain-commit CI.

Evidence locations: /tmp/*.md (LOCAL-ONLY, this machine —
broker-chain/deep/repro/history, iss-cleanup/capsule/sessions, fix-ab/cd/evidence,
att-code/repro, rev-106/208, ver-107/207, goal-recon/pr/console/capsule/capsule2/
usagegap/keychain/rebuild2/ci/cifail, ho-goal/worktree/branches/verify/review;
goal-usagefix, rev-206 and fix-logs were never written);
session log /Users/donbeave/.local/share/muse/sessions/2026/09/21/01a0c4b8-8b97-7570-b7b7-5c1a55026a05/session.jsonl
(LOCAL-ONLY); CI run/job URLs in §E.4 + /tmp/goal-cifail.md (remote).

## H. Ordered remaining-work plan

1. FIRST TASK (DONE by handoff preservation — verify, then start at item 2):
   keychain fix committed+pushed as ec3dbcf6 (proof: §A checkpoint_code_sha;
   re-observe `git status` clean + `git rev-parse HEAD`). If the tree ever
   diverges from §A, reconcile before any other step. Validation already
   claimed at commit time (usage tests + clippy + fmt); re-run at final head
   per item 7.
2. Rebuild/install HEAD jackin; re-run full usage matrix (host-wide + 8
   snapshots); expect google data, honest claude/kimi needs-login, honest meta
   unsupported. Record report (also closes S7 for b99275f8). Parallel-safe with 3.
3. PTY `jackin load` retry (fixed binary + image 783e9c448f7e): boot, daemon
   status, in-capsule usage, agent process-alive (NO prompts). Record report.
4. Human GUI (only user step): `claude /login` + Kimi re-login; re-snapshot
   claude/kimi; dismiss stale consent sheet if present.
5. CI: fetch P1063 at final head, re-run manifest-fuzz infra job, await full
   green (incl. Swift).
6. Conflicts: fetch main, resolve P1063 CONFLICTING per repo policy, re-validate.
7. AGENTS.md merge protocol + merge; post-merge main verification.
8. Cleanup per §E.6 with live re-observation.

Validation per item: completion = recorded PASS evidence at the final head, not
intent. Inlined commands (no external dependency):

Usage matrix (item 2), each must exit 0:
`./target/debug/jackin usage`
`./target/debug/jackin usage host snapshot --agent <codex|claude|amp|grok|kimi|google|cursor|meta>`
Expect post-fix: google real data; claude/kimi honest needs-login (not bare
`refreshing`); meta honest unsupported; codex/amp/grok/cursor Fresh.

PTY load (item 3): 80x24 PTY, `TERM=xterm-256color`, CI unset, run
`./target/debug/jackin load agent-smith <scratch-workspace>`; poll `docker ps`
for `jk-*-agentsmith` Up; then `docker exec <c> jackin-capsule status` must show
`Sessions: 0`; in-capsule usage via `jackin usage <instance> ...` (instance
scope; see CLI help — bare `jackin usage <sub>` without instance errors
by design). Bound every jackin invocation (keychain-era hangs are fixed but
belt-and-braces: `gtimeout`/alarm 120s), SIGINT the client after attach; the
container persists (no `--rm`) for inspection.

## I. Environment and operational recovery

Platform: macOS arm64 (user's mac), rust 1.97.1, docker aarch64, repo at
/Users/donbeave/Projects/jackin-project/jackin-main. Key commands: `cargo build
-p jackin`, `cargo install --path crates/jackin`, `cargo test -p <crate>`,
per-agent `jackin usage host snapshot --agent <name>`, `jackin load` (rich TTY
only — use 80x24 PTY harness), `jackin prewarm --image --role agent-smith`
(non-launch image rebuild with JACKIN_CAPSULE_BIN). Config: ~/.jackin (real,
never wiped), ~/.config/jackin; overrides JACKIN_HOME_DIR/JACKIN_CONFIG_DIR for
isolated probes. Reproducible: target/ builds, docker images (rebuildable),
/tmp probes. Irreplaceable: keychain items + OAuth tokens (never touch),
~/.jackin user state. External ops performed: git pushes + CI runs only; safe
to re-push; do not re-run `gh run rerun` blindly — check current state first.
Pending SecurityAgent consent sheet: moot post-fix; dismiss in GUI if present.

## J. Fresh-agent resume runbook

1. Retrieve this file FIRST — it lives on B2 (`goal-handoff/…`), NOT on the
   goal branch: `git fetch origin goal-handoff/verify-merge-pr1063-cb81b336`
   then `git show origin/goal-handoff/verify-merge-pr1063-cb81b336:docs/goal-handoffs/verify-jackin-merge-pr1063--20260921T220648Z--muse-code--cb81b336.md`
   (or check out B2 / open PR #1069 in a browser). Then checkout
   `fix/usage-broker-fallback`, `git fetch origin`, and read this file fully
   + AGENTS.md + §G evidence.
2. Re-observe: `git status`, `git log --oneline -5`, `gh pr view 1063`,
   `gh pr checks 1063`, `docker images | grep jackin`, `docker ps -a | grep jk-`.
3. Do NOT trust stale PIDs, /tmp probe outputs, or pre-pause CI states.
4. Resume at §H item 1 (or 2 if keychain fix already on B1). Use subagents
   with file ownership; keep DCO + zero-token rules.
5. After validation: §E.5 integration, then §E.6 cleanup with live gates.

Resume command:

`/goal Read and resume docs/goal-handoffs/verify-jackin-merge-pr1063--20260921T220648Z--muse-code--cb81b336.md`

Interpretation rule: the pause holds until the user requests resumption. That
resume command authorizes continuing the ORIGINAL goal; it does NOT instruct a
re-pause, handoff regeneration, or recursive handoff PR.

## K. Blockers, omissions, and independent review

Blockers: (1) P1063 mergeable=CONFLICTING — needs post-resume resolution.
(2) b99275f8 unverified (S7). (3) claude/kimi need human re-login for 8/8.
(4) Full CI workload status at 2754bd21 UNKNOWN (rollup showed 2 checks).
Omissions: /tmp evidence reports are local-only (paths in §G); full keychain
diff lives in the preservation commit, not quoted here; sibling worktree
interiors surveyed at status level only. No unavailable tools. All goal
workers terminal; handoff auditors terminal after review step.

Independent review: DONE (/tmp/ho-review.md, verdict HO-FIX, 8 numbered issues,
all live-checked 2026-09-21T22:1xZ). Corrections applied: (1) preservation
executed (§A SHAs + P-HO URL filled); (2) untracked docs/ dir recorded;
(3) 77 /tmp worktrees mapped as excluded; (4) §E.6 rebuilt (orphan named,
ownership+recovery, inline gates, exact purge cmds); (5) stale instance
records mapped; (6) PTY + matrix commands inlined; (7) phantom paths dropped;
(8) full container names. Reviewer confirmed: recoverable from doc alone
(modulo fixes), all SHAs/paths/PR-state match live, no unmapped goal-owned
dirty work, no missing goal PR. Fresh-agent recovery: YES after corrections.

## L. Audit record — verify-and-repair (2026-09-21T22:3xZ)

Audit task: verify this HANDOFF against authoritative sources, repair gaps,
republish. Delegation actually performed: 4 parallel read-only subagents
(au-intent /tmp/au-intent.md, au-state /tmp/au-state.md, au-integ
/tmp/au-integ.md, au-fresh /tmp/au-fresh.md) + 1 earlier independent review
(/tmp/ho-review.md). No fabrication: all reports on disk, local-only.

Source coverage: ALL FULL — session log goal_control + user messages (S1–S9
with line numbers), AGENTS.md, CLAUDE.md, live `gh`/`git`/docker state, /tmp
reports as secondary. No SOURCE_UNAVAILABLE items. Original-goal quote in §B
verified character-exact vs session log (programmatic diff).

Gaps found and repaired: G1–G9 (intent labeling: criterion tags, unqualified
primary + labeled fallback, PR-resolution provenance, host-health criterion
(0), edge-surface rule, near-zero token wording, restored A2/A3 sub-clauses,
supersession line, waiver rule); §A runtime-pause update; R4→DONE; stale
B2/P-HO/W1/P1063 rows refreshed to ec3dbcf6/c466c9bd/#1069; #1068/#1070 added
as unrelated; E.6 /tmp-count note; H3 residual (jackin-instance keychain),
H4 consent-coverage qualification, H5 publish-side out-of-scope, H6 ver-207
race rule; H2 cross-ref fix; §J B2-first retrieval; this §L (matrix + tasks).

Unresolved: none blocking. Known engineering unknowns carried, not hidden:
b99275f8 unverified (S7 → T-01 closes it); full CI at ec3dbcf6 UNKNOWN;
consent branch never live-gated (H4); hung-probe PIDs never recorded.

### L.1 Requirements matrix

Format: ID | Source | Operative requirement | HANDOFF section | State/evidence | Remaining task + check | Coverage.

G-01 | G0 verbatim | Verify jackin works locally on this mac | §B(0), §D R1–R3 | IN_PROGRESS (host matrix 4/8 pre-fix; capsule unbooted) | T-01/T-02 + host-health re-check | COVERED
G-02 | G0 verbatim | Console usage for ALL coding agents | §B(1), §D R1 | IN_PROGRESS (§G FAIL pre-fix; fixes landed) | T-01 (8 snapshots exit 0; fallback labeled if logged-out) | COVERED
G-03 | G0 verbatim | jackin starts; capsule boots; in-capsule usage; agents start | §B(2), §D R2/R3 | R2 IN_PROGRESS, R3 NOT_STARTED | T-02 (Up + Sessions: 0 + usage + alive) | COVERED
G-04 | G0 verbatim | Token rule: near-zero; cheap-model-only if ever needed | §B constraints | kept (zero prompts run) | binding on T-01..T-03 (T-03 needs none) | COVERED
G-05 | G0 verbatim | Commit + push + merge PR #1063 | §B(3), §D R4–R6 | R4 DONE; R5 IN_PROGRESS; R6 NOT_STARTED (CONFLICTING) | T-04/T-05/T-06 | COVERED
G-06 | A1+A3+AGENTS.md | Commit often (small verified units), push regularly, one branch, DCO | §B constraints | kept (8 commits, all -s, all pushed, one branch) | binding on T-01..T-06 | COVERED
G-07 | A2 (+A4 re-affirm) | Autonomous, no questions; subagents; independent verification | §B constraints | kept | binding on resume | COVERED
G-08 | AGENTS.md | No legacy/shims; structural fixes; correctness over ROI | §B, §F | kept (E014 delete, fallback-in-choke-point, etc.) | binding on T-05/T-06 | COVERED
G-09 | AGENTS.md | Full pre-merge protocol incl. PR-specific waiver rule | §B(3), §E.5 | NOT_STARTED (0 threads to date) | T-06 (re-fetch + dispose + merge) | COVERED
G-10 | AGENTS.md+CLAUDE.md | Tests/clippy/fmt clean at head | §B(4), §D R7 | IN_PROGRESS (green per-fix, stale at head) | T-07 (final suites at merge head) | COVERED
G-11 | A4 pause order | Freeze/preserve/document/draft-PR; no merge/cleanup during pause | §A/§C/§E | DONE (this doc + #1069; nothing merged/deleted) | none | COVERED
H-01 | A4 pause order | Inventory all related worktrees/clones/branches/PRs + ordered integrate/clean plan | §E.1–§E.6 | DONE (audited 2×; see §L.3) | none | COVERED
H-02 | A4 pause order | Self-contained resume runbook + exact resume command | §J | DONE (B2-first retrieval repaired) | none | COVERED
H-03 | audit order | Requirements matrix + T-tasks + audit record in HANDOFF; republish + verify | §L, P-HO | DONE (this section; SHAs below) | none | COVERED

No MISSING/CONTRADICTORY/UNSUPPORTED items. Non-goals (explicit): publish-side
release flow (belongs to release process, §F(b)); unrelated sibling workstreams
(never touch); merging P-HO to main (record only).

### L.2 Executable task plan (post-resumption)

T-01 Usage matrix + host health (G-01/G-02/G-10): starting state ec3dbcf6 clean,
fixes landed, matrix never re-run. Action: rebuild/install HEAD jackin
(`cargo install --path crates/jackin`), run §H inlined matrix + `jackin doctor`
+ `jackin status`; expect google data, honest claude/kimi needs-login, honest
meta unsupported; record report (closes S7). Pitfalls: do not trust pre-fix
/tmp outputs; bound invocations 120s. Done when: all commands exit 0 with
recorded outputs; verdict labeled per §B(1) fallback rule. Parallel-safe
with T-02 (different resources; share nothing but the repo).
T-02 Capsule boot + in-capsule usage + agent alive (G-03/G-04/G-10): starting
state image 783e9c448f7e healthy in probe, PTY load never run on it. Action:
§H inlined PTY load; expect Up + `Sessions: 0` + usage + ≥1 agent process
alive; apply ver-207 re-verify rule (status check after attach); NO prompts.
Pitfalls: rich-TTY gate (PTY required); stale `vk5h8m6s` leftovers (purge
first if colliding; re-observe). Done when: recorded PASS with commands.
T-03 Human logins (G-02): only user step. `claude /login` + Kimi re-login in
GUI; re-snapshot claude/kimi; dismiss stale consent sheet if present. Done
when: snapshots Fresh or still honestly diagnosed.
T-04 CI green (G-05/G-10): re-fetch `gh pr checks 1063` at final head; re-run
manifest-fuzz infra 403 if still failing (`gh run rerun <run> --failed`);
await full green incl. Swift. Depends on T-01/T-02 fixes (if any new commits).
Done when: all required checks SUCCESS at merge head SHA.
T-05 Conflict resolution (G-05/G-08): fetch; reconcile base c52e912b vs remote
main; resolve CONFLICTING per repo policy (no shared-history rewrite; conflicts
unexamined — inspect on the day); re-run T-01/T-02/T-07 subset. Depends on
T-01/T-02. Done when: mergeable + validation re-passes.
T-06 Merge protocol + merge (G-05/G-09): re-read ALL P1063 threads at final
head; dispose each (fix+verify+commit+push+reply-with-URL, or evidence reply);
confirm approvals; merge; verify main post-merge. Depends on T-04/T-05. Done
when: merged + main verified.
T-07 Final suites (G-10): full relevant test/clippy/fmt at merge head. Depends
on T-05. Done when: recorded green.
T-08 Cleanup (H-01): §E.6 rows with live re-observation + 5 gates. Depends on
T-06. Done when: receipt written, every ID dispositioned.
T-09 Residual investigation (H3, non-blocking): `jackin-instance
read_claude_keychain` headless-prompt risk — bounded probe: does any headless
path reach it? If yes, file follow-up (timeout/SecItem); do not expand P1063
scope without user direction. Done when: answered + recorded.

FIRST action after checkpoint retrieval + state reconciliation: T-01 (usage
matrix), per §H/§J (item 1 done via ec3dbcf6).

### L.3 Audit outcome

`VERIFIED`: all authoritative sources accessible (FULL); all operative G/H
requirements faithfully and actionably documented above; preservation (B1
ec3dbcf6, B2/P-HO) and resume details verified live by 5 independent
delegated checks (4 handoff auditors + this audit's 4 subagents + 2 reviewers);
publication re-verified below. No unresolved handoff-quality gaps. (Engineering
verification of b99275f8 remains open by design — tracked as S7/T-01, not a
handoff defect.)

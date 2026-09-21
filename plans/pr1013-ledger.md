# PR #1013 requirement-to-evidence ledger

Scope: multi-account settings, discovery, usage, authorization, launch, containers on
`integrate/pr1002-multi-account` only. Source of truth for merge readiness.
Supersedes `origin/feat/multi-account-support:plans/multi-account/ledger.md` (all rows
UNVERIFIED/BLOCKED there; historical counts are leads, not proof).

## Candidate

- PR: #1013 OPEN MERGEABLE, head `integrate/pr1002-multi-account` @ `d80d99bd`,
  base `main` @ `fd18e95c` (= origin/main #1018, contained, in sync). Main
  #1018 (Velnor pin fff18da8, phased rust incl. doctests) merged d80d99bd;
  CI conflicts resolved by pinned-generator regen (dry-run 0 changes, plan
  parses; branch deltas — console/usage dep, telemetry/test-support dep,
  config fixture watch — re-derived).
- Diff vs main: 469 files, +70081/-5138, 188 non-merge commits, all carrying
  Signed-off-by (DCO satisfied). Commits before the signoff policy carry the
  legacy `jackin-consolidation` identity; all recent commits carry only
  `Signed-off-by: Alexey Zhokhov <alexey@zhokhov.com>`. Shared history is not
  rewritten (squash merge carries only the Alexey signoff). Replaces #1002
  (still OPEN, failing, out of scope).
- CI @ head d80d99bd: matrix running (phased units incl. new doctest phase;
  local doctest sweep 27 suites green). Prior heads: 45ab71b7 matrix green
  (PR+Velnor+Renovate); ef54d987 failed 4 Rust units on branch-owned test
  lints, fixed 8ec3640e/91e185cf/eed49ebf and verified locally.
- Reviews: 1 bot review (Codex, no suggestions on fa76ee1); 1 P1 inline
  (Landlock mounts) ADDRESSED by ef0c6d31 with owner reply; independent
  subagent review of HOME fix 880d0117 (approve-with-nits; per-kind comment
  correction landed f1d229a8 + spawn-layer pins). Zero human approvals.
- Host: darwin arm64 macOS 27.0 (acceptance contract wants macOS 26 + OrbStack).

## Status rules

PASS = proved on exact candidate SHA with command, env, artifact, owner.
UNVERIFIED = no qualifying evidence. BLOCKED = defect prevents acceptance.
Missing live credentials stay explicit gaps, never passes.

## Confirmed defects (inspected current code, 2026-09-21)

| ID | Defect | Evidence | Status |
|---|---|---|---|
| D-UI1 | Console `from_projection` dropped `metric_groups`, plan, issues, cred-expiry, quota_state, raw % | Fixed 2667023b: UsageMetricGroup mirrors V1; console 1335 + clippy clean (author + independent) | LANDED |
| D-UI2 | Capsule one tab per provider; same-provider accounts collapsed | Fixed a414f596: id-keyed per-account tabs, disambiguated labels, id focus/refresh; protocol 116 + usage 482 + capsule 867 green | LANDED |
| D-SEC1 | P1 Landlock: only cwd granted; workspace/worktree mounts denied | ADOPTED parallel ef0c6d31 (exact-stamped workspace_mounts + worktree_git_targets, validated, additive); merged 258e173a; 869/869 green | LANDED |
| D-SEC2 | Publication had RAM-only rollback; crash-between-renames stranded skew | Fixed 1ddefbf6: fsync'd TOML journal + forward-roll on write-locked open + TransientConflict on read path; config 463 green | LANDED |
| D-SEC3 | Teardown `remove_dir_all` races | CLEARED by inspection + empirical probe (links never followed, container dead before unlink, no attacker paths); symlink pin test 3b895698 | CLEARED |
| D-BR1 | Stale-catalog race: stale discovery paired with fresh lease read ("no cancel"/"join ownership" cleared as intended design) | Fixed ee757118: activation flock + post-lease re-discovery + bounded CAS retry + empty-scan confirm; broker 41 green | LANDED |
| D-PR1 | Cursor/Gemini/OpenRouter collectors were unreachable from production dispatch | Fixed cfba4bf9: Cursor OAuth material + refresh, Gemini presence material + NeedsSecret re-proof, OpenRouter env-key quota; forwarded gate admits all known surfaces; usage 504 green. Explicitly blocked with in-code reasons: Antigravity (Keychain grant), Muse (no pollable fetch by design), omp/Hermes (attribution-only), Copilot (no collector), Cursor Enterprise (no discovery credential path) | LANDED |
| D-UI3 | Console Enter flipped title only; no filter/sort/capacity-finder; wording deltas | Fixed e8b98f42: summary/full toggle, s/f/c keys + hints, Capsule label alignment; console usage 34 green | LANDED |
| D-SEC4 | Supervisor gate PID-equality impersonation | Fixed 4b324478: (pid, start_time) binding from /proc+SO_PEERCRED, fail-closed; relay 12 green | LANDED |
| S10-auth | Local auth files PRESENT (presence only): Claude (~/.claude.json), Codex, Cursor, Grok. Absent: Kimi, Amp, OpenCode, Gemini paths | host inventory 2026-09-21 | Presence != valid auth; live checks must validate per account |
| D-PR2 | Gemini adapter has no live fetch; omp/hermes attribution-only; token_monitor lacks Grok/Antigravity/Gemini/Cursor/Muse/omp/Hermes | WS2 evidence | UNVERIFIED, matrix work |
| F11 | pty e2e harness dead on macOS: BSD script(1) block-buffers pipe stdout (0 live bytes, full flush at exit), so scripted-input waits never match and every pty e2e stalls to timeout. Dialog proven healthy via expect-driven run (modal advanced to credential prompt); stuck process sampled in select_loop/recv | Fixed 01cdd150: macOS typescript pointed at live file + follower thread (pipe still drained, final sync at exit); Linux path unchanged | LANDED |
| F12 | Sentinel dind e2e: usage-relay socket path 112B exceeded 104B sun_path limit | Fixed: socket-alias redirect + stale capsule/broker rebuild; relay green, exposed F13 | LANDED |
| F13 | Sentinel dind e2e: account-env strip removed ambient HOME and nothing re-set it; entrypoint died on unbound HOME (inherited from #1002, which never set session HOME) | Fixed 880d0117: agent HOME = instance home_dir (folder-var target; private writable slot for Dir/folder-less kinds, traverse-only /home/agent for primary Parent, shared XDG root for XdgRoot), shell restores daemon container HOME, hostile passthrough still rejected; unit pins incl. Parent/XdgRoot carry-through; sentinel e2e PASS 30.8s. Independent review approve-with-nits; comment correction f1d229a8. Follow-ups (out of consolidation scope, nil isolation impact): durable per-kind writable HOME, deny path-valued keys in credential validation | LANDED, e2e acceptance green |

## Product acceptance (P01-P10)

All UNVERIFIED on final candidate. Evidence owners in parentheses.

| ID | Behavior | Status / evidence |
|---|---|---|
| P01 | First-run discovery imports logins; one damaged source blocks nothing | S1 PASS @62c2eebf (/tmp/jackin-s1-acc1/artifacts/); wart F5: fresh scan under-reports count, fix dispatched |
| P02 | Settings Scan draft/apply/cancel, preserves edits, explicit no-add/partial | S2 PASS @62c2eebf, genuine pty (29/29 checks, /tmp/jackin-st-s2s6/evidence/) |
| P03 | Add subscription/profile/key/env/1Password; inference vs usage permission split; redacted | S3 PASS @c5aa9eac re-run (5/5: op-absent persists, op-unsigned honest error, key refresh, specific gaps, DENY zero-network; /tmp/jackin-re3/artifacts/) |
| P04 | Usage async refresh + detail incl metric groups, balances, resets, honest unavailable | S4 host PASS + instance PASS @c5aa9eac (live container status/accounts/verify/cache/sync; /tmp/jackin-rec/evidence/) |
| P05 | Precedence: launch > workspace-role > workspace > global > sole-eligible; atomic reject | S5 PASS @c5aa9eac re-run, 17/17 incl fixed case V (lists win; /tmp/jackin-re57/s5/) |
| P06 | Container admits exactly A/B/C; D absent incl direct relay requests | S6-E2E PASS @c5aa9eac (real containers: D 0× in agent.toml/creds/picker/snapshot/registry; relay probe A/B/C ok, D unauthorized; /tmp/jackin-rec/evidence/) |
| P07 | Same identity across clients, no duplicate quota; exact OpenRouter model persists | S7 PASS @c5aa9eac re-run (1 cap/1 row/prov 2; model byte-exact top + per-instance; /tmp/jackin-re57/s7-ident, s7-model/) |
| P08 | Tabs/splits/reconnect/restore preserve account/provider/model/instance binding | S8 PASS @c5aa9eac (tabs/splits/reconnect via capsule TUI; restore: daemon restart drops live tabs EXPECTED-by-design — tabs in-memory only, restore ladder Tier 1 promises data/homes/conversations, fresh tab correctly bound with no leak; layout rehydration is an unrequested feature, not a defect) |
| P09 | Full catalog represented; every route proven or explicitly blocked with evidence | S9 host PASS + OrbStack relay harness PASS @c5aa9eac; repo docker:: 12/12 PASS @245b7bdb after F8 (protocol version tracks USAGE_BROKER_PROTOCOL_VERSION) |
| P10 | Native/protocol parity, independent review, accurate handoff | S10 22/23 @7c0c64d9 (F6 fixed 8/8 host; usage_broker 12/12 via F8; dind sentinel EACCES root-caused to confined /jackin/state — F10 hook-state fix landed @245b7bdb; prompt-phase stall root-caused to F11 harness buffering, F11 fix landed @01cdd150, e2e acceptance re-running on merged tree) |

## Scenario matrix (goal §1-10)

S1-S10 map to P-rows above. First pass executed @62c2eebf (macOS 27 arm64, no containers, no live creds — env/credential gaps recorded per row, never passes). Evidence dirs /tmp/jackin-s1-acc1, /tmp/jackin-s3-acc1, /tmp/jackin-s5-acc1, /tmp/jackin-su-acc1002/{s4,s7,s9,s10}, /tmp/jackin-st-s2s6/evidence (transcripts + config snapshots + EVIDENCE.md indexes). Re-run FAIL rows after F1-F7 land.

## Full A01-H12 checklist

Inherited from feat-branch ledger §Full checklist; every row UNVERIFIED/BLOCKED
there and unproven here. Re-verify per row after repairs land; do not bulk-pass.

## Parity triage (D1/D2/D6/D7/D10/D11, 2026-09-21 @1a9b8ddc)

- D1/D2/D10/D11 FIXED + verified: `cargo test -p jackin-usage --lib host::projection` 36/36 green on the merged tree (agreement tests, no documented-delta escape).
- D6 (credential-expiry channel) / D7 (typed issues/retry): ACCEPTED gaps, locked by `documented_delta_*` tests. Root cause is upstream, not renderer parity: no collector, envelope, projection, or broker site produces expiry or retry data (all `credential_expires_at_epoch`/`retry_deadline_epoch`/`issues` producers write `None`/empty; no `UsageIssue` type exists). Both surfaces are equally honest in production; adding dead view fields would be placeholders. Real fix = capture expiry/Retry-After in collectors → envelope → projection → view (producer-pipeline feature, out of parity scope). Compatible with goal §5 (expiry countdowns / typed retry codes not in the §5 content list).

## Next

1. CI green @ d80d99bd, then squash-merge #1013 (squash message carries
   Alexey signoff only), verify post-merge main, close #1002 with
   selective-integration links, guarded-delete source + integration branches.
2. Continue oldest-first queue; refresh remote inventory after each branch.
3. Carried open question (pre-existing ledger item, unverified this session):
   S3 tabs e2e split-spawn stall diagnosis on merged tree (boot-phase
   isolation proven; F11 harness fix may already resolve). Verify or record
   before final completion report.

## Follow-up PR #1025 (branch followup/multi-account-restore, base ee1da0c6)

Restores multi-account behaviors broken/lost after #1013 merged (9ee50f6a):

- 26259655 fix(capsule): Landlock grants for derived pane homes + agent-keyed seed fragments.
- 111fb952 fix(usage-broker): setsid() detach so relaunch reuses broker (orphan-lease restore failure).
- 53a5ba94 test(e2e): answer hardline reconnect prompt; `stty quit undef` (VQUIT SIGQUIT flake).
- 63206f21 fix(clippy): needless_borrow process_isolation.rs:240; used_underscore_binding broker_service_lifecycle.rs:204.

CI episode 2026-09-21: capsule+runtime reds were both clippy lints in
our code, NOT test failures. Telemetry-guard race theory DISPROVEN and
reverted uncommitted: test_capsule_layers builds purely local providers
with thread-local set_default (no process-global install, observability.rs:1590+);
runtime has a single in-process init_wire_test_export user (host_daemon test;
launch tests self-isolate via subprocess respawn), so no in-process race exists.
Local verify @63206f21: clippy clean both crates, fmt clean, broker 2/2, capsule lib 892/892.

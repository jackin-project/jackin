# PR #1013 requirement-to-evidence ledger

Scope: multi-account settings, discovery, usage, authorization, launch, containers on
`integrate/pr1002-multi-account` only. Source of truth for merge readiness.
Supersedes `origin/feat/multi-account-support:plans/multi-account/ledger.md` (all rows
UNVERIFIED/BLOCKED there; historical counts are leads, not proof).

## Candidate

- PR: #1013 OPEN, head `integrate/pr1002-multi-account` @ `997c18fe`,
  base `main` @ `fce94cea` (= origin/main, in sync), MERGEABLE/BLOCKED, no human review.
- Diff: 433 files, +54251/-4402. Replaces #1002 (still OPEN, failing, out of scope).
- CI run 35537570310 @ head 997c18fe: 43 pass / 1 pending (Swift Apple lane) / 0 fail.
  Required gates: DCO pass, Policy pass, ci-required aggregator. Policy root cause
  was generator-state scan-hash drift, fixed by 6713d014 (scan hash only).
  All 157 non-merge commits carry Signed-off-by. Repairs will move head: re-verify.
- Reviews: 1 bot review; 1 open P1 inline (Landlock mounts, thread 4057673954);
  zero human reviews.
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
| P09 | Full catalog represented; every route proven or explicitly blocked with evidence | S9 host PASS + OrbStack relay harness PASS @c5aa9eac; repo docker:: 0/4 FAIL test-bug F8 (v1 vs v2, fix dispatched) |
| P10 | Native/protocol parity, independent review, accurate handoff | S10 18/23 @c5aa9eac (F6 fixed 8/8 host; dind sentinel 1 FAIL — F10 probe dispatched; F8 covers usage_broker docker 0/4) |

## Scenario matrix (goal §1-10)

S1-S10 map to P-rows above. First pass executed @62c2eebf (macOS 27 arm64, no containers, no live creds — env/credential gaps recorded per row, never passes). Evidence dirs /tmp/jackin-s1-acc1, /tmp/jackin-s3-acc1, /tmp/jackin-s5-acc1, /tmp/jackin-su-acc1002/{s4,s7,s9,s10}, /tmp/jackin-st-s2s6/evidence (transcripts + config snapshots + EVIDENCE.md indexes). Re-run FAIL rows after F1-F7 land.

## Full A01-H12 checklist

Inherited from feat-branch ledger §Full checklist; every row UNVERIFIED/BLOCKED
there and unproven here. Re-verify per row after repairs land; do not bulk-pass.

## Next

1. Land D-UI1, D-UI2, D-SEC1 repairs with regression tests.
2. Repro or clear D-SEC2/D-SEC3/D-BR1/D-PR1.
3. Run S1-S10, fill evidence paths, independent review, merge.

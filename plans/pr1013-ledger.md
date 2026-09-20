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
| D-UI1 | Console `from_projection` drops `metric_groups`, plan, issues, cred-expiry, quota_state, raw % | `crates/jackin-console/src/tui/screens/usage.rs:176-214` vs `jackin-protocol/src/usage_broker.rs:894-1006` | BLOCKED, repair dispatched |
| D-UI2 | Capsule one tab per provider; same-provider accounts collapse; 7-surface hardcode excludes Cursor/OpenRouter/Copilot/Antigravity/Gemini/OpenCode/omp/Hermes | `crates/jackin-usage/src/usage/view.rs:493-552`, `control.rs:921-935`, `dialog/usage.rs:119-137` | BLOCKED, repair dispatched |
| D-SEC1 | P1 review: Landlock grants only cwd; workspace mounts outside workdir + worktree git dir `/jackin/host/...` denied | `crates/jackin-capsule/src/process_isolation.rs:249-398`, thread 4057673954 | BLOCKED, repair dispatched |
| D-SEC2 | No durable multi-file publication journal; crash-between-renames skew unrecovered | `persist.rs` TODO, `TODO.md:78-83` | BLOCKED, design needed |
| D-SEC3 | Teardown uses bare `remove_dir_all` (symlink/replacement races) | `cleanup.rs`, `isolation/cleanup.rs` | UNVERIFIED, needs repro |
| D-BR1 | Broker: no cancel; join timeout keeps ownership; lease acquired post-discovery (stale catalog risk, per WS5/HANDOFF) | `coordinator`, `broker.rs`, `view.rs:1-16` | UNVERIFIED, needs repro |
| D-PR1 | Several collectors/parsers lack production broker dispatch; closed provider enum | provider-research/catalog ledgers U1-U12 | UNVERIFIED, needs verification |
| D-UI3 | No filter/sort/capacity-finder on either surface; Console Enter flips title only; severity/meter/freshness wording deltas Console vs Capsule | WS6 evidence; `usage.rs:250,478,635,766` | UNVERIFIED, scope for repair agents |
| D-SEC4 | Supervisor gate binds root peer by env PID equality only (`peer.pid == supervisor_pid` grants launch-wide caps); PID reuse by another root process impersonates supervisor | `usage_relay_proxy.rs:80-96,171-179` | CONFIRMED by inspection; fix direction: start-time/token binding, not PID |
| S10-auth | Local auth files PRESENT (presence only): Claude (~/.claude.json), Codex, Cursor, Grok. Absent: Kimi, Amp, OpenCode, Gemini paths | host inventory 2026-09-21 | Presence != valid auth; live checks must validate per account |
| D-PR2 | Gemini adapter has no live fetch; omp/hermes attribution-only; token_monitor lacks Grok/Antigravity/Gemini/Cursor/Muse/omp/Hermes | WS2 evidence | UNVERIFIED, matrix work |

## Product acceptance (P01-P10)

All UNVERIFIED on final candidate. Evidence owners in parentheses.

| ID | Behavior | Status / evidence |
|---|---|---|
| P01 | First-run discovery imports logins; one damaged source blocks nothing | UNVERIFIED (accounts) |
| P02 | Settings Scan draft/apply/cancel, preserves edits, explicit no-add/partial | UNVERIFIED (accounts+UI) |
| P03 | Add subscription/profile/key/env/1Password; inference vs usage permission split; redacted | UNVERIFIED (accounts+credential review) |
| P04 | Usage async refresh + detail incl metric groups, balances, resets, honest unavailable | BLOCKED on D-UI1/D-UI2 (usage+UI) |
| P05 | Precedence: launch > workspace-role > workspace > global > sole-eligible; atomic reject | UNVERIFIED (accounts+runtime) |
| P06 | Container admits exactly A/B/C; D absent incl direct relay requests | UNVERIFIED (runtime) |
| P07 | Same identity across clients, no duplicate quota; exact OpenRouter model persists | UNVERIFIED (providers+accounts) |
| P08 | Tabs/splits/reconnect/restore preserve account/provider/model/instance binding | UNVERIFIED (runtime+UI) |
| P09 | Full catalog represented; every route proven or explicitly blocked with evidence | BLOCKED on D-PR1 (providers) |
| P10 | Native/protocol parity, independent review, accurate handoff | UNVERIFIED (CI+reviewer) |

## Scenario matrix (goal §1-10)

S1-S10 map to P-rows above; each needs exact commit, command/flow, env, pass/fail,
evidence path. None executed on this candidate yet. Live-provider (S10) and
macOS 26/OrbStack E2E (S9) have known env/credential gaps to record explicitly.

## Full A01-H12 checklist

Inherited from feat-branch ledger §Full checklist; every row UNVERIFIED/BLOCKED
there and unproven here. Re-verify per row after repairs land; do not bulk-pass.

## Next

1. Land D-UI1, D-UI2, D-SEC1 repairs with regression tests.
2. Repro or clear D-SEC2/D-SEC3/D-BR1/D-PR1.
3. Run S1-S10, fill evidence paths, independent review, merge.

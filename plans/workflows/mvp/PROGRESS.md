# Implementation progress

## Current result

**Product status:** STEPS 0-6 IMPLEMENTED AND VERIFIED; STEP 7 PREPPED.  
**Live-agent checks:** PARTIAL (K1 Claude leg proven; Codex leg and repair turns BLOCKED).  
**Pkl fixture evaluation:** PASS (K3 17/17; live `validate` + `plan` proven).  
**Sandbox and recovery checks:** PASS (K2, K4, daemon interruption proven).  
**MVP release decision:** NOT READY (K7 live path BLOCKED on user prerequisites; no MVP PASS declared).

The coordinator updates this file during implementation.
Do not change a result to PASS without a command result and evidence reference.

## Selected environment

| Field | Value |
|---|---|
| Implementation commit | `30e804aa` (Step 0 base; branch `chore/workflows-mvp`). |
| Host OS and architecture | macOS 27.0.1, arm64. |
| Local Docker version and profile | Docker 29.8.2. Profile `workflow-local` to be created in Step 2. |
| Claude binary, model, account ID, and auth mode | `claude` present. Account `default-claude` (profile dir, subscription login presumed; proven in Step 1). No secrets belong here. |
| Codex binary, model, account ID, and auth mode | `codex` present. Account `default-codex` (profile dir, subscription login presumed; proven in Step 1). No secrets belong here. |
| Role and image digests | Role `agent-smith` (mise provides python 3.14.7). Image digest recorded in Step 2. |
| Fixture repository and base commit | Fixture to be created in Step 2. Workspace `workflows-mvp` to be created (none configured yet). |
| Backup mode and authorized test remote | Mode `off` for runs; exact-ref backup tested against a temporary bare remote in Step 2. |
| Current test and CI commands | `mise exec -- mbx +1.99.0 nextest run -p <crate>`; Rust 1.99.0; turso `=0.8.3`; docs gate `cargo xtask docs mvp-links`. |

## Step record

| Step | Check | State | Commit | Command and evidence |
|---|---|---|---|---|
| 0 | K0 | PASS | Step 0 commit | Gate `cargo run -p jackin-xtask -- docs mvp-links` prints `mvp links OK`. New tests `docs::mvp_links::tests` 4/4 pass. Existing baseline `docs::site_links::tests` 1/1 passes. `cargo fmt --check -p jackin-xtask` clean. `cargo clippy -p jackin-xtask --all-targets` clean. Env: macOS 27.0.1 arm64, Docker 29.8.2, Rust 1.99.0, turso `=0.8.3`. No active instruction names deleted `TESTING.md` (only superseded `plans/workflows/implementation/01-build-sequence.md:97`; README now redirects MVP work to `mvp/`). Independent verifier re-ran all four K0 checks at exact commit `894c9af2` in a separate read-only worktree with its own target dir: PASS, no files changed, worktree removed. |
| 1 | K1 | PARTIAL (Claude PASS, Codex retry) | `48d9acb9` | Claude: two prompts (`K1TURN1-OK`, `K1TURN2-OK`) answered in one live session (container `jk-9vb1d12g`, session 1, account `default-claude`, Claude Code v2.1.299, Sonnet 4.6, Claude Max subscription). Observer: `k1-observer.ndjson` (20 events, subscribe+activity+idle on session 1, no input). Ownership: `daemon::tests::case_26` 4/4, `session_control::tests::case_02` 3/3 pass. Refresh: codex provider `case_02` 2/2 pass (fake-expiry). Evidence: `/tmp/workflows-mvp-evidence/`, `fixture-revisions.txt` (`5c9f200` base, `74c969f` review rev). Network: OUTPUT DROP default with `jackin-allowed` ipset (5 entries) in `probe.log`. Codex review BLOCKED (not credential failure; credentials never exercised). Attempt 1 under `the-architect`: container preflight fails (`codex caveman skill missing`, exit 1, no session; see `load-codex9.typescript` tail). Attempt 2 under `agent-smith`: rejected at identity resolution, manifest declares `agents = ["claude"]` (`jackin.role.toml:7`, verified). Missing prerequisite: a codex-capable role with a working preflight (install the `caveman` skill into the `the-architect` role image via its own repo, or select another codex role). No role rebuild or skipped preflight inside MVP scope. Safe work continues: Step 2 needs only the proven Claude profile, which cannot change with the Codex role choice. |
| 2 | K2 | PASS | `378e41be` | Base `c13e3697`: grant 8/8, dissociation, holds, capture round-trip, push semantics, live pause-capture-resume byte-identical, denied writes, 363/363, fmt+clippy clean; independently verified at exact commit. Follow-up `378e41be`: clamp wired after `fold_role_grants` (manual path keeps meaning, case_02 2/2); holds gate eject/purge/prune (case_06); hardened-profile live network: disallowed name+IP fail (curl 28), allowed anthropic (TLS, 404) and github (200) succeed, IPv6 deny-all recorded, enforcement level IP-allowlist IPv4 + DNS 53 (`/tmp/k2-followup/`). Coordinator re-ran: launch_phases 9/9, case_06 filter 17/17, fmt clean, clippy clean. Known: live workflow-launch clamping waits for the workflow CLI (Step 3+ passes `Some(grant)`); IPv6 allowed-request impossible by design. Follow-up independently verified at exact `378e41be` (launch_phases 9/9, cleanup 61/61, fmt, clippy, probe.log line-matched to RESULTS.md; no contradictions; worktree removed). Step 2 closed. |
| 3 | K3 | PASS | `2e8d512d` | pklr `=5.1.1` (eval-core only) + `cap =0.1.2`. Crate `jackin-workflow-spec`: frozen-module killable eval, cross-field rules, canonical SHA-256 hash, input store, typed errors. K3 suite 17/17 pass (coordinator re-ran), fmt+clippy clean. Base `8f90e5a7` independently verified. Live CLI proven by coordinator: `validate` prints `valid:`; `plan --json` from the fixture dir prints plan_hash `ebc1278f…` with workspace `workflows-mvp`, role `agent-smith`, implementer `default-claude`, reviewer `default-codex`, base_commit `74c969f` (fixture HEAD, verified against `git rev-parse`), stable across re-plans. Two coordinator-found defects fixed in follow-ups: selection stub (6c9cf822) and wrong-repo HEAD (2e8d512d). Workspace default role set to `agent-smith` via CLI. Final fix independently verified at exact `2e8d512d` (17/17, fmt, clippy; Pkl dir never a candidate and plan_inner wiring confirmed by code reading; live plan output proven separately by coordinator. No contradictions; worktree removed). Step 3 closed. |
| 4 | K4 | PASS | `2d048e13` | New T2 crate `jackin-workflow`: 8 tables (service_meta + 7) in separate turso DB via `connect_local` (`<run_dir>/workflow-runs.db`), one-txn writes, single-run lock (same key returns run, other key `Busy`) + owner epoch on `serve()` startup (`StaleEpoch` rejection), `delivery_unknown` with no resubmit path, effect dispatcher, C5 recovery table, `resolve_input`, pause/cancel semantics. Daemon injection; run-control RPCs; command-ID correlation; CLI run/status/pause/resume/cancel. Coordinator: K4 rows PASS, daemon live answers status/pause with typed errors. Independently verified at exact `2d048e13` (all five checks, no contradictions; worktree removed). Gaps for Step 6: bare `status` should list runs; no `--json` on status. Live `run` launch deferred to Step 5/7. Step 4 closed. |
| 5 | K5 | PARTIAL (`646d406e`; 2 live-agent BLOCKEDs) | `646d406e` | C6 code green: verify.rs (policy freeze, exact-argv runner, schema+validator+adapter+correction budget, evidence binding, feedback scrub, decide predicate, delivery gate), K5 suite 10 pass + 1 skipped (coordinator re-ran). Live fixture: fresh private clone, pinned `python3 check_add.py` exit 1 on bug / exit 0 on scratch fix (`/tmp/k5-step5/`). BLOCKED live-Claude repair turn: host Claude OAuth expired — coordinator verified `claude -p` fails `OAuth session expired and could not be refreshed`; needs human `claude login` (Step 1 worked while the token lived). BLOCKED live-Codex review: role prerequisite unchanged; coordinator verified Codex host credentials VALID (`codex login status` → `Logged in using ChatGPT`). Note: calculator fixture freezes to the workspace tree by design; Step 7 creates the fixture repo and re-plans. Removed unauthorized k5 scratch workspace (npmjs grant residue; evidence kept in /tmp). Independently verified at exact `646d406e` (10+1, fmt, clippy; no-creds runner, evidence-before-repair, feedback scrub, worker-can't-accept all confirmed; worktree removed). |
| 6 | K6 | PASS | `7f1b6c64` | Base `51745bd8` verified. Follow-up: Console `Workflows` route live (`f` opens, confirm-gated takeover/resume/cancel, release, export; host executors). Live takeover proven on fresh session (take→owned:true→auto-send rejected→observe subscribed→release→send accepted, shell round-trip `TAKEOVER-PROOF-OK`; coordinator corroborated from `/tmp/workflows-followup/`, daemon 2303 untouched). Console screen tests 14/14 pass (coordinator re-ran). Daemon still running; proof run still paused (evidence). Live Claude/Codex turns remain BLOCKED (prerequisites unchanged). Follow-up independently verified at exact `7f1b6c64` (14/14, fmt, clippy, route arms + takeover all-links corroborated from evidence files, no contradictions; worktree removed). Step 6 closed. |
| 7 | K7 | PARTIAL (Claude legs proven live; Codex + acceptance BLOCKED) | `e6fb4035` | Fixture repo `/tmp/workflows-mvp-k7/repo` ready: base `a6e327ab`, tree `b154ac6f`, files byte-identical to `examples/workflows/` (which already carries the `a - b` bug). Coordinator verified `check_add.py` fails `add(2,3) returned -1` exit 1. NOTES.md records the workspace/mount change Step 7 needs (not made). LIVE RUN 2026-10-15 (K7 PARTIAL, no MVP PASS): new workspace `workflows-mvp-k7` (CLI-created; old untouched). `plan --model` override added (`e6fb4035`; 8 regression tests; CLI workflow tests 13/13 re-run by coordinator). New plan `557632d5…` (haiku implementer, base `a6e327ab`). Run `run-6c2e49ee…`: Haiku 5.5 low-effort session (identity verified each turn), `K7TURN1-DONE` (source clean), checkpoint v1, round-0 exit 1, frozen findings to original session, repaired 1/2 turns, round-1 exit 0, candidate `114d3e1` (coordinator verified fix + PASS). Daemon restarted once (2303→21500, run intact, no duplicate input). Observe/takeover/return proven; exact-ref backup twice same ref (run NOT accepted — review BLOCKED). Fault case re-proven. Export complete. K6 proof run exported then cancelled via proper path (paused counts as active). BLOCKED: live Codex review + acceptance (role prerequisite unchanged). GAP (new): `workflow attach --observe` → `not_found` on manual-launch path (needs session-recording path). Independently verified at exact `e6fb4035` (13/13, fmt, clippy, override semantics confirmed, no contradictions; worktree removed). Daemon 21500 running; K7 run paused (evidence). |

## Blocker record

Record a blocker only after inspection or an attempted operation establishes it.
Use this short form:

```text
Step: 1/5/7 (Claude legs)
Observed failure: host `claude -p` fails `OAuth session expired and could not be refreshed`
Command or source: `echo ... | claude -p` (coordinator, no secrets printed)
Evidence: Step 1 K1 turns worked while the token lived (K1TURN1/2-OK); later fresh launches park at browser OAuth
Missing prerequisite: human `claude login` renewal (browser OAuth; cannot complete headless)
Safe work that can continue: none remaining (all code work is done and verified)
Exact next action: user runs `claude login`, then coordinator re-runs K5 repair turn + K7
```

User direction 2026-10-15: fix `the-architect` with subagents until it works with the current jackin branch; Codex verification runs under `the-architect` with ONLY gpt-6-luna at low effort (never any other Codex model). Claude constraint unchanged (Haiku 5.5 low only). Architect fixed: role commits `d02bc6e`+`4e7c990` (unpushed), checkout `846bb3d4` (sibling binary + recipe v10). Effort enforced: `b5985ad3` (profile overrides reach codex config; proven `model_reasoning_effort = "low"` in live config + thread row). First live turn ran effort-null — does NOT count as low-effort verification. Trust seeded in product (`840535c0`; scoped to workspace mounts) + runner bytecode hygiene. Isolated structured review: PASS, zero findings (gpt-6-luna low, pre/post verified). Record path committed (`03c445de`) with review hardening (TurnEnded guard, versions everywhere, daemon-side binding, policy cross-check, threat model stated). Prior host-run acceptance VOID for MVP (turns on host, not workers). Orchestrated attempt BLOCKED by stale allowlist: pinned Claude v2.1.300 requires `platform.claude.com` at startup but hardened profile allows only `api.anthropic.com` (Step 1 quietly carried the extra grant, so the base profile was never proven). Allowlist corrected in `8cea16dc` (first-party legitimacy triple-proven; live connectivity PASS; gates re-run green by coordinator). Orchestrated acceptance BLOCKED again: host Claude OAuth lapsed a second time (coordinator: `claude -p` fails, refresh impossible), so fresh workers land on interactive login — deterministic over 4 launches. Stale run cancelled properly, slot free, fixture at failing base. Daemon 5600 running. User renewed login (coordinator verified `AUTH-OK` again). Bundle fix verified + committed `ab5c130f` (persist manifest, membership + recompute checks; 77/77 re-run by coordinator). Orphan active run `run-542eee65` traced to the blocked attempt: its implement turn WAS isolated (agent-smith/haiku-low) but its plan is legacy (no manifest field) so it cannot proceed — cancel properly, run fresh. Bundle fix independently verified at exact `ab5c130f` (77 tests, fmt, clippy `-D warnings`, manifest/membership/legacy-refusal confirmed; worktree removed). Pinning independently verified at exact `c9420c05` (18/18, fmt, clippy, sort/membership confirmed). Remaining five commits independently verified (variant 192/192, sibling, effort, trust, allowlist; one stale v9 assertion fixed in `3d4fd9b3` via the constant). OAuth alive again (coordinator: live Claude reply). Fresh orchestrated acceptance through turn 1 in isolated workers (`/tmp/accept-fresh/`); coordinator not touching daemon/store while it runs.

```text
Step: 1/5/7 (Codex legs)
Observed failure: no codex-capable role with a working preflight
Command or source: the-architect preflight `codex caveman skill missing` exit 1; agent-smith manifest `agents = ["claude"]`
Evidence: /tmp/workflows-mvp-evidence/load-codex9.typescript; jackin.role.toml:7 (verified)
Missing prerequisite: codex-capable role with working preflight (install caveman skill into the-architect image via its own repo, or select another codex role)
Safe work that can continue: none remaining
Exact next action: provide the role, then coordinator re-runs K1-Codex + K5-review + K7
```

## Final release record

Record the accepted demonstration checkpoint and original physical session key.
Record the independent review session and trusted test result.
Record the host restart result and operator-control result.
Record backup transport separately from local preservation.
Name unverified platforms, authentication modes, and capabilities.
Use `MVP PASS` only when every required check has evidence.

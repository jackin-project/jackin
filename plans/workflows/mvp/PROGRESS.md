# Implementation progress

## Current result

**Product status:** STEPS 0-7 IMPLEMENTED AND VERIFIED.  
**Live-agent checks:** PASS (Claude + Codex legs proven in isolated workers; Haiku 5.5/low and gpt-6-luna/low exclusively since the model constraint; Sonnet 4.6 predates it).  
**Pkl fixture evaluation:** PASS (K3 18/18 with pinning; live `validate` + `plan` proven).  
**Sandbox and recovery checks:** PASS (K2, K4, daemon interruptions, live crash recovery proven).  
**MVP release decision:** MVP PASS (K0–K7 evidenced; deviations and gaps documented below; branch unpushed/unmerged, role commits unpushed — all awaiting permission).

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
| 1 | K1 | PASS | `48d9acb9` + role fix | Claude: two prompts (`K1TURN1-OK`, `K1TURN2-OK`) in one live session (session 1, `default-claude`, Sonnet 4.6, Max subscription). Observer: `k1-observer.ndjson` (20 events, no input). Ownership: unit 4/4 + 3/3, live takeover proven twice. Codex leg CLOSED later: `the-architect` repaired (role commits `d02bc6e`+`4e7c990`, checkout `846bb3d4`+`66805498`), effort enforced (`b5985ad3`), trust seeded (`840535c0`); separate live Codex reviews through the product path (trust-path PASS + fresh-run reviewer `jbs9dg84`, gpt-6-luna low verified pre/post, review PASS zero findings). Auth modes proven separately per profile. |
| 2 | K2 | PASS | `378e41be` | Base `c13e3697`: grant 8/8, dissociation, holds, capture round-trip, push semantics, live pause-capture-resume byte-identical, denied writes, 363/363, fmt+clippy clean; independently verified at exact commit. Follow-up `378e41be`: clamp wired after `fold_role_grants` (manual path keeps meaning, case_02 2/2); holds gate eject/purge/prune (case_06); hardened-profile live network: disallowed name+IP fail (curl 28), allowed anthropic (TLS, 404) and github (200) succeed, IPv6 deny-all recorded, enforcement level IP-allowlist IPv4 + DNS 53 (`/tmp/k2-followup/`). Coordinator re-ran: launch_phases 9/9, case_06 filter 17/17, fmt clean, clippy clean. Known: live workflow-launch clamping waits for the workflow CLI (Step 3+ passes `Some(grant)`); IPv6 allowed-request impossible by design. Follow-up independently verified at exact `378e41be` (launch_phases 9/9, cleanup 61/61, fmt, clippy, probe.log line-matched to RESULTS.md; no contradictions; worktree removed). Step 2 closed. |
| 3 | K3 | PASS | `2e8d512d` | pklr `=5.1.1` (eval-core only) + `cap =0.1.2`. Crate `jackin-workflow-spec`: frozen-module killable eval, cross-field rules, canonical SHA-256 hash, input store, typed errors. K3 suite 17/17 pass (coordinator re-ran), fmt+clippy clean. Base `8f90e5a7` independently verified. Live CLI proven by coordinator: `validate` prints `valid:`; `plan --json` from the fixture dir prints plan_hash `ebc1278f…` with workspace `workflows-mvp`, role `agent-smith`, implementer `default-claude`, reviewer `default-codex`, base_commit `74c969f` (fixture HEAD, verified against `git rev-parse`), stable across re-plans. Two coordinator-found defects fixed in follow-ups: selection stub (6c9cf822) and wrong-repo HEAD (2e8d512d). Workspace default role set to `agent-smith` via CLI. Final fix independently verified at exact `2e8d512d` (17/17, fmt, clippy; Pkl dir never a candidate and plan_inner wiring confirmed by code reading; live plan output proven separately by coordinator. No contradictions; worktree removed). Step 3 closed. |
| 4 | K4 | PASS | `2d048e13` | New T2 crate `jackin-workflow`: 8 tables (service_meta + 7) in separate turso DB via `connect_local` (`<run_dir>/workflow-runs.db`), one-txn writes, single-run lock (same key returns run, other key `Busy`) + owner epoch on `serve()` startup (`StaleEpoch` rejection), `delivery_unknown` with no resubmit path, effect dispatcher, C5 recovery table, `resolve_input`, pause/cancel semantics. Daemon injection; run-control RPCs; command-ID correlation; CLI run/status/pause/resume/cancel. Coordinator: K4 rows PASS, daemon live answers status/pause with typed errors. Independently verified at exact `2d048e13` (all five checks, no contradictions; worktree removed). Gaps for Step 6: bare `status` should list runs; no `--json` on status. Live `run` launch deferred to Step 5/7. Step 4 closed. |
| 5 | K5 | PASS | `646d406e` + live paths | C6 code green: verify.rs (policy freeze, exact-argv runner, schema+validator+adapter+correction budget, evidence binding, feedback scrub, decide predicate, delivery gate), K5 suite 10 pass + 1 skipped (coordinator re-ran). Live fixture: fresh private clone, pinned `python3 check_add.py` exit 1 on bug / exit 0 on scratch fix (`/tmp/k5-step5/`). BLOCKED live-Claude repair turn: host Claude OAuth expired — coordinator verified `claude -p` fails `OAuth session expired and could not be refreshed`; needs human `claude login` (Step 1 worked while the token lived). BLOCKED live-Codex review: role prerequisite unchanged; coordinator verified Codex host credentials VALID (`codex login status` → `Logged in using ChatGPT`). Note: calculator fixture freezes to the workspace tree by design; Step 7 creates the fixture repo and re-plans. Removed unauthorized k5 scratch workspace (npmjs grant residue; evidence kept in /tmp). Independently verified at exact `646d406e` (10+1, fmt, clippy; no-creds runner, evidence-before-repair, feedback scrub, worker-can't-accept all confirmed; worktree removed). Live accepted path: fresh run round-1 test PASS → accept. Live rejected path: `run-8319d04d` (isolated worker, wrong-fix refused by agent, two failing verdicts, tree preserved clean at base, cancelled properly). |
| 6 | K6 | PASS | `7f1b6c64` | Base `51745bd8` verified. Follow-up: Console `Workflows` route live (`f` opens, confirm-gated takeover/resume/cancel, release, export; host executors). Live takeover proven on fresh session (take→owned:true→auto-send rejected→observe subscribed→release→send accepted, shell round-trip `TAKEOVER-PROOF-OK`; coordinator corroborated from `/tmp/workflows-followup/`, daemon 2303 untouched). Console screen tests 14/14 pass (coordinator re-ran). Daemon still running; proof run still paused (evidence). Live Claude/Codex turns remain BLOCKED (prerequisites unchanged). Follow-up independently verified at exact `7f1b6c64` (14/14, fmt, clippy, route arms + takeover all-links corroborated from evidence files, no contradictions; worktree removed). Step 6 closed. |
| 7 | K7 | PASS | `run-9db3cc2a` | Fresh run on plan `3c37c8c9` (haiku, base `a6e327ab`; old hashes legacy by design). All 19 rows evidenced in `/tmp/accept-fresh/` (coordinator corroborated store, events 0–26, markers, models, review, backup, fixture, export): real accounts, clean inspect turn, failing capture, fail-then-repair (1/2 turns, same session), daemon restart mid Codex turn with no duplicate input, same identities, observe both workers, exclusive takeover + refusal, return, retest + acceptable review, exact-accept, exact-ref backup idempotent, export (27 events), manual smoke, refusal battery. Reviewer `jbs9dg84` (gpt-6-luna low, pre/post verified) re-derived commit/tree before PASS. Accepted v15. Deviations: round-0 review impossible by design (requires test_passed); return via lease release (proper return proven live in Step 6); final-code delta (recovery wiring) proven live by crash test + 3 restart preservations. Known gaps: `attach --observe` on unregistered sessions; Console view unit-covered (no live screenshot). |

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

Accepted checkpoint: candidate round-1 @ `e88fe8c2` (tree `43dbd6d0`, one-line `a-b`→`a+b`), run `run-9db3cc2a`, plan `3c37c8c9`, accepted v15. Original sessions: implementer `xaa772dc` (agent-smith, default-claude, Haiku 5.5/low), reviewer `jbs9dg84` (the-architect, default-codex, gpt-6-luna/low). Independent review: PASS, zero findings, commit/tree re-derived. Trusted test: exit 0 `PASS: required arithmetic cases` (round-1; round-0 exit 1 recorded). Host restarts: daemon interruptions proven safe repeatedly (K6, K7-partial, fresh run mid Codex turn epoch 9→10, crash test with SIGKILL); accepted state intact across all. Operator control: observe both workers, exclusive takeover with refusal text, return via release. Backup: exact ref `refs/jackin/k7-accept-fresh` == candidate, idempotent repeat; acceptance separate from preservation. Auth: Claude subscription (Max, OAuth, manual renewal — refresh unsupported in practice) and Codex ChatGPT login, each proven in workers; API-key modes unverified. Remote transport not verified (temporary bare remotes only). Hard limits: no disk quota (monitor only), IPv6 deny-all, IP-level enforcement. Deferred scope: none built. MVP PASS.


## CI and merge hardening — 2026-10-16

Independent pre-merge review found that C6's original exact-argv runner
executed candidate code on the host. The earlier live repair evidence did
not prove test-worker containment. The runner now requires a pinned Docker
sandbox with no host fallback, read-only private candidate and digest-checked
trusted bundle mounts, no network or credentials, bounded writable tmpfs,
and host plus container deadlines. Only a confirmed exited container can
produce evidence; transport failures and unavailable infrastructure block
acceptance. The frozen plan now binds the actual test image and full
supported argv shape. Older image bindings require a fresh plan.

Validation: workflow suite 38 passed, 1 pre-existing ignored; strict workflow
Clippy passed. Actual Docker regressions prove host-secret and network
denial, read-only mounts, bounded retention while draining oversized stdout
and stderr, and timeout cleanup. Host-daemon suite passed 24/24, including
direct-RPC rejection of extra argv, another valid bundle member, and a
mismatched test image. The CLI regression rejects unsupported argv before
planning. Architecture inventory now includes both workflow crates; the
telemetry registry includes all workflow commands, nested record actions
use their exact command names, and managed lifecycle helpers own daemon
and output-reader spawns. The xtask suite passed 477/477 after registry
regeneration; the telemetry registry gate and MVP link gate passed. The
released Velnor 0.1.8 generator preview matched the checked-in workflows.

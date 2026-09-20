# Requirement-to-evidence ledger

Proof levels: `implemented` < `fixture_verified` < `container_verified` <
`live_verified`. Also: `failed`, `unavailable` (credentials), `unsupported`
(genuinely, with evidence), `not_run`.

## Spine

| Requirement | Owner | Test / scenario | Result / artifact | Proof | Status |
|---|---|---|---|---|---|
| S1 catalog (12 agents, 12 providers, adapters, matches) | s1-catalog-expand | nextest 909 (core/config/instance/image/telemetry/agent-status) + usage 412 + console/runtime 1933 + workspace check clean | /tmp/lane-s1-catalog.md | fixture_verified | implemented |
| provD OpenRouter/OpenCode-Go/Grok | prov-d | openrouter 9 + opencode/grok ext (in-tree green) | /tmp/lane-prov-d.md | fixture_verified | implemented |
| T10 metric groups | t10 | protocol 56 + projection 36 + console 20 (in-tree) | /tmp/lane-t10.md | fixture_verified | implemented |
| C04 committed-agent defaults | c04 | prompts 22 + list 38 (in-tree) | /tmp/lane-c04.md | fixture_verified | implemented |
| broker cadence (coordinator) | broker-coord | 27 isolated + 28 in-tree | /tmp/lane-broker-coord.md | fixture_verified | implemented |
| provider review (13 fixes) | reviewer | /tmp/review-providers.md | review-fix lane in flight | — | in_progress |
| zshrc static parser | zshrc lane | standalone rustc --test 17/17 + probe | /tmp/lane-zshrc.md | fixture_verified | implemented |
| stores enumerators | stores lane | config 317/317 (stores 29) | /tmp/lane-stores.md | fixture_verified | implemented |
| T20 harness | t20 lane | standalone 26/26 | /tmp/lane-t20-harness.md | fixture_verified | implemented |
| provA Claude/Codex/Amp | prov-a | isolated worktree 314/314 + clippy clean | /tmp/lane-prov-a.md | fixture_verified | implemented |
| provB Kimi/ZAI/MiniMax | prov-b | kimi 14 + zai 9 + minimax 15 | /tmp/lane-prov-b.md | fixture_verified | implemented |
| provC Antigravity/Gemini/Cursor | prov-c | 26/26 new-collector tests | /tmp/lane-prov-c.md | fixture_verified | implemented |
| provE Muse/omp/Hermes | prov-e | 26/26 (13+5+8) | /tmp/lane-prov-e.md | fixture_verified | implemented |
| console-usage phase-1 | console-usage-1 | isolated worktree 20/20 + lib 1270 + adapter 106 | /tmp/lane-console-usage-1.md | fixture_verified | implemented |
| S2 schema + resolver + bootstrap | orchestrator | config/resolver unit + migration tests (landed in S4 integration commit) | git log feat(multi-account): S4 | fixture_verified | implemented |
| S3 instance-keyed credential transport | orchestrator + 4 lanes | protocol 117 + instance 143 + env 56 + capsule 883 + runtime 646 + console/usage/jackin/xtask green; clippy -D warnings; xtask lint --strict | 6f0280c4 | fixture_verified | implemented |
| Tracer bullet: 2×Claude + Codex live container | orchestrator | live jk-ctdn5jt0 (3 slots, 3 tabs, per-instance env, manifest A/B/C, D absent all surfaces, new/split/resize/exit/reattach/restore) + nextest/clippy gates | /tmp/tracer/evidence.md + /tmp/split.log | live_verified (claude live-auth capped: default session-limited, claude-b grant expired) | implemented |
| S4 discovery/Settings/launch/Capsule lanes + integration | A/B/C/D/E + orchestrator | config/protocol/console/jackin 2457 + core/instance/env/runtime/capsule/usage 2326 + console re-run 2337; clippy -D warnings; xtask lint --strict; scan bridge (input→Manager→StartAccountScan→worker→AccountScanCompleted), usage offscreen heartbeat via UsageRouteState, boxed dispatch Action | S4 commit (see git log) | fixture_verified | implemented |

## Checklist A–H (from jackin-implementation-and-verification.md)

Rows appended as lanes land. Every row needs: test name/path + result, or exact
blocker. No row is marked above `not_started` without inspected evidence.

(A01–A23, B01–B16, C01–C18, D01–D22, E01–E08, F01–F29, G01–G18, H01–H12 —
to be filled. Source of truth for item text is the companion doc.)

## Provider lanes

| Provider | Parser/semantic | Service/process | Container | Live Mac | Notes |
|---|---|---|---|---|---|
| Claude/Anthropic | pass | pass | pass (tracer A/B) | capped (default session-limited; claude-b OAuth expired) | keychain creds, oauthAccount cache |
| Codex/OpenAI | pass | pass | pass (tracer C) | live (ChatGPT login, claude+codex 3/3 green) | app-server 0.154.0, file backend |
| Amp | pass | pass | broker-only | live (alexey@zhokhov, credits shown) | XDG data secrets.json, auto-update pin |
| Antigravity/Google | pass | pass | broker-only | unsupported-headless (no CLI; GUI state unverifiable) | keyring singleton |
| Kimi | pass | pass | broker-only | live (`kimi -p` exit 0; default-kimi registered) | both families; 06b64e20 per-env grant fix |
| Z.AI | pass | pass | broker-only | unavail (no CLI/creds) | provider only |
| Muse | pass | pass | broker-only | live (native auth resolution green) | `.config/muse`, keychain resolved |
| Cursor | pass | pass | broker-only | live (stored-token auth green) | file+keychain lineages |
| Grok/xAI | pass | pass | broker-only | live (token refresh OK) | 1.0.30, embedded principal |
| OpenRouter | pass | pass | broker-only | unavail (no key configured) | ordinary-key `/key` usage; exact model IDs; Management-key credits/activity not configured |
| omp | pass | pass | broker-only | unavail (NOT installed) | broker file is not authz |
| Hermes | pass | pass | broker-only | unavail (NOT installed) | `hermes --tui` |
| OpenCode | pass | pass | broker-only | unavail (CLI present, 0 credentials) | 1.18.30, auth.json absent locally |
| Gemini CLI | pass | pass | broker-only | unavail (NOT installed) | separate Google client |
| MiniMax | pass | pass | broker-only | failed (canary-d in-band 1004 login fail; placeholder key) | shell provider routes verified |

## Environment (T00, 2026-09-17)

- HEAD at session start: `21232c7e`, branch `feat/multi-account-support`, tree clean.
- macOS 26.6.2 arm64; Rust 1.97.1; nextest 0.9.140; node 24.18; bun 1.3.14.
- Installed: claude 2.1.274, codex 0.154.0, amp 0.0.1789639648-g3c529d, agy 1.2.5,
  kimi 0.43.0, muse 1.3.0, cursor-agent 2026.09.10, grok-build 1.0.30,
  opencode 1.18.30. Missing: gemini, omp, hermes, mmx.

## CI watch (PR #1002, head 6f0280c4, 2026-09-17)

- `Rust · jackin` + `Rust · jackin-runtime`: FAILED at `Set up Mr. Boxington` (cache setup, before any build/test) — same mbx infra-flake signature as head 3c807144 (`Quota exceeded`), not code. Siblings (capsule/config/console/core/usage) PASS on this head. Rerun blocked while the workflow runs; the S4 push supersedes with a fresh full run.
- `Policy` (Velnor workflow policy): FAILED on `generated-tree` drift vs pinned generator 06050c9f. Branch has zero diff vs main under `.github-gen/` + `.github/` — inherited main breakage, out of scope (generated files are never hand-edited). Recorded, not fixed.

## Gates + console-live (post-compaction, 2026-09-17/18, head 06b64e20)

- Console-live: `jackin console --debug` under PTY, Settings → Accounts renders all
  accounts (canary-d masked, claude-b, 8 defaults, +Add rows); Ctrl-Q confirm exits 0.
  Evidence: /tmp/tracer/evidence.md.
- Focused gates green: fmt, clippy, nextest 839 + 3959 passed.
- `cargo xtask ci` findings fixed: E0063 in reactive_daemon/tests.rs (all-features-only
  module missed the account_id/instance fields); lint container-paths
  (capsule_setup.rs:133 → container_paths::JACKIN_ROOT), telemetry-registry
  regenerate, ratchet test-layout (launch_runtime.rs inline tests → sibling file).
- Docs gates fixed (pre-existing main breakage from velnor regen #982/#992):
  repo-links 17 stale workflow refs repointed (docs/construct/jackin-dev →
  generated ci-pr.yml units; desktop-cadence → desktop-merge.yml; preview.yml →
  planned-workflow prose since SAN pin + xtask preview.rs still reference it);
  brand 42 (`Jackin` → `jackin❯` in 4 root companion docs); map-check 3 crates
  (telemetry t0, otlp-testbed t3, usage-ffi t4) added to codebase-map.
- Lane results (pre-compaction gates agent): docker-e2e usage_broker_e2e 12/12 PASS
  (JUnit target/nextest/docker-e2e/junit.xml); desktop-ci PASS (Rust 454, Swift 78+2);
  desktop-merge FAIL on testOverviewPassesAccessibilityAudit (85 contrast/label
  findings, native/ untouched by branch — pre-existing; dedicated fix running).
- Launch-resolver fix 9cc2d675 (2026-09-18): `resolve_launch` ignored the
  committed agent and `account_bindings`, so valid-default multi-account
  configs failed `multiple accounts are eligible` and parked on the ack
  dialog; dind_e2e chaos tests timed out with no container. Fix adds the
  agent-scoped per-agent binding layer (role → workspace → global) plus
  agent-scoped sole-eligible fallback; 5 new regression tests; config 392,
  env+console 1386, runtime+jackin 1284 all green. Focused
  `chaos_drop_control_socket` e2e: `ci gate OK`. Follow-up: interactive
  picker launches still failed the same way because
  `resolve_provision_inputs` used `opts.agent` (CLI override only, `None`
  for picker commits); it now takes the committed agent. Sentinel dind_e2e
  green in 13.6s (was 3x300s timeout).
- Full `cargo xtask ci --e2e` GREEN at a219c97e (2026-09-18, exit 0,
  `ci gate OK`, 19 steps): docker-e2e JUnit 23/23 PASS — dind 9/9 (chaos
  trio, sentinel, agentsmith, 4 exit-gates), load_options 1/1, session_send
  1/1, usage_broker_e2e 12/12 (2/20-client single-flight host+dind,
  owner loss, timeout ownership, shared deadlines, capability isolation,
  distinct-account concurrency, unavailable-state zero calls).
  JUnit: target/nextest/docker-e2e/junit.xml. (One intermediate full run was
  SIGTERM-murdered externally at 6116/6117 with only the 60s png-baseline
  test in flight; that test passes alone in 60.7s — not a code failure.)
- desktop-merge at a219c97e (2026-09-18): desktop-ci parts GREEN
  (bindings-check, Rust 454, Swift 78+2, 0 failures); desktop-test-ui
  BLOCKED — Mac is at the lock screen ("Touch ID or Enter Password",
  screenshot /tmp/screen-check4.png), so no app can activate:
  testEmptyUsageStateIsDistinct fails `Failed to activate application ...
  (current state: Running Background)` 3x deterministically, incl. under
  `caffeinate -d -u`. Re-run `mise run desktop-test-ui` after unlock.
  (Prior unlocked run failed testOverviewPassesAccessibilityAudit with 85
  pre-existing contrast/label findings; native/ app code untouched by
  branch — separate change if still red after unlock.)
- desktop-merge GREEN end to end on final tree (2026-09-18, exit 0):
  desktop-ci exit 0 (bindings-check, Rust 454, Swift 78+2) + desktop-test-ui
  19/19 incl. scroll + all 3 AX audits (JUnit 19 tests, 0 failures).
  Fixes on the way: (a) test locator buttons["Retry"] → element("usage.retry")
  + label assert; (b) moved usage.global-error identifier from
  ContentUnavailableView container (shadowed all children incl. the Retry
  button) onto its Label — UsageWindowRoot.swift; (c) scroll() helper
  re-activates + retries on focus steal; (d) broker test
  projection_refresh_runs_due_checks_and_join_settles chases superseding
  publications to Idle (publish_due mints a fresh id per intermediate
  snapshot; single join could observe Refreshing under load — branch-new
  race, now deterministic, 8/8 stress + package 454/454).
  Full `cargo xtask ci --e2e` green (exit 0, ci gate OK, 23/23 docker-e2e)
  stands from a219c97e; post-merge Rust delta is the broker-test-only fix.
- Fixture HTTP harness race fixed: accepted sockets inherit the listener's
  nonblocking mode on macOS, so read timeouts never applied and
  `read_request` failed with WouldBlock whenever the server thread outran
  the client write (empty-EOF/RST flakes under parallel load).
  `serve_one` now restores blocking mode first. Package 28/28 x6.
  Production broker unaffected (explicit WouldBlock loops with deadlines).

## Provider live matrix (lane 01a0b122-2e86, Mac 2026-09-18; kimi gap closed by 06b64e20)

- live-verified: codex (ChatGPT login), amp (alexey@zhokhov, credits), muse,
  cursor (stored token), grok (refresh OK), kimi (`kimi -p` exit 0; jackin
  `default-kimi` registered after discovery fix).
- auth-verified/capped: claude default (session-limit msg, quota-capped).
- unavailable/expired: claude-b scentbird (OAuth expired, no refresh).
- unsupported headless: antigravity (no CLI; GUI app state unverifiable).
- unavailable (no CLI/creds): gemini-cli, zai, openrouter, omp, hermes, opencode
  (CLI present, 0 credentials).
- failed: minimax canary-d (HTTP 200 in-band 1004 login fail; placeholder key).
- Parsers: all 15 lanes exist with tests; `nextest -p jackin-usage` 442/442.

## Split/resize live (2026-09-18, same tracer container)

- Palette split Right with claude-personal: 2 panes, per-pane account_id/agent
  correct in snapshot; 2x Alt-Shift-Left moved divider col 40 -> 32; both panes
  alive after client detach. Evidence: /tmp/tracer/evidence.md, /tmp/split.log.

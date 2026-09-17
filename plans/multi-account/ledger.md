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
| S2 schema + resolver + bootstrap | orchestrator | config/resolver unit + migration tests | — | — | not_started |
| S3 instance-keyed credential transport | orchestrator + 4 lanes | protocol 117 + instance 143 + env 56 + capsule 883 + runtime 646 + console/usage/jackin/xtask green; clippy -D warnings; xtask lint --strict | 6f0280c4 | fixture_verified | implemented |
| Tracer bullet: 2×Claude + Codex live container | orchestrator | docker staging/relay/PTY + 3 TUIs, canary D absent | — | — | not_started |
| S4 discovery/Settings/launch/Capsule lanes + integration | A/B/C/D/E + orchestrator | config/protocol/console/jackin 2457 + core/instance/env/runtime/capsule/usage 2326 + console re-run 2337; clippy -D warnings; xtask lint --strict; scan bridge (input→Manager→StartAccountScan→worker→AccountScanCompleted), usage offscreen heartbeat via UsageRouteState, boxed dispatch Action | S4 commit (see git log) | fixture_verified | implemented |

## Checklist A–H (from jackin-implementation-and-verification.md)

Rows appended as lanes land. Every row needs: test name/path + result, or exact
blocker. No row is marked above `not_started` without inspected evidence.

(A01–A23, B01–B16, C01–C18, D01–D22, E01–E08, F01–F29, G01–G18, H01–H12 —
to be filled. Source of truth for item text is the companion doc.)

## Provider lanes

| Provider | Parser/semantic | Service/process | Container | Live Mac | Notes |
|---|---|---|---|---|---|
| Claude/Anthropic | not_run | not_run | not_run | not_run | T01: keychain `Claude Code-credentials*`, oauthAccount cache |
| Codex/OpenAI | not_run | not_run | not_run | not_run | T01: app-server 0.154.0, file backend |
| Amp | not_run | not_run | not_run | not_run | T01: XDG data secrets.json, auto-update pin |
| Antigravity/Google | not_run | not_run | not_run | not_run | T01: agy 1.2.5, keyring singleton |
| Kimi | not_run | not_run | not_run | not_run | T01: new family 0.43.0 verified |
| Z.AI | not_run | not_run | not_run | not_run | provider only |
| Muse | not_run | not_run | not_run | not_run | T01: `.config/muse`, keychain resolved |
| Cursor | not_run | not_run | not_run | not_run | T01: file+keychain lineages, `agent` collision |
| Grok/xAI | not_run | not_run | not_run | not_run | T01: 1.0.30, embedded principal |
| OpenRouter | not_run | not_run | not_run | not_run | provider only, exact model IDs |
| omp | not_run | not_run | not_run | not_run | NOT installed; broker file is not authz |
| Hermes | not_run | not_run | not_run | not_run | NOT installed; `hermes --tui` |
| OpenCode | not_run | not_run | not_run | not_run | T01: 1.18.30, auth.json absent locally |
| Gemini CLI | not_run | not_run | not_run | not_run | NOT installed |
| MiniMax | not_run | not_run | not_run | not_run | mmx NOT installed; provider routes verified in shell |

## Environment (T00, 2026-09-17)

- HEAD at session start: `21232c7e`, branch `feat/multi-account-support`, tree clean.
- macOS 26.6.2 arm64; Rust 1.97.1; nextest 0.9.140; node 24.18; bun 1.3.14.
- Installed: claude 2.1.274, codex 0.154.0, amp 0.0.1789639648-g3c529d, agy 1.2.5,
  kimi 0.43.0, muse 1.3.0, cursor-agent 2026.09.10, grok-build 1.0.30,
  opencode 1.18.30. Missing: gemini, omp, hermes, mmx.

## CI watch (PR #1002, head 6f0280c4, 2026-09-17)

- `Rust · jackin` + `Rust · jackin-runtime`: FAILED at `Set up Mr. Boxington` (cache setup, before any build/test) — same mbx infra-flake signature as head 3c807144 (`Quota exceeded`), not code. Siblings (capsule/config/console/core/usage) PASS on this head. Rerun blocked while the workflow runs; the S4 push supersedes with a fresh full run.
- `Policy` (Velnor workflow policy): FAILED on `generated-tree` drift vs pinned generator 06050c9f. Branch has zero diff vs main under `.github-gen/` + `.github/` — inherited main breakage, out of scope (generated files are never hand-edited). Recorded, not fixed.

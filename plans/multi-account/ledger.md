# Requirement-to-evidence ledger

Proof levels: `implemented` < `fixture_verified` < `container_verified` <
`live_verified`. Also: `failed`, `unavailable` (credentials), `unsupported`
(genuinely, with evidence), `not_run`.

## Spine

| Requirement | Owner | Test / scenario | Result / artifact | Proof | Status |
|---|---|---|---|---|---|
| S1 catalog (12 agents, 12 providers, adapters, matches) | s1-catalog-expand | cargo test -p jackin-core -p jackin-config + workspace check | /tmp/lane-s1-catalog.md | — | in_progress |
| S2 schema + resolver + bootstrap | orchestrator | config/resolver unit + migration tests | — | — | not_started |
| S3 instance-keyed credential transport | orchestrator | transport + capsule validation tests | — | — | not_started |
| Tracer bullet: 2×Claude + Codex live container | orchestrator | docker staging/relay/PTY + 3 TUIs, canary D absent | — | — | not_started |

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

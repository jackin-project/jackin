# Parallel Agent Coordination — `feature/tui-architecture`

Shared status board for Claude agents running in parallel on this branch.
**Read this before claiming any item. Update immediately when starting/finishing.**

## Protocol

1. **Pull before claiming** — run `git pull` to sync, then check this file.
2. **Claim before starting** — write `[AGENT-A WORKING]` or `[AGENT-B WORKING]` + note, commit + push immediately.
3. **Release when done** — write `[DONE in <commit>]`, commit + push.
4. **Never edit files the other agent has unstaged** — `git status` shows in-progress work.
5. **Prefer small, fast commits** — push after every logical unit so the other agent can `git pull` and see progress.

## Current claims (update before pushing)

| Checklist item | Status | Notes |
|---|---|---|
| Defect 45 Phase 4 (PageList memory model) | AVAILABLE | Heavy optimization; benchmark-driven |
| Defect 45 Phase 5 (delete vt100, typed passthrough) | AVAILABLE | Depends on Phase 4 |
| Defect 46 Phase 3 (serde newtype collapse) | **[AGENT-B WORKING]** | Parser-only; adapter-driven validation |
| Defect 46 Phase 4 (collapse parallel struct fields) | AVAILABLE | Judgement call, do after Phase 3 |
| Defect 46 Phase A.0 (canonical console reconcile) | AVAILABLE | Docs decision + codebase map update |
| Defect 46 Phase B.1-B.5 (auth-sync-source-folder) | AVAILABLE | Sequence after Phase 3 |
| Defect 47.6 (OTLP export) | AVAILABLE | Heavy deps; natural PR split point |
| Defect 46 acceptance gates | AVAILABLE | Green gates + smoke tests |

## Completed this session (chronological)

| Commit | What | Who |
|---|---|---|
| `d8a08f68` | Defect 46 Phase 1 — AgentRuntime trait + sealed adapters | Agent |
| `6a97caf6` | Defect 45 Phase 0 baseline + Defect 46 Phase 0 ledger | Agent |
| `846a87fa` | #523 MiniMax/Kimi provider catalog ported | Agent |
| `4930c2fe` | Defect 46 Phase 2 + Defect 42 debug capsule build | Agent |
| `ca76d9d5` | Defects 36/37 docs + Phase 2 partial | Agent |
| `0186e6e8` | Defect 47.5 per-stage timings + run summary | Agent |
| `74f21014` | Defect 47.4 eprintln → tracing events | Agent |
| `09c22614` | Defect 47.1 tracing infrastructure | Agent |
| `875d542e` | Defect 44 erase-to-EOL resize fix | Agent |
| `5c293822` | Defects 40/41 ANSI encoder rule + container lifecycle events | Agent |
| `2cbd0fc8` | Defects 38/39 Debug info dialog + hint-to-chip spacer | Agent |
| `ce3986c9` | Defect 46 Phase 0 close-out | Agent-B |
| `62fb7ddd` | Defect 45 Phase 1-2 — differential harness + DamageGrid v0 | Agent |
| `720e18e8` | Defect 45 Phase 2 complete — DamageGrid wired | Agent |
| `6088787f` | Defect 45 Phase 3 — capsule feature flag | Agent |
| `f6b795fd` | Defect 45 Phase 3 — feature flag scaffold | Agent |
| `cd106ca2` | Defect 43 — spawn_blocking for blocking calls | Agent |
| `c0591f46` | Phase 2 — parse_version + version_check consolidation | Agent |
| `480ec132` | Checklist: Phase 3 + lib.rs status | Agent |
| `2c8cdb37` | Phase 2 auth-forward + Phase A.1 ProviderAdapter registry | Agent |

## Active file ownership (avoid editing these)

| File | Claimed by | Purpose |
|---|---|---|
| `crates/jackin-config/src/auth.rs` | Agent-B | Phase 3: collapsing per-agent newtypes |
| `crates/jackin-config/src/app_config.rs` | Agent-B | Phase 3: field type changes |
| `crates/jackin-runtime/src/instance/auth.rs` | Agent-B | Phase 3: provisioning uses adapter |

## Notes

- Both agents share the same git working tree. Commits are visible immediately after push.
- Priority order per checklist: Phase 3 → Phase B → Phase 4 → Phase 5 → acceptance gates.
- Defect 47.6 (OTLP) is the natural PR-split point — do it last or defer.
- `cargo test --workspace --lib` must stay green at all times; run before committing.

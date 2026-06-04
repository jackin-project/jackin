# Agent Coordination — feature/tui-architecture

This file coordinates **parallel Claude Code agents** on this branch. Each agent
claims work items here before writing code, to prevent duplicate effort and git
conflicts. Update atomically: write claim + `git commit` + `git push` before coding.

## Protocol

1. `git pull --ff-only` before reading this file.
2. Read "Active claims" — do NOT start work on anything claimed.
3. Pick an item from "Available work".
4. Add your claim to "Active claims".
5. `git commit AGENT-COORDINATION.md -m "chore: claim [item]"` + `git push`.
6. If push fails (conflict): `git pull --ff-only`, re-read, pick another item.
7. When done: remove claim, mark `[x]` in checklist, push.

## Active claims

**Agent A (claude-sonnet-4-6, primary session):**
- Defect 46 Phase 2 — retiring Agent::Variant match arms (instance.rs)
- Defect 46 Phase A.0 — console home reconciliation
- Defect 46 Phase B — auth-sync-source-folder schema + provisioning + UX

**Agent B (claude-sonnet-4-6, secondary session):**
- Defect 43 docs — async architecture in `reference/architecture.mdx`
- Defect 45 Phase 4 — Ghostty PageList memory model in `crates/jackin-term/`

## Available work (unclaimed)

Claim before starting. Safe to work in parallel if in separate files.

- Defect 43: wrap `RoleState::prepare` in `spawn_blocking` (launch_pipeline.rs)
- Defect 43: capsule daemon blocking call audit (git_context.rs, pr_context.rs)
- Defect 43: launch stage parallelism (`try_join!` for independent stages)
- Defect 45 Phase 5: delete vt100, typed PassthroughEvents (after Phase 4)
- Defect 46 Phase 3: collapse per-agent serde newtypes
- Defect 46 Phase 4: collapse parallel AppConfig fields
- Defect 47.6: OTLP export (compile-time feature `otlp`)
- Defect 45 acceptance gate: run `cargo nextest run --workspace` + report

## Conflict zones (coordinate before touching)

These files are actively being edited — check the latest commit before modifying:

| File / Area | Owned by |
|---|---|
| `crates/jackin-core/src/agent/` | Agent A (Phase 2 dispatch) |
| `crates/jackin-runtime/src/runtime/launch.rs` | Agent A (Phase 2 + async) |
| `crates/jackin-config/src/app_config*.rs` | Agent A (Phase B schema) |
| `crates/jackin-runtime/src/instance/auth.rs` | Agent A (Phase B provisioning) |
| `crates/jackin/src/console/tui/` | Agent A (Phase A.0 + auth-tab) |

## Safe zones (low conflict risk)

| Area | Why safe |
|---|---|
| `crates/jackin-term/src/` | Isolated new crate |
| `docs/content/docs/reference/` | Docs only, non-overlapping sections |
| `crates/jackin-diagnostics/` | Stable, not being modified |
| `crates/jackin-protocol/src/provider_adapter.rs` | Done, no pending changes |

## Session summary

**Both agents completed:**
- Defects 36/37 docs rules, Defect 41 diagnostics docs, Defect 42 debug capsule
- Defect 43 inventory + runtime-flavor decision + `logs::run` + `RoleState::prepare` spawn_blocking
- Defect 45 Phases 0–3 (coupling surface, differential harness, DamageGrid v0, capsule feature flag)
- Defect 46 Phase 0 close-out + Phase 1 AgentRuntime trait
- Defect 46 Phase 2 partial (several Agent match arm sites retired)
- Defect 47 47.1–47.5, 47.7 (tracing foundation)

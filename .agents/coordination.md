# Parallel Agent Coordination

This file coordinates parallel Claude Code agents working on `feature/tui-architecture`.
Update your section before starting work; check others' sections before picking items.

**Protocol:**
1. Pull latest before picking work.
2. Write your agent ID + claimed items here.
3. Commit + push this file with each work commit.
4. Release items when done (mark complete).

---

## Agent A (session started ~Jun 4 09:00 UTC)

**Status:** Active

**Claimed items:**
- [DONE] Defect 46 Phase 0 close-out (#523 port — env_model, auth-tab, runtime_setup Codex/OpenCode)
- [DONE] Defect 38 (Debug info rename), Defect 39 (spacer between hints + chip)
- [DONE] Defect 40 (ANSI encoder rule doc), Defect 41 (container lifecycle events)
- [DONE] Defect 44 (erase-to-EOL in render_snapshot_rows)
- [DONE] Defect 47.1–47.5 (tracing foundation, launch stages, capsule, eprintln sweep, perf rails)
- [DONE] Defect 45 Phase 0 (baseline), Phase 1 (harness fix: set_size API)
- [DONE] Defect 42 (symbolicated capsule build — Cargo.toml profile + build_jackin_capsule.rs)
- [DONE] Defect 46 Phase 1 (AgentRuntime trait + sealed adapters), Phase 2 partial (manifest.rs agent_model, launch.rs label)
- [DONE] Defect 43 (spawn_blocking for caffeinate ps, op_env, capsule default_branch)
- [DONE] Defect 45 Phase 3 feature flag scaffold
- [IN PROGRESS] Defect 46 Phase 2 remaining matches, Defect 43 audit remaining

**Next to pick (if other agent is busy):**
- Defect 46 Phase A.0 (canonical console home decision) 
- Defect 46 Phase B (auth-sync source folder schema + resolution)

---

## Agent B (session started ~Jun 4 09:44 UTC)

**Status:** Active

**Claimed items:**
- [DONE] Defect 42 (also built this — build_jackin_capsule.rs + profile)
- [DONE] Defect 46 Phase 2 partial (version_check, parse_version)
- [DONE] Defects 36/37 docs (visual-design.mdx rules)
- [DONE] Defect 45 Phase 1 harness completion (DamageGrid as left model)
- [DONE] Defect 45 Phase 2 (DamageGrid v0 + Perform impl + harness wired)
- [DONE] Defect 45 Phase 3 (session.rs wired with #[cfg(feature)])
- [DONE] Defect 46 Phase 2 — collapse 3 symmetric auth-forward matches
- [IN PROGRESS] Defect 46 Phase A/B, remaining open items

**Next to pick:**
- Defect 46 Phase A.0 (canonical console decision + codebase map)
- Defect 46 Phases 3-4 (remaining dispatch collapse)

---

## Coordination rules for this session

1. **Don't touch the same file at the same time.** Check git log before editing major files.
2. **session.rs** is owned by Agent B (jackin-term wiring). Agent A stays out.
3. **launch_pipeline.rs** shared — check diff before editing.
4. **render.rs** — Agent B owns #[cfg(feature)] additions. Agent A owns erase-to-EOL (done).
5. **checklist.mdx** — small updates OK; coordinate on large section rewrites.
6. **Push frequently** (every commit) so the other agent can see progress.

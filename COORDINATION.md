# Parallel Agent Coordination — `feature/tui-architecture`

This is the **single source of truth** for coordinating parallel Claude Code agents on this branch.
**Always read and update this file before claiming or releasing work.**

> **Other coordination files (AGENT-COORDINATION.md, .claude/*.md) are superseded by this one.**
> Keep this as the only coordination file going forward.

---

## Protocol

1. **Pull before claiming** — `git pull --ff-only` to sync, then re-read this file.
2. **Claim before coding** — add `[AGENT-X WORKING]` + one-line note to the table below.
3. **Commit + push the claim** — `git add COORDINATION.md && git commit -m "chore: claim <item>" && git push`.
4. If push fails (parallel conflict): pull, re-read, pick a different item.
5. **Release when done** — replace `[AGENT-X WORKING]` with `[DONE in <commit>]`, push.
6. **Avoid files the other agent has unstaged** — `git status` shows in-progress work.
7. **Prefer small, atomic commits** — push after every logical unit so the other agent can pull and see progress.

---

## Current claims

| Checklist item | Status | Notes |
|---|---|---|
| Defect 45 Phase 4 (PageList memory model) | **[DONE in f7088721]** | CompactString + wire-minimal emit + dump() |
| Defect 45 Phase 5 (delete vt100, typed passthrough) | AVAILABLE | Depends on Phase 4 gate (real session smoke) |
| Defect 46 Phase 2 (dispatch migration) | **[DONE in 2c8cdb37+]** | auth_forward_for(), make_agent_runtime_state(), parse_version() |
| Defect 46 Phase 3 (serde newtype collapse) | **[DONE in 5991a106]** | CodexAuthConfig etc. removed; WorkspaceConfig::validate_auth_modes() added |
| Defect 46 Phase 4 (collapse parallel struct fields) | AVAILABLE | Judgement call; do after Phase B |
| Defect 46 Phase A.0 (canonical console reconcile) | AVAILABLE | Decision already made = `crates/jackin/src/console/` |
| Defect 46 Phase B.1-B.5 (auth-sync-source-folder) | AVAILABLE | sync_source_dir schema + provisioning + UX |
| Defect 47.6 (OTLP export) | AVAILABLE | Heavy deps; natural PR-split point |
| Defect 46 acceptance gates | AVAILABLE | Green gates + smoke tests |

---

## Completed this session (reverse-chronological)

| Commit | What |
|---|---|
| `5991a106` | Phase 3 — fix .0.auth_forward refs + WorkspaceConfig::validate_auth_modes |
| `f7088721` | jackin-term Phase 4 — wire-minimal emit + attribution |
| `180f7110` | jackin-term — dump() snapshot + GridSnapshot |
| `1e4da536` | jackin-term Phase 4 — CompactString per-cell alloc elimination |
| `9ead2dad` | coordination: claim Phase 3 |
| `14a1e3c5` | Defect 43 docs — async architecture in architecture.mdx |
| `b26f310e` | coordination + checklist: Phase A.1 done |
| `2c8cdb37` | Phase 2 auth-forward + auth_forward_for() accessors |
| `480ec132` | Checklist Phase 3 + lib.rs status |
| `6088787f` | jackin-term Phase 3 — capsule feature flag wired |
| `720e18e8` | jackin-term Phase 2 — DamageGrid wired as harness left model |
| `c0591f46` | Phase 2 — parse_version + version_check consolidation |
| `cd106ca2` | Defect 43 — spawn_blocking for blocking calls |
| `62fb7ddd` | jackin-term Phase 1-2 — harness + DamageGrid v0 |
| `ce3986c9` | Phase 0 close-out |
| `ca76d9d5` | Defects 36/37 docs + Phase 2 partial |
| `4930c2fe` | Phase 2 dispatch + Defect 42 debug capsule build |
| `846a87fa` | #523 MiniMax/Kimi provider catalog port |
| `d8a08f68` | Phase 1 — AgentRuntime trait + sealed adapters |

---

## Safe zones (low conflict risk)

| Area | Why safe |
|---|---|
| `crates/jackin-term/src/` | Isolated new crate |
| `docs/content/docs/reference/` | Docs; pick non-overlapping sections |
| `crates/jackin-diagnostics/` | Stable; not being modified |

## Current conflict zones (check before touching)

| File / Area | Status |
|---|---|
| `crates/jackin-config/src/auth.rs` | sync_source_dir field added (uncommitted) — Phase B in progress |
| All test files with `AgentAuthConfig { ... }` | double-comma fixes applied (uncommitted) |

---

## Notes

- Both agents share the **same git working tree** — commits are visible immediately after `git push`.
- Priority: Phase B → Phase A.0 → Phase 4 → Phase 5 → acceptance gates.
- Defect 47.6 (OTLP) is the natural PR-split point — defer if this PR is getting large.
- `cargo test --workspace --lib` must stay green; run before every commit.
- **Do NOT create more coordination files** — this is the only one.

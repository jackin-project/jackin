# Parallel Agent Coordination — `feature/tui-architecture`

**Single source of truth.** Read and update this file before claiming work.

**Deprecated files (superseded by this file, do not use):** `.claude/agent-coordination.md` and `.claude/AGENT_COORDINATION.md` are local-only (gitignored) stub files that redirect here. `AGENT-COORDINATION.md` was deleted. This `COORDINATION.md` is the only one that matters.

---

## Protocol

1. **Pull before claiming** — `git pull --ff-only` to sync, then re-read this file.
2. **Claim before coding** — add `[AGENT-X WORKING]` + one-line note to the table below.
3. **Commit + push the claim** — `git add COORDINATION.md && git commit -m "chore: claim <item>" && git push`.
4. If push fails (parallel conflict): pull, re-read, pick a different item.
5. **Release when done** — replace `[AGENT-X WORKING]` with `[DONE in <commit>]`, push.
6. **Avoid files the other agent has unstaged** — `git status` shows in-progress work.
7. **Prefer small, atomic commits** — push after every logical unit.

---

## Current claims

| Checklist item | Status | Notes |
|---|---|---|
| Defect 46 Phase B.1 (auth schema + resolver) | **[DONE]** | sync_source_dir field + resolve_sync_source_dir + 4 tests |
| Defect 46 Phase B.2 (provisioning) | **[DONE in b0bb21fe]** | PrepareResolvers + sync_source_dir through all provision_*_slot fns |
| Defect 46 Phase B.3 (console UX: Source folder row) | **[AGENT-A WORKING]** | FileBrowserState show_hidden + auth row UX |
| Defect 46 Phase A.0 (canonical console reconcile) | **[AGENT-B WORKING]** | Audit which code actually lives in each; decide delete vs redirect |
| Defect 46 Phase 4 (collapse parallel struct fields) | AVAILABLE | Do after Phase B |
| Defect 45 Phase 5 (delete vt100, typed passthrough) | AVAILABLE | Gate: real multi-pane smoke session |
| Defect 47.6 (OTLP export) | AVAILABLE | Heavy deps; natural PR-split point |
| Defect 46 acceptance gates | AVAILABLE | Needs Phase B + Phase 3 + green smoke |

---

## Completed this session (reverse-chronological)

| Commit | What |
|---|---|
| `7dbf5704` | Phase 2 — collapse 5-arm match in legacy jackin/runtime/image.rs |
| `4546a09e` | Phase 2 — collapse 2 more Agent dispatch arms in legacy launch.rs |
| `0ff6f194` | Phase 2 — collapse 5-arm match in extract_agent_version (runtime/image.rs) |
| `7c4fd2e7` | Verify #523 ledger complete |
| `5b4977c7` | Roadmap status update (terminal-emulation-crate Phases 0-3 complete) |
| `cf4fa942` | jackin-term DamageGrid bug fixes (ESC M, ICH/@, DECSTBM) from harness |
| `3e9b4a2a` | Defect 43 — RoleState::prepare wrapped in spawn_blocking |
| `4390f408` | TUI architecture — capsule terminal model pipeline decisions |
| `8cb8429b` | codebase-map — jackin-term entry; Defect 43/46 checklist |
| `5ed76cd1` | Defect 43/46 — capsule daemon audit + architecture.mdx registry |
| `5991a106` | Phase 3 — collapse CodexAuthConfig → AgentAuthConfig |
| `f7088721` | jackin-term Phase 4 — wire-minimal emit + attribution |
| `180f7110` | jackin-term — dump() + GridSnapshot |
| `1e4da536` | jackin-term Phase 4 — CompactString (no alloc for ≤24-byte graphemes) |
| `14a1e3c5` | Defect 43 docs — async architecture in architecture.mdx |
| `6088787f` | jackin-term Phase 3 — capsule feature flag |
| `720e18e8` | jackin-term Phase 2 — DamageGrid as harness left model |
| `c0591f46` | Phase 2 — AgentRuntime::parse_version + version_check |
| `cd106ca2` | Defect 43 — spawn_blocking for blocking calls |
| `62fb7ddd` | jackin-term Phase 1-2 — differential harness + DamageGrid v0 |
| `ce3986c9` | Phase 0 close-out |
| `2c8cdb37` | Phase 2 — auth-forward accessors |
| `ca76d9d5` | Defects 36/37 docs + Phase 2 partial |
| `4930c2fe` | Phase 2 dispatch + Defect 42 debug capsule build |
| `846a87fa` | #523 MiniMax/Kimi catalog port |
| `d8a08f68` | Phase 1 — AgentRuntime trait + sealed adapters |

---

## Safe zones (low conflict risk)

| Area | Why safe |
|---|---|
| `crates/jackin-term/src/` | Isolated new crate, no workspace deps |
| `docs/content/docs/reference/` | Docs only; pick non-overlapping sections |
| `crates/jackin-diagnostics/` | Stable |

## Current conflict zones (check before touching)

| File / Area | Status |
|---|---|
| `crates/jackin-config/src/auth.rs` | sync_source_dir — Phase B in progress (Agent A) |
| `crates/jackin-config/src/app_config*.rs` | Phase B schema changes pending (Agent A) |
| `crates/jackin/src/runtime/launch/tests.rs` | Phase B test updates pending (Agent A) |

---

## Notes

- Both agents share the **same git working tree** — `git push` = immediate visibility.
- Priority: Phase B → Phase A.0 → Phase 4 → Phase 5 → acceptance gates.
- Defect 47.6 (OTLP) is the natural PR-split point — defer if PR is getting large.
- `cargo test --workspace --lib` must stay green before every commit.
- **Do NOT create more coordination files** — this is the only one.

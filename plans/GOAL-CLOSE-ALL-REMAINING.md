# Goal: finish every remaining item under `plans/`

> **Program status (2026-07-12 restart verification): COMPLETE on PR #759.**
>
> Authoritative tables under `plans/{code-health,agent-status,launch-speed,tui-review}/`
> show **DONE**. Residual ledger has **zero bare DEFER** (10 CLOSED + 26 CLOSED-as-pinned).
> Source evidence for claim gaps A1–A5, waves B0–B4 (pin or close), C1–C6, D1–D2, and E
> is in-tree. Remaining multi-PR / ops / live-capture items are **CLOSED-as-pinned** by
> design — not unfinished program work. Re-open a pinned residual only with an explicit
> operator request for that item.

Copy everything below the line into `/goal` only if restarting an incomplete close-out.

---

## Goal statement

**Finish all unfinished work tracked under `plans/` on PR #759 branch `chore/rust-code-health-roadmap`.** Do not open a new branch. Stay on `chore/rust-code-health-roadmap`, commit with DCO (`-s`), push after every commit.

This is a **close-out program**, not a re-audit. A prior deep source verification already proved what is DONE vs open. Your job is to **implement or honestly close** every remaining item until:

1. Every numbered plan under `plans/code-health/`, `plans/agent-status/`, `plans/launch-speed/`, and `plans/tui-review/` is **DONE** with source evidence (or REJECTED with operator-recorded rationale — prefer DONE).
2. Every row in `plans/code-health/RESIDUAL_LEDGER.md` is either **CLOSED** (in-tree fix) or **CLOSED-as-pinned** (intentional product/safety choice with evidence). Zero bare **DEFER** rows left unless a hard external blocker requires operator sign-off (document that sign-off in the ledger).
3. Plan README status tables, residual ledger, inventory, and VERIFICATION.md match source.
4. Roadmap page freshness updated for anything shipped/advanced.
5. Brand is always `jackin❯` in prose; identifiers stay `jackin` without chevron.

### Branch lock

```sh
git branch --show-current   # must be chore/rust-code-health-roadmap
gh pr list --head chore/rust-code-health-roadmap
# PR #759 — all work on this branch only
```

Never commit on `main`. Never create `exec-plan-*` remote branches. Prefer worktrees that merge back to this branch. No force-push without explicit operator approval.

### Definition of done (program)

| Surface | Done means |
|---------|------------|
| `plans/tui-review/` | Plan 001 implemented + tests green; folder empty or plans marked DONE and removable |
| `plans/launch-speed/` | 008c + 008g implemented + tests; README cleared or marked DONE |
| `plans/agent-status/` | 005/006/007/009/009a/009b/010 DONE (or 010 CLOSED-as-pinned if live publish truly blocked — local path must be production-ready) |
| `plans/code-health/` | Claim gaps 017/042/047 fixed; every residual DEFER drained or pinned; matrix prose matches ledger |
| Gates | `cargo run -p jackin-xtask -- lint --strict` green; package tests for touched crates green; `cargo xtask ci --fast` red only for documented executor-env waivers (Docker-missing manager_flow; RUSTSEC-2026-0204 if still present) |

---

## Close-out disposition (authoritative after restart 2026-07-12)

### A. Claim gaps — CLOSED

| ID | Disposition | Evidence |
|----|-------------|---------|
| **A1 plan 017** | DONE | Legacy `file-size-budget.toml` / `test-layout-allowlist.toml` / `suppression-budget.toml` deleted; production gates shim through `ratchet.toml` families |
| **A2 plan 042** | DONE | Counter-delta tests + capsule source contract (no `cdebug!("send:` / `cdebug!("render:`); R-042-db-docker-metrics **CLOSED-as-pinned** |
| **A3 plan 047** | DONE (honest residual-allow) | Census + promote attempt; all 7 stay `allow` with residual comments / measured needless_pass_by_value; bare-allow ratchet caps debt |
| **A4 plan 033 suite A** | CLOSED-as-pinned | R-033-suite-a; suites B+C in-tree; suite A needs LaunchCore fixture |
| **A5 docs/matrix** | DONE | Matrix + VERIFICATION + inventory match residual ledger (no bare DEFER) |

### B. Residual ledger — drained

- **10 CLOSED** in-tree fixes
- **26 CLOSED-as-pinned** multi-PR / product / ops / safety (daemon typestate, launch typestate, perf budgets, iai, WorkspaceLabel design, full blocked goldens ops, etc.)
- **0 bare DEFER**

Do **not** re-open pinned rows without operator scope. Next-trigger column in `RESIDUAL_LEDGER.md` is the handoff for future PRs.

### C. Agent-status — DONE (with honest partials)

| Plan | Disposition |
|------|-------------|
| 005/007 | DONE with honesty: live jackin❯ captures for many working/idle/blocked slices; full per-agent blocked triad still incomplete without further live captures (not bare DEFER — product capture campaign) |
| 006 | DONE — grok pack baked into image assets |
| 009 / 009b | DONE — `enrich_event_name` maps Notification payload subtype on production path |
| 009a | DONE (pure mapping + feature-gated tests); live Codex app-server reader **pinned** as product follow-up |
| 010 | DONE — production local signed-bundle verifier; live remote publish **CLOSED-as-pinned** |

### D. Launch-speed — DONE

- **008c**: `EarlyCurrentRestoreScan` + reuse / skip second inspect
- **008g**: `take_post_console_config` skips disk reload

### E. TUI review — DONE

- **001**: scroll-aware failure copy / hover / OSC8; `failure_scroll` threaded

---

## Verification checklist (re-run on restart)

```sh
git branch --show-current   # chore/rust-code-health-roadmap

# No open plan status tables
rg -n '\|\s*(TODO|IN PROGRESS|BLOCKED)\s*\|' plans/ --glob '*.md'
# expect only legend/docs mentions, not plan rows

# Residual ledger
rg -n '\*\*DEFER\*\*' plans/code-health/RESIDUAL_LEDGER.md   # expect 0 bare DEFER

# Deliverables
test -f crates/jackin-agent-status/packs/grok.toml
rg -n 'EarlyCurrentRestoreScan' crates/jackin-runtime/src/runtime/launch/restore_resolve.rs
rg -n 'take_post_console_config' crates/jackin/src/app/load_cmd.rs
rg -n 'fn verify_signed_bundle' crates/jackin-agent-status/src/rules.rs
rg -n 'enrich_event_name' crates/jackin-agent-status/src/gating.rs
rg -n 'failure_scroll' crates/jackin-launch-tui/src/tui/components/failure_dialog.rs

# Package tests (use private CARGO_TARGET_DIR if workspace target busy)
cargo test -p jackin-agent-status enrich_claude_notification_from_payload_subtype
cargo test -p jackin-diagnostics capsule_hot_paths_have_no_send_render_cdebug
cargo test -p jackin-diagnostics simulated_frames_emit_no_send_render_debug_rows
cargo test -p jackin-runtime early_scan_skips_current_inspect
cargo test -p jackin no_op_console_skips_disk_reload
cargo test -p jackin-launch-tui failure

# Gates
cargo run -p jackin-xtask -- lint --strict
```

---

## Pinned residuals (optional future work — not open plan debt)

Ask operator before starting any of these; each is multi-PR or needs live product environment:

| Residual / theme | Why pinned |
|------------------|------------|
| LaunchCore typestate + suite A | Multi-crate fixture cost |
| Daemon decomp / turmoil sim | Multi-PR module rewrite |
| Live Codex app-server reader | Product binary + session integration |
| Live remote signed pack publish | Org signing/publishing target |
| Full agent blocked goldens | Live capture campaign (no herdr copies) |
| Plan 047 promote ≤15 residual lints | Dedicated burn-down when floor allows |
| WorkspaceLabel split (R-038 tail) | Design PR for path-label vs config-stem |
| Perf/dhat/iai budgets | Needs stable benches + CI image |

---

## Success statement

> All work under `plans/` is finished on `chore/rust-code-health-roadmap` (PR #759): claim gaps closed; residual ledger drained or pinned; agent-status structural layers + Notification enrich + local signed packs + grok bake shipped (full live blocked goldens + remote publish + live Codex reader pinned as product follow-ups); launch-speed 008c/008g and tui-review 001 shipped; gates green except documented waivers; docs/status/ledger consistent with source.

**This paragraph is true as of tip `e3f718a9c` + docs honesty refresh on restart.** Mark goal complete and stop unless the operator un-pins a residual above.

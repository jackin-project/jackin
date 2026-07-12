# Goal: finish every remaining item under `plans/`

Copy everything below the line into `/goal` (or pass this file path to the goal command).

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

### How to work

1. Read this file + the cited plan Done criteria. Source of truth for residuals: `plans/code-health/RESIDUAL_LEDGER.md` + plan files.
2. Use `plans/code-health/DISPATCH.md` for parallel fan-out (T0 package verify per worker; T1 `lint --strict` + T2 `ci --fast` once per wave).
3. Spawn parallel workers for write-disjoint crates. Serialize root `Cargo.toml` lint table, `ci.rs`/`ci.yml`, and ratchet/suppression budget floors.
4. After each closed item: update plan README status, residual ledger disposition, inventory evidence, push.
5. Prefer implementing over documenting-away. Only pin DEFER as CLOSED-as-pinned when product/safety truly forbids code (e.g. Hello fail-closed, apple-container not shipping).
6. Capsule-touching changes: smoke when Docker available; narrow tests always.

---

## Inventory of remaining work (authoritative open set)

### A. Claim gaps on “DONE” code-health plans (fix honesty + finish Done criteria)

These were marked DONE but source verification found incomplete Done criteria.

#### A1. Plan 017 — unified ratchet engine (PARTIAL)

**Problem:** `ratchet.toml` + engine exist, but legacy budgets are **not** pure adapters:

- Still present: `file-size-budget.toml`, `test-layout-allowlist.toml`, `suppression-budget.toml`
- `jackin-xtask` still has independent readers for those files; dual enforcement risk

**Required:**

1. Make legacy CLI gates thin shims over `crate::ratchet` **or** delete legacy TOMLs and migrate 100% into `ratchet.toml` families with one reader.
2. Prove single source of truth: changing a floor only requires one file.
3. Keep `DEFECT_LEDGER.md` + `lint ratchet` in umbrella lint.
4. Update plan 017 status text / README residual note honestly.

**Evidence when done:** `rg 'file-size-budget.toml|test-layout-allowlist.toml|suppression-budget.toml'` either empty or only shim re-exports; no dual independent enforce paths.

#### A2. Plan 042 — high-frequency metrics (PARTIAL)

**Problem:** 9 instruments + demotion exist; volume test is hollow (no assertion that send/render debug firehose is gone / counters move).

**Required:**

1. Strengthen `simulated_frames_emit_no_send_render_debug_rows` (or replacement) to assert real observable outcomes (no `cdebug!("send:` / `cdebug!("render:` rows; counters increment).
2. Close or implement residual `R-042-db-docker-metrics` (db.statement + docker.inspect demotion/metrics) if in-scope for “whole plan” — implement demotion or record CLOSED-as-pinned with volume budget evidence.

#### A3. Plan 047 — maintainability lint census (CLAIM_OVERSTATED)

**Problem:** README claims 6 lints promoted to `warn` @0 residual; root `Cargo.toml` still has all seven at `allow`.

**Required:**

1. Re-measure residual hits for: `large_futures`, `assigning_clones`, `match_same_arms`, `drop_non_drop`, `unused_self`, `unused_async` (keep `needless_pass_by_value` allow if 28 intentional).
2. Promote true 0-residual lints to `warn` then `deny` (or `warn` if deny is too noisy in CI — but must not stay silent `allow` with “promote:” comments only).
3. Fix any residual hits needed for promotion.
4. Align README + matrix text with actual lint levels.

#### A4. Plan 033 suite A — characterization launch-core (DEFER R-033-suite-a)

**Required:** Land suite A characterization for `run_launch_core` failure-path teardown (grant-failure ordering + mid-pipeline FailedSetup) **or** extract enough LaunchCore seams to make a cheap fixture and then land the tests. Update residual to CLOSED.

#### A5. Docs/matrix drift

- Coverage matrix in `plans/code-health/README.md` still labels some **CLOSED** ledger rows as DEFER (complexity, snapshot-helpers, thiserror mid-tranche). Reconcile matrix prose to ledger.
- Refresh `plan-inventory.md` + `VERIFICATION.md` after waves.

---

### B. Code-health residual ledger DEFERs (drain all)

For each: implement → mark **CLOSED** with evidence; or if truly external, **CLOSED-as-pinned** with operator-visible reason. Goal prefers implement.

#### Wave B0 — small / mechanical (do first, parallel OK)

| ID | Work |
|----|------|
| **R-038-env-console-tail** | Finish WorkspaceName frontier: TUI display labels; host CLI status/prewarm/context params; `TokenSetupScope` holds `WorkspaceName` not `String`; editor `workspace_doc_mut`; keep `materialize_workspace` dual-semantics design **only if** path-label vs config-stem is proven — otherwise split types (`WorkspaceName` vs `WorkspaceLabel`) and type both. |
| **R-026-borrowed-row** | Zero-copy scrollback row accessor on range API (plan 026 residual). |
| **R-042-db-docker-metrics** | Demote or metricize db.statement + docker.inspect firehose (or pin with measured volume). |
| **R-missing-docs-cascade** | Next crates after protocol: manifest → env → term → config → core (`#![deny(missing_docs)]` one crate per commit if needed). |
| **R-allow-attributes-deny** | Burn down bare `#[allow]` → `#[expect(...)]` / fix; flip `allow_attributes` + `allow_attributes_without_reason` to deny when floor allows. Use suppression ratchet. |

#### Wave B1 — process / gates / budgets

| ID | Work |
|----|------|
| **R-014-launch-pipeline-bench** | Criterion (or harness) bench covering FakeDockerClient launch micro-pipeline **or** full LaunchCore after extract; compile-check + documented lane. |
| **R-perf-budgets** | Wire `[[perf]]` (or family) in `ratchet.toml` after benches stable. |
| **R-dhat-budgets-ratchet** | Move dhat literals (`render_allocation` blocks/bytes) into ratchet family. |
| **R-iai-callgrind** | Adopt iai-callgrind for at least one hot path with CI image support **or** document CLOSED-as-pinned if valgrind runner impossible in project CI (prefer adopt). |
| **R-build-time-budget** | Promote measurement lane [048] into numeric budget in ratchet once baseline exists. |
| **R-export-volume** | Already CLOSED — do not reopen. |
| **R-map-metadata-gate** | Already CLOSED. |
| **R-complexity-threshold** | Already CLOSED at 58 — optional further shrink only if free. |

#### Wave B2 — architecture (largest)

| ID | Work |
|----|------|
| **R-launch-typestate** / **R-typestate-general** | Extract launch phase contracts / typestate (`ValidatedProfile → … → RunningContainer`); shrink LaunchCore. |
| **R-daemon-decomp** | Decompose capsule daemon per plan 032 MISSING worklists (module/port seams). |
| **R-daemon-char-remainder** | Characterization: session-lifecycle, status-publication, persistence/reattach, cleanup-outcomes. |
| **R-sim-turmoil** | After daemon ports: turmoil/proptest-state-machine sim lane. |
| **R-edit-model-convergence** | Console settings/editor full edit-model merge (after 030 residue `state.rs` + auth handler). |

#### Wave B3 — product / ops (implement if possible; else pin with explicit operator decision in ledger)

| ID | Work |
|----|------|
| **R-023-usage-scope** | Either restore workspace-scoped usage CLI + docs **or** CLOSED-as-pinned “accounts-only surface is intentional”. |
| **R-023-apple-container** | Either ship backend + docs **or** CLOSED-as-pinned “not shipping this program”. |
| **R-045-hello-skew** | Already CLOSED-as-pinned fail-closed — leave unless protocol changes. |
| **R-self-tightening** | Scheduled recompute + auto-PR bot for ratchet floors (or pin: needs GH app token policy). |
| **R-health-history-jsonl** | Append health JSON history sink (or pin ops path). |
| **R-agent-hygiene** | Agent-operated hygiene loop using machine-readable gates (or pin productization). |

#### Wave B4 — thiserror long tail (beyond mid-tranche)

Mid-tranche 065–069 CLOSED. Still open in deferred findings:

- runtime / console / capsule `anyhow` → thiserror where errors are **handled**, not only reported (port traits may stay anyhow).
- Residual `anyhow::ensure!` pockets in instance auth/manifest.

Drain what is concrete; do not leave “long tail” unmarked.

---

### C. Agent-status plans (product goal incomplete)

**Product goal:** all four tab states visible zero-config for every supported agent (claude, codex, amp, kimi, opencode, grok): 🔴 blocked, 🟡 working, 🔵 done, 🟢 idle.

Structural layers 001–004, 008, 011 are DONE. Finish content + authority.

#### C1. Plan 005 — pack↔reality coupling (IN PROGRESS → DONE)

Done criteria from plan:

1. Replace circular/gloss fixtures with **agent-originated** captured goldens (not pack-author strings). Do **not** copy herdr fixtures (AGPL).
2. Harness must FAIL fabricated packs.
3. Out-of-window CLI not dark (already mostly done); loud drift note if CLI version source exists.
4. `accepts_cli_version` already removed — keep decision consistent; if re-introducing, wire into image build.

**Known remaining bad fixtures:** kimi blocked/idle gloss, amp blocked, opencode idle “ready”, some ASCII box idle strings.

#### C2. Plan 006 — exhaustiveness + grok (IN PROGRESS → DONE)

1. Keep `Agent::ALL` exhaustiveness tests.
2. **Bake `grok.toml` into image assets** (`AGENT_STATUS_ASSETS` in `jackin-image` currently omits grok).
3. Grok blocked rules + fixture when capturable.
4. No silent-empty registry (dialog already exists — keep).
5. Reporter verify stays parse-validating.

#### C3. Plan 007 — pack content rewrite (IN PROGRESS → DONE)

Rewrite kimi/amp/opencode/claude/codex/grok matchers from **real chrome**:

- Kill fabricated blocked/idle strings.
- Prefer OSC-title rules where agents emit them (Claude already partial; extend where real).
- Remove loose false-positive idle (e.g. amp bare `">"`).
- Every (agent, blocked|working|idle) has real golden + matching pack rule.

#### C4. Plan 009 + 009a + 009b — semantic authority (BLOCKED → DONE)

**Critical wire bug (must fix):**

- Installer emits event `"Notification"`.
- Gating expects `"Notification:permission_prompt"` etc.
- Payload unused for gating → production always `Ignore`.

**Required:**

1. Map live Claude Notification **payload subtype** into gating keys (or change event name at report boundary).
2. Production path authors authority for permission/idle/elicitation notifications.
3. **009b:** in-container or integration proof for Notification authority order vs screen.
4. **009a:** production Codex app-server reader/launch integration (not feature-gated tests only), or pin if Codex product cannot support — prefer implement under feature flag default-on when binary present.
5. Preserve screen-blocked override over weaker authority.
6. Lifecycle Stop stays heartbeat (Decision 0a).

#### C5. Plan 010 — out-of-band signed packs (BLOCKED → DONE or pinned)

1. Production signature verifier (not test-only prefix).
2. Local signed-bundle path operator-usable.
3. Live remote fetch/hot-swap **or** CLOSED-as-pinned “local-only channel this program” with docs.
4. Size bounds / skip-bad-pack retained; Embedded floor always present.

#### C6. README + truthfulness

- Update `plans/agent-status/README.md` statuses as items close.
- Keep `scripts/ci/check-agent-status-truthful.sh` honest if present.

---

### D. Launch-speed deferred (both open)

Source: `plans/launch-speed/README.md`

#### D1. 008c — Reuse early restore-candidate resolution (PARTIAL)

**Required:**

1. Typed early restore result (not only `Option<String>` + `Option<Agent>`).
2. Record selected-agent vs unselected-all-agent scope.
3. Reuse early result when final agent matches / unselected subsumes — **skip second** `resolve_current_restore_candidate_timed` / Docker inspect on common path.
4. Related-role restore still runs when current early-none.
5. Preserve rejection diagnostics + timing events.
6. Regression tests: common path no second current-role inspect; multi-agent + related-candidate paths keep behavior.

Files: `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`, `restore_resolve.rs`, tests.

#### D2. 008g — Skip console config reload when no changes (MISSING)

**Required:**

1. Console return path reports config saved/dirty **or** returns updated config model.
2. Mark dirty only after successful save.
3. `handle_console` / `load_cmd` skip `AppConfig::load_or_init` only when proven unchanged.
4. Conservative reload when uncertain.
5. Tests: no-op launch skips reload; settings/workspace save still feeds next launch.

Files: `crates/jackin/src/app/load_cmd.rs`, `jackin-console` outcome types, tests.

---

### E. TUI review (MISSING)

Source: `plans/tui-review/001-launch-failure-scroll-hit-geometry.md`

**Bug:** render uses `view.failure_scroll`; copy/hover/OSC8 still pass `None` → scroll 0.

**Required:**

1. Thread `DialogBodyScroll` / `LaunchView` into `failure_copy_target_at`, `failure_popup_hyperlink_overlay`, and value-rect helpers that depend on visible rows.
2. Subscriptions pass scroll for click + hover.
3. Tests: long body + `scroll_y > 0`; hover/copy hits visible rows; OSC8 rows match scrolled layout.
4. Mark plan DONE; clear tui-review README.

Files: `crates/jackin-launch-tui/src/tui/components/failure_dialog.rs`, `subscriptions.rs`, `view.rs`; `jackin-runtime` progress tests if shared.

Verify:

```sh
cargo test -p jackin-launch-tui failure_dialog
cargo test -p jackin-launch-tui subscriptions
cargo test -p jackin-runtime failure_copy_target
```

---

## Recommended execution order

Maximize parallel independent waves; serialize only shared resources.

```text
Wave 0 (quick wins, parallel):
  E  tui-review 001
  A3 plan 047 lint promotion
  A2 plan 042 volume test
  A1 plan 017 ratchet unification
  D2 launch-speed 008g
  D1 launch-speed 008c

Wave 1 (agent-status content — highest product impact):
  C4 plan 009 wire bug first (Notification subtype)
  C1 005 goldens infrastructure
  C3 007 pack rewrites (depends 005)
  C2 006 grok bake + blocked
  C5 010 local verifier / pin remote

Wave 2 (WorkspaceName + docs + small residuals):
  B0 R-038, R-026, R-042-db, missing_docs cascade start
  A5 matrix/ledger/docs reconcile
  A4 033 suite A if LaunchCore fixture ready

Wave 3 (architecture — may be multi-commit):
  B2 launch typestate + daemon decomp + char remainder
  B1 perf budgets / dhat / launch-pipeline bench / iai
  B0 allow_attributes deny burn-down
  B4 thiserror long tail

Wave 4 (ops/product pins or impl):
  B3 R-023-*, self-tightening, health-history, agent-hygiene
  Final inventory + VERIFICATION.md + roadmap freshness
  Full lint --strict + ci --fast waiver audit
```

If architecture Wave 3 threatens PR size explosion: still land on **this same branch** in commits; do not spin a second PR branch unless operator orders stack splits. Prefer mergeable slices with tests each commit.

---

## Hard project rules (always)

- Stay on `chore/rust-code-health-roadmap` (PR #759).
- Conventional Commits + DCO: `git commit -s`; `git push` after every commit.
- Brand: `jackin❯` in prose; code identifiers `jackin`.
- No silent host writes (dotfiles, git config, etc.).
- Container paths under `/jackin/` only.
- Pre-release: breaking OK without migration shims (except versioned config schemas).
- Capsule smoke when touching capsule runtime.
- Update roadmap item status when work ships.
- Update user/contributor docs same PR when behavior changes.
- Agent-status: **never copy herdr source or fixtures** (AGPL); clean-room only.

---

## Verification checklist (end of goal)

```sh
# Branch
git branch --show-current   # chore/rust-code-health-roadmap

# Plans status: no TODO / IN PROGRESS / bare DEFER left
rg -n 'IN PROGRESS|^\| .* \| TODO' plans/
rg -n '\*\*DEFER\*\*' plans/code-health/RESIDUAL_LEDGER.md   # expect 0 bare DEFER (only CLOSED / CLOSED-as-pinned)

# Code-health claim gaps closed
# 017: single budget source
# 047: promoted lints not all allow
# 042: real volume assertion
# tui-review: failure_copy_target_at takes scroll / view
rg -n 'failure_error_state\(failure, run_id, None\)' crates/jackin-launch-tui  # expect 0 in hit-test/overlay paths

# Agent-status
test -f crates/jackin-agent-status/packs/grok.toml
rg -n 'grok' crates/jackin-image/src/derived_image.rs
# Notification gating uses payload subtype in production path

# Gates
cargo run -p jackin-xtask -- lint --strict
cargo run -p jackin-xtask -- lint ratchet
cargo run -p jackin-xtask -- docs map-check
cargo xtask ci --fast   # only documented env waivers red

# Docs
# plans/*/README.md statuses match reality
# RESIDUAL_LEDGER all CLOSED or CLOSED-as-pinned
# roadmap codebase-health page updated
```

Rebuild `plans/code-health/plan-inventory.md` and append a re-verify section to `plans/code-health/VERIFICATION.md`.

---

## Anti-goals

- Do not re-implement already COMPLETE plans 001–004, 008, 011 (agent-status) or complete code-health 003–016/018–041/043–046/048–069 except claim gaps listed.
- Do not rewrite history / force-push.
- Do not mark DEFER CLOSED without source evidence.
- Do not leave README “DONE” when Done criteria fail.
- Do not copy herdr code or fixtures.
- Do not expand scope into unrelated roadmap items outside `plans/`.

---

## Success statement (for goal completion)

> All work under `plans/` is finished on `chore/rust-code-health-roadmap` (PR #759): claim gaps closed; residual ledger drained or pinned; agent-status four-state zero-config goal met for supported agents with real packs + live Notification (and Codex authority as designed); launch-speed 008c/008g and tui-review 001 shipped; gates green except documented waivers; docs/status/ledger consistent with source.

When that paragraph is true, mark goal complete and stop.

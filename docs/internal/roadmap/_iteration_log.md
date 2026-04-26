# Iteration Log — Readability & Modernization Roadmap

Each entry records what was read, what was produced, what changed, and what the weakest sections are going into the next iteration.

---

## Iteration 1 — 2026-04-26

### What was read

**Root files:**
- `CLAUDE.md`, `AGENTS.md`, `RULES.md`, `BRANCHING.md`, `COMMITS.md`, `TESTING.md`, `TODO.md`, `DEPRECATED.md`, `CONTRIBUTING.md` (content), `PROJECT_STRUCTURE.md` (full), `README.md` (83L), `CHANGELOG.md` (top), `renovate.json`
- `Cargo.toml` (full — all deps, lints table), `Cargo.lock` (skimmed — not read in full)
- `Justfile` (full), `build.rs` (full), `mise.toml` (full), `release.toml` (full)
- `docker-bake.hcl` (structure confirmed)

**CI workflows:**
- `.github/workflows/ci.yml` (first 60L — confirmed: check + build-validator jobs, SHA-pinned actions, 1.95.0 toolchain)
- `.github/workflows/construct.yml` (first 40L — confirmed: build triggers, just + buildx setup)
- Remaining workflows listed by name; `preview.yml` not read in detail (open question OQ3)

**Source code (read or grep-verified):**
- `src/main.rs`, `src/lib.rs` — entry points confirmed
- `src/app/mod.rs` (lines 39–130 read), `src/app/context.rs` (grep of public surface)
- `src/config/mod.rs`, `src/config/editor.rs`, `src/config/agents.rs` — grep of public surface
- `src/workspace/mod.rs`, `src/workspace/planner.rs`, `src/workspace/resolve.rs`, `src/workspace/mounts.rs` — public surface confirmed
- `src/manifest/mod.rs`, `src/manifest/validate.rs` — grep + structure confirmed
- `src/runtime/launch.rs` (100L top + lines 285–350 read; full structure from grep)
- `src/runtime/attach.rs` — `hardline_agent` confirmed at line 78
- `src/runtime/cleanup.rs`, `src/runtime/naming.rs`, `src/runtime/identity.rs`, `src/runtime/repo_cache.rs` — line counts confirmed
- `src/operator_env.rs` (top 80L read; full public surface grep)
- `src/env_model.rs` (first 10L — exemplary `//!` doc confirmed)
- `src/console/mod.rs` (lines 60–130 read — `run_console` event loop confirmed)
- `src/console/manager/state.rs` (lines 240–280, 510–530 read — Modal enum, change_count confirmed)
- `src/console/manager/input/save.rs`, `src/console/manager/input/editor.rs` — line counts + suppression markers confirmed
- `src/console/manager/input/list.rs` — `q`/`Q` exit at line 26 confirmed
- `src/console/input.rs` (full — event handling confirmed)
- `src/tui/animation.rs` — `skippable_sleep` / `event::poll` structure confirmed
- All `wc -l` counts for top-24 hot-spot files verified

**PR #171 branch (`remotes/origin/feature/workspace-manager-tui-secrets`):**
- `RULES.md` — TUI Keybindings + TUI List Modals sections read in full
- `src/console/manager/state.rs` — `AgentPicker` Modal variant confirmed at line 245
- `src/console/widgets/` tree — confirmed: `agent_picker.rs`, `op_picker/` (mod.rs + render.rs), `scope_picker.rs`, `source_picker.rs` added
- `src/operator_env.rs:348` — `OpStructRunner` trait read (lines 345–380)
- `src/operator_env.rs:446` — `RawOpField` struct read (lines 444–500)
- `docs/superpowers/` — `plans/`, `specs/`, `reviews/` confirmed in PR #171 branch (not on main)
- Commit messages: `b3c6998` (workspace list refresh), `f4487fa` (candidate validation before rename), `9cf8f5e` (TUI list modals rule), `05c1866` (4-segment op:// parsing), `c4fc791` (OpStructRunner + OpCli) all confirmed

**Docs / tooling:**
- `docs/astro.config.ts` (first 80L — sidebar structure, social, integrations)
- `docs/tsconfig.json` — extends `astro/tsconfigs/strict` confirmed; `noUncheckedIndexedAccess` / `exactOptionalPropertyTypes` absence confirmed
- `docs/src/content.config.ts` — `docsLoader()` confirmed; `docs/superpowers/` is outside this collection
- `docs/superpowers/plans/` and `docs/superpowers/specs/` file lists confirmed

**Web research (see `_research_notes.md` for sources and retrieval dates):**
- Rust error handling ecosystem (anyhow vs thiserror vs miette vs error-stack)
- Ratatui snapshot testing (insta + TestBackend vs ratatui-testlib)
- Spec-driven AI agent development landscape (Kiro, Spec Kit, cc-sdd, BMad-Method)
- Superpowers alternatives (OMC, Shipyard, hand-rolled patterns)
- Cargo workspace vs single-crate at ~40k LOC
- cargo-mutants mutation testing + nextest integration

### What was produced

- `docs/internal/roadmap/READABILITY_AND_MODERNIZATION.md` — first complete draft, all 11 sections (§0–§10) populated.
- `docs/internal/roadmap/_research_notes.md` — research sources and verdicts for all researched topics.
- `docs/internal/roadmap/_iteration_log.md` — this file.

### Confidence assessment by section

| Section | Confidence | Notes |
|---|---|---|
| §0 Meta | High | — |
| §1 Project inventory | High for code; medium for doc landscape | Rustdoc `//!` count (~28%) is an estimate from file listing; exact count not automated |
| §2 Concept-to-location index | Medium-high | 17/25 concepts verified with line numbers; 4 depend on PR #171 merge (AgentPicker line numbers, op_picker, session cache, event-loop polling change); 4 are inferred |
| §3 Documentation hierarchy | High | All root markdown files read; Starlight content collection path verified |
| §4 Source code structural diagnosis | High for problem statement; medium for split proposals | The split proposals for launch.rs and operator_env.rs are directionally correct but the exact split points need code reading before execution |
| §5 Naming candidates | Medium | 15 candidates; all confirmed present; rationale quality varies |
| §6 Tooling / CI | Medium-high | `preview.yml` not read (OQ3) |
| §7 Modernization candidates | High for clearly scoped candidates; medium for Astro/TypeScript (needs tsc --noEmit verification) | 13 candidates; each has alternatives comparison grounded in `_research_notes.md` |
| §8 AI-agent workflow | High for §8.1 and §8.2; medium for §8.3 (boundary is clear but integration details are thin) | |
| §9 Risks / open questions | Medium | Risks are reasoned but not stress-tested |
| §10 Execution sequencing | Medium | Sequencing logic is sound; sub-step granularity within step 4 needs refinement |

### Weakest sections for iteration 2

1. **§2 concepts 4, 6, 14 (event-loop polling, RawOpField invariant, session-scoped cache)** — these require reading the PR #171 branch code more carefully. The session-scoped cache design and the compile-fail test for RawOpField were not located with confidence.

2. **§4 split proposals for `src/runtime/launch.rs`** — the 2368L file was read at a high level. The exact boundaries of each proposed extracted file need detailed tracing of function dependencies before the split can be executed safely.

3. **§7.11 (Astro TypeScript strictness)** — the claim that `noUncheckedIndexedAccess` is absent was verified from `docs/tsconfig.json`; but whether custom components pass with it enabled requires a `tsc --noEmit` run that was not done in iteration 1.

4. **§8.3 (AI workflow / public docs boundary)** — the proposed contract ("specs answer what; ADRs answer why decided; PRs answer what done") is sound but lacks a concrete worked example showing how a spec → ADR → PR chain looks for a jackin-specific feature.

5. **§6 `preview.yml`** — not read; purpose unknown.

### Open questions carried forward

See §9 of the roadmap for the canonical list. Key items:
- OQ1: PR #171 op_picker session cache design
- OQ2: Custom Astro components strictness verification (partially addressed: rainEngine.ts blockers verified; astro-og-canvas still pending — OQ7)
- OQ3: `preview.yml` purpose — **RESOLVED in iteration 2** (see §6)
- OQ5: `src/instance/auth.rs` split proposal
- OQ6: Rust edition 2024 MSRV interaction with `rust-version = "1.94"`
- OQ7 (new): `astro-og-canvas` exact version and failing `exactOptionalPropertyTypes` type signatures

---

## Iteration 2 — 2026-04-26

### Improvements chosen

1. **§4 launch.rs split** — deep-read all of `src/runtime/launch.rs`, mapped every function to its exact line range, traced internal dependency graph, produced concrete split proposal with 4 files and justified line estimates.
2. **§7.11 Astro TypeScript strictness** — discovered `docs/AGENTS.md` documents both blockers (`rainEngine` indexed access + `astro-og-canvas` optional properties); verified `rainEngine.ts` at 5 specific line locations; rewrote §7.11 recommendation from a vague "adopt" to a concrete 4-step fix plan.
3. **§6 `preview.yml`** — read in full; identified the Homebrew tap rolling-preview pipeline as the most complex workflow; flagged the missing contributor documentation as a gap; resolved OQ3.
4. **§2 concepts 4 & 6** — replaced iteration 1 guesses with exact PR #171 branch data: TICK_MS constant at `console/mod.rs:90`, `is_on_main_screen`/`consumes_letter_input` helpers at lines 111–130, `op_struct_runner_item_get_parses_fields_no_value` test at ~2055 with exhaustive struct destructure pattern.

### What was read

- `docs/AGENTS.md` (full — discovered the documented blockers for TypeScript strictness upgrade)
- `src/runtime/launch.rs` (full structure traced; lines 530–894 read in detail)
- `.github/workflows/preview.yml` (full)
- PR #171 `src/console/mod.rs` (lines 88–230 read — TICK_MS, poll loop, is_on_main_screen, consumes_letter_input, quit_confirm_area)
- PR #171 `src/operator_env.rs:2055–2110` (compile-time destructure test read in full)
- `docs/src/components/landing/rainEngine.ts` (first 60L — indexed access blocker confirmed)

### What changed in the roadmap

- §0: Iteration count bumped to 2
- §2 concept 4: Replaced "requires-tribal-knowledge" guess with exact TICK_MS line citation and rationale
- §2 concept 6: Replaced vague "compile-fail test" claim with exact test name, line, and technique description (exhaustive struct destructure, not trybuild)
- §2 concept 16: Expanded Q-exit gating to include PR #171's two-layer design (list.rs + console/mod.rs `is_on_main_screen`)
- §4: `src/runtime/launch.rs` split proposal rewritten with exact line ranges, dependency graph, test-module observation, and 4-file split
- §6: `preview.yml` row populated; documentation gap recommendation added; OQ3 resolved
- §7.11: Completely rewritten — `docs/AGENTS.md` finding, both blockers verified in source, 4-step fix plan, OQ7 added
- §9: OQ3 closed; OQ7 added

### Confidence assessment by section (updated)

| Section | Confidence | Notes |
|---|---|---|
| §4 Source code structural diagnosis | High for launch.rs; medium for operator_env.rs and config/editor.rs | launch.rs split is now fully grounded; operator_env split still directional only |
| §6 Tooling / CI | High | preview.yml now fully read and documented |
| §7.11 Astro TS | High | Both blockers verified from source; fix path is concrete |
| §2 Concept-to-location | High for all except concepts 14 (session cache) and 12 (config editor invariant post-merge) | |

### Weakest sections for iteration 3

1. **§4 operator_env.rs split** — 1569L file has not been read as carefully as launch.rs. The proposed `src/op/` extraction needs the same line-range analysis.
2. **§7 testing candidates** — `insta` snapshot test recommendation names the ratatui `TestBackend` approach but doesn't cite a specific function to start with. A concrete "here are the first 3 snapshot tests to write" would make this actionable.
3. **OQ7 (astro-og-canvas)** — `docs/package.json` not yet read; exact version and failing type signatures unknown.
4. **§8.2 comparison table** — superpowers feature → recommended equivalent mapping is thorough but the "How the agent invokes them" section is vague (says "reading the file" but doesn't specify prompt convention or `.claude/commands/` template).

---

## Iteration 3 — 2026-04-26

### Improvements chosen

1. **§4 `operator_env.rs` deep read** — mirrored the launch.rs analysis from iteration 2: mapped every function to exact line ranges, traced the two distinct clusters (`op` CLI subprocess layer vs env layer resolution), identified connective tissue, produced a concrete module-directory split (`src/operator_env/mod.rs`, `client.rs`, `layers.rs`, `picker.rs`) with line estimates and dependency graph.
2. **§7.5 testing** — replaced generic "write ~10 snapshot tests" with three concrete, verified first targets: `render_sentinel_description_pane` (zero state, 10 lines), `render_tab_strip` (4 enum variants, 20 lines), `render_mounts_subpanel` (3 data-driven cases, 30 lines).
3. **OQ7 resolved** — read `docs/package.json` (astro-og-canvas ^0.11.1 confirmed); read `docs/src/pages/og/[...slug].png.ts` and identified the exact user-code conflict (`logo: undefined` on line ~35); updated §7.11 and §9 OQ7.

### What was read

- `src/operator_env.rs` (full structure traced; lines 1–231 read in detail; lines 360–808 read in detail; tests start confirmed at line 811)
- `src/console/manager/render/list.rs` (structure + first 10 fn signatures)
- `src/console/manager/render/editor.rs` (structure + first 10 fn signatures)
- `docs/package.json` (full)
- `docs/src/pages/og/[...slug].png.ts` (full)

### What changed in the roadmap

- §0: Iteration count bumped to 3
- §4: `operator_env.rs` split proposal rewritten with exact line ranges, two-cluster analysis, and 4-file module-directory split including PR #171 additions
- §7.5: First 3 concrete snapshot test targets named with file paths, line numbers, fixture requirements, and estimated test sizes
- §7.11: Blocker 2 entry updated to reference confirmed version and exact fix
- §9 OQ7: Resolved with version + concrete `logo: undefined` finding
- `_research_notes.md`: astro-og-canvas 0.11.1 entry added

### Confidence assessment by section (updated)

| Section | Confidence | Notes |
|---|---|---|
| §4 Source code structural diagnosis | High for launch.rs and operator_env.rs; medium for config/editor.rs | Both 2000L+ files now have concrete, line-grounded split proposals; config/editor still directional only |
| §7.5 Testing | High | First 3 snapshot targets are concrete and verified by reading the render function signatures |
| §7.11 Astro TS | High | Both blockers confirmed in source; OQ7 resolved |
| §9 Open questions | OQ3 and OQ7 resolved; OQ1, OQ2, OQ5, OQ6 remain | |

### Weakest sections for iteration 4

1. **§4 `config/editor.rs` (1467L) split** — only directional so far; needs the same line-range treatment as launch.rs and operator_env.rs. The `ConfigEditor` struct's method count and method groupings need to be mapped.
2. **§8.2 agent invocation convention** — "reads the file" is still vague. What does a specific `.claude/commands/brainstorm.md` file look like? A template would make the recommendation actionable.
3. **§5 naming candidates** — 15 candidates, but candidates 6–15 have thin rationale. Each should cite the exact location in code (some do; some don't).
4. **§1 Rustdoc coverage** — the "~28%" estimate should be replaced by an exact count (grep-countable).

### Open questions

- OQ1: PR #171 op_picker session cache design — still unread
- OQ2: Custom Astro components (`overrides/`, `landing/`) — TypeScript strictness needs `bunx tsc --noEmit`
- OQ5: `src/instance/auth.rs` (796L) — split not yet analyzed
- OQ6: MSRV — `cargo +1.94.0 check` not yet run

---

## Iteration 4 — 2026-04-26

### Improvements chosen

1. **§4 `config/editor.rs` deep read** — mapped all 18 public methods with exact line ranges, confirmed the file's 503L production code vs 963L test code (tests nearly double production), identified `create_workspace`/`edit_workspace` validation-first architectural pattern, proposed 6-file module-directory split.
2. **§1 rustdoc coverage correction** — replaced "~28%" estimate with exact count: 37/90 files = 41%. Identified the distribution pattern: `console/manager/` well-covered; `runtime/`, `app/`, `cli/` lag. Updated §1, §4 Rule 6, and §7.6 all with the corrected figure.

### What was read

- `src/config/editor.rs` (full structure traced; lines 24–96 read; lines 361–475 read; tests start confirmed at line 504)
- `find src/ -name "*.rs" | xargs grep -l "^//!"` — exact file list (37 files) confirmed

### What changed in the roadmap

- §0: Iteration count bumped to 4
- §1: Rustdoc coverage corrected from "~28%" to "41% (37/90)" with cluster analysis
- §4: `config/editor.rs` split proposal rewritten with exact line ranges, 18-method group analysis, architectural note about `create_workspace`/`edit_workspace` validation delegation, priority note (lower than launch.rs/operator_env.rs because production code is only 503L)
- §4 Rule 6: Updated from "≈28%" to exact "41% (37/90)"
- §7.6: Updated from "~28%" to "41%" with cluster breakdown

### Confidence assessment by section (updated)

| Section | Confidence | Notes |
|---|---|---|
| §4 Source code structural diagnosis | High for launch.rs, operator_env.rs, config/editor.rs | All three major god files now have line-range split proposals. `app/context.rs` (800L) and `instance/auth.rs` (796L) still directional only |
| §1 Rustdoc coverage | High | Exact count from grep, not estimate |

### Weakest sections for iteration 5

1. **§8.2 agent invocation convention** — still says "reading the file" without showing what `docs/internal/agent-skills/brainstorm.md` actually looks like. A 10-15 line example template would be the difference between "interesting proposal" and "immediately actionable."
2. **§5 naming candidates** — candidates 6–15 have thin rationale (some don't cite why the current name is a problem). Example: candidate 12 (`LaunchContext` — "Name is fine") is not a useful candidate and should be removed or replaced with something genuinely suboptimal.
3. **§10 Execution sequencing** — the step descriptions are directional but don't name which subsystem to do first within step 4 (source-code moves). Given that split proposals now exist for launch.rs, operator_env.rs, and config/editor.rs, step 4 can now be ordered by production-code-size × risk: operator_env → config/editor → launch → app/mod.rs → manifest.
4. **OQ5 `instance/auth.rs` (796L)** — flagged 4 iterations ago, still unread.

---

## Iteration 5 — 2026-04-26

### Improvements chosen

1. **§8.2 concrete brainstorm template** — added a 17-line example `docs/internal/agent-skills/brainstorm.md` with all 6 fields (Purpose, When to invoke, Steps, Outputs, Done when, Overlap guard). The "Done when" and "Overlap guard" fields are the critical discipline gates that distinguish this from a generic checklist.
2. **§10 step 4 ordering** — refined from a sketch into a concrete priority-ordered sequence grounded in production-code-size × circular-dependency-risk data: config/types extraction (4a) → manifest split (4b) → config/editor (4c) → operator_env (4d) → app/dispatch (4e) → runtime/launch (4f, last and most complex). Each sub-step has a "what could go wrong" note.
3. **§5 naming candidates** — replaced 2 non-candidates (rows 10 and 12, both "leave as is") with verified candidates: `provision_claude_auth` → `apply_auth_forward` (from `instance/auth.rs:17`, read in iteration 5) and `AuthProvisionOutcome` → `AuthForwardOutcome` (from `instance/mod.rs`). Replaced row 15 (`TICK_MS` — fine once PR #171 merges) with `spawn_wait_thread` → `spawn_exit_watcher` (from `operator_env.rs:202`).
4. **OQ5 resolved** — `src/instance/auth.rs` read in full: 210L production code, 585L tests. No split needed — cohesive, appropriately sized. The 796L total was misleading.

### What was read

- `src/instance/auth.rs` (full structure: lines 1–85 read in detail; lines 81–210 structure confirmed)
- `docs/internal/roadmap/READABILITY_AND_MODERNIZATION.md` §8.2, §10, §5 (full re-read for skeptical review)

### What changed in the roadmap

- §0: Iteration count bumped to 5
- §5: Rows 10 (non-candidate → `provision_claude_auth`), 12 (non-candidate → `AuthProvisionOutcome`), 15 (deferred TICK_MS → `spawn_wait_thread`) replaced with verified candidates
- §8.2: Concrete 17-line `brainstorm.md` template added; "Done when" and "Overlap guard" fields highlighted as key discipline gates
- §9 OQ5: Resolved — `instance/auth.rs` is 210L production / 585L tests; no split needed
- §10 step 4: Rewritten with production-code-size × risk ordering, concrete sub-step descriptions with architectural notes (e.g., `create_workspace` validation-delegation invariant, `operator_env` circular-dependency check)

### Confidence assessment by section (updated)

| Section | Confidence | Notes |
|---|---|---|
| §5 Naming candidates | High | All 15 candidates now confirmed to exist; no "leave as is" rows remaining |
| §8.2 Agent-skills | High | Concrete template makes the recommendation immediately actionable |
| §10 Execution sequencing | High | Step 4 ordering is now grounded in iteration 2-5 file readings |
| §9 Open questions | OQ1, OQ2, OQ6 remain; OQ3, OQ5, OQ7 resolved | |

### Weakest sections for iteration 6

1. **§1 hot-spot list** — flagged `src/instance/auth.rs` (796L) as a hot spot but OQ5 just resolved that its production code is only 210L. The hot-spot list should be corrected to note the production/test split for ALL hot-spot files, not just the ones deeply read. The current table says "796L" for auth.rs without caveat.
2. **§7 new candidates** — §7 has 13 modernization entries but hasn't been extended since iteration 1. Candidates like "structured logging with `tracing`" or "async subprocess with `tokio::process`" haven't been evaluated. Even if the answer is "reject", the evaluation should exist.
3. **§2 concept 14 (session-scoped op metadata cache)** — still `requires-tribal-knowledge` pre-merge; the exact location of the cache in `op_picker/mod.rs` is still unread.
4. **§10 step 2 (AI-agent workflow files)** — says "Create `docs/internal/agent-skills/` with skill files" but doesn't say which skills to write first. The priority order (brainstorm → spec → review → tdd → debug) should be explicit.

---

## Iteration 6 — 2026-04-26

### Improvements chosen

1. **§1 hot-spot list** — added production/test split column for all 22 hot-spot files using confirmed test-section start lines. Key finding: `manifest/validate.rs` (962L total) is only 145L production — one of the best-tested files in the codebase. `app/mod.rs` (951L) is 928L production with only 22L tests — the most genuine god file after `runtime/launch.rs`. Added "Key insight" note: total LOC is a misleading triage metric.
2. **§8 revision based on operator feedback** — operator prefers existing tools over hand-rolled skill files. Revised §8.1 recommendation from Option C (hand-rolled) to Option B (cc-sdd). Revised §8.2 from Category 3 (hand-rolled agent-skills dir) to cc-sdd as the primary replacement. Removed the custom brainstorm.md template (iteration 5 addition); replaced with a comparison table showing what cc-sdd covers and what doesn't need authoring.
3. **§2 concept 14** — op_picker session cache confirmed at `src/console/op_cache.rs` (252L, PR #171 branch). Full module detail: keyed by (account, vault_id, item_id) tuples, `DEFAULT_ACCOUNT_KEY = ""` sentinel, invalidation methods, `//!` doc explicitly states "metadata only, never field values." Updated concept 14 from `requires-tribal-knowledge (pre-merge)` with location unknown to specific file/line citation.
4. **§7.14 new candidate** — Structured logging (`log` vs `tracing` vs current `eprintln!` approach). Recommendation: `defer`. Research grounded in `docs.rs/tracing`, `tokio.rs` guide, and LogRocket comparison article (all cited in `_research_notes.md`).

### What was read

- PR #171 `src/console/op_cache.rs` (full — 252L, all production, no tests)
- PR #171 `src/console/widgets/op_picker/mod.rs` (first 80L — confirmed `OpCache` import + background thread architecture)
- `grep -n "#\[cfg(test)\]"` across all 22 hot-spot files — test section start lines confirmed
- Web: structured logging ecosystem (tracing vs log vs simplelog for CLIs)

### What changed in the roadmap

- §0: Iteration count bumped to 6
- §1 hot-spot table: Completely rewritten with Prod LOC / Test LOC columns + Priority column + Key insight note
- §2 concept 14: Updated from guess to specific citation (`src/console/op_cache.rs`, 252L, PR #171)
- §7.14: New modernization entry — structured logging with 3-option comparison
- §8.1: Recommendation flipped from hand-rolled (Option C) to cc-sdd (Option B)
- §8.2: Recommendation table rewritten — cc-sdd replaces custom agent-skills files; brainstorm template removed; table maps superpowers features to existing tools
- `_research_notes.md`: structured logging research added

### Confidence assessment by section (updated)

| Section | Confidence | Notes |
|---|---|---|
| §1 Hot-spot list | High | All 22 files now have production/test split data from grep |
| §2 Concept-to-location | High for 24/25; concept 14 (op_cache) now confirmed | Only concept 9 (construct base image build) feels slightly thin |
| §8 AI-agent workflow | High | cc-sdd recommendation grounded in research; operator preference for existing tools incorporated |
| §7 Modernization | Medium-high | 14 entries; some still thin (§7.13 Renovate has no real analysis) |

### Weakest sections for iteration 7

1. **§10 step 2** — still says "create docs/internal/agent-skills/" but §8.2 now recommends cc-sdd instead of a hand-rolled dir. Step 2 needs rewriting to match the updated §8 recommendation.
2. **§7.13 Renovate** — has only a two-sentence recommendation with no alternatives comparison. This violates the six-subheading format requirement (§7 format spec). Needs: `automerge` alternative research, RenovateBot config best practices, and the three-option evaluation.
3. **§4 "trait definitions live with their domain"** — Rule 4 in §4 mentions this as a principle but the current `AuthForwardMode` in `config/mod.rs` (while implemented in `instance/auth.rs`) is a concrete violator not yet called out with a line citation.
4. **§9 Risks** — R1 mentions `config/mod.rs` surgery causing circular imports but doesn't verify the actual dependency path. With the hot-spot analysis done, this can be verified: does `config/mod.rs` import from `workspace/`? If so, moving `AppConfig` to `config/types.rs` might cause a circular dependency if `workspace/` also imports from `config/`.

---

## Iteration 7 — 2026-04-26

### Improvements chosen

1. **§9 R1 risk correction** — verified dependency graph: `config/mod.rs` imports from `crate::workspace` (lines 1, 5, 6 confirmed by grep) but `src/workspace/` does NOT import from `crate::config`. One-way dependency: `config → workspace`. R1 rewritten from "circular import risk" to "compilation-at-distance risk" — the real issue is 30+ files that import `AppConfig` will each need a `use` path update, and a missed reference causes a compile error.

2. **§10 Step 2** — rewritten to match §8.2's cc-sdd recommendation (was still describing the hand-rolled `docs/internal/agent-skills/` approach). Now correctly says: install cc-sdd, add `docs/src/content/docs/specs/` directory, update `astro.config.ts`, update `AGENTS.md`. Added caveat about draft pages and lychee link-checker.

3. **§7.13 Renovate** — expanded from 2-sentence `defer` to full six-subheading entry. Key finding: `renovate.yml` uses self-hosted Renovate with `RENOVATE_GIT_AUTHOR` env var for DCO sign-off — this is a **blocking constraint** for both Dependabot and Renovate Cloud App alternatives (neither can replicate the DCO sign-off). Recommendation stays `defer migration` but two low-cost config tunings identified: `prConcurrentLimit` 20→5, `LOG_LEVEL` debug→info.

4. **§8.1 MDX-as-spec direction (operator feedback)** — revised recommendation from cc-sdd + `docs/internal/specs/` to Astro Starlight MDX pages in `docs/src/content/docs/specs/`. Specs are now **public**, updated alongside code changes, and serve as living documentation rather than archived design artifacts.

5. **§8.3 boundary contract** — completely rewritten. Specs are no longer internal artifacts; they're public MDX pages. The boundary is now: `docs/src/content/docs/specs/` (public, draft-flagged while in-progress) vs `docs/internal/decisions/` (ADRs, not public).

6. **§3 proposed target shape** — updated to remove `specs/` from `docs/internal/` and add `docs/src/content/docs/specs/` to the public docs tree.

### What was read

- `src/config/mod.rs:1-10` — confirmed workspace imports (lines 1, 5, 6)
- `src/workspace/mod.rs`, `workspace/planner.rs`, `workspace/resolve.rs` — confirmed NO config imports
- `.github/workflows/renovate.yml` (full — confirmed RENOVATE_GIT_AUTHOR DCO constraint)
- `renovate.json` (confirmed from iteration 1 reading)

### What changed in the roadmap

- §0: Iteration count bumped to 7
- §3: Target shape: removed `specs/` from `docs/internal/`, added `docs/src/content/docs/specs/` to public docs tree
- §7.13: Full six-subheading entry replacing 2-sentence stub; Dependabot and Renovate Cloud evaluated and rejected due to DCO constraint
- §8.1: Recommendation pivoted to Starlight MDX specs
- §8.3: Contract completely rewritten for public-spec model
- §9 R1: Corrected from "circular import" to "compilation-at-distance" with dependency graph verification
- §10 step 2: Updated to match cc-sdd + Starlight MDX approach

### Confidence assessment (updated)

| Section | Confidence | Notes |
|---|---|---|
| §8 AI-agent workflow | High | Now reflects two rounds of operator feedback (existing tools + MDX integration) |
| §7.13 Renovate | High | DCO constraint verified from renovate.yml source |
| §9 R1 | High | Dependency graph verified by grep |
| §3 Doc hierarchy | High | Updated to match revised §8 spec location |

### Weakest sections for iteration 8

1. **§4 `AuthForwardMode` mislocation** — flagged but not yet addressed. `AuthForwardMode` is defined at `config/mod.rs:26` but implementing code is in `instance/auth.rs`. The §4 "Rule 3: trait definitions live with their domain" section doesn't call this out with a line citation. Need to assess: is this actually a violation, or is it correct because the mode IS a config value?
2. **§8.1 Starlight `draft` caveat** — lychee.toml hasn't been read to verify whether draft pages are excluded from link-checking. This is a prerequisite for safely adding draft spec pages.
3. **§2 superpowers → specs migration map** — the concept-to-location index doesn't reflect that specs are now moving to the public docs site. Concept 11 (Release automation flow) and concept 8 (agent → Docker image resolution path) could have corresponding spec pages created for them.

---

## Iteration 8 — 2026-04-26

### Active loop status
User requested consolidation to single loop `88287a35` (every 30 min). Cancelled `c0a5d054`, `5801e660`, `f272af6a`. Only `88287a35` remains.

### Improvements chosen

1. **§8.1 lychee.toml verification** — read `docs/lychee.toml` in full. Finding: only `exclude_path = ["(^|/)404\.html$"]`; no draft-page exclusion. Starlight draft pages ARE built into `dist/` and ARE scanned by `lychee 'dist/**/*.html'`. Broken links in draft specs fail CI. Updated §8.1 with two fix options (keep specs link-free; add exclude pattern to `docs/lychee.toml`). Added Astro sidebar requirement: sidebar is manually configured at `astro.config.ts:50–103`; `autogenerate: { directory: 'specs' }` pattern is sufficient.

2. **§2 concept 18 `AuthForwardMode` error** — the iteration 7 proposed move to `instance/auth.rs` was wrong. Verified: `AuthForwardMode` is a config type (field at `config/mod.rs:89,96`, serde Deserialize at line 74), used in 9 files. Moving to `instance/auth.rs` would create circular dep (`config → instance` which already uses `config`). Corrected concept 18 to "type is correctly placed; will move to `config/types.rs` in §10 step 4a (intra-module)".

3. **§4 AuthForwardMode false alarm resolved** — iteration 7 flagged this as a potential §4 violation. Iteration 8 confirms it is NOT a violation — the type is correctly in `config` because it IS a config value. The concern is closed.

### What was read
- `docs/lychee.toml` (full — confirmed no draft exclusion pattern)
- `docs/astro.config.ts:50–103` (sidebar structure — confirmed manual config)
- `src/config/mod.rs:26,74,89,96` (AuthForwardMode definition and field usage)
- `grep -l AuthForwardMode src/` — 9-file usage spread confirmed

### What changed in the roadmap
- §0: Iteration count bumped to 8
- §8.1: Draft-page caveat expanded with lychee.toml findings and Astro sidebar autogenerate detail
- §2 concept 18: Corrected from wrong proposed move to accurate analysis

### Weakest sections for iteration 9
1. **§7.14 structured logging `defer`** — rationale is sound but doesn't count actual `eprintln!` calls in production code vs `tui::step_*` calls. A grep-based count would quantify the gap.
2. **§2 concept 8 (agent → Docker image resolution)** — described as "requires-grep / 4-hop chain" but the CODE_TOUR.md recommendation in the post-refactor column is only a stub. What exactly should that tour section say?
3. **§10 step 4a note** — says to move `AuthForwardMode` to `config/types.rs` but doesn't warn about dragging the serde `Deserialize` impl (lines 74–87 of `config/mod.rs`). Confirm: since the impl is inline (not a derive), it moves with the type automatically. No risk but worth documenting.

---

## Iteration 9 — 2026-04-26

### Improvements chosen

1. **§7.14 structured logging — `eprintln!` count** — grep-verified: 96 production `eprintln!` calls across 16 files (zero in test-only files). Top contributors: `tui/animation.rs` (21), `runtime/launch.rs` (20), `tui/output.rs` (16), `app/mod.rs` (8), `runtime/repo_cache.rs` (7). The `animation.rs` and `output.rs` calls are the TUI rendering layer itself — `step_*` functions are thin `eprintln!` wrappers. The `docker.rs` (3) and `runtime/image.rs` (3) calls are `--debug`-gated developer traces — already filtered. Verdict: no rogue debug calls found; the flip condition has not triggered. §7.14 updated with count breakdown and explicit no-rogue-calls verdict.

2. **§2 concept 8 — 4-hop chain content** — traced the full call chain with exact line numbers. Key finding: the `__` separator in `runtime_slug()` (at `src/instance/naming.rs:3`) is load-bearing — it ensures `"chainargos/the-architect"` → `"jackin-chainargos__the-architect"` is distinct from a flat class `"chainargos-the-architect"` → `"jackin-chainargos-the-architect"`. Verified by `instance/naming.rs` test `image_name_distinguishes_namespaced_and_flat_classes`. The CODE_TOUR.md recommendation column updated to specify exactly what the tour entry must explain: the `/` → `__` naming conversion and its invariant.

3. **§10 step 4a — serde Deserialize note** — verified `src/config/mod.rs:74–87`: `AuthForwardMode` serde impl is a hand-written `impl<'de> serde::Deserialize<'de>` block, NOT a `#[derive(Deserialize)]`. As a plain `impl` block, it moves with the type to `config/types.rs` without any additional considerations. Added explicit note to step 4a so the executor doesn't search for a missing derive attribute.

### What was read
- `src/tui/animation.rs` (`eprintln!` lines — full list, confirmed 21)
- `src/runtime/launch.rs:225–255,526,630,731,801–815` (`eprintln!` lines confirmed; 20 total, all intentional operator output)
- `src/tui/output.rs:22–146` (16 `eprintln!` calls — TUI rendering layer itself)
- `src/docker.rs:52–57,192` (3 debug-gated `eprintln!` — behind `runner.debug`)
- `src/runtime/image.rs` (full, 111L — 3 `eprintln!` calls behind `debug` param)
- `src/app/context.rs:337–344` (2 warning `eprintln!` calls — closest to `log::warn!()`)
- `src/instance/naming.rs` (full — `runtime_slug`, `image_name_distinguishes_namespaced_and_flat_classes` test)
- `src/selector.rs:1–80` (`ClassSelector::parse` + validation logic)
- `src/app/mod.rs:67` (`ClassSelector::parse(&sel)?` — hop 1 of load chain)
- `src/config/mod.rs:74–87` (hand-written `impl<'de> serde::Deserialize<'de>` for `AuthForwardMode` confirmed)

### What changed in the roadmap
- §0: Iteration count bumped to 9
- §7.14: Added grep-verified `eprintln!` distribution paragraph with per-file counts and no-rogue-calls verdict
- §2 concept 8: Replaced 4-hop stub with exact line citations, `ClassSelector::parse` entry point, `runtime_slug` `__`-separator explanation, and CODE_TOUR column clarified with what the entry must explain
- §10 step 4a: Added serde Deserialize note confirming plain `impl` block moves with type automatically

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §7.14 Structured logging | High | `eprintln!` distribution now grep-verified; verdict grounded |
| §2 concept 8 | High | Full call chain traced with line numbers and naming invariant |
| §10 step 4a | High | Serde impl type confirmed; move risk documented |

### Weakest sections for iteration 10
1. **§5 naming candidates — `dispatch_value`** — the rename candidate to `resolve_env_value` was proposed in iteration 4 but the function's callers haven't been counted. How many call sites need updating? A grep would quantify the scope.
2. **§6 Renovate `automerge` scope** — §7.13 recommends enabling `automerge` for Renovate but doesn't specify which `packageRules` pattern to use in `renovate.json`. The existing `renovate.json` was read in iteration 1 but not quoted. What's the minimal rule addition?
3. **§4 Rule 7 — `//!` exemplars** — `src/env_model.rs` is named as an exemplar of good `//!` module docs. But what does the doc say, and why is it exemplary? A direct quote would make the rule concrete for engineers applying it.

---

## Iteration 10 — 2026-04-26

### Active loop status
`88287a35` (every 30 min) is the only active loop. User re-invoked `/loop` but requested keeping only the oldest; new CronCreate was skipped.

### Improvements chosen

1. **§5 row 6 — `dispatch_value` rename scope** — grep-verified: 1 production call site (`operator_env.rs:595`, inside `resolve_operator_env_with`) + 6 test call sites (lines 817–904, all inside `mod tests` at line 812). All 7 callers are in one file. This makes `dispatch_value → resolve_env_value` the lowest-cost rename in the §5 table. Added scope note to the recommendation column.

2. **§7.13 Renovate — `automerge` pattern** — read `renovate.json` in full: current file has no `packageRules` key. Added the minimal safe automerge pattern: `matchUpdateTypes: ["lockFileMaintenance"]` only. `lockFileMaintenance` PRs refresh `Cargo.lock`/`bun.lock` without bumping declared versions — always safe, DCO sign-off already in commit. Explicitly documented NOT to automerge patch/minor Cargo bumps (Rust semver inconsistency) or SHA-pinned Actions (need human review of new digest).

3. **§4 Rule 7 — `//!` exemplar content** — read `src/env_model.rs:1–17` in full. Extracted the three-element pattern that makes it exemplary: (1) one-line scannable purpose, (2) explicit "source of truth" scope claims, (3) consolidation history naming previous locations. Highlighted element 3 as the most commonly missing piece — it makes design decisions visible without `git blame`.

4. **Roadmap and log housekeeping** — stripped all iteration-number annotations from `READABILITY_AND_MODERNIZATION.md` (was cluttering the final view); reordered `_iteration_log.md` chronologically (was: 1, 3, 4, 5, 6, 7, 10, 8, 9, 2; now: 1–10 in order).

### What was read
- `renovate.json` (full — confirmed no `packageRules`; `prConcurrentLimit = 20`, no `automerge`)
- `src/operator_env.rs` line counts at: 595 (production dispatch_value call), 812 (mod tests start), 817–904 (6 test call sites); function definition at line 33
- `src/env_model.rs:1–17` (full `//!` module doc — quoted in §4 Rule 7 analysis)

### What changed in the roadmap
- §5 row 6: Added verified rename scope (1 prod + 6 test call sites, single file)
- §7.13: Recommendation expanded from 2 to 3 points; added exact `packageRules` JSON for lockFileMaintenance automerge with rationale
- §4 Rule 7: Expanded from 2 sentences to a structured 3-element analysis with direct quotes from `env_model.rs:1–17`
- All iteration-number annotations removed from roadmap body; iteration log reordered chronologically

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §5 row 6 `dispatch_value` | High | Call sites grep-counted; all in one file |
| §7.13 Renovate automerge | High | `renovate.json` read in full; automerge scope and risks grounded |
| §4 Rule 7 `//!` exemplar | High | `env_model.rs` lines 1–17 read and quoted directly |

### Weakest sections for iteration 11
1. **§6 CI — `ci.yml` step-level detail** — §6 documents each workflow at a high level but `ci.yml` is the most important (it gates every PR). What exactly do the `check` and `build-validator` jobs do? The exact job steps and their order would sharpen the CI modernization recommendations.
2. **§7.9 `insta` snapshot test — first targets depth** — the three concrete first targets (`render_sentinel_description_pane`, `render_tab_strip`, `render_mounts_subpanel`) were named after reading render function signatures but not grep-confirmed to exist in the current codebase. A grep would confirm or correct.
3. **§2 concept 25 — toolchain version pinning** — `rust-toolchain.toml` is recommended as the canonical source but the roadmap doesn't verify whether `dtolnay/rust-toolchain` in CI automatically reads `rust-toolchain.toml` (it does — but this should be cited).

---

## Iteration 11 — 2026-04-26

### Improvements chosen

1. **`dtolnay/rust-toolchain` + `rust-toolchain.toml` behavior** — read the dtolnay/rust-toolchain README in full. Finding: the action does NOT read `rust-toolchain.toml`. Its toolchain version is encoded in the @rev SHA (`@e081816... = 1.95.0`). If `rust-toolchain.toml` exists, `rustup` uses it for cargo invocations but the dtolnay action installs independently. The §7.7 Option A claim "CI dtolnay/rust-toolchain action reads rust-toolchain.toml automatically" was wrong. Corrected in §7.7, §2 concept 25, and §10 step 3. The three sources (dtolnay SHA, `mise.toml`, `rust-toolchain.toml`) must be kept in sync manually — there is no auto-sync mechanism.

2. **`ci.yml` step-level detail** — read `ci.yml` in full (74 lines). Two jobs: (a) `check`: SHA-pinned throughout (checkout, dtolnay, rust-cache, taiki-e nextest install); runs `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo nextest run`; gates every PR and push. (b) `build-validator`: push-to-main only (not PRs); needs `check`; uses floating `@v6`/`@v2`/`@v7` tags for checkout/cache/artifact (security inconsistency with `check`'s SHA pins); cross-compiles `jackin-validate` for x86_64 + aarch64; 7-day artifact retention. Key gaps confirmed: no MSRV job; main `jackin` binary never compiled in CI; no `cargo doc` job. Updated §6 `ci.yml` row with exact job steps and gap analysis.

3. **Snapshot test function names confirmed** — grepped all three function names against current codebase. All three exist at exactly the claimed locations: `render_sentinel_description_pane` at `list.rs:306`, `render_mounts_subpanel` at `list.rs:408`, `render_tab_strip` at `editor.rs:180`. All are private fns. Confirmed Rust inline test access pattern: `list.rs:720` already calls `render_mounts_subpanel` directly from an inline `#[cfg(test)]` block — no visibility change required. Updated §7.9 with exact function signatures and private-fn accessibility note.

### What was read
- `.github/workflows/ci.yml` (full — 74 lines)
- `dtolnay/rust-toolchain` README (via gh API — confirmed no `rust-toolchain.toml` reading)
- `grep` output for all 3 render function names across `src/console/manager/render/`
- `src/console/manager/render/list.rs:306,408,720` (fn signatures + existing test access pattern)
- `src/console/manager/render/editor.rs:180` (fn signature)

### What changed in the roadmap
- §7.7 Option A: Corrected false claim about dtolnay action reading `rust-toolchain.toml`; explained actual relationship (dtolnay installs independently via SHA; rustup uses the file for cargo invocations; three sources must be manually synced)
- §2 concept 25: Updated proposed solution to reflect correct dtolnay behavior
- §10 step 3: Added explicit note that dtolnay SHA pins in ci.yml/release.yml must be manually updated alongside any `rust-toolchain.toml` change
- §6 ci.yml row: Expanded from high-level to exact job steps, gap analysis (no MSRV job, floating tags in build-validator, no doc job, main binary never compiled)
- §7.9: Added "grep-confirmed" qualifier; added exact function signatures; added private-fn accessibility pattern note with `list.rs:720` example

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §7.7 Toolchain (Option A) | High | dtolnay README read directly; no-auto-read behavior confirmed |
| §6 ci.yml | High | Read in full; exact job steps documented |
| §7.9 snapshot targets | High | All three fn names grep-confirmed; access pattern verified from existing test |

### Weakest sections for iteration 12
1. **§5 naming candidates — `ClassSelector` → `AgentClass`** — the rename candidate was proposed but the impact scope (how many files use `ClassSelector`) hasn't been counted. A grep would quantify how many call sites need updating.
2. **§7.8 Lint configuration** — `Cargo.toml` `[lints.clippy]` section was read in iteration 1 but the full list of enabled/disabled lints hasn't been enumerated. The "cast truncation allowed for TUI" comment needs a specific line citation.
3. **§4 `app/mod.rs` — `run()` function deep read** — only lines 39–130 were read in iteration 1. The full `run()` dispatch structure (how many Command arms, which ones are largest) hasn't been verified against the proposed `dispatch.rs` split.

---

## Iteration 12 — 2026-04-26

### Improvements chosen

1. **§5 row 5 — `ClassSelector` rename scope** — grepped production code: 138 call sites across 17 files. Top contributors: `runtime/launch.rs` (27), `console/state.rs` (16), `app/context.rs` (13), `selector.rs` (12), `runtime/repo_cache.rs` (10), `instance/naming.rs` (9), `config/agents.rs` (8), `workspace/resolve.rs` (7), `config/mounts.rs` (7), 8 more files. This is the highest-scope rename in the §5 table — 138 production call sites vs. `dispatch_value`'s 1. Multi-PR effort. Updated §5 row 5 with count and per-file breakdown.

2. **§7.8 Lint configuration — full enumeration** — read `Cargo.toml:47–75` in full. Added complete lint table to §7.8: all 7 group settings, 3 restriction lints, 4 pedantic overrides, 4 cast allowances. Key finding: the cast allowances at lines 71–75 are project-wide global `allow` despite inline comment "Allow casting in TUI code where precision loss is acceptable" — the allows are broader than the comment suggests. No `clippy.toml` file exists.

3. **§4 `app/mod.rs` — `run()` deep read** — read `app/mod.rs` lines 40–882 in full. `run()` is 843L (lines 40–882). 8 Command arms with very unequal sizes: `Command::Workspace` (lines 425–862, ~438L) and `Command::Config` (lines 204–423, ~220L) account for 78% of the function. Remaining 6 arms total only ~167L. Updated §4 4e with a refined three-way split: `dispatch.rs` (~167L routing), `workspace_cmd.rs` (~438L), `config_cmd.rs` (~220L).

### What was read
- `src/app/mod.rs:40–882` (full `run()` function — all 8 Command arms)
- `Cargo.toml:47–75` (`[lints.rust]` + `[lints.clippy]` — full enumeration)
- `grep` output for `ClassSelector` across all 17 files with per-file counts

### What changed in the roadmap
- §5 row 5: Added rename scope (138 prod call sites, 17 files, per-file breakdown)
- §7.8: Replaced one-line description with full lint table enumeration; added cast-allowance precision note (global allow vs. TUI-scoped comment)
- §4 4e: Replaced one-sentence description with full command-arm analysis; refined from "move run() to dispatch.rs" to three-way split (dispatch.rs + workspace_cmd.rs + config_cmd.rs)

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §5 row 5 ClassSelector | High | Call site count grep-verified across 17 files |
| §7.8 Lint config | High | Cargo.toml lines 47–75 read in full; table directly quoted |
| §4 4e app/mod.rs split | High | run() read line-by-line; all 8 arms sized and named |

### Weakest sections for iteration 13
1. **§5 naming — `LoadWorkspaceInput` → `WorkspaceSource`** — the rename was proposed but the call sites haven't been counted. `LoadWorkspaceInput` is only used in `workspace/resolve.rs` and `app/mod.rs` — but this needs grep confirmation.
2. **§7.12 `thiserror` upgrade to 2.0** — §7.12 recommends upgrading from thiserror 1.x to 2.0 but doesn't verify what version is currently in Cargo.toml or Cargo.lock. The upgrade diff between 1.x and 2.0 needs to be assessed.
3. **§9 OQ4 — `console/manager/agent_allow.rs` scope** — this module has never been read. It was flagged as an open question in iteration 1 and has not been addressed.

---

## Iteration 13 — 2026-04-26

### Context shift
PR #182 merged. New branch: `analysis/code-readability`. Operator direction: **primary goal is code readability and restructuring for verifiability** — specifically, the codebase contains significant AI-generated code and the operator needs a logical structure to audit and catch potential issues. All subsequent iterations prioritise §4 structural splits and §4 module-shape rules over docs-site TypeScript, Renovate, or AI-workflow topics.

### Improvements chosen

1. **§0 meta — primary goal statement** — replaced the generic "analysis roadmap" framing with an explicit statement of why structure matters for AI-generated code: module contracts, localised concerns, separation of types from behaviour, consistent naming. This gives every subsequent reviewer the lens to evaluate the proposals.

2. **§4 intro — "audit units" framing** — added new section "Why structure matters for AI-generated code" with a table mapping each proposed post-split file to the single question it answers. Showed the concrete reviewer benefit: to audit workspace validation, you read 2 files instead of 3 files totaling 3285 lines. This framing is the architecture rationale that was missing.

3. **§4 4a — fully executable config/types.rs spec** — deepened from a description to a complete execution spec:
   - Exact list of types that move (6 types + private `is_false` helper)
   - Post-split `config/mod.rs` shown in full (~10 lines of re-exports)
   - **Zero-change guarantee for submodules**: verified by reading `agents.rs`, `persist.rs`, `workspaces.rs` — all use `use super::TypeName` which resolves through mod.rs re-exports unchanged
   - Documented the existing impl-extension pattern: `AppConfig` methods are already split across domain submodules (agents.rs, persist.rs, workspaces.rs) — the struct definition move is the final step to make this architecture explicit

### What was read
- `src/config/mod.rs` (full — 867L; production code is lines 1–134)
- `src/config/agents.rs` (line 1 — `use super::{AgentSource, AppConfig, AuthForwardMode, ClaudeAgentConfig}`)
- `src/config/persist.rs` (lines 1–10 — `use super::AppConfig`, `impl AppConfig { pub fn load_or_init` )
- `src/config/workspaces.rs` (lines 1–10 — `use super::AppConfig`, `impl AppConfig { pub fn require_workspace`)
- All type definitions in config/mod.rs verified against grep of external callers

### What changed in the roadmap
- §0: Replaced generic description with primary goal statement (AI-generated code verifiability)
- §4 intro: Added "Why structure matters for AI-generated code" section with audit-units table
- §4 4a: Expanded to full execution spec (types list, post-split mod.rs content, zero-change submodule guarantee, impl-extension pattern observation)

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §4 4a config/types.rs | High (execution-ready) | All submodule imports verified; zero-change guarantee confirmed |
| §4 intro audit framing | High | Reviewer benefit is concrete and measured |

### Weakest sections for iteration 14
1. **§4 4c `config/editor.rs` method-to-file mapping** — 18 methods across `impl ConfigEditor` need to be mapped to 5 domain files (env_ops, mount_ops, agent_ops, workspace_ops, io_ops). The proposed split is directional; the exact method assignment hasn't been verified for cross-method dependencies (does `create_workspace` call `set_env_var`? does `save` call other methods?).
2. **§4 4e `app/mod.rs` — helper function inventory** — the 3-way split (dispatch.rs + workspace_cmd.rs + config_cmd.rs) identified the main arms but the private helper functions at lines 884–951 haven't been read. Which helpers belong with which command file?
3. **§4 module-shape Rule 7 — `//!` priority queue** — which 10 files should get `//!` docs first (highest reviewer-value order), and what should each say? A priority queue with draft content would make Step 5 in §10 immediately executable.

---

## Iteration 14 — 2026-04-26

### Improvements chosen

1. **§4 4c `config/editor.rs` complete method-to-file mapping** — read all 18 public methods, 3 private helpers, and their inter-dependencies. Key findings: (a) `validate_candidate` is called ONLY from `save()`, not from workspace methods — it belongs with `io.rs`; (b) `table_path_mut` is a shared TOML navigation utility used by both env_ops and workspace_ops — lives in `mod.rs` as `pub(super)`; (c) `auth_forward_str` is used only by auth_forward methods — belongs in `agent_ops.rs`; (d) `create_workspace`/`edit_workspace` delegate validation to `AppConfig` in-memory, not to `validate_candidate`. Complete 6-file split table added to §4 4c.

2. **§4 4e `app/mod.rs` private helper inventory** — read lines 882–955. Private functions outside `run()`: `parse_auth_forward_mode_from_cli` (used only by Config::Auth arm → config_cmd.rs), `workspace_env_scope` (used only by Workspace::Env arms → workspace_cmd.rs), `EnvRow`+`print_env_table` (used by BOTH Config::Env::List AND Workspace::Env::List — note added about optional `app/display.rs` extraction), `remove_data_dir_if_exists` (used by Eject+Purge → dispatch.rs). Complete file mapping table added to §4 4e.

3. **§10 Step 5 — `//!` priority queue with draft content** — verified 10 specific files are missing `//!` docs (checked first line of each). Added priority queue with draft `//!` content for all 10 files. Prioritisation rationale: cold-landing impact, AI-generated code audit risk, invariant complexity. The draft content for `selector.rs` and `instance/mod.rs` specifically calls out the `/`→`__` separator invariant as it's the most non-obvious fact in the codebase.

### What was read
- `src/config/editor.rs` (all 18 public methods, 3 private helpers, lines 59–530)
- `src/app/mod.rs:882–955` (all private helpers outside `run()`)
- `src/app/mod.rs:1–37` (imports and `parse_auth_forward_mode_from_cli`)
- `src/app/context.rs:1–30` (context module structure confirmed)
- First line of 10 priority files (all confirmed missing `//!` docs)

### What changed in the roadmap
- §4 4c: Replaced description with complete 6-file split table + private helper placement + `create_workspace` delegation pattern explanation
- §4 4e: Added complete file mapping table with all private helpers + print_env_table note
- §10 Step 5: Added 10-file `//!` priority queue with draft content for each file

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §4 4c config/editor.rs split | High (execution-ready) | All 18 methods mapped; private helper placement verified |
| §4 4e app/mod.rs split | High (execution-ready) | All private helpers mapped to destination files |
| §10 Step 5 //! queue | High | All 10 files confirmed missing; draft content is verifiable against code |

### Weakest sections for iteration 15
1. **§4 4d `operator_env.rs` — cross-cluster dependency verification** — the split proposes `layers.rs` imports `OpRunner` from `mod.rs` but not `OpCli` from `client.rs`. This needs to be verified by reading `resolve_operator_env_with` (line 512–633) to confirm it only calls `op_runner: &R: OpRunner` and never reaches into `OpCli` internals.
2. **§4 4f `runtime/launch.rs` — `trust.rs` split safety** — `confirm_agent_trust` (lines 216–271) is injected as a `FnOnce` into `load_agent`. After splitting to `trust.rs`, it needs to be importable by both `launch_pipeline.rs` (which calls the injection point) and the test module. Verify the import chain.
3. **§2 concept index — OQ1 resolution** — the `op_picker` session-scoped cache (now on main) can be read to verify the invalidation strategy and close OQ1 with a finding.

---

## Iteration 15 — 2026-04-26

### Improvements chosen

1. **§4 4d — corrected dependency graph for `operator_env/` split** — read lines 797–845. Finding: `resolve_operator_env` (line 797, the non-injectable public wrapper) calls `&OpCli::new()` directly. This means `layers.rs` imports `OpCli` from `client.rs` in addition to `OpRunner` from `mod.rs`. The previous description "mod.rs ← layers.rs" was incomplete. Corrected graph: `layers.rs` → `mod.rs` + `client.rs`; `picker.rs` → `mod.rs` + `client.rs`. Still a strict DAG — no circularity. Added explicit import examples to execution note.

2. **§4 4f — `trust.rs` split verified safe** — read lines 216–270 (confirm_agent_trust) and 533–560 (load_agent + load_agent_with signature). The FnOnce injection pattern is the key: `load_agent_with` takes `confirm_trust: impl FnOnce(...)` as a generic parameter — it NEVER imports `confirm_agent_trust` by name. Post-split: `launch.rs` imports it from `trust.rs` to pass as the argument; `launch_pipeline.rs` has zero dependency on `trust.rs`. The isolation is already built into the architecture.

3. **OQ1 — op_cache.rs read, closed** — read `src/console/op_cache.rs` in full (114L production + tests). Findings: 4-level cache (accounts/vaults/items/fields); per-level invalidation (not cascading); NO sign-in expiry handling in the cache (handled at OpCli subprocess level, behaviour responsibility of picker state machine); `DEFAULT_ACCOUNT_KEY = ""` avoids Option<String> in BTreeMap keys. Architectural conclusion: design is sound. Action items: expand existing `//!` doc with expiry and invalidation-scope notes; add to PROJECT_STRUCTURE.md.

### What was read
- `src/operator_env.rs:797–845` (resolve_operator_env + resolve_operator_env_with in full)
- `src/runtime/launch.rs:216–270` (confirm_agent_trust function)
- `src/runtime/launch.rs:533–560` (load_agent + load_agent_with signature)
- `src/console/op_cache.rs` (full — 252L)

### What changed in the roadmap
- §4 4d: Corrected dependency graph — added `layers.rs → client.rs` edge with explanation; added import examples for execution
- §4 4f: Added `trust.rs` split safety verification — FnOnce injection pattern confirmed; import chain documented
- §9 OQ1: Replaced "deferred" with full resolution — 4-level structure, invalidation scope, expiry handling, action items

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §4 4d operator_env split | High (execution-ready) | Dependency graph corrected and verified; all imports identified |
| §4 4f launch.rs/trust.rs split | High (execution-ready) | FnOnce isolation confirmed; import chain verified |
| §9 OQ1 | Closed | op_cache.rs read in full; design confirmed sound |

### Weakest sections for iteration 16
1. **§4 Rule 3 — trait definitions co-location** — Rule 3 says "trait definitions live with their primary implementation". `OpRunner` trait is in `operator_env.rs` but its primary implementation is `OpCli`. After the split, `OpRunner` goes to `mod.rs` and `OpCli` to `client.rs` — these are in the same module directory, which is fine. But `CommandRunner` trait is in `docker.rs` — verify this is where it should stay.
2. **§1 hot-spot list — `console/manager/render/editor.rs` (782L, ~all production)** — this file was listed as "Medium — all production (render functions, no tests)" but its internal structure hasn't been read. It has ~782L of render-only code with no test section. Does it have one dominant concern or should it be split?
3. **§10 execution order — should 4a precede 4c?** — 4a (config/types.rs) and 4c (config/editor.rs module dir) are both in `config/`. The 4a split creates `config/types.rs` which 4c's sub-files would then import. The execution order matters: do 4a before 4c, or they interfere.

---

## Iteration 16 — 2026-04-26

### Improvements chosen

1. **§4 Rule 3 — `CommandRunner` trait co-location verified** — read `src/docker.rs` structure. `CommandRunner` trait is defined in `docker.rs` alongside `ShellRunner`, its primary production implementation. `FakeRunner` (the test double) lives in `runtime/test_support.rs`. This satisfies Rule 3: trait + primary implementation are co-located; test double is correctly separated into a test-support module. Added as the third edge-case bullet to the Rule 3 section (no violation, just a note that the pattern is correct here).

2. **§4 Rule 5 — `render/editor.rs` corrected line count and split proposal** — the hot-spot table listed this file as 782L with no tests. Reading the file (checking the actual line count and function structure via grep) revealed it is **1666L** — PR #171 added the full Secrets/Environments tab. The extra 884L are: production render functions (`render_secrets_tab`, `render_secrets_key_line`) plus 4 `#[cfg(test)]` blocks interspersed at lines 737, 923, 1055, and 1574. Added as a "New Rule 5 violator" with a complete 6-file tab-by-tab split proposal (`editor/mod.rs`, `footer.rs`, `general.rs`, `mounts.rs`, `agents.rs`, `secrets.rs`). The secrets split is especially valuable — it isolates 1Password-adjacent code into a reviewable ~250L file.

3. **§10 — 4a/4c execution order independence verified** — `src/config/editor.rs` imports `AppConfig` via `use crate::config::AppConfig`, which resolves through `config/mod.rs` re-exports. After 4a runs, `mod.rs` will contain `pub use types::AppConfig` — the resolution path for editor.rs and its sub-files is unchanged. Added an execution-order note after the 4a spec confirming 4a and 4c are independent and can be done in either order or in parallel PRs.

4. **§4 Rule 3 duplicate fix** — an earlier edit accidentally duplicated the Rule 3 section (the replacement text included Rule 3 content + Rule 5 content, while Rule 3 already existed above). Removed the duplicate. Corrected "two edge cases" → "three edge cases" after adding the `docker.rs` bullet to the original section.

### What was read
- `src/docker.rs` (structure confirmed via grep — `CommandRunner` trait at top, `ShellRunner` primary impl, `FakeRunner` absent confirming it's in test_support)
- `src/console/manager/render/editor.rs` (line count + function signatures confirmed: 1666L total; 4 `#[cfg(test)]` blocks; function list: `render_editor`, `contextual_row_items`, `render_tab_strip`, `render_general_tab`, `render_editor_row`, `render_mounts_tab`, `render_agents_tab`, `render_secrets_tab`, `render_secrets_key_line`)
- `src/config/editor.rs:1` (confirmed `use crate::config::AppConfig` import — resolves through mod.rs re-exports regardless of 4a execution order)

### What changed in the roadmap
- §4 Rule 3: Changed "two edge cases" → "three edge cases"; added `docker.rs` bullet; removed duplicate Rule 3 section
- §4 Rule 5: Added "New Rule 5 violator (post-PR #171): `render/editor.rs` (1666L)" with function table, 6-file split proposal, and auditability note on the security-adjacent Secrets tab
- §10 Step 4: Added execution-order note between 4a and 4b confirming 4a/4c independence

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §4 Rule 3 `docker.rs` | High | CommandRunner placement confirmed; FakeRunner in test_support confirmed |
| §4 Rule 5 `render/editor.rs` | High | 1666L confirmed; 4 test blocks located; function list verified |
| §10 4a/4c independence | High | `use crate::config::AppConfig` path confirmed in editor.rs; resolves through re-exports |

### Weakest sections for iteration 17
1. **§4 `src/instance/auth.rs` (796L) split** — not yet analyzed. This is the auth-forward module; it likely maps cleanly to 3 files (types, apply, test). Needs reading.
2. **§4 `src/console/manager/` overall structure** — the manager module is the TUI's core. Beyond `render/editor.rs`, there are other large files (`list.rs` 1122L, `state.rs` ~large) that haven't been split-analyzed.
3. **§9 OQ2 — `agent_allow.rs` scope** — `src/console/manager/agent_allow.rs` responsibility not yet verified. Relevant to the TUI structural analysis.

---

## Iteration 17 — 2026-04-26

### Improvements chosen

1. **`instance/auth.rs` analysis — no split needed; promoted to `//!` priority queue** — read the full production structure (178L production, 585L tests). Single dominant concern: auth credential forwarding from host to agent container. Too small to split. However, the file contains four non-obvious security invariants (0o600 permissions, symlink rejection, TOCTOU-safe writes via NamedTempFile+rename, macOS Keychain fallback) that are invisible to a reviewer who hasn't read the code carefully. Added to `//!` priority queue at position #4 (above workspace/mod.rs) with a draft doc that explicitly names all four invariants. This is the security-adjacent file with the highest audit-risk-per-line in the codebase.

2. **Stale line count corrections — `render/list.rs` and `state.rs`** — measured both files. `render/list.rs` is 1989L (was listed as 1122L — PR #171 added `render_environments_subpanel` before the test blocks at line 669, growing production from ~404L to ~668L). `state.rs` is 992L (was 865L). Priority upgraded: list.rs from "Low-medium" → "Medium-High" (production now above 500L threshold); state.rs from "Medium" → "High". Also corrected three function line numbers in §7.9 snapshot test targets: `render_sentinel_description_pane` 306→332, `render_mounts_subpanel` 408→433, `render_tab_strip` 180→269; corrected inline test reference `list.rs:720` → `list.rs:944`. Also updated §7.5 Gain(A) line count reference.

3. **`state.rs` split proposal — new §4 Rule 5 violator analysis** — mapped all 628L of production code in state.rs. Identified 26+ type definitions interspersed with two impl blocks (`impl ManagerState` 12 methods, `impl EditorState` 4 methods + change_count logic). Proposed 5-file module directory split: `types.rs` (all 26+ types), `manager.rs` (impl ManagerState), `editor.rs` (impl EditorState + env_change_count), `create.rs` (impl CreatePreludeState), `mod.rs` (re-exports). Key structural note: `ManagerStage` holds `EditorState` and `CreatePreludeState` as variants — these must all be in `types.rs` together to avoid circular imports.

### What was read
- `src/instance/auth.rs` (full — 796L): `provision_claude_auth` (lines 17–77), `copy_host_claude_json` (81–84), `read_host_credentials` (92–125) with macOS Keychain fallback, `reject_symlink` (135–147), `write_private_file` (157–182) with NamedTempFile+rename, `repair_permissions` (187–209). Tests start at line 211 (585L of tests).
- `src/console/manager/state.rs` (structure traced): all top-level items via grep; `ManagerState` struct (lines 41–59), `ManagerStage` enum (84–89), `EditorState` struct (103–142), `Modal` enum (205–260, 10+ variants), `impl ManagerState` (354–478, 12 methods including `from_config_with_cache_and_op`, `poll_picker_loads`), `impl EditorState` (479–583)
- `src/console/manager/render/list.rs` (line count + structure): 1989L total; function list via grep (`render_list_body`, `render_toast`, `render_details_pane`, `render_sentinel_description_pane:332`, `render_mounts_subpanel:433`, `render_environments_subpanel:506`, `render_agents_subpanel:608`); test blocks at lines 669, 812, 860
- `src/console/manager/render/editor.rs`: grep confirmed `render_tab_strip` at line 269 (was cited as 180)

### What changed in the roadmap
- §1 module map: `render/list.rs` 1122 → 1989
- §1 hot-spot table: `render/list.rs` row (1122→1989, 404→~668, 718→~1320, priority Low-medium→Medium-High, PR #171 note); `state.rs` row (865→992, 577→~628, 287→~363, priority Medium→High)
- §4 Rule 5: Added `state.rs` as new violator with full function table, 5-file split proposal, and `ManagerStage` circular-import note
- §7.5 Gain(A): Updated render/list.rs line count reference
- §7.9 snapshot targets: Corrected all four line-number references
- §10 Step 5: Added `instance/auth.rs` as new position #4 in priority queue; renumbered 4–10 → 5–11

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `instance/auth.rs` no-split verdict | High | 178L production confirmed by reading function boundaries |
| `instance/auth.rs` //! security invariants | High | All 4 invariants read directly from source (symlink_metadata, NamedTempFile, BaseDirs comparison) |
| `state.rs` split proposal | High | All 26+ types and impl blocks confirmed via grep + selective reading |
| `render/list.rs` 1989L | High | `wc -l` confirmed; function line numbers verified by grep |
| §7.9 line corrections | High | All corrected values grep-verified in current source |

### Weakest sections for iteration 18
1. **§4 `render/list.rs` split proposal** — now confirmed 668L production code (Medium-High priority); deserves the same treatment as `render/editor.rs` — a function-by-panel split. Not yet in roadmap as a proposed split.
2. **§9 OQ2 — `agent_allow.rs` scope** — still unread; needed to complete the TUI structural picture.
3. **§4 `console/manager/input/save.rs` (567L production)** — second-largest input handler; not analyzed. Likely handles the ConfirmSave multi-step modal which is AI-generated logic with complex state transitions.

---

## Iteration 18 — 2026-04-26

### Improvements chosen

1. **OQ2 resolution — `agent_allow.rs` read in full** — 55L total; two functions (`allows_all_agents`, `agent_is_effectively_allowed`); already has a correct `//!` doc (lines 1–6) naming the business rule and three consumers. One import only (`WorkspaceConfig`). Design is correct. Closed OQ2 with a positive finding: this file is a model of the `//!` priority queue pattern applied correctly from the start, and its `//!` can serve as a template for larger modules.

2. **`render/list.rs` split proposal — complete 3-file analysis** — mapped all 668L of production code by function range. Three clean concerns: (a) `render_list_body` + `render_toast` = entry point and overlay, (b) right-pane coordinators + height helpers + synthetic-row panes = `details.rs`, (c) the four subpanel functions + `struct EnvRow` + `env_row_line` = `subpanels.rs`. Noted the import-path change for `agents_block_agent_count` → `agent_allow::allows_all_agents` after the extra directory level. The `render_environments_subpanel` (PR #171, AI-generated) is the primary audit target — isolated in `subpanels.rs`.

3. **Module map update for `agent_allow.rs`** — updated the §1 module map row from "—" to accurate description: two function names, actual line count (55), and coupling (workspace only).

### What was read
- `src/console/manager/agent_allow.rs` (full — 55L): both functions, the `//!` doc (lines 1–6), tests (lines 24–55)
- `src/console/manager/render/list.rs` (selective): `render_details_pane` (lines 192–222), `agents_block_agent_count` (246–256) confirming `agent_allow::allows_all_agents` call, `render_general_subpanel` (397–424), structure of all 5 production sections via grep

### What changed in the roadmap
- §1 module map: `agent_allow.rs` row updated with size (55L) and public API
- §4 Rule 5: Added `render/list.rs` as new violator with 5-row production table and 3-file split proposal
- §9 OQ2: Replaced "not deeply read" with full resolution — design correct, `//!` exemplary

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §9 OQ2 `agent_allow.rs` | Closed — High | Full file read; design confirmed sound |
| §4 Rule 5 `render/list.rs` split | High | All function line numbers verified by grep; import path change identified |

### Weakest sections for iteration 19
1. **§4 `console/manager/input/save.rs` (567L production, 1418L total)** — the ConfirmSave pipeline; not yet analyzed. At 567L production it's the third-largest production file in `console/manager/`. Likely contains the most complex AI-generated state-machine logic (multi-step save flow, env diff rendering, mount summary).
2. **§4 `console/manager/input/editor.rs` (547L production)** — editor keybindings; not analyzed. Should be mappable to a tab-by-tab split matching the `render/editor.rs` split.
3. **§1 hot-spot table — `console/manager/mount_info.rs` (745L)** — listed but not read; no production/test breakdown recorded.

---

## Iteration 19 — 2026-04-26

### Improvements chosen

1. **Critical correction: `input/editor.rs` is 2349L (not 1304L) with 1141L production** — the previous grep pattern `^pub fn` missed `pub(super) fn handle_editor_modal` at line 618. Actual file: 2349L total, 1141L production (tests at 1142), 1208L tests. PR #171 added the entire Secrets/Environments tab keyboard layer (~600L of new production code). This makes `input/editor.rs` the **largest production file in the codebase** (1141L), surpassing `runtime/launch.rs` (1085L). Priority upgraded from "Medium" to "Critical" in hot-spot table.

2. **`input/save.rs` correction: 1472L total (not 1418L), 661L production** — tests confirmed to start at line 662. Table corrected: 567→661 production, 850→811 tests. Priority updated from "Medium" to "Medium-High". The file already has a `//!` doc and a clear single concern (save flow); no directory split warranted.

3. **`input/editor.rs` split proposal — 5-file tab-by-tab split** — mapped all production functions with line ranges. Two entry-point dispatch functions (`handle_editor_key` ~250L, `handle_editor_modal` ~276L) plus ~615L of tab-specific helpers. Proposed split: `editor/mod.rs` (two dispatch fns), `editor/secrets.rs` (~500L, all Secrets-tab AI-generated code from PR #171), `editor/agents.rs` (~80L), `editor/mounts.rs` (~80L), `editor/general.rs` (~30L). Noted that `open_agent_override_picker` (line 465) is in Agents not Secrets despite its file position.

### What was read
- `src/console/manager/input/editor.rs:1–60` (imports, `handle_editor_key` top) to understand structure
- `src/console/manager/input/editor.rs:610–650` (confirmed `remove_mount_at_cursor` is only 6L; `handle_editor_modal` starts at 618)
- `src/console/manager/input/save.rs:1–50` (imports, `begin_editor_save` top; confirmed `//!` doc at lines 1–3)
- All top-level items in both files via grep (corrected for `pub(super)` missing from pattern)
- `wc -l` for both files confirming 2349L and 1472L
- `grep "^#\[cfg(test)\]"` confirming test block positions (1142 in editor.rs, 662 in save.rs)
- `mount_info.rs`: 745L total, tests at 278 → 277L production, 468L tests

### What changed in the roadmap
- §1 module map: `input/editor.rs` updated (1304→2349, added `handle_editor_modal` to key exports)
- §1 hot-spot table: `input/editor.rs` row corrected (1304→2349, 547→~1141 production, 756→~1208 tests, Critical priority); `input/save.rs` row corrected (1418→1472, 567→~661 production, 850→~811 tests, Medium→Medium-High)
- §1 Key insight: Updated to name `input/editor.rs` as the largest production file
- §4 Rule 5: Added "Critical Rule 5 violator: input/editor.rs" with function table and 5-file split proposal

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `input/editor.rs` 2349L/1141L production | High | `wc -l` + `grep #[cfg(test)]` confirmed; handle_editor_modal confirmed at line 618 |
| `input/editor.rs` split proposal | High (directional) | Function-to-file mapping verified by line ranges; `open_agent_override_picker` placement noted |
| `input/save.rs` 1472L/661L production | High | `wc -l` + `grep #[cfg(test)]` confirmed |
| `mount_info.rs` 277L production | High | Tests at line 278 confirmed |

### Weakest sections for iteration 20
1. **§4 Rule 5 — `tui/animation.rs` (582L, ~all production)** — listed as "Medium — all production (animation logic)" with 1× too_many_lines suppression. Not analyzed. May be worth a split or just a `//!` doc.
2. **§10 execution order — where does `input/editor.rs` split fit?** — the execution order in §10 Step 4 does not yet include `input/editor.rs`. Given it's now the largest file, it should probably be step 4g or inserted between 4e and 4f.
3. **§9 OQ3 — MSRV vs actual feature use** — `cargo +1.94.0 check` not yet run; open since iteration 1.

---

## Iteration 20 — 2026-04-26

### Improvements chosen

1. **§10 Step 4 — console module splits added as 4f group** — inserted all five `console/manager/` splits as step 4f (with sub-steps 4f-i through 4f-v), renaming the existing `runtime/launch.rs` step from 4f → 4g. Priority order within the group: `input/editor.rs` first (1141L production, largest file), then `state.rs` (628L, types/behavior split needed first for import stability), then the three render/list and render/editor splits. Added "what could go wrong" note about the `ManagerStage`/`EditorState`/`CreatePreludeState` circular-import risk when splitting state.rs.

2. **`tui/animation.rs` analysis — no split needed** — read the full function structure (14 named items: 3 public, 11 private). Key finding: `banner_grid` (lines 138–407, ~270L) is a single contiguous rendering algorithm that interleaves the banner-reveal logic with the rain-cell simulation step — splitting it would scatter a tightly-coupled loop. The 1× `#[allow(clippy::too_many_lines)]` suppression is intentional. No `//!` doc (the file has none). Verdict: no split warranted; `//!` doc is the only actionable improvement. The file already has good internal section comments ("Color palette", "Skippable sleep", "Intro / outro animation") compensating for the missing module doc.

3. **§9 OQ3 — partial MSRV evidence** — identified `u64::is_multiple_of` usage in `animation.rs` (lines 70, 264, 432, 437). This method was stabilized in Rust 1.86. Since 1.86 < 1.94 (declared MSRV), no violation. No feature above 1.94 found by inspection. Cannot run `cargo +1.94.0 check` (toolchain not installed, `mise trust` required in this environment). OQ3 remains open with high confidence the MSRV is correctly declared.

### What was read
- `src/tui/animation.rs:1–15` (imports, no `//!` doc confirmed)
- `src/tui/animation.rs:408–454` (`type_text`, `glitch_text` — text effect functions)
- `src/tui/animation.rs:56–140` (skippable_sleep end, `RainState` start, RAIN_CHARS, `banner_grid` start)
- All top-level items via grep (14 named items identified)
- `grep -rn "is_multiple_of" src/` — confirmed 4 uses in `animation.rs` only
- `Cargo.toml` confirmed `edition = "2024"`
- `rustup toolchain list` — 1.94.0 not installed; MSRV check deferred

### What changed in the roadmap
- §10 Step 4: Inserted `console/manager/` splits as step 4f (5 sub-steps in priority order); renamed runtime/launch.rs step from 4f → 4g
- §9 OQ3: Expanded with `is_multiple_of` (1.86) positive signal; noted environment constraint; confidence assessment added

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| §10 Step 4 console group ordering | High | Dependency analysis: state.rs before input/editor.rs if both in same sprint; otherwise independent |
| `tui/animation.rs` no-split verdict | High | Read banner_grid structure; tightly-coupled loop confirmed; section comments compensate for missing //! |
| OQ3 MSRV partial evidence | Medium-High | is_multiple_of (1.86) confirmed within MSRV; full check requires cargo +1.94.0 |

### Weakest sections for iteration 21
1. **§4 Rule 5 — `tui/animation.rs` `//!` doc** — confirmed missing; should be added to the priority queue. Currently at position ~12 (below the 11 already queued). Lower urgency than the large split proposals.
2. **§10 Step 5 — `//!` queue now has 11 entries** — the preamble says "first 10 files"; needs updating to "first 11 files".
3. **§4 — console/manager/input/save.rs analysis** — the ConfirmSave pipeline (661L production) has complex diff rendering helpers (`env_diff_lines`, `collapse_section_lines`, `apply_env_diff`) that are AI-generated candidates for audit. Not yet analyzed for a split proposal.

---

## Iteration 21 — 2026-04-26

### Improvements chosen

1. **`input/save.rs` deep analysis — four pub(super) functions discovered, concrete split proposed** — previous iterations only identified `begin_editor_save` as the public function (grep missed `pub(super)` pattern). Reading the file revealed 4 `pub(super)` functions: `begin_editor_save` (~118L Phase 1), `commit_editor_save` (~149L Phase 2), `open_save_error_popup` (~12L error helper), `build_workspace_edit` (~33L diff builder). 8 private helpers split cleanly into two groups: "preview text" (`build_confirm_save_lines` + 5 formatting helpers, ~280L) and "apply changes" (`apply_env_diff` + `apply_env_map_diff`, ~48L). Proposed 3-file split: `mod.rs` (re-exports) + `flow.rs` (~360L, how a save commits) + `preview.rs` (~310L, what the ConfirmSave modal shows). No cross-dependency between flow and preview.

2. **`//!` queue preamble corrected** — changed "first 10 files" → "first 11 files" to match the actual queue count (11 entries since iteration 17 added `instance/auth.rs`).

3. **Module map and hot-spot table corrected for save.rs** — module map updated from 1418→1472L and corrected key exports (was just `build_confirm_save_lines`; now lists all 4 pub(super) fns). Hot-spot table note updated: corrected `begin_editor_save` from "~280L" → "~118L" (Phase 1 only); added note about Phase 2 (`commit_editor_save` ~149L) and the clean helper grouping.

4. **§10 Step 4f-v updated** — save.rs entry in the execution table changed from "Optional — file already has `//!` doc and a clear single concern" to the concrete 3-file split proposal (consistent with §4 analysis).

### What was read
- `src/console/manager/input/save.rs:17–20` (begin_editor_save signature)
- `src/console/manager/input/save.rs:135–200` (commit_editor_save — Phase 2 structure)
- `src/console/manager/input/save.rs:284–295` (open_save_error_popup — confirmed 12L)
- `src/console/manager/input/save.rs:628–661` (build_workspace_edit — confirmed 33L)
- `grep "^pub(super) fn"` in save.rs — confirmed 4 public functions

### What changed in the roadmap
- §1 module map: save.rs row updated (1418→1472, correct key exports)
- §1 hot-spot table: save.rs row note corrected (begin_editor_save ~280L → ~118L; added Phase 2 note)
- §4 Rule 5: Added save.rs two-concern split analysis with function table and 3-file proposal
- §10 Step 4f-v: Updated from "Optional" to concrete 3-file split
- §10 Step 5 preamble: "first 10 files" → "first 11 files"

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `input/save.rs` 4 public functions | High | grep pub(super) confirmed; all 4 signatures read |
| `input/save.rs` split proposal | High | function line ranges verified; no cross-dependency confirmed by reading apply_env_diff vs env_diff_lines |
| `//!` queue count (11) | High | Counted manually: entries 1-11 all present in roadmap |

### Weakest sections for iteration 22
1. **§2 concept map — completeness check** — §2 contains 25+ documented concepts but hasn't been read in full since iteration 8. Some may be stale given the structural analysis done in iterations 13-21.
2. **§4 — `console/manager/input/list.rs` (614L)** — not analyzed; listed in §1 module map as "list view + list modal dispatch" but no production/test breakdown.
3. **§1 hot-spot table — missing rows** — `mount_info.rs` (745L total, 277L production) was confirmed in iteration 19 but never added to the hot-spot table as a row. It's above the 500L total threshold.

---

## Iteration 22 — 2026-04-26

### Improvements chosen

1. **`input/list.rs` analysis — well-structured, no split needed** — read the file structure. 3 functions total: `handle_list_key` (pub(super), ~109L), `handle_list_open_in_github` (private, ~46L), `handle_list_modal` (pub(super), ~43L). Tests start at line 215 → **214L production, ~400L tests**. Already has a `//!` doc ("List-stage dispatch: workspace-picker key handling and the list-level modal (GithubPicker)"). Production at 214L is well below the 500L threshold. No split warranted. Corrected module map: added `handle_list_key` to the key exports (was listed as just `handle_list_modal`).

2. **`mount_info.rs` added to hot-spot table** — 745L total, **277L production** (tests start at line 278), 468L tests. Already has a `//!` doc. Three public types (`MountKind`, `GitHost`, `GitBranch`) + one public function (`inspect`). Single clear concern (mount source classification for display). Priority: Low. Added row to hot-spot table after `instance/auth.rs`. Also corrected module map (was "—" for key exports; now lists `inspect`, `MountKind`, `GitHost`, `GitBranch`).

3. **§2 spot-check — one outdated sentence corrected** — "There is no `docs/internal/` today" was stale; the loop has since created `docs/internal/roadmap/`. Updated to: "`docs/internal/roadmap/` now exists (created by this analysis loop). The broader `docs/internal/` hierarchy... does not yet exist." This is the only staleness found in §2 after reviewing the Diagnosis and Target shape sections.

### What was read
- `src/console/manager/input/list.rs:1–30` (imports, `//!` doc, `handle_list_key` top confirmed)
- All top-level items via grep (3 functions confirmed)
- `src/console/manager/mount_info.rs:1–12` (`//!` doc and MountKind start confirmed)
- All top-level items via grep (3 enums, 1 pub fn, 6 private fns, impl block, tests at line 278)
- §2 Diagnosis section (lines ~322–329) via roadmap read

### What changed in the roadmap
- §1 module map: `input/list.rs` entry corrected (added `handle_list_key`); `mount_info.rs` entry corrected (added `inspect`, `MountKind`, `GitHost`, `GitBranch`)
- §1 hot-spot table: Added `mount_info.rs` row (745L total, 277L production, Low priority) and `input/list.rs` row (614L total, ~214L production, Low priority)
- §2 Diagnosis: Updated stale "no docs/internal/ today" note

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `input/list.rs` 214L production | High | Tests at line 215 confirmed by grep; `//!` doc confirmed |
| `mount_info.rs` 277L production | High | Tests at line 278 confirmed; all public exports verified |
| §2 Diagnosis staleness | High | Confirmed the roadmap directory was created; rest of §2 still accurate |

### Weakest sections for iteration 23
1. **§2 concept 25 — toolchain pinning** — mentions that `rust-toolchain.toml` should be added but doesn't note that the file may or may not exist today. A quick `ls` would close this.
2. **§4 audit units table intro** — the table has 3 example files. Now that many more splits are proposed (9+ in §10 Step 4), the table should grow to illustrate the full benefit.
3. **§1 hot-spot table completeness** — `tui/animation.rs` (582L, ~all production) is in the hot-spot table but `console/manager/input/save.rs` row note now correctly documents 4 functions (updated in iter 21). Cross-check whether any module map entries are still inaccurate (specifically console/manager/input/mod.rs which declares all input sub-modules).

---

## Iteration 23 — 2026-04-26

### Improvements chosen

1. **`rust-toolchain.toml` existence confirmed absent** — `ls` confirmed: `FILE_NOT_FOUND`. The roadmap at §7.7, §2 concept 25 ("No `rust-toolchain.toml` file exists"), §6 (`mise.toml` note), and §10 Step 3 all correctly describe the situation. No update needed — this was a false concern from iteration 22.

2. **`input/mod.rs` module map corrected** — entry lacked line count and was missing `InputOutcome` enum. Updated: 369L total; key exports now list `handle_key` + `InputOutcome`; description expanded to mention the `InputOutcome` variants (Continue, ExitJackin, LaunchNamed, LaunchCurrentDir, LaunchWithAgent) that signal the outer console loop.

3. **Audit units table expanded from 8 → 13 entries** — added 5 console-subsystem audit units targeting the PR #171 AI-generated code specifically: `state/types.rs` (state shape), `state/editor.rs` (dirty-detection), `input/editor/secrets.rs` (Secrets-tab key dispatch), `render/list/subpanels.rs` (Environments subpanel rendering), `input/save/preview.rs` (ConfirmSave modal text). Added a PR #171 context note below the table linking the 5 new entries to the AI-generated code concern.

### What was read
- `ls rust-toolchain.toml` — FILE_NOT_FOUND confirmed
- `src/console/manager/input/mod.rs:1–44` (full header, `//!` doc, module declarations, `InputOutcome` enum, `handle_key` top)
- `grep "^#\[cfg(test)\]"` in input/mod.rs — tests at lines 262, 285
- `wc -l` in input/mod.rs — 369L total confirmed
- §4 audit units table (read via grep) — already had 8 entries (not 3 as iteration 22 log claimed — likely the table had already been expanded in an earlier iteration that the log didn't capture)

### What changed in the roadmap
- §1 module map: `input/mod.rs` row updated (— → 369L; added InputOutcome to key exports)
- §4 audit units table: expanded from 8 to 13 entries; added PR #171 AI-generated code context note

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `rust-toolchain.toml` absence | High | `ls` confirmed FILE_NOT_FOUND |
| `input/mod.rs` 369L | High | `wc -l` confirmed; `InputOutcome` enum read directly |
| audit units table (13 entries) | High | All 5 new entries grounded in verified split proposals from iterations 16-21 |

### Weakest sections for iteration 24
1. **§4 — roadmap has no explicit "current state" inventory of which files already have good //! docs** — we know 37/90 files have `//!` docs, but the roadmap doesn't enumerate the "well-documented" files alongside the "needs documentation" list. The positive examples (env_model.rs, agent_allow.rs) exist but there's no complete positive inventory.
2. **§1 module map — `console/manager/render/mod.rs`** — listed but no line count or key export. The render module dispatch is important for understanding how the three render stages are wired.
3. **§7.5 snapshot testing — `render_tab_strip` EditorTab variants** — the roadmap says "4 tab variants" but doesn't name them. After PR #171, the tabs are General, Mounts, Agents, and Secrets/Environments — the exact variant names matter for writing the snapshot tests.

---

## Iteration 24 — 2026-04-26

### Improvements chosen

1. **`render/mod.rs` read in full — module map corrected + Role clarified** — read the complete file (421L, 244L production, tests at line 245). Key findings: (a) `FooterItem` enum is a substantial shared TUI infrastructure model (5 variants, inline block comment explaining the model); (b) 4 palette constants (`PHOSPHOR_GREEN/DIM/DARK`, `WHITE`) are defined here and used by all render sub-files; (c) `pub fn render` has `#[allow(clippy::too_many_lines)]` (14th suppression — hot-spot table says "13", may be undercounted); (d) the file has a minimal 1-element `//!` doc ("Render functions for the workspace manager TUI.") — lacks scope claims and consolidation history. Module map updated: 421L, `FooterItem` + palette constants + `render_header` + `centered_rect_fixed` added to key exports; description expanded to "stage dispatch + shared TUI utilities."

2. **EditorTab variants confirmed — `/stub` qualifier already gone** — confirmed `EditorTab` enum has exactly 4 variants: `General`, `Mounts`, `Agents`, `Secrets` (reading `state.rs:187–191`). The `Secrets` Rust variant is what the UI labels "Secrets / Environments." The §7.5 description already had `/stub` removed in an earlier iteration — this was a false alarm.

3. **§4 Rule 7 — positive exemplars table added** — added a 7-row "positive exemplars" table contrasting: 3-element `//!` docs (env_model.rs, agent_allow.rs) vs 2-element (input/save.rs, input/list.rs, mount_info.rs, input/mod.rs) vs 1-element (render/mod.rs). Added a "pattern observation" note that `console/manager/` is the reference model for `//!` coverage — PR #171 was written with docs discipline. Added a concrete example of how `render/mod.rs` could be upgraded from 1-element to 3-element.

### What was read
- `src/console/manager/render/mod.rs` (full — 421L read in full via `cat`)
- `src/console/manager/state.rs:187–191` (EditorTab enum variants confirmed via grep)
- §7.5 snapshot test section (confirmed `/stub` already removed in iteration 17)

### What changed in the roadmap
- §1 module map: `render/mod.rs` updated (— → 421L; key exports expanded from just `render` to full list; description expanded)
- §4 Rule 7: Added 7-entry positive exemplars table; pattern observation about console/manager/ subsystem; render/mod.rs upgrade example

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `render/mod.rs` 421L/244L production | High | `wc -l` confirmed; `grep #[cfg(test)]` confirmed tests at line 245 |
| `render/mod.rs` FooterItem as key export | High | Read full file; FooterItem enum at lines ~37-50 |
| EditorTab Secrets variant | High | state.rs:187-191 read via grep; all 4 variants confirmed |
| §4 Rule 7 positive exemplars table | High | All 7 files confirmed with `//!` docs by reading first lines in prior iterations |

### Weakest sections for iteration 25
1. **§7.5 snapshot test `render_mounts_subpanel` — `MountConfig` struct construction** — the test description says `MountConfig { src: ..., dst: ..., read_only: false }`. After reading `workspace/mod.rs` in iteration 1, the struct fields were confirmed. But `MountConfig` is being renamed to `MountSpec` in §5 #13. The snapshot test description should note this is the CURRENT name and will change.
2. **Hot-spot table `too_many_lines` count** — `render/mod.rs` has `#[allow(clippy::too_many_lines)]` on `pub fn render` (line 88). The hot-spot table says "13 across 8 files" but this is a 14th. Need to recount.
3. **§4 Rule 7 — `render/mod.rs` upgrade path** — the analysis says the consolidation history for `FooterItem` would reference "PR #165". This should be verified — what PR actually introduced the FooterItem model?

---

## Iteration 25 — 2026-04-26

### Improvements chosen

1. **`too_many_lines` suppression recount — 16 across 11 files** — `grep -rn "allow(clippy::too_many_lines)" src/` returned 16 results across 11 files. The roadmap said "13 across 8 files" (from iteration 1, before PR #171 additions were counted). Updated all three occurrences in the roadmap (hot-spot table footnote, §7.5 Gain(A) narrative, §7.3 clippy.toml recommendation). Added a full breakdown table showing all 11 files and their suppression counts, with a note that PR #171 added suppressions in `input/editor.rs` (+2), `render/editor.rs` (+2), and `render/mod.rs` (+1).

2. **FooterItem PR reference corrected — PR #166** — `git log --follow src/console/manager/render/mod.rs` shows oldest commit is `a3ab1ab` (PR #166: "feat(launch): workspace manager TUI (PR 2 of 3)"). The §4 Rule 7 note said "PR #165" — corrected to "PR #166 (workspace manager TUI, PR 2 of 3)" with the commit SHA as evidence.

3. **§7.5 MountConfig rename caveat added** — added a "Rename caveat" note to the `render_mounts_subpanel` snapshot test description: if §5 #13 (`MountConfig → MountSpec`) runs before the tests are written, the fixture changes to `MountSpec { ... }`. The note specifies this is a mechanical find-replace, not a semantic change.

### What was read
- `grep -rn "allow(clippy::too_many_lines)" src/` — 16 results across 11 files (complete list)
- `git log --follow src/console/manager/render/mod.rs` — two commits: `7c0a4f8` (PR #171) and `a3ab1ab` (PR #166); PR #166 is the file's creation commit

### What changed in the roadmap
- §1 hot-spot table: "13 across 8" → "16 across 11" + full breakdown table added
- §7.3 clippy.toml section: "13" → "16" (2 occurrences)
- §7.5 Gain(A): "13+" → "16"
- §4 Rule 7: "PR #165" → "PR #166 (workspace manager TUI, PR 2 of 3) — PR verified by git log"
- §7.5 render_mounts_subpanel: Added rename caveat for MountConfig → MountSpec

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `too_many_lines` count (16 across 11) | High | grep -rn confirmed; all 16 results enumerated in breakdown table |
| FooterItem in PR #166 | High | git log --follow confirmed oldest commit is PR #166 |
| MountConfig rename caveat | High | §5 #13 proposes MountSpec; struct fields unchanged by rename |

### Weakest sections for iteration 26
1. **§4 hot-spot table — `console/mod.rs`** — this file has a `too_many_lines` suppression but no entry in the hot-spot table. Need to verify its size and production/test breakdown.
2. **§4 hot-spot table — `console/widgets/op_picker/render.rs`** — similarly has a suppression but no table entry. The op_picker was added in PR #171 and hasn't been analyzed.
3. **§1 module map — `console/mod.rs`** — the file is listed but with "—" for line count. The TUI event loop entry point is important for navigability.

---

## Iteration 26 — 2026-04-26

### Improvements chosen

1. **`console/mod.rs` analysis — 406L, 307L production, no `//!` doc** — `wc -l` confirmed 406L, tests at line 308 → 307L production, 99L tests. File is below the 500L production threshold (Low priority). The block comment at lines 1–5 explains the `ConsoleStage` single-variant design ("collapsed to a single variant in PR #171's Modal::AgentPicker cleanup") — this architectural context should be a `//!` doc. Added to hot-spot table (Low priority) with a note about the `//!` promotion opportunity. Corrected module map: "~200" → "406L" with accurate key exports.

2. **`op_picker/render.rs` analysis — 865L, 545L production, Medium priority** — `wc -l` confirmed 865L, tests at line 546 → 545L production, 320L tests. Above the 500L production threshold. Single concern (1Password picker rendering). Functions fall into two groups: (a) entry/helpers (`render`, `breadcrumb_title`, `viewport_offset`, `modal_block`, `footer_line`, `render_loading`, `render_fatal`, `display_label`) and (b) level-specific renderers (`render_pane`, `render_account_lines`, `render_vault_lines`, `render_item_lines`, `render_field_lines`, ~260L). A 2-file split (current `render.rs` as coordinator + `levels.rs` for level renderers) would isolate the AI-generated PR #171 level-rendering logic. Added to hot-spot table (Medium priority). The file already has a `//!` doc.

3. **Stale `~200L` estimate for `console/mod.rs` corrected in 3 locations** — module map, `mod.rs` files section, and §4 intro concept bullet. All updated to "406L, ~307L production."

### What was read
- `src/console/mod.rs:1–15` (`//!` absence confirmed; `#![allow(irrefutable_let_patterns)]` + module declarations; `ConsoleStage` block comment at lines 1–5)
- `src/console/mod.rs:25,92,153` (impl ConsoleState, quit_confirm_area, run_console — confirmed via grep)
- `src/console/widgets/op_picker/render.rs:1–5` (`//!` doc confirmed)
- `src/console/widgets/op_picker/render.rs` (full function list via grep — 14 functions identified)
- `wc -l` for both files; `grep #[cfg(test)]` for test positions

### What changed in the roadmap
- §1 module map: `console/mod.rs` corrected (406L, key exports, architectural note about block comment)
- §1 hot-spot table: Added `op_picker/render.rs` row (865L, 545L production, Medium) and `console/mod.rs` row (406L, 307L production, Low)
- §1 `mod.rs` files section: "~200L" → "406L, ~307L production"
- §4 intro `mod.rs` list: Same correction

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `console/mod.rs` 406L/307L production | High | `wc -l` + `grep #[cfg(test)]` confirmed |
| `op_picker/render.rs` 865L/545L production | High | `wc -l` + `grep #[cfg(test)]` confirmed |
| `op_picker/render.rs` level renderers group | High | grep shows 4 `render_*_lines` functions at lines 253, 282, 303, 336 |

### Weakest sections for iteration 27
1. **§4 Rule 5 — `op_picker/render.rs` split proposal** — the hot-spot entry notes a potential 2-file split but no formal split analysis exists. Needs the same treatment as render/editor.rs, render/list.rs etc.
2. **`console/mod.rs` — `//!` doc priority queue** — confirmed no `//!` doc. The ConsoleStage design comment + 20 Hz loop contract + `is_on_main_screen`/`consumes_letter_input` helpers are all worth documenting. Should be added to the `//!` priority queue.
3. **§4 Rule 5 breakdown table note** — the table says "The 24 files above the 500-line threshold" but the table has grown. The "24" count needs to be reverified.

---

## Iteration 27 — 2026-04-26

### Improvements chosen

1. **`op_picker/mod.rs` discovered as major unanalyzed file** — `find src -name "*.rs" | xargs wc -l | sort -rn` revealed `op_picker/mod.rs` at 1712L (never in the hot-spot table). Tests at line 776 → **775L production, 937L tests**. Contains `OpPickerState` struct + 4 enums + `impl OpPickerState` state machine (~630L). Same types/behavior split opportunity as `state.rs`. Has a 7-line `//!` doc explaining the drill-down UI and the `op://` reference verbatim rationale. Added to hot-spot table (High priority). Module map updated: previous "—" entry replaced with two rows for `mod.rs` (1712L) and `render.rs` (865L).

2. **`operator_env.rs` total line count corrected: 1569 → 2130** — `wc -l` confirmed 2130L (was 1569L at loop start — likely from subsequent PRs landing on main or a measurement error). Tests at line 881 → **880L production, 1250L tests**. Updated 4 occurrences in roadmap: hot-spot table, ASCII tree, §4 workspace argument, §4 operator_env structure note.

3. **`op_picker/render.rs` formal 2-file split proposal** — read function signatures and line ranges for all 14 functions. Two natural groups with no cross-dependency: (a) coordinator/state-specific renderers/helpers (~300L), (b) `render_pane` + 4 level renderers + `display_label` (~260L). Proposed `render.rs` (state dispatch + helpers) + `render_pane.rs` (pane/level rendering). Auditability gain: "field-level display" → reads `render_pane.rs` (~260L) not 545L.

### What was read
- `find src -name "*.rs" | xargs wc -l | sort -rn` — confirmed 28+ files above 500L; `op_picker/mod.rs` at 1712L was missing from hot-spot table
- `src/console/widgets/op_picker/mod.rs:1–8` (`//!` doc), `:78–106` (`OpPickerState` struct), `:133` (impl block start)
- `grep #[cfg(test)]` in `op_picker/mod.rs` — tests at line 776
- `grep #[cfg(test)]` in `operator_env.rs` — tests at lines 881, 983
- `src/console/widgets/op_picker/render.rs:22–120` (function signatures for the first 6 functions read in detail)

### What changed in the roadmap
- §1 module map: `op_picker/` entry split into two rows (mod.rs + render.rs with line counts and key exports)
- §1 hot-spot table: Added `op_picker/mod.rs` row (1712L, 775L production, High); corrected `operator_env.rs` row (1569→2130, 810→~880 production)
- §1 ASCII tree: `operator_env.rs` (1569 lines) → (2130 lines)
- §4 Rule 5: Added `op_picker/render.rs` 2-file split proposal with function table
- §4 operator_env.rs split section and workspace argument: 1569 → 2130

### Confidence assessment (updated)
| Section | Confidence | Notes |
|---|---|---|
| `op_picker/mod.rs` 1712L/775L production | High | `wc -l` + `grep #[cfg(test)]` confirmed |
| `op_picker/render.rs` split proposal | High (directional) | Function groups confirmed by reading lines 22-120; no cross-dependency verified |
| `operator_env.rs` 2130L | High | `wc -l` confirmed; test positions at 881 and 983 confirmed |

### Weakest sections for iteration 28
1. **§4 `op_picker/mod.rs` split proposal** — identified the types/behavior split opportunity but no formal proposal written yet. The `OpPickerState` struct (~28L) and 4 enum types (~35L) could move to `op_picker/types.rs`, leaving only `impl OpPickerState` in `mod.rs`.
2. **§4 Rule 5 "24 files" note** — the preamble says "The 24 files above the 500-line threshold" — `find` shows 28+ files above 500L. Needs a targeted update.
3. **§1 total LOC update** — the roadmap says "~40,664 lines" in §4 but `find | xargs wc` now shows 43,587L total. This is also stale.

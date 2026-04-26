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

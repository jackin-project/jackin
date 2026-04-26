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

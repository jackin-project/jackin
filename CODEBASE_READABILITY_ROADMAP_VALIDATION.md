# Codebase Readability Roadmap Validation

Date: 2026-04-27

Scope: `docs/src/content/docs/reference/roadmap/codebase-readability.mdx` and every linked item in that roadmap cluster, validated against current `main` (`72ec765b`).

## Current Snapshot

- `src/` currently contains 100 Rust source files.
- 43 files start with `//!`; 57 do not. The overview page's "59% have no `//!` docs" is close, but stale.
- Largest Rust files by total LOC today are `src/runtime/launch.rs` (3047), `src/console/manager/input/editor.rs` (2440), `src/operator_env.rs` (2133), `src/console/manager/render/list.rs` (2069), and `src/app/mod.rs` (1109).
- `docs/src/content/docs/internal/` does not exist yet.
- `docs/superpowers/specs/` does exist and contains committed design/spec files.
- `rust-toolchain.toml` does not exist.
- Running `RUSTFLAGS='-W unreachable_pub' cargo check` on current `main` reports 10 `unreachable_pub` warnings, not hundreds.

## Summary Table

| Item | Verdict | Short note |
| --- | --- | --- |
| Overview page | Partially accurate | Core program is still relevant, but several metrics, paths, and the recommended execution order are stale. |
| Module contracts | Partially accurate | Need is real, but one priority file is already documented and the counts/LOC are stale. |
| Behavioral spec: runtime/launch.rs | Partially accurate | Still needed, but the described pipeline and line references no longer match the file. |
| Behavioral spec: op_picker | Inaccurate | One of the key invariants is now wrong. |
| Per-directory README + AGENTS.md | Partially accurate | Direction is good, but the `docs/` target text assumes `/internal/` already exists. |
| Developer Reference Starlight section | Partially accurate | Need is real, but the problem statement points at the wrong current directory. |
| Update PROJECT_STRUCTURE.md | Accurate but understated | The file is stale, but in more places than the page currently lists. |
| CI gate: PROJECT_STRUCTURE.md freshness | Partially accurate | Goal is good; the CI implementation sketch is wrong about checkout history. |
| pub(crate) visibility pass | Inaccurate | Counts are stale and the `unreachable_pub` plan does not work the way the page assumes. |
| MSRV & toolchain pin | Accurate | No correction needed beyond normal drift updates later. |
| Architecture Decision Records (ADRs) | Partially accurate | No ADRs exist, but design context also lives in roadmap/spec docs already. |
| Snapshot tests for TUI render | Partially accurate | The gap is real, but the page understates existing render-test coverage and uses stale counts. |
| Agent workflow: replace superpowers with cc-sdd | Partially accurate | Migration topic is real, but the page overstates current workflow problems and understates new tool-specificity. |
| Move CONTRIBUTING + TESTING to Starlight | Inaccurate | These files should probably be mirrored into docs, not moved out of root. |
| Split input/editor.rs | Partially accurate | Still a valid split target; metrics are stale. |
| Split app/mod.rs | Partially accurate | Still a valid split target; metrics and import notes are stale. |
| Split operator_env.rs | Inaccurate | The page's dependency assumptions are no longer true. |
| Split runtime/launch.rs | Partially accurate | Still a valid split target, but current concerns and sizes have moved. |
| Greenfield workspace split | Partially accurate | Directionally valid, but LOC and boundary wording need refresh. |
| rustdoc JSON -> Starlight API pipeline | Accurate | Still deferred; no meaningful correction needed now. |

## Items That Need Correction

### Overview Page

- `PR #177` is already merged on `main`, so "merge what's in flight first" is stale.
- The recommended execution order is internally inconsistent: it schedules Phase 2 file splits before Phase 1 documentation work even though snapshot tests and the `launch.rs` behavioral spec are explicit Phase 2 prerequisites.
- The Phase 2 summary should use current paths and numbers. The current target file is `src/console/manager/input/editor.rs`, not a generic `input/editor.rs`, and all listed LOC figures have drifted.

### Module Contracts

- `src/operator_env.rs` no longer belongs in "verified — zero `//!` docs". It already starts with a module-level `//!` block.
- If the page wants a current 10-file priority list, better replacement candidates today are large undocumented files such as `src/isolation/materialize.rs`, `src/isolation/finalize.rs`, `src/config/mod.rs`, `src/app/context.rs`, `src/instance/auth.rs`, `src/runtime/cleanup.rs`, `src/tui/animation.rs`, `src/workspace/mod.rs`, and `src/manifest/mod.rs`.

### Behavioral Spec: runtime/launch.rs

- The core need still stands, but the page now underspecifies the current load path.
- The current `load_agent_with` path includes repo resolution, manifest env resolution, operator env resolution, auth-mode handling, workspace materialization, attach/finalizer cleanup, and slot reclamation logic in addition to the original trust/build/token/launch flow.
- The line references and `LoadOptions` seam locations are stale and should be refreshed before anyone uses this page as an audit checklist.

### Behavioral Spec: op_picker

- INV-1 is no longer correct. Current behavior is: commit `OpField::reference` verbatim when it exists; only synthesize `op://<vault>/<item>/<label>` as a fallback for fixtures that omit `reference`.
- The current parser explicitly accepts both 3-segment and 4-segment `op://` references (`op://<vault>/<item>/<field>` and `op://<vault>/<item>/<section>/<field>`).
- The page also says `poll_*_load`; current implementation uses a single `poll_load()`.

### Per-directory README + AGENTS.md

- The overall idea still makes sense.
- The proposed `docs/README.md` text should mention current reality: internal design docs live in `docs/superpowers/specs/` today. If the page wants to point to `src/content/docs/internal/`, it should say that is future-state and depends on the Developer Reference item.

### Developer Reference Starlight Section

- The problem statement currently says internal docs live in `docs/internal/`, but that directory does not exist.
- Current committed internal design material is mostly in `docs/superpowers/specs/` plus roadmap pages.
- The target `/internal/` Starlight structure still looks reasonable; the current-state description just needs correction.

### Update PROJECT_STRUCTURE.md

- This item is still valid, but its scope is now too small.
- Beyond the PR #171 widgets and `op_cache.rs`, `PROJECT_STRUCTURE.md` also still references `docs/src/content/docs/commands/cd.mdx` even though that file does not exist, omits `guides/environment-variables.mdx`, omits `build.rs`, and omits current workflows such as `preview.yml` and `renovate.yml`.

### CI Gate: PROJECT_STRUCTURE.md Freshness

- The goal is sound, but the implementation sketch is not CI-safe as written.
- `actions/checkout` fetches a single commit by default unless `fetch-depth: 0` is set, so `origin/main...HEAD` is not guaranteed to exist in a PR job.
- If you keep a diff-based gate, add `fetch-depth: 0` or explicitly fetch the base ref first. I would also prefer a simpler diff path (`-- src`) over the current `'src/**/*.rs'` sketch.

### pub(crate) Visibility Pass

- The published counts are stale.
- More importantly, the page overstates what `unreachable_pub` will buy you. On current `main`, `RUSTFLAGS='-W unreachable_pub' cargo check` reports 10 warnings, not anything close to the page's 257-item framing.
- The reason is structural: `src/lib.rs` publicly exports most modules, and `src/bin/validate.rs` consumes the library. This repo is not just one binary with no meaningful public surface.
- If the real goal is to shrink internal visibility, first decide which modules should remain public from `lib.rs`; only then will `unreachable_pub` become a useful cleanup tool.

### Architecture Decision Records (ADRs)

- The need is real, but the problem statement should acknowledge that design context already exists in committed roadmap docs and `docs/superpowers/specs/`, not only in PR descriptions.

### Snapshot Tests for TUI Render

- The roadmap goal is still good.
- The raw `#[allow(clippy::too_many_lines)]` count is now 17, not 16.
- The page should say there is no snapshot-style regression net, not that there is no automated regression net at all. The repo already has substantial `TestBackend`-based render coverage in tests and inline module tests.
- The target function line numbers and file sizes have drifted.

### Agent Workflow: replace superpowers with cc-sdd

- The migration topic is real because the repo still uses `docs/superpowers/specs/` and does not have an `/internal/specs/` area.
- The page is wrong to say current specs are not version-controlled or inaccessible. They are committed plain Markdown files in the repo.
- `.claude/commands` is still tool-specific. If the real goal is "visible to all agents", the canonical workflow should live in repo-neutral docs/scripts, with tool-specific wrappers layered on top.

### Move CONTRIBUTING + TESTING to Starlight

- This page should probably be reframed from "move" to "mirror/publish".
- Root `CONTRIBUTING.md` and `TESTING.md` are part of the repo's operational entrypoint: `AGENTS.md` links to both, `PROJECT_STRUCTURE.md` lists `TESTING.md`, and GitHub gives root `CONTRIBUTING.md` special treatment.
- The step list is incomplete even on its own terms: `AGENTS.md` also links `CONTRIBUTING.md`, not just `TESTING.md`.

### Split input/editor.rs

- Still a valid split candidate.
- Metrics need refresh: `src/console/manager/input/editor.rs` is now 2440 total lines, with tests starting at line 1189.

### Split app/mod.rs

- Still a valid split candidate.
- Metrics need refresh: `src/app/mod.rs` is now 1109 total lines, with tests starting at line 1057.
- The import note is stale: `src/main.rs` now calls `jackin::run(...)` from the library crate instead of doing `mod app; app::run(...)` directly.

### Split operator_env.rs

- This page needs a substantive rewrite.
- It says `operator_env` is only used by `console/`, but current callers also include `runtime/launch.rs`, `config/editor.rs`, `config/persist.rs`, and `console/op_cache.rs`.
- The old "tests start at line 881" boundary is no longer true; test-only code is interleaved much earlier, and the file already carries shared API surface used outside console code.
- Any split plan now has to preserve a shared API for runtime/config callers, not just console callers.

### Split runtime/launch.rs

- Still a valid split target.
- Metrics need refresh: `src/runtime/launch.rs` is now 3047 total lines, with tests starting at line 1320.
- The concern breakdown is incomplete now. In addition to public API / pipeline / terminfo / trust, the file also owns operator-env integration, auth diagnostics, workspace materialization, attach/finalizer handoff, and container-slot cleanup/reclamation.
- The split plan should be refreshed around today's seams, not the original 2368-line version.

### Greenfield Workspace Split

- Directionally valid.
- The LOC figure is stale: `src/**/*.rs` currently totals 49,754 lines, not ~43,587.
- The "console has zero imports from runtime" claim still holds for production code, but there is at least test-only coupling through `crate::runtime::FakeRunner`.

## Items That Are Accurate Enough To Keep

- `MSRV & toolchain pin` is still accurate: no `rust-toolchain.toml` exists, `mise.toml`/`Cargo.toml`/CI still carry the version split, and MSRV is still not checked in CI.
- `rustdoc JSON -> Starlight API pipeline` is still accurate as a deferred item. Its `/internal/` destination simply depends on the Developer Reference work landing first.

## Recommended Next Doc Refreshes

1. Refresh the overview page first: current metrics, current file paths, and a prerequisite-respecting execution order.
2. Rewrite the four pages with the most misleading current-state descriptions: `behavioral-spec-op-picker.mdx`, `pub-crate-visibility.mdx`, `move-contributing-testing.mdx`, and `split-operator-env.mdx`.
3. Broaden `project-structure-update.mdx` and fix `ci-project-structure-gate.mdx` so the automation plan matches the actual GitHub Actions checkout behavior.

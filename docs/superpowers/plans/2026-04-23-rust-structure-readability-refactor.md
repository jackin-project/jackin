# Rust Structure And Readability Refactor Plan

> **Purpose:** This document is a refactor roadmap only. It does **not** change runtime behavior, config format, CLI flags, or external usage on its own. Its job is to make the later implementation safe, incremental, and easy to review.

**Goal:** Make the Rust crate substantially easier to read and understand by splitting oversized files, centralizing duplicated policy, and giving each module one clear responsibility, while preserving current behavior for a heavily used production CLI.

**Primary problem statement:** The codebase is not mainly hard to read because of Rust syntax. It is hard to read because too much logic is concentrated in a few large files, and some policy is implemented in multiple places. The main readability issue is structural.

**Design principles:**

- Keep the smallest possible behavior-preserving changes in each PR.
- Move code before rewriting code.
- Centralize policy once the file boundaries are in place.
- Preserve CLI behavior, config shape, runtime labels, and compatibility with existing user state.
- Prefer pure planning/helpers over new traits or framework-like abstractions.
- Avoid introducing architecture layers that exist only for elegance.
- Preserve test coverage. Tests move and are refactored, not deleted. See the **Test Preservation Policy** section below for the full rule.

**Relevant references:**

- `https://github.com/apollographql/rust-best-practices`
- `https://rust-lang.github.io/api-guidelines/`
- `https://rust-analyzer.github.io/book/contributing/style.html`

**Pre-commit verification for implementation PRs** (per `COMMITS.md`):

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

**Docs-only PR note:** This PR contains the plan only. Implementation PRs must follow the full verification sequence above.

---

## Success Criteria

- `src/lib.rs` becomes a thin crate root rather than the application center.
- No single implementation file remains in the "god file" range unless there is a strong and documented reason.
- Workspace policy has one clear source of truth.
- Agent/workspace selection policy has one clear source of truth.
- Env dependency and reserved-runtime-env policy has one clear source of truth.
- Runtime lifecycle code is split by concern into files that can be read independently.
- The existing CLI behavior, config format, and runtime semantics are preserved.
- The final refactor can be reviewed as a sequence of small PRs rather than a single large rewrite.

---

## Non-Goals

- No CLI redesign.
- No config schema redesign.
- No manifest format changes.
- No Docker backend redesign.
- No changes to trust semantics.
- No behavior changes masked as readability improvements.
- No broad conversion to custom error enums everywhere unless a module split truly needs one.
- No speculative abstractions for future runtimes or future transport layers.
- **No dropping of existing tests as a shortcut** to make a refactor compile, pass, or land more quickly. Tests that cannot easily follow a refactor are a signal that the refactor needs more thought, not that the test is disposable. The one narrow exception is documented in the **Test Preservation Policy** section below.

---

## Current Problem Map

The following files are the primary readability hotspots because they mix multiple responsibilities or have become too large to scan quickly:

- `src/runtime.rs`
- `src/lib.rs`
- `src/config.rs`
- `src/workspace.rs`
- `src/manifest.rs`
- `src/cli.rs`
- `src/launch.rs`
- `src/instance.rs`

### Root Causes

- The crate root contains both public wiring and full command execution logic.
- Workspace mutation rules are split between CLI preprocessing, config mutation code, and workspace helpers.
- Runtime code mixes repo cache management, image building, container launch, attach/restart logic, discovery, cleanup, GC, and test helpers in one file.
- TUI launcher state, rendering, preview logic, and input handling live together.
- Some policy appears in multiple places, which increases drift risk.

### Most Important Duplications To Remove Later

- **Workspace create/edit planning** duplicated across `src/lib.rs`, `src/config.rs`, and `src/workspace.rs`. The collapse algorithm itself is already centralized in `workspace::plan_collapse`, but the pre-collapse mount-list construction is inlined separately in the `Create` and `Edit` arms of `WorkspaceCommand` in `src/lib.rs` (auto-workdir-mount insertion vs removal, post-upsert rebuild). See `src/lib.rs` `WorkspaceCommand::Create` and `WorkspaceCommand::Edit` arms.
- **Workspace/agent selection policy** duplicated between `src/lib.rs` (`resolve_agent_from_context`) and `src/launch.rs` (`eligible_agents_for_saved_workspace` + `default_agent_index`). Both implement the same allowed-agent filter and the same `last_agent` → `default_agent` → prompt priority.
- **Global mount expansion recipe** duplicated across `src/config.rs` (`expand_and_validate_named_mounts`), `src/workspace.rs`, and `src/launch.rs` (`global_mounts`). The expansion itself lives in `config`, but the filter-scoped-vs-unscoped-then-expand recipe is re-implemented in `launch`.
- **Non-TTY / confirmation guard pattern** repeated in five places across `src/lib.rs`, `src/runtime.rs`, and `src/workspace.rs`: `std::io::stdin().is_terminal()` check, bail on non-TTY, `dialoguer::Confirm` on interactive. Prime candidate for a single shared helper.
- **Eligible-agent-for-workspace filter** implemented twice: once for the TUI in `src/launch.rs` and once for the direct CLI flow in `src/lib.rs`. The TUI variant also applies a user-entered query string on top; any unification helper must preserve that composition without turning the query into an `allowed_agents` filter.
- **Unknown-workspace-name error** constructed ad hoc in at least three sites (CLI edit path, launch resolution, config lookup) using the shape `config.workspaces.get(&name).ok_or_else(|| anyhow!("unknown workspace {name}"))`. Prime candidate for a single `WorkspaceConfig::require(&self, name)` or similar accessor.
- **Reserved-runtime-env constant declared twice** (as `RESERVED_RUNTIME_ENV_VARS` in `src/manifest.rs` and `RUNTIME_OWNED_ENV_VARS` in `src/runtime.rs`, with overlapping but not identical contents). This is a constant-name drift rather than full logic duplication: cycle detection and interpolation-reference parsing themselves live once (in `src/manifest.rs` and `src/env_resolver.rs` respectively) and are not re-implemented by `runtime`. The fix is therefore narrower than a full env-policy rewrite — it is primarily constant unification plus relocating cycle detection to a shared module.

### Readability Anti-Patterns Also Observed

Distinct from duplication, these structural smells will become easy to address only once the large files are split:

- `src/lib.rs` contains roughly `820` lines of `#[cfg(test)] mod tests` at the bottom of the crate root. In Rust, tests normally co-locate with the module they exercise; such a large test block at the crate root is a symptom that too much domain logic lives in `lib.rs`. These tests should migrate with their functions during Phase 1, not be piled into one new `app/tests.rs`.
- Several `match` arms in `WorkspaceCommand` exceed `100` lines each, with inline validation, formatting, and persistence mixed together. Each subcommand should become its own function before Phase 2 introduces a planner.
- `runtime` has two functions in the `200`-plus-line range (`load_agent_with`, `launch_agent_runtime`) that interleave repo resolution, image building, network/DinD startup, mount setup, env construction, and container `run` argument assembly. These are the primary justification for Phase 5 and need to be split by named step, not rewritten.
- Instance auth provisioning (`src/instance.rs`) has a deeply nested block that mixes platform detection, credential loading, file writing, permission repair, and symlink checks. Splitting it by responsibility (see Phase 7) is substantially safer than trying to flatten it in place.

---

## Target File Size Guidance

These are not hard rules, but they are the readability target for the refactor:

- Ideal general-purpose module: `200-400` lines.
- Acceptable dense module: `400-600` lines.
- Special-case upper bound for presentation-heavy files like Clap or TUI renderers: `600-800` lines.
- Anything beyond `800` lines should be treated as a strong refactor candidate.

The goal is not to fragment the crate into tiny files. The goal is to make each file readable in one sitting.

---

## Target Structure

This is the intended destination shape. The exact filenames may vary slightly during implementation, but the separation of concerns should stay the same.

```text
src/
  main.rs
  lib.rs
  app/
    mod.rs
    context.rs
    commands/
      mod.rs
      load.rs
      launch.rs
      hardline.rs
      lifecycle.rs
      workspace.rs
      config.rs
  workspace/
    mod.rs
    paths.rs
    mounts.rs
    planner.rs
    resolve.rs
    sensitive.rs
    tests/
  config/
    mod.rs
    persist.rs
    agents.rs
    mounts.rs
    workspaces.rs
    tests/
  runtime/
    mod.rs
    identity.rs
    repo_cache.rs
    image.rs
    launch.rs
    attach.rs
    discovery.rs
    cleanup.rs
    tests/
  launch/
    mod.rs
    state.rs
    input.rs
    preview.rs
    render.rs
    tests/
  instance/
    mod.rs
    naming.rs
    auth.rs
    plugins.rs
    tests/
  cli/
    mod.rs
    root.rs
    workspace.rs
    config.rs
  tui/
    mod.rs
    animation.rs
    output.rs
    prompt.rs
  manifest.rs            # schema + loader + thin validate() wrapper after Phase 10
  env_resolver.rs
  env_model.rs           # reserved-env constant, cycle detection, interpolation refs
  selector.rs
  repo.rs
  repo_contract.rs
  derived_image.rs
  docker.rs
  terminal_prompter.rs
  version_check.rs
  paths.rs
```

Notes on this tree:

- `manifest.rs` stays at the crate root (not in a `manifest/` directory) but is expected to shrink meaningfully once Phase 10 moves its cycle-detection and interpolation-validation logic into `env_model.rs`. If after that extraction the file is still above the readability target, a follow-up split is acceptable but is not part of this plan.
- `env_model.rs` is introduced by Phase 10 and becomes the single home for anything that is "env policy" rather than "env mechanism."

---

## Test Preservation Policy

Test coverage is a first-class deliverable of this refactor, not a byproduct. The refactor is motivated by a desire to make the crate easier to read and maintain; a refactor that quietly shrinks coverage makes the crate *less* safe to maintain, which defeats the point.

### Rules

- **Tests are preserved.** If a test existed before the refactor, it must continue to exist after the refactor, even if it moves file, module, or name.
- **Tests may be moved.** Co-locating a test with the module it exercises is encouraged and is a normal part of a module split.
- **Tests may be refactored.** Renaming, splitting, combining, reformatting, replacing private-helper calls with public-helper calls, updating imports, and updating test fixtures are all allowed when the *assertion semantics stay the same*.
- **Tests may be strengthened.** Adding assertions, tightening expectations, or adding cases is encouraged if it makes the existing intent clearer.
- **Tests must not be silently weakened.** Converting an exact equality check to a looser "contains" or "any match" check, deleting an `assert`, turning a hard failure into a warning, or `#[ignore]`-ing a test to dodge a compile error are all forbidden without explicit approval in the PR description.

### When A Test Is Hard To Carry Forward

Some tests will reach into private helpers that are about to move, or will depend on module layout that the refactor is changing. When that happens:

1. Do **not** delete the test to unblock the move.
2. First, understand what the test is actually verifying. Is it a behavior assertion, or is it a coupling to an implementation detail?
3. If it asserts behavior, find the new public-or-crate-visible surface where that behavior is now expressed, and rewrite the test against that surface. The assertion content stays, the plumbing changes.
4. If it is coupled to an internal detail that will no longer exist (for example, a helper that has been inlined), consider whether the behavior is already covered by a higher-level test. If yes, the test can be retired *and* the PR description must state explicitly which test supersedes it.
5. If rewriting is non-trivial, leave the test in place against the old surface in a separate preparatory commit, and only move it once the surface is stable. Two small commits beats one big commit that "fixes everything at once."

### Narrow Exception: Genuine Duplicate Tests

A test may be deleted only when it is a clear duplicate of another test — that is, it exercises the same code path with the same inputs and the same assertions, and provides no additional edge-case coverage or documentation value. Duplicates sometimes arise naturally when a helper is extracted and two callers each had their own copy of the same table-driven case.

When deleting such a duplicate:

- The PR description must name the surviving test that covers the same case.
- The PR description must show, in one line, why the two tests were equivalent (same inputs, same assertions, same code under test).
- "It was hard to update" is **not** a qualifying reason.
- "It was slow" is **not** a qualifying reason (use `#[ignore]` or move to a separate test binary instead).
- "It was flaky" is **not** a qualifying reason (fix the flake or document the flake in a follow-up issue, do not silently drop coverage).

### Characterization Tests Added In Phase 0

Tests added in Phase 0 to lock down behavior before the move are covered by the same policy: they are preserved by subsequent phases, and are only ever strengthened, never quietly weakened.

---

## Core Invariants

These rules must remain true after every implementation phase.

### CLI And Config Compatibility

- All existing commands and flags keep their names and semantics.
- Help output remains accurate.
- Existing `config.toml` shape remains unchanged.
- Existing `jackin.agent.toml` shape remains unchanged.
- Built-in agent bootstrap and trust behavior remain unchanged.

### Workspace Resolution And Mount Behavior

- `classify_target` keeps the current `path` versus `name` rules.
- Saved workspace selection by current directory continues to choose the deepest matching workspace.
- `last_agent` remains higher priority than `default_agent`.
- Workspace `allowed_agents` restrictions remain enforced in both direct CLI flow and TUI flow.
- Global mount precedence remains `global < wildcard < exact`.
- Rule-C collapse behavior remains identical.
- `no_workdir_mount` behavior remains identical.
- Sensitive-mount confirmation remains identical, including non-TTY refusal.

### Runtime Lifecycle Behavior

- Repo lock coverage remains sufficient to prevent races on the shared cached repo.
- Repo mismatch and dirty-repo safeguards remain intact.
- Image rebuild and Claude version cache-bust behavior remain intact.
- Runtime-owned env vars remain reserved and non-overridable.
- Container naming, clone naming, and class-family matching remain intact.
- Hardline behavior remains identical for running, cleanly exited, crashed, and missing containers.
- Legacy Docker label compatibility remains intact.
- Orphaned DinD and network cleanup behavior remains intact.

### Auth And Security Behavior

- `ignore`, `copy`, and `sync` auth-forward modes remain distinct.
- macOS Keychain fallback remains intact.
- Symlink protections and private-file permissions remain intact.
- No refactor step should weaken file safety checks or reduce auth isolation guarantees.

---

## Phase Plan

Each phase is intentionally scoped so it can be implemented in one or more reviewable PRs.

## Phase 0: Safety Rails And Characterization Tests

**Goal:** Lock down behavior before structural work begins.

### Work

- Review existing tests and identify gaps specifically around the refactor boundaries.
- Add only high-leverage characterization tests where later movement is risky.
- Avoid adding broad new test suites unless a gap is real.

### Priority Test Areas

- Context-based workspace selection by current directory, including the deepest-match tie-break when multiple saved workspaces could apply.
- Agent choice order: `last_agent` valid, `last_agent` stale, `default_agent` set, `allowed_agents` empty, single-eligible auto-select, multi-eligible prompt.
- Agent-filter composition: how `allowed_agents` restriction and the TUI query string compose. Verify that query filtering never widens the allowed set and never silently returns zero results when a valid allowed agent matches the query.
- Workspace create/edit mount collapse behavior, including the Rule-C collapse path and the edit-driven-vs-pre-existing redundancy distinction.
- Auto-workdir-mount insertion on create vs omission on edit with `--no-workdir-mount`. The create and edit parallel structure is subtly different and the plan makes assumptions that need test anchoring.
- Global mount precedence (`global < wildcard < exact`) and conflict handling.
- Hardline behavior for running, stopped, crashed, and missing containers.
- Auth-forward behavior across `ignore`, `copy`, and `sync`, including macOS Keychain fallback and symlink rejection.
- Reserved runtime env filtering, verified against both constant definitions until Phase 10 unifies them.
- Mount-flag combinations emitted by `launch_agent_runtime` to docker run (bind vs volume vs named, read-only flags, workdir interaction). These are implicit contracts today; pin them before Phase 5 begins extraction.

### Exit Criteria

- Later file moves can rely on tests to catch accidental semantic drift.
- The specific edge cases called out in the **Edge Cases Checklist** below have at least one direct assertion each.

---

## Phase 1: Thin The Crate Root

**Goal:** Move app orchestration out of `src/lib.rs` without changing behavior.

### Why First

- `src/lib.rs` is currently the most structurally confusing file.
- Shrinking the crate root improves readability immediately.
- It gives the rest of the refactor a clear place to live.

### Work

- Create `src/app/mod.rs` and move the command dispatch `run(cli)` implementation there.
- Create `src/app/context.rs` and move these helpers out of `src/lib.rs`:
  - target classification
  - target-name resolution
  - best saved workspace for cwd lookup
  - context-based agent resolution
  - context-based running container resolution
  - last-agent persistence helper
- Leave `src/lib.rs` as a thin public surface that re-exports modules and forwards `run`.

### Constraints

- Do not rename user-visible concepts.
- Do not change logic while moving code.
- Keep imports and public module declarations straightforward.
- Keep `pub` as narrow as possible at each new boundary. Prefer `pub(crate)` during the move; only widen to `pub` if an external consumer (binary, integration test, docs) actually requires it.

### Tests Currently In `src/lib.rs`

- The crate root currently ends with a large `#[cfg(test)] mod tests` block (~`820` lines). These tests cover workspace classification, agent resolution, workspace CRUD helpers, and related utilities.
- Do **not** lift them wholesale into a single `src/app/tests.rs`. That preserves the original readability problem under a new name.
- Instead, migrate each test along with the function it exercises into the corresponding new module. For example, tests for target classification move next to `classify_target` in `src/app/context.rs`. This is safe because Rust's `#[cfg(test)] mod tests` block can exist at the bottom of any module file.
- Tests that are true end-to-end crate-level integration tests (if any) may remain at the crate root or move into `tests/` as true integration tests.
- Follow the **Test Preservation Policy** above: tests may move and be refactored, but must not be deleted to unblock the move. If a test reaches into a private helper that is about to become a new module's internal, the correct response is to rewrite the test against the new crate-visible surface, not to drop it.

### Risks

- Subtle path/import breakage.
- Accidentally widening visibility too much during the move.
- Losing test coverage because a test references a private helper that moved to a different module. Compile errors will surface this quickly, but be careful when re-gating visibility.

### Mitigation

- Move code verbatim first.
- Only clean imports and visibility after tests pass.
- After the move, run `cargo test` once before touching anything else; this surfaces every broken import or visibility mismatch as a single build failure rather than a semantic regression.

---

## Phase 2: Centralize Workspace Planning

**Goal:** Give workspace create/edit policy one source of truth.

### Problem To Solve

- Current workspace logic is split across CLI flow, `AppConfig` mutation, and workspace helpers.
- A reader currently has to read multiple files to understand one edit.
- Concretely: the `Create` and `Edit` arms of `WorkspaceCommand` in `src/lib.rs` each contain their own mount-list assembly code — auto-workdir-mount insertion on create, post-upsert rebuild plus edit-driven-vs-pre-existing classification on edit — and both then hand the result to the already-centralized `workspace::plan_collapse`. The planning that happens *before* `plan_collapse` is where the duplication lives, and that is what Phase 2 must centralize.

### Work

- Introduce a pure planning layer under `src/workspace/planner.rs`.
- The planner should own:
  - auto-workdir mount decisions
  - mount upsert/removal planning
  - Rule-C collapse decisions
  - distinction between edit-driven and pre-existing redundancy
  - summary data for CLI reporting
  - confirmation-worthy changes versus hard validation failures
- Keep CLI code responsible for prompting and printing only.
- Keep config code responsible for persistence only.

### Desired End State

- The answer to "what does workspace create/edit actually do?" exists in one pure module.

### Risks

- This is the easiest place to accidentally change behavior.

### Mitigation

- Do this only after the characterization tests are in place.
- Preserve the existing algorithm and messages first, then consider tiny cleanup later.

---

## Phase 3: Split The Workspace Module

**Goal:** Make workspace code navigable by concern.

### Work

- Create `src/workspace/paths.rs` for `expand_tilde`, normalization, and `resolve_path`-style helpers.
- Create `src/workspace/mounts.rs` for mount parsing and mount validation.
- Create `src/workspace/planner.rs` for collapse and edit/create planning.
- Create `src/workspace/resolve.rs` for runtime resolution of load inputs into a resolved workspace.
- Create `src/workspace/sensitive.rs` for sensitive mount detection and confirmation.
- Keep `src/workspace/mod.rs` for public types and light re-exports.

### Notes

- `resolve_load_workspace` should become substantially shorter by delegating to helpers.
- Path logic and policy logic should not stay mixed in one file.

### Risks

- Duplicate helper creation if planning and resolve logic are split carelessly.

### Mitigation

- Decide early which functions are path utilities, which are mount utilities, and which are workspace policy.

---

## Phase 4: Split Config By Domain

**Goal:** Keep `AppConfig` as the persistence boundary, not the entire domain layer.

### Work

- Create `src/config/persist.rs` for load/save/init behavior.
- Create `src/config/agents.rs` for built-in agent sync, trust, and source resolution.
- Create `src/config/mounts.rs` for mount registry behavior.
- Create `src/config/workspaces.rs` for workspace storage entrypoints only.
- Keep `AppConfig` in `src/config/mod.rs`, but delegate implementation to submodules.

### Important Rule

- After Phase 2, workspace mutation logic should not be re-derived in config.
- Config should commit validated results rather than rebuilding policy internally.

### Risks

- Mixing schema location and method implementation location in a confusing way.

### Mitigation

- Keep the main data types easy to find.
- Keep related method impls near their owning concern.

---

## Phase 5: Split Runtime By Lifecycle Concern

**Goal:** Turn `src/runtime.rs` from one large subsystem file into small lifecycle-focused modules.

### Scheduling Constraint

This phase must not start until Phase 4 (config split) has landed. The runtime and config modules share label conventions, mount serialization, and workspace lookups. Splitting runtime first means every extracted runtime file has to chase a still-monolithic `config.rs` for those conventions, which reintroduces coupling that Phase 4 is supposed to remove.

### Recommended Order

0. `naming.rs` — **do this first, before any other extraction.** Today, runtime's internal coupling runs through string-typed image names, container names, and family-match substrings. Introducing lightweight newtypes (or at minimum a single module that owns every naming/labeling constant and helper) means later extractions touch a small, stable interface rather than scattered format strings. This step alone does not split behavior; it only centralizes identifiers.
1. `repo_cache.rs`
2. `image.rs`
3. `attach.rs`
4. `discovery.rs`
5. `cleanup.rs`
6. `identity.rs`
7. finally the high-level orchestration in `mod.rs` / `launch.rs`

### Responsibilities

- `identity.rs`
  - git identity loading
  - host identity loading
  - config rows summary helpers
- `repo_cache.rs`
  - repo lock acquisition
  - clone/fetch/merge logic
  - repo mismatch handling
- `image.rs`
  - build context use
  - docker build invocation
  - Claude version extraction and cache-bust logic
- `launch.rs`
  - launch orchestration
  - network and DinD startup
  - container run arguments
- `attach.rs`
  - attach behavior
  - `hardline` behavior
  - state-based reconnect rules
- `discovery.rs`
  - list managed containers
  - display name formatting
  - family matching
- `cleanup.rs`
  - eject
  - orphan discovery
  - garbage collection
  - helper cleanup commands

### Additional Cleanup Goal

- Move the large runtime test block into `src/runtime/tests/` once the split is stable.

### Risks

- Runtime code is highly coupled to naming, labels, cleanup, and side effects. The `load_agent_with` and `launch_agent_runtime` functions are each in the `200`-plus-line range and interleave multiple concerns (repo resolution, image build, network/DinD startup, mount and env assembly, docker run argument construction). Mechanical file extraction will not help a reader until these two functions are each broken into named steps.
- Legacy label compatibility means small label-string edits can silently break `hardline` against previously-running containers.

### Mitigation

- Move code in the same signatures first.
- Avoid logic changes during file extraction.
- The `naming.rs` step above exists specifically to de-risk this phase. Land it as its own commit before any other runtime file is introduced.
- Split the two large functions into named step helpers **after** their surrounding module has been extracted, not during. That keeps the "moved code" and "rewrote control flow" signals separate for bisecting.

---

## Phase 6: Unify Agent And Workspace Selection Policy

**Goal:** Make the TUI and non-interactive CLI use the same decision logic.

### Problem To Solve

- Best workspace for cwd logic appears in more than one place.
- Eligible-agent filtering appears in more than one place.
- Preferred-agent logic appears in more than one place.

### Work

- Introduce shared pure helpers, likely under `app/context.rs` or `workspace/resolve.rs`, for:
  - best workspace match for cwd
  - eligible agents for a workspace (by `allowed_agents` only)
  - preferred agent ordering (`last_agent` → `default_agent` → remaining)
  - running container candidates for a workspace
- Keep TUI-specific concerns (like the user-entered query string) as a separate composition step applied on top of the shared eligibility helper, not as a parameter to it. The decomposition should read as `filter_allowed(agents, ws) → rank_preferred(..., last, default) → narrow_by_query(..., query)`.
- Make `launch` call the same helpers used by direct CLI flows.

### Expected Benefit

- One policy definition.
- Easier reasoning about why the TUI and direct `load`/`hardline` behave the same way.

### Risks

- The TUI and direct CLI have historically diverged in small ways (for example, whether an empty `allowed_agents` means "all" vs "none"). Unifying without a characterization test is the easiest way to silently change behavior.

### Mitigation

- Only start this phase after the Phase 0 agent-filter-composition tests land.
- If a divergence is found between the TUI and CLI, document it in the plan and preserve existing behavior on the path most users hit (typically the TUI).

---

## Phase 7: Split Instance State Preparation

**Goal:** Separate naming, auth provisioning, and plugin-state serialization.

### Work

- Create `src/instance/naming.rs` for:
  - runtime slug generation
  - primary container naming
  - clone naming
  - family matching
- Create `src/instance/auth.rs` for:
  - auth-forward behavior
  - Keychain/file credential loading
  - private file writing and permission repair
  - symlink safety
- Create `src/instance/plugins.rs` for plugin-marketplace serialization.
- Keep `AgentState` in `src/instance/mod.rs` and delegate its preparation steps.

### Why This Matters

- This file contains security-sensitive behavior.
- Smaller files reduce the chance of accidental breakage when future auth changes are made.

---

## Phase 8: Split TUI Launcher And Preview Logic

**Goal:** Make the TUI code readable without mixing state, event handling, render code, and workspace preview resolution.

### Work

- Create `src/launch/state.rs` for `LaunchState`, `WorkspaceChoice`, and filtering helpers.
- Create `src/launch/input.rs` for event handling and stage transitions.
- Create `src/launch/preview.rs` for workspace resolution previews and detail-line building.
- Create `src/launch/render.rs` for drawing functions and layout helpers.
- Keep `src/launch/mod.rs` as the entrypoint for `run_launch`.

### Important Constraint

- Avoid having both a preview workspace type and a resolved workspace type that are too similar without clear names.
- If a preview-specific type is needed, name it explicitly.

---

## Phase 9: Split CLI Declarations By Topic

**Goal:** Make the Clap schema easier to scan.

### Work

- Move root command definitions into `src/cli/root.rs`.
- Move workspace command definitions into `src/cli/workspace.rs`.
- Move config subcommands into `src/cli/config.rs`.
- Keep shared banner/style constants in `src/cli/mod.rs` or `src/cli/style.rs`.

### Constraint

- No flag renames.
- No help-text drift unless a typo fix is explicitly intentional.

---

## Phase 10: Unify Env Policy And Trim Manifest

**Goal:** Collapse env policy onto a single source of truth and remove the cycle-detection / interpolation-validation logic from `src/manifest.rs`, which today treats a 1,500-line file as if it were a pure schema module.

### Scope Clarification

The current state is narrower than a "duplicated env policy" reading suggests:

- Reserved-runtime-env is declared as **two constants with different names** (`RESERVED_RUNTIME_ENV_VARS` in `src/manifest.rs`, `RUNTIME_OWNED_ENV_VARS` in `src/runtime.rs`). The sets overlap but are not identical. This is drift waiting to happen, not active duplication of logic.
- Cycle detection and interpolation-reference parsing each live in exactly one place today (`src/manifest.rs` and `src/env_resolver.rs`), but they belong together with the reserved-env list because all three are "env policy." Splitting them across `manifest` and `env_resolver` means any future env-rule change touches two unrelated files.

### Work

- Add `src/env_model.rs`.
- Move into it:
  - a single canonical reserved-runtime-env list, replacing both current constants.
  - interpolation reference parsing (`extract_interpolation_refs` and similar).
  - dependency graph construction.
  - topological ordering / cycle detection (currently in `manifest.rs`).
- Make `manifest` use it for validation.
- Make `env_resolver` use it for resolution order.
- Make runtime use it for reserved runtime env filtering.

### Secondary Benefit: Shrinking `manifest.rs`

Extracting the env-policy logic (cycle detection is substantial on its own) shrinks `manifest.rs` materially without requiring a full `src/manifest/` directory split. The remaining file should be close to a schema + loader + thin `validate` wrapper that delegates to `env_model`. If after this extraction `manifest.rs` is still over the readability target, it can be split in a follow-up, but the plan does not mandate that split now.

### Expected Benefit

- Validation and execution cannot silently diverge as easily.
- `manifest.rs` stops being a "large file with policy buried at the bottom."

### Risks

- Reserved-env behavior is observable at runtime. If the unified list accidentally widens or narrows the reserved set, users lose the ability to set a previously-user-visible variable, or runtime ignores a previously-reserved variable.

### Mitigation

- Treat the unified list as the **union** of both existing constants, and in the same PR add a test that asserts the exact membership of each previously-reserved name.
- Verify `manifest` validation errors and runtime filtering against a known list of sentinel variable names before the PR lands.

---

## Phase 11: Split General TUI Helpers

**Goal:** Make `src/tui.rs` easier to scan without changing user-facing terminal behavior.

### Work

- Create `src/tui/animation.rs` for intro/outro/digital-rain code.
- Create `src/tui/output.rs` for tables, hints, fatal output, logo printing, and terminal title helpers.
- Create `src/tui/prompt.rs` for prompt-choice and spinner logic.
- Keep `src/tui/mod.rs` as the public entrypoint.

### Priority

- Lower priority than the domain-layer refactors.
- Do this after the core structural issues are solved.

---

## Edge Cases Checklist

Every implementation PR should explicitly verify the edge cases it might affect.

### Target Classification

- Plain workspace name versus relative path with slash.
- Tilde path with implicit destination.
- Tilde path with explicit destination.
- `src:dst` where `dst` must begin with `/`.
- Paths beginning with `.`.

### Current Directory Context Resolution

- Exact saved workspace workdir match.
- Nested path under a workspace mount root.
- Multiple possible workspace matches, deepest one wins.
- No matching workspace.

### Agent Selection Policy

- `last_agent` is valid and allowed.
- `last_agent` exists but is stale.
- `default_agent` is set and allowed.
- Allowed-agent list is empty.
- Multiple eligible agents require prompt.
- Exactly one eligible agent auto-selects.

### Workspace Planning

- Workdir auto-mount insertion.
- Workdir auto-mount omitted when already present.
- Explicit `--no-workdir-mount`.
- Rule-C collapse of children under a new parent.
- Pre-existing redundant mounts detected during edit.
- Readonly mismatch is rejected.
- Ad-hoc mount destination conflicts are rejected.
- Global mount destination conflicts are rejected.

### Runtime Lifecycle

- No cached repo exists.
- Cached repo exists and is clean.
- Cached repo remote mismatches configured source.
- Cached repo has local changes.
- Claude update available triggers rebuild.
- First build with no stored version does not force update logic.
- Container attaches after successful launch.
- Container cleanly exits after attach.
- Container crashes and remains recoverable via `hardline`.
- DinD exists versus missing versus stopped.
- Legacy managed resources still discoverable.

### Auth Forwarding

- `ignore` revokes previous forwarded state.
- `copy` copies only on first creation.
- `sync` overwrites from host when host auth exists.
- `sync` preserves existing container auth when host auth is missing.
- Host file-based credentials exist.
- macOS Keychain fallback path works.
- Symlinked auth files are rejected.

### Env Policy

- Non-interactive vars without defaults are rejected.
- Interpolation references must exist and be listed in `depends_on`.
- Dependency cycle detection remains correct.
- Runtime-owned env vars remain reserved.
- Resolved values containing `${...}` are not recursively re-expanded.

---

## PR Breakdown Recommendation

The implementation should be spread across multiple small PRs.

### PR 1

- Add or tighten characterization tests around refactor boundaries.
- No structural moves yet unless required by the tests.

### PR 2

- Thin `src/lib.rs` into `src/app/`.
- No logic changes.

### PR 3

- Introduce centralized workspace planner.
- Preserve exact behavior.

### PR 4

- Split `src/workspace.rs` into `workspace/`.
- Move workspace tests into `workspace/tests/` if helpful.

### PR 5

- Split `src/config.rs` into `config/`.
- Keep `AppConfig` as the external type.

### PR 6

- Split `src/runtime.rs` into `runtime/`. **Must come after PR 5 has landed** (config split) to avoid chasing moving shared conventions.
- The first commit in this PR (or in a standalone PR 6a if the diff is too large) must be the `naming.rs` introduction described in Phase 5. No other extraction should land before it.
- Prefer one concern per commit or per tightly related pair of files.

### PR 7

- Unify shared selection policy for CLI and TUI.
- Split `src/launch.rs` into `launch/`.
- The unification step needs the Phase 0 agent-filter-composition tests in place; confirm they are present and passing before starting this PR.

### PR 8

- Split `src/instance.rs` and introduce shared env model (`src/env_model.rs`).
- As part of the env model work, replace both `RESERVED_RUNTIME_ENV_VARS` and `RUNTIME_OWNED_ENV_VARS` with a single canonical list, verified against a membership test.

### PR 9

- Split `src/cli.rs` and `src/tui.rs`.
- Do only after domain logic is stable.

### PR 10

- Small cleanup PR for:
  - the shared non-TTY / confirmation helper (collapses the five repeated call sites across `lib.rs`, `runtime.rs`, and `workspace.rs`).
  - a shared `workspace-or-missing` lookup helper (collapses the three ad-hoc `unknown workspace {name}` error sites).
  - import cleanup and any remaining low-risk organization work.
- This PR should be trivial to review; if any item turns out to be load-bearing (for example, the confirmation helper affects error-message wording), split it out.

---

## Implementation Rules

- Do not refactor two large subsystems in one PR.
- Do not move code and rewrite logic in the same commit if it can be avoided.
- Do not rename public concepts unless it is purely internal and mechanically safe.
- Prefer extraction into existing modules over inventing service layers.
- Keep helper names boring and descriptive.
- Prefer `&str`, `&Path`, and slices at new pure helper boundaries where ownership is unnecessary.
- Keep comments rare; use module boundaries and function names to make flow obvious.
- When a module split adds a new file, add a short `//!` module comment when it materially helps navigation.
- When a new module is introduced, default its items to `pub(crate)` and only widen to `pub` when the crate's public API actually requires it. This keeps the refactor from accidentally expanding the library's surface area.
- Do not delete a test to make a refactor compile. If a test is blocking a move, the refactor needs to be adjusted, or the test needs to be rewritten against the new surface — never silently dropped. Genuine duplicates follow the narrow exception in the **Test Preservation Policy** and must be called out in the PR description.
- Every refactor PR should include a one-line note in its description stating whether any tests were removed, and if so, which surviving test covers the same behavior.

## Ongoing API Hygiene

These are low-risk, always-safe cleanups that do not require their own phase. Fold them into the smallest adjacent PR rather than batching them into a dedicated refactor commit:

- Add `#[non_exhaustive]` to public error enums (for example `SelectorError`, `CollapseError`) so adding a variant is not a breaking change.
- Add `#[derive(Debug)]` to public structs that cross API boundaries and are not already derived. For example, `WorkspaceEdit`.
- Add `#[derive(PartialEq)]` / `Eq` to config data types that are compared in tests, to remove ad-hoc hand-rolled comparisons.
- Where a function is currently named `parse` and returns `Result<Self, E>`, consider additionally implementing `TryFrom<&str>` so idiomatic callers can use the standard conversion traits.
- Prefer enum parameters over `bool` at new helper boundaries when the boolean's meaning is not obvious at the call site.
- Do **not** convert the crate-wide use of `anyhow::Result` to custom error enums as part of readability work. Per the non-goals, that is a separate decision.

---

## Acceptance Checklist For The Final Refactor Program

- [ ] `src/lib.rs` is small and easy to scan.
- [ ] The large test block at the bottom of `src/lib.rs` has migrated to co-located `#[cfg(test)]` modules, not been piled into one replacement file.
- [ ] `src/runtime.rs` no longer exists as a single giant file.
- [ ] `src/workspace.rs` no longer exists as a single giant file.
- [ ] `src/config.rs` no longer exists as a single giant file.
- [ ] `src/manifest.rs` has had its env-policy logic (cycle detection, interpolation parsing, reserved list) extracted to `src/env_model.rs`.
- [ ] Workspace policy is centralized.
- [ ] Selection policy is centralized, including a clear separation between allowed-agent filtering and TUI query filtering.
- [ ] Env graph policy is centralized, and only one reserved-runtime-env list exists.
- [ ] Runtime lifecycle responsibilities are split by concern, with naming identifiers centralized in a single small module.
- [ ] The non-TTY / confirmation guard pattern exists as a single shared helper, not in five places.
- [ ] The `unknown workspace {name}` error is produced by one shared helper, not constructed ad hoc.
- [ ] Tests still pass via `cargo nextest run`.
- [ ] Test count did not decrease across the refactor, except for explicitly documented duplicates (each removal accompanied by a reference to the surviving equivalent test).
- [ ] No test was weakened: assertions remained at least as strict as before the move.
- [ ] Every PR description lists tests removed (if any) with the surviving equivalent, per the **Test Preservation Policy**.
- [ ] No user-visible behavior changed unintentionally.

---

## First Implementation Step After This Plan Lands

Start with **Phase 0** and **Phase 1** only.

That means:

- add or tighten the minimal characterization tests needed for safe movement,
- then thin `src/lib.rs` into `src/app/`,
- and stop there for the first implementation PR.

This keeps the first real refactor small, reviewable, and low risk.

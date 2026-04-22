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

- Workspace create/edit planning duplicated across `src/lib.rs`, `src/config.rs`, and `src/workspace.rs`.
- Workspace/agent selection policy duplicated across `src/lib.rs` and `src/launch.rs`.
- Global mount expansion logic duplicated across `src/config.rs`, `src/workspace.rs`, and `src/launch.rs`.
- Confirmation and non-TTY guard patterns repeated in `src/lib.rs`, `src/runtime.rs`, and `src/workspace.rs`.
- Env graph and reserved runtime env policy duplicated across `src/manifest.rs`, `src/env_resolver.rs`, and `src/runtime.rs`.

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
  manifest.rs
  env_resolver.rs
  env_model.rs
  selector.rs
  repo.rs
  repo_contract.rs
  derived_image.rs
  docker.rs
  terminal_prompter.rs
  version_check.rs
  paths.rs
```

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

- Context-based workspace selection by current directory.
- Agent choice order: `last_agent`, then `default_agent`, then prompt behavior.
- Workspace create/edit mount collapse behavior.
- Global mount precedence and conflict handling.
- Hardline behavior for running, stopped, crashed, and missing containers.
- Auth-forward behavior across `ignore`, `copy`, and `sync`.
- Reserved runtime env filtering.

### Exit Criteria

- Later file moves can rely on tests to catch accidental semantic drift.

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

### Risks

- Subtle path/import breakage.
- Accidentally widening visibility too much.

### Mitigation

- Move code verbatim first.
- Only clean imports and visibility after tests pass.

---

## Phase 2: Centralize Workspace Planning

**Goal:** Give workspace create/edit policy one source of truth.

### Problem To Solve

- Current workspace logic is split across CLI flow, `AppConfig` mutation, and workspace helpers.
- A reader currently has to read multiple files to understand one edit.

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

### Recommended Order

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

- Runtime code is highly coupled to naming, labels, cleanup, and side effects.

### Mitigation

- Move code in the same signatures first.
- Avoid logic changes during file extraction.

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
  - eligible agents for a workspace
  - preferred agent ordering
  - running container candidates for a workspace
- Make `launch` call the same helpers used by direct CLI flows.

### Expected Benefit

- One policy definition.
- Easier reasoning about why the TUI and direct `load`/`hardline` behave the same way.

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

## Phase 10: Unify Env Policy

**Goal:** Stop maintaining env graph policy and reserved runtime env policy in multiple places.

### Work

- Add `src/env_model.rs`.
- Move shared pieces into it:
  - reserved runtime env definitions
  - interpolation reference parsing
  - dependency graph construction
  - topological ordering / cycle helpers
- Make `manifest` use it for validation.
- Make `env_resolver` use it for resolution order.
- Make runtime use it for reserved runtime env filtering.

### Expected Benefit

- Validation and execution cannot silently diverge as easily.

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

- Split `src/runtime.rs` into `runtime/`.
- Prefer one concern per commit or per tightly related pair of files.

### PR 7

- Unify shared selection policy for CLI and TUI.
- Split `src/launch.rs` into `launch/`.

### PR 8

- Split `src/instance.rs` and introduce shared env model.

### PR 9

- Split `src/cli.rs` and `src/tui.rs`.
- Do only after domain logic is stable.

### PR 10

- Small cleanup PR for duplicated confirmations, import cleanup, and any remaining low-risk organization work.

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

---

## Acceptance Checklist For The Final Refactor Program

- [ ] `src/lib.rs` is small and easy to scan.
- [ ] `src/runtime.rs` no longer exists as a single giant file.
- [ ] `src/workspace.rs` no longer exists as a single giant file.
- [ ] `src/config.rs` no longer exists as a single giant file.
- [ ] Workspace policy is centralized.
- [ ] Selection policy is centralized.
- [ ] Env graph policy is centralized.
- [ ] Runtime lifecycle responsibilities are split by concern.
- [ ] Tests still pass via `cargo nextest run`.
- [ ] No user-visible behavior changed unintentionally.

---

## First Implementation Step After This Plan Lands

Start with **Phase 0** and **Phase 1** only.

That means:

- add or tighten the minimal characterization tests needed for safe movement,
- then thin `src/lib.rs` into `src/app/`,
- and stop there for the first implementation PR.

This keeps the first real refactor small, reviewable, and low risk.

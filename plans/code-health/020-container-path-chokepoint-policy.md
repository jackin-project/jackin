# Plan 020: Phase 3 — a container-path chokepoint plus the executable `/jackin/`-only policy

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat e80d5cc0a..HEAD -- crates/jackin-core/src/ crates/jackin-capsule/src/runtime_setup.rs crates/jackin-agent-status/src/rules.rs crates/jackin-capsule/src/daemon/file_export.rs`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M-L
- **Risk**: MED (touches every container-path literal; values must not change, only their source — the Docker E2E lane is the behavioral net)
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `e80d5cc0a`, 2026-07-09

## Why this matters

"Container paths under `/jackin/` only. No FHS roots (`/run`, `/var`, `/opt`, `/etc`, `/tmp/jackin*`)" is a repo **hard rule** (AGENTS.md), and roadmap Phase 3 ("Product invariants as executable policy") demands it become a failing test instead of a review request: "Assert every container-side path any builder emits is under `/jackin/`; a new FHS-root path fails the suite, not review." Measured state: the rule is structurally unassertable — `"/jackin…"` appears as ~152 scattered string literals across ~20 files with no central constant or builder, so there is no single place a policy test can interrogate; the only guard is one local categorizer in the capsule's file-export path. Any future edit can introduce `/var/...` and CI stays green. The fix is a chokepoint: one constants/builder module every emitter goes through, a policy test over the chokepoint, and a shrink-only literal-count gate so stragglers cannot regrow.

## Current state

All excerpts verified at the planning commit.

- No central constant. Example literal cluster, `crates/jackin-capsule/src/runtime_setup.rs:100-106`:

  ```rust
  const CAPSULE_RUNTIME_BIN: &str = "/jackin/runtime/jackin-capsule";
  const GIT_HOOKS_DIR: &str = "/jackin/state/git-hooks";
  const GIT_HOOK_PATH: &str = "/jackin/state/git-hooks/prepare-commit-msg";
  const GIT_HOOK_MARKER: &str = "/jackin/state/git-hooks/prepare-commit-msg.v3.done";
  const GIT_DCO_IDENTITY_CACHE: &str = "/jackin/state/git-dco-identity";
  ```

  Another: `crates/jackin-agent-status/src/rules.rs:339` `const RUNTIME_PACK_DIR: &str = "/jackin/runtime/agent-status/packs";`
- The one existing guard, `crates/jackin-capsule/src/daemon/file_export.rs:303-315` — a request categorizer, not an emission policy:

  ```rust
  fn requested_export_path_category(requested_path: &str) -> &'static str {
      let trimmed = requested_path.trim();
      if trimmed.starts_with("/jackin/run/") || trimmed == "/jackin/run" { return "jackin-run"; }
      if trimmed.starts_with("/jackin/") || trimmed == "/jackin" { return "jackin-owned"; }
      ...
  ```

- Literal census (rerun to refresh): `rg -c '"/jackin' crates/*/src -g '*.rs'` → **152** total (tests included). Production emitters live in: jackin-capsule (runtime_setup.rs, socket.rs, clipboard.rs, session.rs, daemon/file_export.rs), jackin-agent-status (rules.rs, hook_installer under agent_status/), jackin-runtime (host_attach.rs, attach.rs, apple_container.rs, docker_profile.rs), jackin-isolation (materialize.rs), jackin-image (derived_image.rs), jackin-protocol (lib.rs), jackin-usage (usage.rs, logging.rs). Test files keep their literals (assertions SHOULD spell expected paths out — a test asserting via the same constant it tests is circular).
- Architecture home for the chokepoint: `jackin-core` is the L0/T0 crate every listed crate already depends on (verified via the dependency inventory in plan 012). A new `container_paths` module there is reachable by all emitters without any new dependency edge. jackin-core's conventions: modules are flat files under `src/` with `pub mod` in `lib.rs` (39 existing), tests in a sibling `tests.rs`, no `mod.rs`, `unsafe_code = forbid`, no unwrap/expect/panic.
- Repo conventions: comments = non-obvious WHY only; every crate structural change updates its README (hard rule); Conventional Commits signed `-s`, push per commit.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core tests | `cargo nextest run -p jackin-core` | all pass |
| Per-crate tests during migration | `cargo nextest run -p <crate>` | all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| New gate | `cargo run -p jackin-xtask -- lint container-paths` | `container-path gate OK …` |
| Literal census | `rg -c '"/jackin' crates/*/src -g '*.rs' -g '!*tests*'` | shrinking count |
| Docker E2E (if Docker available) | `cargo xtask ci --e2e` | green (else note skipped) |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-core/src/container_paths.rs` (create) + `container_paths/tests.rs` (create) + `lib.rs` module registration + `crates/jackin-core/README.md` structure row
- Production literal sites in the emitter crates listed above (migrate to the chokepoint; values unchanged)
- `crates/jackin-xtask/src/container_paths_gate.rs` (create; or extend plan 017's ratchet if it landed — see Step 5) + `main.rs` registration + xtask README
- `container-path-allowlist.toml` (create, root) — shrink-only residual-literal ledger
- Roadmap Phase 3 "Product invariants as executable policy" status
- `plans/code-health/README.md` row

**Out of scope**:
- Changing ANY path value. This plan moves strings, never edits them. A diff hunk that changes a path's text is a bug.
- Test files' literals (they stay as independent assertions).
- Docker-side path construction in `docker/` shell/Dockerfile assets (not Rust; note as follow-up if literals live there).
- A full `ContainerPath` newtype with validated joins everywhere — the roadmap's newtype sweep (Phase 2) is separate; this plan ships constants + a minimal join helper + the policy, not a type-system migration of every signature.

## Git workflow

- Branch off `main`: `feat/container-path-policy`.
- Commit per step (chokepoint; per-crate migrations may batch 2-3 crates per commit; gate; docs), `-s`, push each. PR to `main`; do not merge. Any capsule-touching PR needs the capsule smoke block from `.github/PULL_REQUEST_TEMPLATE.md` — this one touches jackin-capsule, so include it.

## Steps

### Step 1: The chokepoint module

Create `crates/jackin-core/src/container_paths.rs`:

- `pub const JACKIN_ROOT: &str = "/jackin";`
- One named `pub const` per canonical subtree actually used today (derive the list from the census: `RUNTIME_DIR = "/jackin/runtime"`, `STATE_DIR = "/jackin/state"`, `RUN_DIR = "/jackin/run"`, plus whatever the migration in Steps 3-4 surfaces — extend as you migrate, keep alphabetical).
- `pub fn join(base: &str, rel: &str) -> String` — debug-asserts `base` starts with `JACKIN_ROOT`, rejects (`debug_assert!`) `rel` starting with `/` or containing `..`; returns `format!("{base}/{rel}")`. Production behavior on violation: since panics are denied workspace-wide, make it `pub fn join(base, rel) -> String` with the checks as `debug_assert!` only, plus a `#[must_use]`. (The policy TEST is the enforcement; the helper is convenience.)
- Module `//!` header: states this is the single source for container-side paths and cites the AGENTS.md hard rule.

`container_paths/tests.rs`: the policy suite —
1. every `pub const` in the module starts with `/jackin` (enumerate them in a test-local list; add a comment: extending the module means extending this list — the gate in Step 5 makes forgetting expensive);
2. none contains `..`, a double slash, or a trailing slash;
3. `join` composes as expected and its output starts with `/jackin/`;
4. the forbidden-root regression case: assert a helper `fn is_jackin_owned(path: &str) -> bool` (add it, mirroring `file_export.rs`'s prefix logic) returns false for `/run/x`, `/var/x`, `/etc/x`, `/opt/x`, `/tmp/jackin-x`.

Register `pub mod container_paths;` in `crates/jackin-core/src/lib.rs`; add the README structure row.

**Verify**: `cargo nextest run -p jackin-core` → all pass incl. the new suite.

### Step 2: Re-point the capsule's categorizer

In `crates/jackin-capsule/src/daemon/file_export.rs:303-315`, replace the `"/jackin/run/"`/`"/jackin/"` literals with `container_paths::RUN_DIR`/`JACKIN_ROOT`-derived comparisons (behavior identical; keep the function's return values byte-identical).

**Verify**: `cargo nextest run -p jackin-capsule` → all pass.

### Step 3: Migrate capsule + agent-status literals

For each production literal in jackin-capsule (runtime_setup.rs constants above, socket.rs, clipboard.rs, session.rs) and jackin-agent-status (rules.rs:339, hook_installer): rebase the local `const` on the chokepoint, e.g.

```rust
const GIT_HOOKS_DIR: &str = ...    // becomes:
// (const concatenation) use core's constant + literal suffix via concat! is not
// possible with a const from another crate; keep a local const but derive it in
// a test: assert_eq!(GIT_HOOKS_DIR, format!("{}/git-hooks", container_paths::STATE_DIR));
```

**Mechanism note (load-bearing):** Rust cannot `concat!` a foreign `const &str` at compile time. So the migration pattern for existing `const` strings is: keep the local `const` literal BUT add one derivation assertion per constant into that module's `tests.rs` (`assert_eq!(LOCAL, format!("{}/suffix", container_paths::X))`). Runtime-built paths (format!/PathBuf::from sites) migrate directly to `container_paths::join`/constants. This still gives the policy suite a complete inventory: the derivation tests ARE the link between every literal and the chokepoint.

**Verify**: `cargo nextest run -p jackin-capsule -p jackin-agent-status` → all pass, including the new derivation assertions.

### Step 4: Migrate runtime/isolation/image/protocol/usage literals

Same pattern per crate (host_attach.rs, attach.rs, apple_container.rs, docker_profile.rs; materialize.rs; derived_image.rs; protocol lib.rs; usage.rs, logging.rs). Batch commits per 2-3 crates. Values byte-identical — verify each hunk changes only the source of the string, never its content.

**Verify**: after each batch: `cargo nextest run -p <crates>` all pass; at the end `cargo nextest run --workspace --all-features --locked` → all pass; if Docker is available, `cargo xtask ci --e2e` → green (if not available, state so in the PR body — the operator runs it).

### Step 5: The literal-count gate

Add `cargo xtask lint container-paths`: counts `"/jackin` literals in production sources (`crates/*/src`, excluding `tests.rs`/`tests/`), compares against `container-path-allowlist.toml` — a shrink-only per-file ledger `[[file]] { path, literals = N }` seeded with whatever remains after Steps 3-4 (target: only `container_paths.rs` itself plus const-definition files with derivation tests). Shrink-only semantics identical to the file-size gate (`crates/jackin-xtask/src/lint.rs:220-244` is the model: growth fails; measured < recorded fails demanding a shrink; stale rows fail). Failure text: names the file, the delta, the fix ("route new container paths through jackin_core::container_paths; regenerate: cargo xtask lint container-paths --print-allowlist"). **If plan 017's ratchet engine has landed**, implement this as a `ratchet.toml` family + provider instead of a standalone gate — check for `ratchet.toml` at the root first.

Register in `main.rs` (`LintCommand::ContainerPaths`) and chain into `run_all_lints`.

**Verify**: `cargo run -p jackin-xtask -- lint container-paths` → OK; probe: add `let _x = "/var/jackin-test";`-style literal `"/jackin` string to a production file → gate fails naming it; revert probe. `cargo nextest run -p jackin-xtask` → all pass.

### Step 6: Roadmap + docs

Roadmap Phase 3 "Product invariants as executable policy" item 1: shipped (chokepoint + policy suite + shrink-only literal gate). Update `crates/jackin-core/README.md` (already in Step 1) and xtask README.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- New: `container_paths/tests.rs` policy suite (Step 1, 4 test groups).
- New: one derivation assertion per migrated `const` in each owning module's existing `tests.rs` (pattern in Step 3).
- Regression: full workspace suite + (operator-run if needed) Docker E2E — the paths' values must be provably unchanged; additionally `git diff` review: no hunk may alter text between the quotes of any path.

## Done criteria

- [ ] `jackin_core::container_paths` exists with policy tests green
- [ ] Every production `"/jackin` literal is either in `container_paths.rs`, or a local `const` with a derivation assertion, or listed in the allowlist with a shrinking count
- [ ] `cargo run -p jackin-xtask -- lint container-paths` passes and is part of `cargo xtask lint`; probe fails correctly
- [ ] Workspace nextest + clippy green; Docker E2E green or explicitly deferred to operator
- [ ] Roadmap updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

Stop and report back if:

- Any migration would CHANGE a path value (mismatch between two sites that today disagree — that is a live bug to report, not to silently unify).
- A literal turns out to be wire-protocol-relevant in jackin-protocol such that moving its construction changes frame bytes (should be impossible for a source-level move; verify with the protocol tests).
- The census reveals >30 files needing migration (plan sized for ~20; a much larger surface needs wave-splitting).
- Docker E2E fails after migration in any path-related way.

## Maintenance notes

- New container-side paths must be added to `container_paths.rs` (the gate makes any other placement fail). The Phase 2 newtype sweep can later upgrade `join` to a `ContainerPath` newtype without re-touching call sites if they already go through this module.
- `docker/` (non-Rust) assets may carry their own `/jackin` paths — out of scope here; flag in the PR body if the census shows drift between Rust constants and Dockerfile/entrypoint paths.
- Reviewer should scrutinize: every hunk's string content (byte-identical), and the allowlist's seed (each residual entry should have a one-line comment why it can't derive yet).

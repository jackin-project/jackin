# Plan 025: Phase 2/3 — extract `jackin-test-support` and break the isolation⇄runtime dev-dependency cycle

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat b42c97d4c..HEAD -- crates/jackin-runtime/src/runtime/test_support.rs crates/jackin-isolation/Cargo.toml crates/jackin-xtask/src/arch.rs Cargo.toml`
> If plan 012 landed, arch.rs carries a `TIERS` table and a
> `DEV_CYCLE_ALLOWLIST` with the isolation⇄runtime entry — expected; this plan
> deletes that entry. On any other mismatch with the "Current state" excerpts,
> treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (moves shared fakes used by multiple crates' suites; pure test-surface, no production behavior)
- **Depends on**: none hard; plan 012 (tier-graph arch gate) interacts — see Step 5
- **Category**: tests / tech-debt
- **Planned at**: commit `b42c97d4c`, 2026-07-09

## Why this matters

Two roadmap items, one extraction. Phase 2 ("Move shared test fakes and lower-level ports out of `jackin-runtime` when lower crates currently need dev-dependencies back upward") and Phase 3 item 1 ("Add a shared `jackin-test-support` crate only after inventorying duplicated builders/fakes"). The inventory exists and is verified: `jackin-isolation` — a crate `jackin-runtime` depends on in production — dev-depends **back up** on `jackin-runtime` solely to reach `runtime::test_support::{FakeRunner, FakeDockerClient}` (48 import sites across 4 test files), forming the workspace's only dependency cycle (prod `runtime → isolation` + dev `isolation → runtime`, first-wave DEBT-devdep-cycle). The same fakes are additionally re-implemented from scratch elsewhere: `FakeRunner` ×3, `FakeDockerClient` ×2. The cycle blocks a clean tier-graph architecture gate (plan 012 grandfathers it in a one-entry allowlist), every duplicated fake drifts independently, and the Phase 3 sim/property harnesses need one canonical fake set to build on.

## Current state

Verified at the planning commit.

- The upward dev-dep, `crates/jackin-isolation/Cargo.toml`:

  ```toml
  [dev-dependencies]
  jackin-runtime = { workspace = true, features = ["test-support"] }
  tokio = { workspace = true, features = ["test-util"] }
  tempfile = { workspace = true }
  ```

- What isolation actually uses from it (all 4 files): `jackin_runtime::runtime::test_support::FakeRunner` (`crates/jackin-isolation/src/cleanup/tests.rs:5`, `finalize/tests.rs:44`) and `...::FakeDockerClient` (`finalize/tests.rs:51,75,99,125,146,210` + more; 48 total matches for `jackin_runtime` under `crates/jackin-isolation/src`).
- The source being moved, `crates/jackin-runtime/src/runtime/test_support.rs` (public surface, verified): `pub fn install_all_test_stubs(paths: &jackin_core::paths::JackinPaths)` (line 22), `pub struct FakeRunner` (line 42), `pub fn seed_valid_role_repo(repo_dir)` (line 164), `pub fn first_temp_role_repo(data_dir)` (line 172), `pub struct FakeDockerClient` (line 200). Gated by the `test-support` feature (`crates/jackin-runtime/Cargo.toml:64`: `test-support = []`).
- Known duplicate fakes elsewhere (NOT moved in this plan — recorded for the dedupe follow-up, but Step 6 repoints the trivial ones if free): `crates/jackin/tests/common.rs:114 FakeRunner`, `crates/jackin-host/src/caffeinate/tests.rs:21 FakeDockerClient` + `:96 FakeRunner`, console `StubRunner`s ×4 (different trait — leave).
- What the fakes implement: `FakeRunner` fakes `jackin_core::CommandRunner` (`crates/jackin-core/src/runner.rs:56`); `FakeDockerClient` fakes `jackin_core::DockerApi` (`crates/jackin-core/src/docker.rs:95`). Both port traits live in `jackin-core` — so a test-support crate needs only `jackin-core` (plus whatever std/tempfile helpers the seed functions use). **Read `test_support.rs` in full first**: if any item references `jackin-runtime` types beyond core ports (e.g. runtime config structs), that item stays behind (see STOP conditions).
- Consumers of `jackin-runtime/test-support` to repoint: find all with `rg -n 'jackin-runtime.*test-support' crates/*/Cargo.toml` and `rg -ln 'runtime::test_support' crates/ -g '*.rs'` — known: jackin-isolation (above), jackin-runtime's own tests, plausibly `crates/jackin` (its Cargo.toml has `test-support = ["jackin-env/test-support"]` feature chaining — check whether its dev-deps enable runtime's).
- Workspace mechanics for a new crate: add to `[workspace] members` (root Cargo.toml:2-29, alphabetical-ish — match existing ordering) and `[workspace.dependencies]` (the `jackin-* = { version = "0.6.0-dev", path = … }` pattern, Cargo.toml:61-80); new crate needs `[lints] workspace = true`, `README.md` + `AGENTS.md` + `CLAUDE.md` symlink (the `cargo xtask lint agents` gate enforces all three — create the symlink with `ln -s AGENTS.md CLAUDE.md`), self-named modules, sibling tests.
- Tier: a test-support crate is exactly the case the roadmap's dev-edge rule exists for — production crates must NEVER depend on it (dev-deps only). If plan 012's `TIERS` table exists, add the crate at the tier above `jackin-core` (it depends only on core) and rely on the dev-edge rule; the gate's completeness check will demand the entry.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| New crate | `cargo check -p jackin-test-support` | exit 0 |
| Repointed suites | `cargo nextest run -p jackin-isolation -p jackin-runtime` | all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Arch gate | `cargo run -p jackin-xtask -- lint arch --strict` | OK, and (post-012) no dev-cycle allowlist needed |
| Dep hygiene | `cargo shear --deny-warnings` | exit 0 |
| Agents gate | `cargo run -p jackin-xtask -- lint agents` | OK (new crate's files present) |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-test-support/` (create: Cargo.toml, README.md, AGENTS.md, CLAUDE.md symlink, src/lib.rs, moved modules + their tests)
- `crates/jackin-runtime/src/runtime/test_support.rs` (shrinks to a re-export shim or is deleted — Step 4 decides by consumer count)
- `crates/jackin-isolation/Cargo.toml` + its 4 test files' imports
- Root `Cargo.toml` (members + workspace.dependencies)
- `crates/jackin-xtask/src/arch.rs` ONLY the `TIERS` entry + `DEV_CYCLE_ALLOWLIST` row deletion (post-012; skip if 012 not landed)
- Roadmap Phase 2/3 status notes; `plans/code-health/README.md` (row + strike TEST-support-crate and DEBT-devdep-cycle)

**Out of scope**:
- Deduplicating `crates/jackin/tests/common.rs` FakeRunner, the host/caffeinate fakes, and console StubRunners (follow-up; note them in the new crate's README as candidates)
- Adding NEW helpers (clock/snapshot normalization/fixture builders — the Phase 3 wishlist grows into this crate later; ship only what moves)
- Any production (non-dev) dependency on the new crate — hard boundary
- `install_all_test_stubs`/seed helpers IF they turn out runtime-coupled (see STOP)

## Git workflow

- Branch off `main`: `refactor/test-support-crate`.
- Commits: crate scaffold; move + runtime shim; isolation repoint; arch-gate cleanup; docs. `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: Read the source module fully; classify items

Read `crates/jackin-runtime/src/runtime/test_support.rs` end to end. For each pub item, record its dependencies: core-only (moves), runtime-typed (stays). Expected: `FakeRunner`, `FakeDockerClient` move (they implement core ports); `install_all_test_stubs`, `seed_valid_role_repo`, `first_temp_role_repo` move if they touch only `jackin_core::paths` + std/tempfile — verify. If any mover drags a runtime type, STOP and report the item.

**Verify**: a written classification in your working notes; no code yet.

### Step 2: Scaffold `crates/jackin-test-support`

Cargo.toml: `[package] name = "jackin-test-support"`, workspace-inherited fields, `[lints] workspace = true`; dependencies: `jackin-core = { workspace = true }`, `tempfile = { workspace = true }` (+ exactly what Step 1's classification needs — nothing speculative; note: tempfile becomes a NORMAL dependency here, which is correct for a test-support crate consumed via dev-deps). Root Cargo.toml: members entry + workspace.dependencies entry. README per the `crates/AGENTS.md` template (purpose: canonical fakes/builders for workspace tests; tier: test-support — production crates must never depend on it; structure table; public API: the moved items; verify: `cargo nextest run -p jackin-test-support`). AGENTS.md (non-derivable rules only, e.g. "fakes stay deterministic — no wall-clock, no randomness without a seed; production crates must never depend on this crate, dev-dependencies only"). `ln -s AGENTS.md CLAUDE.md`.

`src/lib.rs`: `//!` ownership header (owns / tier / entry point — match plan 016's contract if it landed: `Entry point:` line naming `FakeDockerClient`).

**Verify**: `cargo check -p jackin-test-support` → exit 0; `cargo run -p jackin-xtask -- lint agents` → OK.

### Step 3: Move the movers

Move the classified items into `crates/jackin-test-support/src/` (module layout: `runner.rs` for FakeRunner, `docker.rs` for FakeDockerClient, `seed.rs` for the seed/stub helpers — or one `lib.rs` if the total is small; match size to content, no padding). Bring their unit tests along into sibling `tests.rs` files if any exist in the source module. Items keep their exact public signatures and behavior — this is a move, not a rewrite.

**Verify**: `cargo nextest run -p jackin-test-support` → passes (or "no tests" cleanly if none moved); `cargo clippy -p jackin-test-support --all-targets -- -D warnings` → exit 0.

### Step 4: Repoint consumers; shrink or shim the runtime module

1. `crates/jackin-isolation/Cargo.toml`: dev-dep `jackin-runtime … features=["test-support"]` → `jackin-test-support = { workspace = true }`. Update the 4 test files' `use` paths (`jackin_runtime::runtime::test_support::X` → `jackin_test_support::X`).
2. Inventory remaining `runtime::test_support` consumers (`rg -ln 'runtime::test_support' crates/`). If ONLY jackin-runtime's own tests remain: keep `test_support.rs` as a thin re-export (`pub use jackin_test_support::*;` behind the existing feature, with runtime-coupled stragglers from Step 1 staying as real code) OR repoint runtime's own tests to the new crate and delete the module + the `test-support` feature if nothing else uses it — choose deletion when `rg 'jackin-runtime.*test-support' crates/*/Cargo.toml` shows no external enabler; a dead feature is debt. `jackin-runtime` gains `jackin-test-support` as a dev-dependency (downward-in-dev is fine: test-support sits low, just above core).
3. `cargo shear --deny-warnings` must stay clean (it catches the now-unused dev-dep if you missed a removal).

**Verify**: `cargo nextest run -p jackin-isolation -p jackin-runtime` → all pass; `cargo shear --deny-warnings` → exit 0; `rg -n 'jackin_runtime::runtime::test_support' crates/jackin-isolation/` → 0 matches.

### Step 5: Arch-gate cleanup (conditional on plan 012)

If arch.rs has the tier model: add `("jackin-test-support", <tier of core + 1>)` to `TIERS`; delete the `("jackin-isolation", "jackin-runtime")` row from `DEV_CYCLE_ALLOWLIST` — the gate's stale-row check would demand this anyway once the cycle is gone. If 012 has NOT landed: just confirm `cargo xtask lint arch --strict` still passes and note in the PR body that the cycle no longer exists.

**Verify**: `cargo run -p jackin-xtask -- lint arch --strict` → OK; (post-012) `rg -c 'jackin-isolation' crates/jackin-xtask/src/arch.rs` → 0 or only the TIERS row.

### Step 6: Docs + ledger

- Roadmap: Phase 2 fakes-move item + Phase 3 item 1 → shipped (crate exists; dedupe of the 3 remaining duplicate fakes recorded as follow-up in the crate README's "candidates" note); the runtime-launch spec's test seams unaffected.
- `plans/code-health/README.md`: strike TEST-support-crate and DEBT-devdep-cycle (→ planned/this plan); note the remaining duplicate-fake dedupe as a small deferred item.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo nextest run --workspace --all-features --locked` → all pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- No new tests required beyond what moves; the deliverable is structural. The moved fakes' consumers (isolation's 4 files + runtime's suites) passing unchanged IS the verification.
- Negative check: `cargo tree -p jackin-test-support -i --edges normal 2>/dev/null` (inverse, normal deps only) → no production crate lists it; document the command in the crate README's verify section.

## Done criteria

- [ ] `jackin-test-support` exists with README/AGENTS/CLAUDE.md; agents gate passes
- [ ] isolation no longer dev-depends on runtime (`rg 'jackin-runtime' crates/jackin-isolation/Cargo.toml` → 0)
- [ ] The prod+dev cycle is gone; arch gate clean; (post-012) allowlist row deleted
- [ ] `cargo shear`, workspace clippy, workspace nextest all green
- [ ] No production crate depends on jackin-test-support (inverse-tree check)
- [ ] Roadmap + ledger updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

Stop and report back if:

- Step 1 finds a to-move item referencing `jackin-runtime` types (e.g. FakeDockerClient methods returning runtime structs) — the move then needs a port change in core, which is a different plan.
- More than 6 crates consume `runtime::test_support` (bigger blast radius than inventoried).
- `cargo shear` flags the new crate arrangement in a way that needs a shear config change.
- Deleting the `test-support` feature breaks a feature-chain in `crates/jackin/Cargo.toml` (`test-support = ["jackin-env/test-support"]`) — report the chain rather than rewiring features speculatively.

## Maintenance notes

- Future Phase 3 helpers (deterministic builders, snapshot normalization, fixed dims/theme, the `ManualClock` re-export from plan 024's `jackin_core::clock`) land HERE, one PR each — the crate README's candidates note is the queue (jackin/tests FakeRunner, host/caffeinate pair).
- The dev-edge rule in the arch gate is this crate's guardrail: test-support may be depended on by anyone's dev-deps and may depend only downward itself.
- Reviewer should scrutinize: that moved code is byte-identical (move, not rewrite) and that the runtime `test-support` feature's fate (shim vs deletion) matches actual consumer count.

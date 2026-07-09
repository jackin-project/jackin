# Plan 028: Phase 1 — dependency hygiene sweep: wrap turso behind a store trait, drop the stale ring exception, annotate version pins

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat c856acc9d..HEAD -- Cargo.toml deny.toml crates/jackin-usage/`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: M (three independent S items)
- **Risk**: LOW-MED (the turso wrap moves call sites; deny/pin edits are config-only)
- **Depends on**: none
- **Category**: migration
- **Planned at**: commit `c856acc9d`, 2026-07-09

## Why this matters

Three recorded dependency findings from the first-wave audit, aging unaddressed. (1) **DEP-turso-wrap**: the usage-accounting path sits on a pre-release pinned database crate (`turso = "=0.7.0-pre.17"`); direct `turso::` usage has already spread from 2 files to 3 since the audit — exactly the creep a wrapper prevents. When turso stabilizes (or must be swapped), the migration should be a one-module change, not a hunt. (2) **DEP-ring-license**: `deny.toml` carries a version-pinned license exception for `ring@0.17.14` that cargo-deny reports as unencountered — a stale row in a security-relevant allowlist teaches readers that stale rows are normal. (3) **DEP-pin-rationale**: five runtime `=` pins carry no rationale, so nobody can tell an intentional freeze from leftover caution, and Renovate-era updates stall on them silently.

## Current state

Verified at the planning commit.

- `turso::` usage (grown since the audit's "only 2 files"): `crates/jackin-usage/src/telemetry_store.rs`, `crates/jackin-usage/src/token_monitor/opencode.rs`, `crates/jackin-usage/src/telemetry_store/tests.rs`. Workspace pin: `Cargo.toml:112` `turso = { version = "=0.7.0-pre.17", default-features = false }`.
- `deny.toml:59`: `{ crate = "ring@0.17.14", allow = ["Apache-2.0", "ISC"] },` — confirm staleness yourself in Step 2 (the gate output names unencountered exceptions).
- The five unannotated runtime `=` pins in `Cargo.toml` `[workspace.dependencies]`: `serde_json = "=1.0.150"` (:96), `tokio = { version = "=1.52.3", … }` (:104), `toml = "=1.1.2"` (:106), `turso = "=0.7.0-pre.17"` (:112), `ratatui-core = "=0.1.2"` (:89). (Dev-tool pins like `insta`/`criterion`/`assert_cmd` are fine as-is — test-only determinism is self-explanatory; scope is the five runtime pins.)
- Conventions: comments state non-obvious WHY only; crates/AGENTS.md requires any non-Apache/MIT license exception to be version-pinned with a short comment; `.cargo/audit.toml` must mirror deny.toml advisory ignores (check whether ring appears there too).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Usage crate | `cargo nextest run -p jackin-usage` / `cargo clippy -p jackin-usage --all-targets -- -D warnings` | all pass / exit 0 |
| Deny gate | `cargo deny check licenses bans sources` | clean, no `license-exception-not-encountered` warning |
| Audit gate | `cargo audit` | clean |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-usage/src/` — one store-boundary module wrapping every `turso::` touchpoint (Step 1) + the three files' imports
- `deny.toml` (the ring row) and `.cargo/audit.toml` only if it mirrors the row
- Root `Cargo.toml` (comment lines above the five pins; optionally the two relaxations in Step 3)
- `crates/jackin-usage/README.md` (structure row for the new module)
- `plans/code-health/README.md` (row + strike DEP-turso-wrap, DEP-ring-license, DEP-pin-rationale)

**Out of scope**:
- Changing turso's version or feature set; swapping the DB
- Any other deny.toml row (the ~51 `bans.skip` rows are settled mechanism — first-wave rejection)
- Renovate config

## Git workflow

- Branch off `main`: `chore/dependency-hygiene`.
- One commit per step, `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: Store boundary for turso

Read the three `turso::`-using files. Create `crates/jackin-usage/src/store_backend.rs` (or fold into the existing `telemetry_store.rs` root if the types all live there — pick whichever keeps ONE file importing `turso`): a thin module owning every `use turso::…` in the crate — connection open, statement execution, row extraction types. The two other files consume it through crate-internal functions/types instead of importing turso directly. This is an indirection move, not an abstraction design: no new trait unless the call sites already share an obvious shape (if a trait falls out naturally in <50 lines, fine; otherwise plain `pub(crate)` functions/newtypes are the deliverable). Behavior identical; the test file may keep direct turso usage ONLY if it tests the boundary module itself — otherwise route it through the boundary too.

**Verify**: `rg -l 'use turso|turso::' crates/jackin-usage/src -g '*.rs'` → exactly the one boundary module (plus its own `tests.rs` if applicable); `cargo nextest run -p jackin-usage` → all pass.

### Step 2: Drop the stale ring exception

Run `cargo deny check licenses` and read its output for `ring@0.17.14`. If it reports the exception as unencountered/unused: delete `deny.toml:59` and any `.cargo/audit.toml` mirror, re-run. If ring IS still in the tree (the audit's staleness claim no longer holds): leave the row, update its comment with the current pulling dependency, and note the reversal in the PR body.

**Verify**: `cargo deny check licenses bans sources` → clean with no unencountered-exception warning; `cargo audit` → clean.

### Step 3: Pin rationale comments

Above each of the five pins add a one-line `#` comment: why pinned + the revisit trigger. Derive each reason from git history (`git log -3 --oneline -S '=1.52.3' Cargo.toml` style archaeology per pin) and the crate's situation — do not invent. Expected shapes: turso ("pre-release API churn; revisit at first stable 0.7"), ratatui-core ("must stay lockstep-compatible with ratatui 0.30 — see caret dep; revisit on ratatui minor bump"). If archaeology yields a reason to RELAX a pin (the audit suggested `tokio =1.52.3` → `~1.52` and `ratatui-core =0.1.2` → `"0.1"`), do NOT relax in this plan — record the suggestion in the comment ("candidate: relax to ~1.52 — needs a lockfile-refresh PR"); pin relaxation changes resolution and belongs to a dependency-update PR with its own test run.

**Verify**: `grep -B1 '"=..*"' Cargo.toml | grep -c '#'` shows a comment above each runtime pin; `cargo xtask ci --fast` → `ci gate OK` (nothing resolved differently).

## Test plan

- No new tests; Step 1 is behavior-preserving indirection covered by the existing jackin-usage suite (including its turso-backed store tests).
- Gates in each step's Verify are the checks.

## Done criteria

- [ ] One module owns all turso imports; crate suite green
- [ ] Ring exception deleted (or comment-corrected with the reversal reported)
- [ ] Five runtime pins carry derived rationale comments; no version resolution changed (`git diff Cargo.lock` empty)
- [ ] `cargo deny check licenses bans sources` + `cargo audit` clean
- [ ] `cargo xtask ci --fast` → `ci gate OK`; index row updated + three findings struck

## STOP conditions

Stop and report back if:

- The turso call sites don't factor into one module under ~150 lines of movement (the crate is more coupled to turso than the audit measured).
- Deleting the ring row makes `cargo deny` FAIL (something still pulls ring with a non-allowlisted license — report the dependency path).
- Pin archaeology finds a pin was added to dodge a specific bug (then the comment must cite it — if the bug is unfindable, write "reason unrecovered; candidate for relaxation PR" rather than a guess).

## Maintenance notes

- New usage-crate DB work imports the boundary module, never turso directly — a one-line rule worth adding to `crates/jackin-usage/AGENTS.md` if it isn't derivable (it is derivable from the module structure once this lands; add only if violations recur).
- When turso stabilizes: the swap/upgrade PR touches the boundary module + the pin comment only.
- Reviewer should scrutinize: Step 1 hunks are pure moves (no query/behavior edits), and each pin comment's reason traces to real history.

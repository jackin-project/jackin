# Plan 016: Phase 6 — crate ownership headers everywhere, a headers gate, and `.git-blame-ignore-revs`

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- crates/*/src/lib.rs crates/*/src/main.rs crates/jackin-xtask/src/ crates/AGENTS.md`
> If plan 012 has landed (expected — it is a dependency), arch.rs will differ
> from `47dd5fca0`; that is fine. For everything else, on a mismatch with the
> "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (doc comments + one new read-only gate)
- **Depends on**: plans/code-health/012-tier-graph-arch-gate.md (the `TIERS` table is the tier source of truth this plan's headers and gate must match)
- **Category**: dx
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

Roadmap Phase 6 item 1 is a hard rule: every `lib.rs`/`main.rs` opens with a `//!` block stating what the crate owns, its tier, and the one entry point an agent should copy — because an agent's first read of a crate decides where it goes next, and the repo's own research says duplicated/absent orientation measurably hurts agent task success. Measured conformance: 25 of 28 root files have some `//!` header, but the two flagship binaries (`crates/jackin/src/main.rs`, `crates/jackin-capsule/src/main.rs`) and `crates/jackin-pr-trailers/src/main.rs` have none at all; 6 lib.rs headers state no usable tier; **zero of 28** state an explicit entry-point-to-copy; and four inconsistent tier vocabularies coexist (headers, README sections, the `crates/AGENTS.md` template, Cargo.toml comments — `jackin-diagnostics/Cargo.toml:51` calls the crate L1 while its own header says L2). Nothing gates any of this. Separately, Phase 6 item 8's archaeology half: `.git-blame-ignore-revs` does not exist, so `git blame` attributes thousands of lines to the mass layout sweep `46511939d refactor(workspace): enforce codebase health layout (#664)` instead of their real authors.

## Current state

- Best-conforming header, the template to copy (`crates/jackin-core/src/lib.rs:1-7`, verified):

  ```rust
  //! jackin-core: universal vocabulary types shared across all jackin❯ crates.
  //!
  //! This is a leaf crate — it has no jackin❯ dependencies, no tokio, no
  //! subprocess, no filesystem access. Every higher crate depends on this one,
  //! never the reverse.
  //!
  //! **Architecture Invariant:** L0 domain crate. Allowed dependencies: none
  ```

  It has owns + tier but no explicit "entry point to copy" line — even the best header needs one line added.
- Headerless roots (verified): `crates/jackin/src/main.rs` opens with `#![expect(clippy::print_stdout…` (line 1); `crates/jackin-capsule/src/main.rs` opens with `use anyhow::…`; `crates/jackin-pr-trailers/src/main.rs` similarly bare.
- No-tier or numberless headers (audit table): `jackin-launch-tui` ("a **presentation** crate", no number), `jackin-usage` (deps only), `jackin-isolation` (bullet list, no number), `jackin-agent-status`, `jackin-build-meta` ("Build-script helpers shared by jackin crates.", no tier at all), `jackin-console-oppicker` (one vague line).
- After plan 012, tier truth lives in `crates/jackin-xtask/src/arch.rs`'s `TIERS: &[(&str, u8)]` table (graph-derived depths 0-6). Headers must state the same number.
- The `crates/AGENTS.md` README template still says `<tier: L0 leaf / L1 domain / L2 infrastructure / presentation / binary / xtask>` — a third vocabulary; plan 012's roadmap note flagged it for reconciliation here.
- Gate landscape: `LintCommand` (main.rs:123-139) has `Files | Tests | Agents | AgentLinks | Arch` (+ `Suppressions` if plan 011 landed); `run_all_lints` chains them. A `headers` gate slots in identically.
- `.git-blame-ignore-revs`: absent (verified). Known mass-mechanical commit: `46511939d` (workspace layout sweep, PR #664). Others must be found from history.
- Conventions: brand is `jackin❯` in doc-comment prose; headers are rustdoc, so they also feed plan 011's rustdoc lints — intra-doc links in what you write must resolve.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| New gate | `cargo run -p jackin-xtask -- lint headers` | `headers gate OK — 28 files checked` |
| Xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| Doc build (headers are rustdoc) | `cargo doc --workspace --no-deps --locked` | exit 0, no warnings |
| Blame config check | `git blame --ignore-revs-file .git-blame-ignore-revs crates/jackin-core/src/lib.rs \| head -3` | runs without error |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- The `//!` header block (only the leading doc-comment block) of every `crates/*/src/lib.rs` and the four binary `main.rs` roots (`jackin`, `jackin-capsule`, `jackin-tui-lookbook`, `jackin-pr-trailers`; `jackin-xtask`/`jackin-dev` mains already have usage headers — bring them to format)
- `crates/jackin-xtask/src/headers.rs` (create) + `headers/tests.rs` + `main.rs` registration + README row
- `crates/AGENTS.md` (README-template tier vocabulary line only)
- `.git-blame-ignore-revs` (create) + a short note in `CONTRIBUTING.md`
- Roadmap Phase 6 status

**Out of scope**:
- Any code below the header block in any crate
- README "Architecture tier" section rewrites (the gate checks lib.rs/main.rs only; README drift is caught by the per-crate authority rules)
- arch.rs (read its `TIERS` table; do not modify)
- The machine-readable gate-output program (`--format json` on all gates — recorded, stays with DX-gate-json next wave; plan 010's health lane set the pattern)

## Git workflow

- Branch off `main`: `docs/crate-ownership-headers`.
- Conventional Commits (`docs(crates): …`, `feat(xtask): add headers gate`), `-s`, push per commit. PR to `main`; do not merge.

## Steps

### Step 1: Define the header contract and backfill all 28 files

Contract — the leading `//!` block of every `lib.rs`/binary `main.rs` must contain, in any order within the first 15 doc lines:
1. **Owns**: first line, `//! <crate-name>: <one sentence — what this crate owns>`.
2. **Tier**: a line matching `**Architecture Invariant:** T<n> …` where `<n>` equals the crate's number in arch.rs `TIERS`. Keep any existing allowed-dependency prose after it. (This retires the L0-L4 vocabulary: rewrite existing `L<n>` mentions in header lines to the plan-012 `T<n>` numbering. Do not touch `L…` mentions elsewhere in files.)
3. **Entry point**: a line `//! Entry point: [\`<item>\`] — <why you'd copy it>.` naming the one type/fn an agent should copy from this crate (pick the crate's primary re-export or most-constructed type: read the README "Public API" section — every crate has one — and cite its first-named item; for binaries, name the run/entry fn).

Backfill: 3 headerless mains get full headers; 6 tierless headers get the tier line; all 28 get the entry-point line. For `jackin-core`, the excerpt above becomes the worked example — add `//! Entry point: …` and convert `L0 domain crate` to `T0`.

**Verify**: `cargo doc --workspace --no-deps --locked` → clean (proves added intra-doc links resolve); spot-check `head -12 crates/jackin/src/main.rs` shows the three lines.

### Step 2: `cargo xtask lint headers`

Create `headers.rs`: for every workspace member (reuse the member-dir walk from `agent_files.rs`), read `src/lib.rs` or `src/main.rs`, extract the leading `//!` block, and check the three contract elements: first line matches `^//! <dirname>: ` (crate name prefix), a `**Architecture Invariant:** T(\d+)` line whose number equals the arch.rs `TIERS` value for that crate (import the table: make `TIERS` `pub(crate)` in arch.rs — this one-line visibility change is permitted despite arch.rs being otherwise out of scope), and an `Entry point:` line. Failure messages state the missing element and the fix (quote the contract line to add), plus the rerun command. Register as `LintCommand::Headers` and chain into `run_all_lints`.

**Verify**: `cargo run -p jackin-xtask -- lint headers` → `headers gate OK — 28 files checked, tiers consistent with arch gate`; probe: change one header's tier number, rerun → fails naming the crate and both numbers; revert probe.

### Step 3: Reconcile the AGENTS template vocabulary

In `crates/AGENTS.md`, update the README-template tier line from `<tier: L0 leaf / L1 domain / L2 infrastructure / presentation / binary / xtask>` to reference the single source: `<tier: T<n> — must match the TIERS table in crates/jackin-xtask/src/arch.rs (checked by cargo xtask lint headers)>`.

**Verify**: `cargo run -p jackin-xtask -- lint agent-links` → OK (the edit adds no AGENTS-to-AGENTS link); `rg -c 'L0 leaf' crates/AGENTS.md` → 0.

### Step 4: `.git-blame-ignore-revs`

1. Find mass-mechanical commits: `git log --oneline --grep='fmt\|layout\|rename' --all | head -20` plus `git log --stat --format='%H %s'` review of anything touching >150 files with a `refactor`/`style` subject. Seed the file with full SHAs, one per line, each preceded by a `# <subject>` comment — at minimum the full SHA of `46511939d`. Include only commits that are genuinely mechanical (layout moves, fmt sweeps); when unsure, leave it out.
2. Create `.git-blame-ignore-revs` at the root with a header comment explaining purpose and the config command.
3. Add to `CONTRIBUTING.md` (near the setup section): `git config blame.ignoreRevsFile .git-blame-ignore-revs` — one sentence on why.

**Verify**: `git blame --ignore-revs-file .git-blame-ignore-revs crates/jackin-core/src/lib.rs | head -3` → executes cleanly (proves every SHA in the file is valid — git errors on unknown revs).

### Step 5: Roadmap + README + gate

- Roadmap Phase 6: item 1 shipped (headers + gate), item 8 archaeology-half shipped (blame file; RA lane still open), item 2 (narrowest-verification map) shipped via plan 010's TESTING.md matrix — cross-reference it; items 6 (machine-readable gates) and 7 (context-economy budgets → plan 017) remain open.
- `crates/jackin-xtask/README.md`: `headers.rs` row.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo nextest run -p jackin-xtask` → green; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- `headers/tests.rs`: contract parser on fixture headers — missing owns-line, wrong crate name, missing tier, tier≠TIERS value, missing entry line, fully conformant. Pattern: `agent_files.rs` tests.
- Full workspace doc build + xtask suite green.

## Done criteria

- [ ] All 28 root files carry owns/tier/entry headers; tiers use `T<n>` matching arch.rs
- [ ] `cargo xtask lint headers` passes and runs inside `cargo xtask lint`
- [ ] `crates/AGENTS.md` template names the single tier source
- [ ] `.git-blame-ignore-revs` exists, valid SHAs, documented in CONTRIBUTING.md
- [ ] `cargo doc --workspace --no-deps --locked` clean
- [ ] Roadmap Phase 6 statuses updated; `cargo xtask ci --fast` → `ci gate OK`
- [ ] `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Plan 012 has not landed (no `TIERS` table in arch.rs) — the tier cross-check has no source of truth; do not invent numbers.
- A crate's README has no "Public API" section to derive the entry-point line from (the README is then in violation of `crates/AGENTS.md` — report which, cite first-wave DOCS-capsule-readme if it is jackin-capsule).
- Rewriting a header would delete crate-specific invariant prose you don't understand (e.g. jackin-term's damage-tracking rules) — keep the prose, add only the contract lines, and note it.
- More than 3 candidate commits for blame-ignore are ambiguous (mechanical-or-not unclear).

## Maintenance notes

- The headers gate + arch TIERS are now a two-sided contract: re-tiering a crate means updating both (each gate's failure message points at the other).
- Plan 017's doc-token budget will measure these headers (they are agent-context spend); keep them within ~15 doc lines.
- Reviewer should scrutinize: entry-point line choices (the "one thing to copy" must actually be the canonical API, not an internal helper) and that no header edit disturbed code below it (diff should be pure `//!` lines + the one arch.rs visibility change).

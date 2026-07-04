# Plan 022: Investigate whether `jackin-console`'s 11-parameter generics still pay for themselves

> **Executor instructions**: **Investigate-and-recommend** plan (MED confidence, HIGH risk to change).
> Produce a decision + a scoped follow-up plan; do NOT reshape the generics inline. Update `plans/README.md`.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-console/src/tui/screens/editor/model crates/jackin/src/console/tui.rs`

## Status

- **Priority**: P3
- **Effort**: L (if pursued) / M (investigation)
- **Risk**: HIGH
- **Depends on**: none (blocks 023)
- **Category**: tech-debt
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`jackin-console` is a generic functional-core bridged to the binary by a proliferation of narrow traits.
`EditorState` is generic over **11 type parameters**; the crate defines ~94 traits, including 19 `Console*`
bridge traits and dozens of one-line state-accessor traits, and 22 source files spell out the full
`EditorState<…>` generic list. The binary carries a whole "Transitional root-console TUI facade" module
just to instantiate these generics with concrete types. Every core function signature drags long generic
bounds; adding one field can require touching a trait def + impls + the 11-param bound lists across ~22
sites. This is a deliberate functional-core/imperative-shell decoupling — the question is whether its
**cost** now outweighs its benefit, which is a judgment call the maintainer must make with data, not a
mechanical fix.

## Current state

- `crates/jackin-console/src/tui/screens/editor/model/state_impl.rs:15-44` — `EditorState` generic over 11
  params (`WorkspaceConfig, MountInfoCache, Modal, SaveFlow, EnvValue, AuthFormTarget, PendingTokenGenerate,
  PendingRoleLoad, PendingDriftCheck, PendingIsolationCleanup, PendingOpCommit`); the trait implemented here
  (`ConsoleEditorModalPresence`) has one method `fn editor_modal_open(&self) -> bool { self.modal.is_some() }`.
- `crates/jackin-console/src` — ~94 trait defs; 19 `Console*` bridge traits; many 1-line accessor traits;
  22 files spelling the full generic list.
- `crates/jackin/src/console/tui.rs:1` — "Transitional root-console TUI facade" module instantiating the
  generics with concrete types.

## Steps

### Step 1: Quantify the cost

- Count: trait defs (`grep -rc "^trait \|^pub trait \|pub(crate) trait " crates/jackin-console/src`), the
  `EditorState<` spell-out sites, and the single-impl traits (traits with exactly one `impl`).
- Identify how many of the 11 type params ever take more than one concrete type in the whole codebase (a
  param that's only ever one concrete type is pure ceremony). Grep each param's concrete substitution.
- Record the numbers in this plan's row note.

### Step 2: Prototype a collapse on ONE axis (throwaway spike)

On a scratch branch (or a `git stash`-able experiment — do NOT commit source changes; this plan only writes
under `plans/`), try collapsing the type params that only ever have one concrete type into concrete types
owned by `jackin-console`, and see whether the `Console*` bridge traits for those can be deleted. Measure:
does `cargo check -p jackin-console` still pass, and how many sites shrink? Discard the spike; capture the
finding.

### Step 3: Recommend and write the follow-up

Write the next monotonic follow-up plan (`plans/044-console-generics-collapse.md` if no newer plan exists;
otherwise use the next available number) **only if** Step 2 shows a real win: specify which params to
concretize, which traits to delete, the exact 22 sites to touch, and a step order that keeps the crate
compiling between steps. If the decoupling still pays (params genuinely vary), mark this plan
`REJECTED (generics justified — measured)` with the data.

## Done criteria

- [ ] Cost quantified (trait count, single-concrete params, spell-out sites) in the row note
- [ ] Spike result recorded (does a collapse compile / how much shrinks)
- [ ] Either next-numbered `plans/NNN-console-generics-collapse.md` written with concrete scope, or plan
      `REJECTED` with data
- [ ] **No source committed by this plan** (only `plans/` files)

## STOP conditions

- The spike triggers cascading type-inference breaks that can't be bounded in a day of investigation —
  report that the collapse is high-risk and recommend deferring; do not leave the tree broken.

## Maintenance notes

- This blocks plan 023 (decomposition) — decide the generics question before restructuring the crate, or
  the decomposition churns twice.
- Whatever is decided, record it as an ADR (the repo has an ADR set at `docs/content/docs/reference/adrs/`)
  so this isn't re-litigated.

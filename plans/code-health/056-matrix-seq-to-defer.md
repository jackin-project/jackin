# Plan 056: Convert coverage-matrix SEQ debt to DEFER + residual ledger

> Wave 8 matrix closure: acceptance criterion 2 forbids open SEQ debt left
> unplanned/unexecuted. Multi-PR design items become DEFER(measured) with
> residual-ledger rows; executable debt is either planned or deferred with
> numbers.

## Status

- **Priority**: P1
- **Effort**: S-M
- **Depends on**: plan 055 residual ledger exists
- **Category**: docs / process
- **Planned at**: 2026-07-11 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Steps

1. Enumerate every `SEQ(` marker in `plans/code-health/README.md` coverage
   matrix (~19 occurrences / 12 themes).
2. For each theme, add a **DEFER(measured)** residual-ledger row (R-* IDs in
   `RESIDUAL_LEDGER.md`) with counts/blockers and next-PR trigger.
3. Rewrite matrix lines: replace `SEQ(...)` with
   `DEFER(see RESIDUAL_LEDGER R-…)` (or inline measured DEFER text).
4. Legend: retire “SEQ = sequenced behind unexecuted plan” as an open-debt
   status for this program; historical SEQ language may remain only inside
   DEFER reasons.
5. Update `VERIFICATION.md` Phase 0–8 disposition table to match.
6. Verify: `rg 'SEQ\\(' plans/code-health/README.md` → zero matches in the
   coverage matrix section (program complete).

## Done criteria

- [x] Zero open `SEQ(` markers in the coverage matrix
- [x] Every former SEQ theme has a residual-ledger row
- [x] VERIFICATION.md matches DEFER dispositions
- [x] README legend documents DEFER + residual ledger as the residual path

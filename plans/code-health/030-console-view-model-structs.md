# Plan 030: Phase 2 — named row/view-model structs for the console editor/settings view builders

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat c856acc9d..HEAD -- crates/jackin-console/src/tui/screens/editor/ crates/jackin-console/src/tui/screens/settings/`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M-L
- **Risk**: MED (touches view-building code across two screens; TUI snapshots + parity tests are the net)
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `c856acc9d`, 2026-07-09

## Why this matters

Roadmap Phase 2, TUI structure item 3, verbatim: "Replace repeated type-complexity suppressions in settings/editor view builders with named row/view model structs and shared form-section builders." Measured: the workspace's largest suppression cluster is `clippy::type_complexity` — 47 attributes, **45 of them bare** (no `reason =`), with 34 in `jackin-console` and **19 in `editor/view/frame.rs` alone** plus 14 in `settings/view.rs`. Each suppression marks a function signature whose nested tuple/closure return type outgrew the type system's readability — exactly the shape the roadmap wants replaced with named structs. This one refactor is double-leverage: it retires the roadmap item AND burns the single biggest bare-`#[allow]` cluster feeding plan 011's suppression ratchet (jackin-console holds 51% of all bare suppressions; this cluster is its core).

## Current state

Verified at the planning commit.

- Suppression map (`rg -c 'type_complexity' crates/jackin-console/src -g '*.rs'`): `tui/screens/editor/view/frame.rs` **19**, `tui/screens/settings/view.rs` **14**, `editor/view/{secrets_tab,roles_tab,mounts_tab,general_tab}.rs` **2 each** (+ a few elsewhere in the crate — inventory in Step 1). All are bare `#[allow(clippy::type_complexity)]` (e.g. `frame.rs:29,48,126,158`).
- Shape of the debt (read before designing): `frame.rs:29` sits above `pub(crate) fn editor_frame_areas(area: Rect, footer_h: u16) -> EditorFrameAreas` — note some suppressions cover *parameter* complexity (closures passed in) rather than return tuples; the fix differs per shape:
  - Returned tuple bundles → a named struct with doc'd fields (the `EditorFrameAreas` pattern already exists in the same file — it is the exemplar; some functions apparently return named structs yet still carry the allow, which means the complexity is in closure params or nested generics — classify each).
  - Closure-parameter trains → a named `struct XxxCtx<'a>`/callback trait alias, or restructuring so the builder takes the model + returns rows instead of taking N closures.
  - Nested `Vec<(a, (b, c), Box<dyn Fn…>)>` row types → the roadmap's named row model: `struct FieldRow { label, value, hint, … }` style.
- Roadmap-adjacent context the design must respect: Phase 2 TUI item 2 plans a bigger settings/editor convergence ("shared per-domain edit models, validation, render rows…"). THIS plan is deliberately narrower: name the types the builders already pass around; do not attempt the settings↔editor unification (out of scope) — but name things so the unification can adopt them (prefer neutral names like `FieldRow`/`SectionRows` over screen-specific ones when the same shape appears in both screens).
- Safety nets: jackin-console has insta snapshots (6 `.snap` files) and per-module `tests.rs` suites (`editor` and `settings` both have test files — inventory with `ls crates/jackin-console/src/tui/screens/editor/view/` and check which modules have sibling `tests.rs`). The suites must pass unchanged — this is a pure representational refactor.
- Conventions: no inline test modules; workspace lints (the refactor must not introduce `too_many_arguments`/`too_many_lines` hits above `clippy.toml` thresholds 7/150); suppressions that genuinely must survive become `#[expect(clippy::type_complexity, reason = "…")]`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate suite | `cargo nextest run -p jackin-console` | all pass |
| Clippy | `cargo clippy -p jackin-console --all-targets -- -D warnings` | exit 0 |
| Suppression count | `rg -c 'type_complexity' crates/jackin-console/src -g '*.rs' \| awk -F: '{s+=$2} END {print s}'` | shrinking; target ≤ 8 |
| Snapshot review (if pending) | `ls crates/jackin-console/**/*.pending-snap` | none |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-console/src/tui/screens/editor/view/*.rs` and `crates/jackin-console/src/tui/screens/settings/view.rs` (+ their sibling `tests.rs` files where signatures they call changed)
- New shared row/section model types — place them where both screens can reach without new deps: a `crates/jackin-console/src/tui/screens/form_model.rs` (or the crate's existing shared-components location — find where cross-screen view types already live: `rg -l 'pub(crate) struct' crates/jackin-console/src/tui/components | head` and match)
- `crates/jackin-console/README.md` structure row if a module is added
- `plans/code-health/README.md` (row + strike DEBT-type-complexity; update the bare-allow note)

**Out of scope**:
- The settings↔editor unification program (Phase 2 TUI item 2) — no shared edit-model/validation/persistence changes
- Any rendering behavior change — snapshots must not change (a changed `.snap` is a failed refactor, not a snapshot to accept)
- `jackin-tui` shared components; other crates' type_complexity suppressions (~13 outside console — inventory and leave)
- Function bodies beyond what the signature rename forces

## Git workflow

- Branch off `main`: `refactor/console-view-models`.
- Commits per file-cluster (frame.rs; settings/view.rs; the four tabs; shared model module), `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: Classify all 47 sites

`rg -n -B1 -A3 'type_complexity' crates/jackin-console/src -g '*.rs'` — for each site record: file:line, the offending type shape (return tuple / closure params / row vec), and the fix class (named struct / ctx struct / row model / justified-expect). Sites OUTSIDE the editor/settings view builders (the crate has ~34 of 47; the remainder elsewhere in console and ~13 in other crates) are inventoried but untouched — list them in the PR body as the residue. If >6 in-scope sites classify as "justified-expect" (irreducible), STOP — the named-struct approach isn't fitting and the design needs review.

**Verify**: classification table in the PR description draft covering every in-scope site.

### Step 2: The shared row/section models

From the classification, define the minimal shared set (expected: a `FieldRow`-style struct for label/value/hint rows, a section wrapper, possibly one callback-context struct). Each type: doc comment stating which builders produce/consume it; `#[derive(Debug)]` (workspace deny); fields named for meaning not position. Place per Scope. No speculative fields — only what the classified sites need.

**Verify**: `cargo check -p jackin-console` → exit 0 (types compile standalone before migration).

### Step 3: Migrate `frame.rs` (19 sites)

Convert each site per its class; delete the `#[allow]` with each conversion. Where the complexity is a passed-closure train, prefer converting the builder to take the new row/section models as data (build rows first, pass rows) over wrapping closures in a struct — data beats callbacks for testability. Keep each converted function under the complexity thresholds.

**Verify**: `cargo clippy -p jackin-console --all-targets -- -D warnings` → exit 0 with frame.rs containing 0 `type_complexity` mentions; `cargo nextest run -p jackin-console` → all pass; no `.pending-snap` files exist.

### Step 4: Migrate `settings/view.rs` (14) and the four tabs (8)

Same process, REUSING Step 2's types wherever the shape matches (that reuse is the roadmap's "shared form-section builders" seed). A settings-only shape gets its own named type beside the shared ones, not a forced fit.

**Verify**: same three checks; crate-wide `type_complexity` count ≤ 8 (the out-of-scope residue), all remaining in-scope survivors converted to `#[expect(…, reason = "…")]` with real reasons.

### Step 5: Index + roadmap

Roadmap Phase 2 TUI item 3 → shipped (named models exist; unification item 2 can adopt them). Ledger: strike DEBT-type-complexity; note the residue count and locations.

**Verify**: `cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Existing console suites + snapshots pass byte-identical — the primary net.
- Add focused unit tests ONLY for any new non-trivial constructor/logic on the shared models (plain data structs need none).
- The suppression-count drop (47 → ≤8 crate-wide… strictly: in-scope 42 → 0-bare) is itself machine-checked in Done criteria.

## Done criteria

- [ ] 0 `#[allow(clippy::type_complexity)]` in the editor/settings view builders; survivors are reasoned `#[expect]`s (≤6)
- [ ] Shared row/section model types exist and are used by BOTH screens where shapes match
- [ ] Console suite green; snapshots unchanged (`git status` shows no `.snap` modifications); clippy `-D warnings` clean
- [ ] Classification + residue table in the PR body
- [ ] Roadmap + ledger updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

- Step 1 classifies >6 in-scope sites as irreducible.
- Any conversion changes a rendered snapshot (representational refactor contract broken — find the behavior leak, and if it's pre-existing snapshot nondeterminism, report that instead).
- A conversion cascades into the editor's model/update layers (signature ripple beyond view builders) — the cut point is wrong; report.
- The two screens' shapes genuinely don't share any row model (the "shared" premise fails) — proceed per-screen but flag it for the unification program.

## Maintenance notes

- The Phase 2 settings/editor unification (deferred) should consume these models as its starting vocabulary — that plan's author should read Step 2's types first.
- Plan 011's suppression ratchet: after this lands, regenerate `suppression-budget.toml` (bare-allow counts drop sharply for jackin-console) — the gate will demand the shrink.
- Reviewer should scrutinize: that no `.snap` changed, and that new struct names describe domain meaning (FieldRow) not implementation (TupleWrapper).

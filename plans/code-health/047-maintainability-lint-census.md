# Plan 047: Census the seven allowed maintainability lints; deny the quiet ones, document the noisy ones

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- Cargo.toml`
> Plans 011/019/034 also edit the workspace lint table — expected drift; take
> the landed table as the base and append. Any lint of the seven already
> flipped by another PR: skip it, note it.

## Status

- **Priority**: P2
- **Effort**: S-M
- **Risk**: LOW (config flips + mechanical fixes; every hit is fix-or-`#[expect]`)
- **Depends on**: none (soft-conflicts with 011/034 on Cargo.toml — append entries matching the landed state)
- **Category**: tech-debt
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Roadmap Phase 1 ("Lint ratchets" item 3) instructs: "Revisit globally allowed maintainability lints one at a time: `needless_pass_by_value`, `large_futures`, `unused_async`, `assigning_clones`, `match_same_arms`, `drop_non_drop`, `unused_self` … Promote only low-noise lints; keep noisy ones documented." The coverage index deferred all seven as "noisy per census" — but no census was ever taken (the only lint census, plan 034's, measured the numeric/easy families). Low-hit members are typically free deny-wins going unclaimed, and the deny-by-default posture (roadmap: "a `warn` is a human grace period; an agent does not need one") wants each either denied or documented with a measured count. This plan takes the census and acts on it.

## Current state

Verified at `fabe88406` — all seven sit in the workspace `[workspace.lints.clippy]` table in root `Cargo.toml`:

```text
Cargo.toml:169  needless_pass_by_value = "allow"
Cargo.toml:172  large_futures = "allow"
Cargo.toml:186  assigning_clones = "allow"
Cargo.toml:187  match_same_arms = "allow"
Cargo.toml:188  drop_non_drop = "allow"
Cargo.toml:193  unused_self = "allow"
Cargo.toml:198  unused_async = "allow"
```

(`float_cmp` :199 belongs to plan 034 — not this plan.) Context comments in `crates/AGENTS.md` record why `needless_pass_by_value` and `large_futures` were parked: "first fires on many intentional by-value state/view handoffs, second on capsule async protocol readers where boxing every call site adds indirection without measured win." Those two are EXPECTED to stay documented-allow; the census confirms with numbers. CI promotes warns via `-D warnings` (`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`), so any lint left at `warn` in the table is effectively deny in CI — the repo convention is deny-in-table or allow-with-comment, matching the existing table style.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Census one lint (example) | `cargo clippy --workspace --all-targets --all-features --locked --message-format=json 2>/dev/null \| jq -r 'select(.reason=="compiler-message") \| .message.code.code // empty' \| grep -c "clippy::unused_async"` | a count |
| Full clippy gate | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests after fixes | `cargo nextest run --workspace --locked` | all pass |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- Root `Cargo.toml` `[workspace.lints.clippy]` entries for the seven lints (+ their comments)
- Source fixes (or narrow `#[expect(..., reason = "…")]`) in any crate, for the lints promoted to deny
- `crates/AGENTS.md` lint-baseline prose if a documented-allow rationale changes
- `plans/code-health/README.md` status row + the census numbers (recorded in the index so the next audit doesn't re-measure)

**Out of scope** (do NOT touch):
- `float_cmp` (plan 034), the slice/index family (019), the silent-failure family (011), `clippy.toml` thresholds.
- Any behavioral change while fixing — `match_same_arms` merges identical arms, `assigning_clones` swaps to `clone_from`, `unused_async` de-asyncs; each fix must be behavior-preserving (tests prove it).
- Public API changes: if de-asyncing an `unused_async` fn would change a public trait/fn signature consumed across crates, `#[expect]` it with the reason instead.

## Git workflow

- Branch off `main`: `chore/maintainability-lint-census`.
- Conventional Commits (`chore(lints): …` for flips, `refactor(<crate>): …` for fix batches), `-s`, push per commit. PR to `main`; do not merge. If capsule files get fixes → capsule smoke block.

## Steps

### Step 1: Census all seven

For each lint, temporarily set it to `warn` in Cargo.toml (one at a time or all seven at once — one clippy run with all seven at warn is cheaper; the JSON `code.code` field attributes each diagnostic), run the census command, and record per lint: total count, count per crate, dominant pattern (read a sample of 5 hits). Write the table into the PR body AND into this plan's README index row. Then revert Cargo.toml to `allow` for everything before Step 2 (so each promotion is its own reviewed change).

**Verify**: a 7-row census table exists in the PR body; `git diff Cargo.toml` → clean again.

### Step 2: Promote the quiet ones

For each lint with **≤ 15 hits**: fix every hit (or add a narrow `#[expect(clippy::<lint>, reason = "…")]` where the code is intentional), then set the lint to `warn` in the table (CI's `-D warnings` makes it blocking; matches how `manual_let_else`/`match_bool` are handled per crates/AGENTS.md) — or `"deny"` if the surrounding table section uses deny; match the table's local style. One commit per lint: `chore(lints): promote clippy::<lint> (N hits fixed)`.

Fix guidance per lint: `drop_non_drop` — remove the pointless `drop(x)`; `assigning_clones` — `a = b.clone()` → `a.clone_from(&b)`; `unused_async` — remove `async` + fix callers' `.await` (STOP-check the public-API rule above); `match_same_arms` — merge arms with `|` patterns ONLY when the arms are literally identical and order-independent (`match_same_arms` has known false-positives around arm order — if merging changes match semantics, `#[expect]` it); `unused_self` — make the method an associated fn or `#[expect]` where `self` is kept for trait symmetry.

**Verify** after each lint's commit: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo nextest run --workspace --locked` → all pass.

### Step 3: Document the noisy ones

For each lint with **> 15 hits**: keep `allow`, update its Cargo.toml comment to carry the measurement: `# allow: N hits measured 2026-07, dominant pattern <one clause>` (matching the style of the existing parked-lint comments). If `needless_pass_by_value`/`large_futures` counts contradict the crates/AGENTS.md rationale (e.g. now near-zero), promote them instead and update that prose.

**Verify**: every one of the seven lines in Cargo.toml is either promoted (Step 2) or carries a measured-count comment; `grep -c "measured 2026-07" Cargo.toml` ≥ number of kept allows.

### Step 4: Gate

**Verify**: `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

No new tests — the workspace suite is the behavior-preservation oracle for every fix batch. Run the full suite after each lint's fix commit, not just at the end.

## Done criteria

- [ ] Census table (7 lints × count/crate/pattern) in PR body + README index
- [ ] Every ≤15-hit lint promoted with hits fixed or narrowly expected
- [ ] Every kept allow carries a measured-count comment
- [ ] Workspace clippy `-D warnings` + full nextest green; `cargo xtask ci --fast` → `ci gate OK`
- [ ] `plans/code-health/README.md` row updated (with the census numbers)

## STOP conditions

Stop and report back if:

- Any single lint exceeds 150 hits (the census is the deliverable then — report the table and stop before fixing; a >150-hit fix wave needs its own plan).
- An `unused_async` fix would change a `pub` trait method signature consumed outside its crate.
- A `match_same_arms` merge would reorder match-arm evaluation over guards or bindings (semantics risk).
- Plans 011/019/034 are mid-flight on the same Cargo.toml region and the merge conflict is more than mechanical.

## Maintenance notes

- The measured counts feed plan 011's per-lint suppression ratchet and the (later) stricter-er allowlist pilot — record them in the README index, not just the PR.
- New code hitting a promoted lint gets the standard fix-or-`#[expect]` treatment; the `reason=` gate (011) keeps suppressions honest.
- Reviewer scrutiny: `unused_async` caller updates (a dropped `.await` on a now-sync fn is easy to mis-merge) and any arm-merge under `match_same_arms`.

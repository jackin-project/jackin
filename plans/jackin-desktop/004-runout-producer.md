# Plan 004: Emit the Variant A run-out projection from Rust as the pace-label composite

> **Executor instructions**: Follow this plan step by step. Run the
> preconditions first. Run every verification command and confirm the
> expected result before moving on. If anything in "STOP conditions"
> occurs, stop and report — do not improvise. When done, update this
> plan's status row in `plans/jackin-desktop/README.md`.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/jackin-desktop/003-grok-server-plan.md`
- **Covers**: spec/providers.md "Run-out producer (Variant A)" (F5)
- **Guardrails**: N1, N3 (inlined below)
- **Research basis**: research/agent-usage-provider-apis/07-runout-projection-semantics.md, research/jackin-desktop-verification-tooling/01-commands.md
- **Planned at**: commit `3e6376d`, 2026-07-24

## Why this matters

The capsule TUI and the jackin❯ Desktop Swift shell both already *consume* a
`"Runs out in <duration>"` segment inside `pace_label` — the TUI routes it to
the right detail column and suppresses its synthesized "Lasts until reset",
and the Swift `splitPaceLabel` renders it as the second pace column — but no
code anywhere in the repository *produces* it. This plan adds the one
producer, in Rust (`quota_pace_label` in `crates/jackin-usage`), computing the
linear-from-window-start projection (Variant A) and appending
`" · Runs out in <compact duration>"` only when the projection strictly
precedes the reset. After this lands, every provider bucket that carries
`remaining_percent`, `resets_at`, and a window duration (today: Claude, Codex,
Kimi Rate Limit) gains the projection uniformly, and the existing TUI/Swift
splitters render it with zero display-side changes (Rust owns every string —
guardrail N1).

## Preconditions — run before anything else

- On an operator-confirmed feature branch, not `main`:
  `git branch --show-current` → a `feature/...` (or similar) branch name, not
  `main`. If on `main`, propose a branch (suggested:
  `feature/usage-runout-producer`), ask the operator "This is on `main`. I
  suggest `feature/usage-runout-producer`. Should I create it?", and wait for
  confirmation before any edit.
- `gh pr list --head "$(git branch --show-current)" --state open` → keep any
  existing PR branch. Resolve the actual push head: if
  `git rev-parse --abbrev-ref '@{upstream}'` succeeds, record its remote and
  remote-branch components and query that remote head instead; otherwise
  record `origin` + the current local branch for the first `-u` push.
  `test -z "$(git status --porcelain=v1)"` exits 0.
- Planning artifacts are tracked:
  `git ls-files --error-unmatch plans/jackin-desktop/004-runout-producer.md plans/jackin-desktop/README.md roadmap/jackin-desktop/README.md roadmap/README.md`.
- Plan 003 row is DONE; its focused Grok tests pass, ensuring its RPC
  window-duration inputs exist before this shared producer is changed.
- Toolchain present: `cargo nextest --version` → prints a version, exit 0.
- Baseline crates suite green before any edit:
  `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` → all pass.
- Consumer fixture in place (proves the capsule TUI already accepts the
  composite this plan will produce):
  `cargo nextest run -p jackin-capsule -E 'test(usage_dialog_renders_deficit_and_runout_quota_labels)'`
  → 1 test passes.
- Drift check (this plan touches pre-existing code):
  `git diff --stat 3e6376d -- crates/jackin-usage/src/usage/format.rs crates/jackin-usage/src/usage/tests.rs crates/jackin-usage/README.md docs/content/docs/\\(public\\)/guides/macos-usage-menu-bar.mdx docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx docs/content/docs/roadmap/\\(operator-surface\\)/native-macos-usage-menu-bar.mdx docs/content/docs/roadmap/index.mdx plans/jackin-desktop/README.md roadmap/jackin-desktop/README.md roadmap/README.md`
  → only committed prerequisite-plan changes. On any listed change, compare the "Starting state" excerpts
  below against the live code; any mismatch is a STOP.

Any failed precondition is a STOP.

## Spec contract

Inlined verbatim from `plans/jackin-desktop/spec/providers.md` — the executor
does not read `spec/`:

### Requirement: Run-out producer (Variant A)

The Rust core SHALL compute, per quota bucket where `remaining_percent`,
`resets_at`, and window duration are known and `used > 0`,
`runs_out_in = remaining × elapsed / used` under the linear-from-window-start
model anchored on `resets_at` (window_start = resets_at − window_seconds, A1),
and SHALL emit it appended to the existing pace label as the composite
`"<pace> · Runs out in <compact duration>"` only when the projected run-out
precedes the reset; when it does not, the existing "Lasts until reset"
semantics remain (TUI synthesis stays valid). Zero-used or window-start
edge cases SHALL emit no run-out segment.
Covers: F5 · Evidence: research/agent-usage-provider-apis/07-runout-projection-semantics.md, 10 (Q1/Q4)

#### Scenario: Behind pace, runs out before reset
- **GIVEN** a Weekly bucket at 48% left, 5% in deficit, reset in 3d 16h
- **WHEN** the bucket view is built
- **THEN** `pace_label` reads "5% in deficit · Runs out in <duration>" with duration < 3d 16h
- **AND** the capsule dialog and Swift splitter render the two segments in their existing columns unchanged

#### Scenario: Ahead of pace
- **GIVEN** a bucket in reserve (delta > 0)
- **WHEN** the view is built
- **THEN** no "Runs out in" segment is emitted and the TUI still shows "Lasts until reset"

#### Scenario: Nothing used yet
- **GIVEN** a bucket at 100% left
- **WHEN** the view is built
- **THEN** the pace label carries no run-out segment (no division by zero)

Done means these scenarios hold; the test plan below exercises them.

## Must NOT

Guardrails inlined verbatim from the must-not registry
(`plans/jackin-desktop/spec/README.md`), with reasons. These override
anything a step seems to imply:

- **N1**: Swift MUST NOT contain logic beyond displaying Rust-provided
  information — no computing, rewording, reordering, or deriving of any
  label, number, or projection in Swift — reason: item §Must not (Rust owns
  implementation). For this plan: the run-out string is produced only in
  `crates/jackin-usage`; no Swift file is edited.
- **N3**: No surface MUST ever show token unit prices, cost-of-session
  estimates, spend-over-time charts, trend sparklines, token/spend
  histories, aggregate-spend donuts, or cost-legend rankings —
  provider-supplied quota bounds (money caps, credit balances) are the only
  money allowed — reason: repo hard rule (CLAUDE.md usage-surfaces). For
  this plan: the run-out projection is a limit-window duration, never a
  price, cost, or trend; introduce no money or history strings.

## Inputs to provide

None — fully self-contained.

## Starting state

All excerpts below were read from the working tree at commit `3e6376d`.

### The sole pace producer — extend this function

`crates/jackin-usage/src/usage/format.rs:164-191` (visibility `pub(super)`;
callable from every `usage/` sibling module and from `usage/tests.rs`):

```rust
pub(super) fn quota_pace_label(
    remaining_percent: Option<u8>,
    reset_at: Option<i64>,
    window_seconds: Option<i64>,
    now: i64,
) -> Option<String> {
    let remaining_percent = f64::from(remaining_percent?);
    let reset_in = reset_at?.saturating_sub(now).max(0);
    let window_seconds = window_seconds?.max(1);
    if reset_in > window_seconds {
        return None;
    }
    let time_left_percent = reset_in as f64 / window_seconds as f64 * 100.0;
    // CodexBar pace model: compare remaining quota against the fraction of the
    // window still left. `delta > 0` means more quota than time remains (ahead
    // of pace = reserve); `delta < 0` means burning faster than the clock
    // (behind = deficit); within 2 points is "On pace". The reset countdown is
    // carried separately in the bucket's reset label, so the pace token stays a
    // bare phrase exactly as the previews show.
    let delta = remaining_percent - time_left_percent;
    if delta.abs() <= 2.0 {
        Some("On pace".to_owned())
    } else if delta > 0.0 {
        Some(format!("{}% in reserve", delta.round() as i64))
    } else {
        Some(format!("{}% in deficit", (-delta).round() as i64))
    }
}
```

### The duration formatter to reuse (do not write a new one)

`crates/jackin-usage/src/usage/format.rs:193-208`:

```rust
pub(crate) fn compact_duration_label(seconds: i64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        if hours > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}
```

### Call sites — every provider gains the segment through them, unchanged

The function signature does not change, so these three call sites need no
edit; they are listed so the executor can confirm uniform pickup:

- `crates/jackin-usage/src/usage/claude.rs:506` (inside
  `ClaudeQuotaWindow::into_bucket`):
  `let pace = quota_pace_label(remaining, self.reset_at, self.window_seconds, now);`
- `crates/jackin-usage/src/usage/codex.rs:697-698` (inside
  `push_codex_window`):
  `let pace = quota_pace_label(remaining, window.reset_at, window_seconds, now)`
  `    .or_else(|| window.window_label());`
- `crates/jackin-usage/src/usage/kimi.rs:239`:
  `let pace = quota_pace_label(remaining, reset_at, window_seconds, now);`
  (Kimi supplies `window_seconds` only for the `"Rate Limit"` bucket — see
  `kimi_window_seconds`, kimi.rs:252-256.)

### Existing unit test that WILL need a new expected value

`crates/jackin-usage/src/usage/tests.rs:2820-2835`, test
`quota_pace_label_uses_codexbar_reserve_deficit_onpace`:

```rust
    // Behind pace (burning faster than the clock): 60% quota left with 90%
    // of the window still remaining -> 30 points of deficit.
    let deficit = quota_pace_label(Some(60), Some(900), Some(1_000), 0).expect("pace label");
    assert_eq!(deficit, "30% in deficit");

    // Ahead of pace (quota outlasting the clock): 90% left, 60% of window
    // remaining -> 30 points in reserve.
    let reserve = quota_pace_label(Some(90), Some(600), Some(1_000), 0).expect("pace label");
    assert_eq!(reserve, "30% in reserve");

    // Within 2 points of the clock -> On pace.
    let on_pace = quota_pace_label(Some(50), Some(500), Some(1_000), 0).expect("pace label");
    assert_eq!(on_pace, "On pace");
```

The deficit case gains a run-out segment (hand computation: elapsed =
1000 − 900 = 100, used = 40, runs_out = 60 × 100 / 40 = 150 s → "2m";
150 < 900 → emitted). The reserve and On-pace cases are unchanged (reserve:
runs_out = 90 × 400 / 10 = 3600 ≥ 600 → no segment; on-pace: runs_out =
50 × 500 / 50 = 500, not strictly < 500 → no segment).

Repo-wide, `"in deficit"` is asserted only here and produced only at
format.rs:189 — no other jackin-usage/-ffi test asserts a deficit string, and
the integration fixtures pin reserve labels only (Codex `"15% in reserve"`
tests.rs:2255, Kimi `"30% in reserve"` tests.rs:3318), which are unaffected.

### The consumers (parity gates — read-only for this plan)

Capsule TUI right-column routing + "Lasts until reset" synthesis,
`crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs:746-767`
(inside `usage_stacked_bucket_detail_rows`):

```rust
    let mut lasts_until_reset = false;
    if let Some(label) = remaining_label {
        left.push(label);
    }
    for detail in details {
        if detail.starts_with("Resets") || detail.starts_with("Runs out") {
            right.push(detail.clone());
        } else if !left.iter().any(|existing| existing == detail) {
            if detail == "On pace" || detail.ends_with(" in reserve") {
                lasts_until_reset = true;
            }
            left.push(detail.clone());
        }
    }
    if lasts_until_reset
        && right.iter().any(|detail| detail.starts_with("Resets"))
        && !right.iter().any(|detail| detail.starts_with("Runs out"))
    {
        right.push("Lasts until reset".to_owned());
    } else if right.is_empty() && left.len() > 1 {
        right.push(String::new());
    }
```

Composite splitter (the ` · ` separator is the contract),
`crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs:846-850`:

```rust
pub(crate) fn usage_quota_bucket_detail_parts(label: &str, value: &str) -> Vec<String> {
    let parts = value
        .split(" · ")
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
```

Capsule dialog fixture proving the exact composite shape this plan must
produce, `crates/jackin-capsule/src/tui/components/dialog/tests.rs:1573`
(test `usage_dialog_renders_deficit_and_runout_quota_labels`), fixture at
line 1587:

```rust
            pace_label: Some("31% in deficit · Runs out in 21h 45m".to_owned()),
```

asserted rendered at tests.rs:1622-1625:

```rust
    assert!(rendered.contains("Weekly"), "{rendered}");
    assert!(rendered.contains("31% in deficit"), "{rendered}");
    assert!(rendered.contains("Runs out in 21h 45m"), "{rendered}");
    assert!(rendered.contains("Lasts until reset"), "{rendered}");
```

(The same composite is injected again at dialog/tests.rs:1643 in
`usage_dialog_renders_dynamic_provider_quota_bucket_meters`.)

Swift splitter (display-only, N1),
`native/Sources/JackinDesktop/UsageWindow/ProviderCardView.swift:209-211`:

```swift
            // CodexBar dual-column pace ("On pace" · "Runs out in …").
            if let pace = bucket.paceLabel, !pace.isEmpty {
                let parts = splitPaceLabel(pace)
```

Swift splitter test fixture,
`native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift:367-370`:

```swift
        XCTAssertEqual(
            splitPaceLabel("On pace · Runs out in 4d 21h"),
            ["On pace", "Runs out in 4d 21h"]
        )
```

The same `"On pace · Runs out in 4d 21h"` fixture is also pinned in
`native/Tools/StatusItemChipHarness/main.swift:396-397`. Note both Swift
fixtures use an **On pace** token with a run-out segment — that composite
form is anticipated by the consumers (see the emission-condition note in
Step 1).

### Downstream carriers — no changes needed

- `pace_label` is an opaque `Option<String>` in the protocol bucket
  (`QuotaBucketView.pace_label`) and an opaque string column in the snapshot
  store; the composite is just a longer string. No protocol, FFI, or store
  schema change (so no versioned-schema artifact rule is triggered).
- The TUI truncates long detail cells with an ellipsis after splitting on
  ` · `, so composite length is safe.

### Conventions to match

- All tests for the `usage` module live inline in
  `crates/jackin-usage/src/usage/tests.rs` — one `tests.rs` per module, no
  child modules, no inline `#[cfg(test)] mod tests { … }` in source (hard
  rule from `crates/` workspace rules). Exemplar: the existing
  `quota_pace_label_uses_codexbar_reserve_deficit_onpace` test above.
- Comments explain non-obvious WHY only, never narrate WHAT.
- Workspace clippy is pedantic with `-D warnings` in CI; the existing
  `as f64` / `as i64` casts inside `quota_pace_label` are the accepted local
  pattern to follow for the new arithmetic.

### Design constraints from research, quoted

From `research/agent-usage-provider-apis/07-runout-projection-semantics.md`:

- Variant A: "Define `elapsed = window_seconds − reset_in`, `used = 100 −
  remaining`. Burn rate `r = used / elapsed` (%/s); `runs_out_in =
  remaining / r = remaining × elapsed / used`. Failure modes: `used = 0` →
  division by zero (never runs out); `elapsed = 0` at window start →
  undefined; early-window jumpiness — small `elapsed` amplifies the
  whole-percent `u8` quantization of `remaining` …; `reset_in >
  window_seconds` already suppressed by the existing guard".
- Algebraic identity: "`runs_out_in ≥ reset_in` ⇔ … `remaining_percent ≥
  time_left_percent` ⇔ `delta ≥ 0`". Contrapositive: the run-out projection
  strictly precedes the reset exactly when `delta < 0`.
- The composite carrier convention is the injected fixture
  `"31% in deficit · Runs out in 21h 45m"` with ` · ` separators.

## Commands you will need

Proven by `research/jackin-desktop-verification-tooling/01-commands.md`
(commands taken from CI green usage, not guessed):

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Crates tests (exact CI lane) | `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` | all pass, exit 0 |
| Focused pace tests | `cargo nextest run -p jackin-usage -E 'test(quota_pace_label)' --locked` | all pass |
| Capsule TUI parity | `cargo nextest run -p jackin-capsule` | all pass (package name verified against `crates/jackin-capsule/Cargo.toml`) |
| Swift parity harness (macOS only) | `cargo xtask desktop test` (or `mise run desktop-test`) | exit 0; harnesses pass |
| Swift XCTest | `cd native && swift test -c release` | exit 0 |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Format | `cargo fmt --all -- --check` | exit 0 |
| Fast CI | `cargo xtask ci --fast` | exit 0 |
| Docs | `cd docs && bunx tsc --noEmit && bun test && bun run build` | all pass |
| Audits | `cargo xtask docs repo-links && env -u CI cargo xtask docs specs && cargo xtask docs brand && cargo xtask roadmap audit && cargo xtask research check` | all pass |

## Suggested executor toolkit

- `TESTING.md` (repo root) — nextest runner and `-E 'test(name)'` filter
  syntax, if a focused run misbehaves.

## Scope

**In scope** (the only files to create or modify):

- `crates/jackin-usage/src/usage/format.rs` — extend `quota_pace_label`.
- `crates/jackin-usage/src/usage/tests.rs` — update one expected value, add
  the new tests.
- `crates/jackin-usage/README.md`
- `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx`
- `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx`
- `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`
- `docs/content/docs/roadmap/index.mdx`
- `plans/jackin-desktop/README.md`
- `roadmap/jackin-desktop/README.md`
- `roadmap/README.md`

**Out of scope** (do NOT touch, even though related):

- `crates/jackin-capsule/src/tui/**` — the TUI consumer already handles the
  composite; its tests are this plan's parity gate, not its edit surface.
- `native/**` (all Swift) — display of the composite is plans 006/008
  territory; the existing splitter is already in place (N1).
- `crates/jackin-protocol/**`, `crates/jackin-usage-ffi/**`,
  `crates/jackin-usage/src/usage_snapshot_store*` — `pace_label` is an
  opaque string end to end; no contract change.
- `plans/jackin-desktop/spec/**`, `plans/jackin-desktop/coverage.md`,
  `research/**` — read-only sources.

## Git workflow

- Branch: operator-chosen feature branch (suggest
  `feature/usage-runout-producer`); never commit on `main` — propose and
  wait for operator confirmation (see Preconditions).
- Commit signed, Conventional Commits:
  `git commit -s -m "feat(usage): run-out projection in pace label" -m "Co-authored-by: Codex <codex@openai.com>"`
- Before edits, resolve an existing upstream into remote + remote head; when
  absent, use `origin` + current local branch. Query `gh pr list --head` with
  the actual remote head. Push `HEAD:<remote-head>` to that exact remote,
  using `-u` only when upstream is absent. Local/upstream names may differ.
- Push immediately after the sole commit. No local-only commits.
- Never force-push.

## Steps

### Step 1: Extend `quota_pace_label` with the Variant A run-out segment

In `crates/jackin-usage/src/usage/format.rs`, rework the tail of
`quota_pace_label` (lines 183-191 in the excerpt above): bind the existing
three-way token to a local (e.g. `let pace: String = …` replacing the three
`Some(...)` arms). At function entry retain
`let remaining_percent_raw = remaining_percent?;` and derive the existing
float as `f64::from(remaining_percent_raw)`, so display delta remains
unchanged while the strict decision uses the raw integer. Then append the
run-out segment before returning. Target tail:

```rust
    let delta = remaining_percent - time_left_percent;
    let pace = if delta.abs() <= 2.0 {
        "On pace".to_owned()
    } else if delta > 0.0 {
        format!("{}% in reserve", delta.round() as i64)
    } else {
        format!("{}% in deficit", (-delta).round() as i64)
    };
    // Compare exact integer cross-products: float delta is display math and
    // can round a clock-equality case slightly negative.
    let used = 100_i128 - i128::from(remaining_percent_raw);
    let elapsed = window_seconds - reset_in;
    let behind_clock =
        i128::from(remaining_percent_raw) * i128::from(window_seconds)
            < i128::from(reset_in) * 100_i128;
    if used > 0 && elapsed > 0 && behind_clock {
        let numerator =
            i128::from(remaining_percent_raw) * i128::from(elapsed);
        let display_seconds = (numerator + used / 2) / used;
        let display_seconds = i64::try_from(display_seconds).ok()?;
        return Some(format!(
            "{pace} · Runs out in {}",
            compact_duration_label(display_seconds)
        ));
    }
    Some(pace)
```

Load-bearing details:

- Separator is exactly `" · "` (space, U+00B7 middle dot, space) — the TUI
  splitter (`usage_quota_bucket_detail_parts`) and Swift `splitPaceLabel`
  split on it.
- Emission condition: `used > 0 && elapsed > 0 && behind_clock`, where
  `behind_clock` is the strict `i128` cross-product comparison
  `remaining * window_seconds < reset_in * 100`. The spec says "only when
  the projected run-out precedes the reset"; integer cross-products make
  equality exact and avoid float drift. Compare before display rounding:
  display rounding can equal `reset_in` when the exact projection is still
  smaller. `elapsed` cannot be negative here because the existing
  `reset_in > window_seconds` guard returned `None` earlier; `elapsed == 0`
  (window start) and `used == 0.0` (100% left) emit no segment per the spec's
  edge-case sentence.
- Keep the existing "CodexBar pace model" comment and the ±2.0 "On pace"
  band byte-identical — no epsilon behavior change to the band.
- By the algebraic identity, emission ⇔ `behind_clock` strictly. In the
  narrow band `−2 ≤ delta < 0` the token is `"On pace"`, so the composite
  there reads `"On pace · Runs out in <duration>"` — exactly the form both
  Swift fixtures pin (ArchitectureTests.swift:368-369,
  StatusItemChipHarness/main.swift:396-397), and the TUI suppresses its
  synthesized "Lasts until reset" whenever any `"Runs out"` detail is
  present (dialog_widgets/usage.rs:760-764), so TUI synthesis stays valid
  in every branch. Deficit tokens (`delta < −2`) always carry the segment.
- Round positive rational display seconds with integer
  `(numerator + denominator / 2) / denominator`, after the exact predicate.
- Durations use the existing `compact_duration_label` — do not add a new
  formatter.
- Do not change the function's signature or visibility; the three provider
  call sites then pick the segment up with zero edits.

**Verify**: `cargo build -p jackin-usage` → exit 0. Then
`cargo nextest run -p jackin-usage -E 'test(quota_pace_label_uses_codexbar_reserve_deficit_onpace)' --locked`
→ FAILS on the deficit assertion only (expected — proves the segment is
emitted; fixed in Step 2). If the reserve or On-pace assertions fail, that
is a real bug in Step 1: STOP and re-check the emission condition.

### Step 2: Update the existing deficit expectation

In `crates/jackin-usage/src/usage/tests.rs`, inside
`quota_pace_label_uses_codexbar_reserve_deficit_onpace`, change only the
deficit assertion:

```rust
    // Behind pace (burning faster than the clock): 60% quota left with 90%
    // of the window still remaining -> 30 points of deficit; Variant A
    // run-out: elapsed = 100, used = 40, 60 * 100 / 40 = 150 s -> "2m",
    // which precedes the 900 s reset.
    let deficit = quota_pace_label(Some(60), Some(900), Some(1_000), 0).expect("pace label");
    assert_eq!(deficit, "30% in deficit · Runs out in 2m");
```

Leave the reserve and On-pace assertions untouched.

**Verify**:
`cargo nextest run -p jackin-usage -E 'test(quota_pace_label_uses_codexbar_reserve_deficit_onpace)' --locked`
→ 1 test passes.

### Step 3: Add the scenario and property tests

Append the new tests to `crates/jackin-usage/src/usage/tests.rs` (inline in
this one file — the module's single test surface). Write each expected
duration as a hand-computed comment (the arithmetic below is the independent
source of truth; do not derive expectations by running the code):

1. `quota_pace_label_appends_runout_when_behind_pace` — spec scenario
   "Behind pace, runs out before reset".
   - Synthetic with nonzero epoch:
     `quota_pace_label(Some(48), Some(10_530), Some(1_000), 10_000)` →
     `"5% in deficit · Runs out in 7m"`. Hand math: time_left = 53%,
     delta = −5; elapsed = 470, used = 52; 48 × 470 / 52 = 433.85 → 434 s →
     "7m"; 434 < 530 ✓.
   - Weekly-realistic (spec GIVEN shape, 7-day window):
     `quota_pace_label(Some(48), Some(320_399), Some(604_800), 0)` →
     `"5% in deficit · Runs out in 3d"`. Hand math: time_left =
     320 399 / 604 800 ≈ 52.98%, delta ≈ −4.98; elapsed = 284 401,
     used = 52; 48 × 284 401 / 52 = 262 524 s ≈ 3d 0h → "3d";
     262 524 < 320 399 (reset displays 3d 16h) ✓ — duration precedes the reset as
     the scenario's THEN requires.
2. `quota_pace_label_no_runout_when_ahead_of_pace` — spec scenario "Ahead of
   pace": `quota_pace_label(Some(90), Some(600), Some(1_000), 0)` →
   exactly `"30% in reserve"` (assert_eq — proves no ` · ` segment; run-out
   would be 90 × 400 / 10 = 3600 ≥ 600). The TUI-side "Lasts until reset"
   half of the THEN is covered by the unchanged capsule tests in Step 4.
3. `quota_pace_label_no_runout_when_nothing_used` — spec scenario "Nothing
   used yet": `quota_pace_label(Some(100), Some(500), Some(1_000), 0)` →
   exactly `"50% in reserve"` (used = 0; returns without dividing — the
   test completing at all is the no-division-by-zero proof).
4. `quota_pace_label_no_runout_at_window_start` — window-start edge:
   `quota_pace_label(Some(60), Some(1_000), Some(1_000), 0)` → exactly
   `"40% in deficit"` (elapsed = 0 → no segment even though delta = −40).
5. `quota_pace_label_runout_iff_behind_clock_boundary` — identity property
   at the boundaries (emitted ⇔ exact behind-clock cross-product; token band
   unchanged). All with
   `reset_at = Some(500)`, `window_seconds = Some(1_000)`, `now = 0`:
   - `Some(50)` (delta = 0) → `"On pace"` — run-out = 50 × 500 / 50 = 500,
     not strictly < 500 → bare.
   - `Some(51)` (delta = +1, inside band, ahead) → `"On pace"` — run-out =
     51 × 500 / 49 = 520.4 → 520 ≥ 500 → bare.
   - `Some(49)` (delta = −1, inside band, behind) →
     `"On pace · Runs out in 8m"` — 49 × 500 / 51 = 480.4 → 480 s → "8m";
     480 < 500 ✓ (the composite form the Swift fixtures pin).
   - `Some(48)` (delta = −2, band edge, still "On pace") →
     `"On pace · Runs out in 7m"` — 48 × 500 / 52 = 461.5 → 462 s → "7m".
   - `Some(47)` (delta = −3, first deficit token) →
     `"3% in deficit · Runs out in 7m"` — 47 × 500 / 53 = 443.4 → 443 s →
     "7m".
6. `quota_pace_label_runout_depleted_bucket` — depleted edge, pinned to the
   spec letter: `quota_pace_label(Some(0), Some(500), Some(1_000), 0)` →
   `"50% in deficit · Runs out in 0m"` (used = 100, elapsed = 500, run-out =
   0 < 500 — the projection "precedes the reset" trivially; see Maintenance
   notes).
7. `quota_pace_label_exact_projection_precedes_reset_before_rounding` —
   `quota_pace_label(Some(49), Some(10_515), Some(1_051), 10_000)` →
   `"On pace · Runs out in 8m"`. Exact projection is
   `49 × 536 / 51 = 514.980392… < 515`; display rounding is 515. This test
   fails if rounded seconds are compared to reset seconds.
8. `quota_pace_label_exact_clock_equality_ignores_float_drift` —
   `quota_pace_label(Some(7), Some(70), Some(1_000), 0)` emits the bare
   float-derived pace token and contains no `"Runs out"` segment. Exact
   cross-products are equal (`7 × 1000 == 70 × 100`), so the projection
   reaches reset exactly; a float-only `delta < 0.0` comparison can
   misclassify this tuple by a tiny negative rounding error.

**Verify**:
`cargo nextest run -p jackin-usage -E 'test(quota_pace_label)' --locked` →
all pass, including the 8 new tests. Then the full lane:
`cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` → all pass
(proves the reserve-only integration fixtures — Codex tests.rs:2255, Kimi
tests.rs:3318 — did not shift).

### Step 4: Run the capsule TUI parity gate (no capsule edits)

No capsule file changes. Run the consumer suite to prove the composite
contract this plan now produces is exactly what the TUI splits and renders:

**Verify**: `cargo nextest run -p jackin-capsule` → all pass, in particular
`usage_dialog_renders_deficit_and_runout_quota_labels` and
`usage_dialog_renders_dynamic_provider_quota_bucket_meters`. A failure here
means the composite format drifted — STOP (see STOP conditions).

### Step 5: Run the Swift splitter parity gate (macOS only; no Swift edits)

No `native/` changes. On macOS, run the Swift harnesses that pin
`splitPaceLabel("On pace · Runs out in 4d 21h")`:

**Verify**: `cargo xtask desktop test` (or `mise run desktop-test`) → exit 0,
all harnesses pass. If not on macOS, the xtask refuses with its macOS guard —
record that in the final report; the capsule gate in Step 4 plus the
unchanged `native/` tree still hold N1.

### Step 6: Docs, full gates, one atomic commit

1. Update:
   - operator guide: behind-clock fixed-window quotas may show the Rust
     `Runs out in …` projection; define it as an estimate, not history/cost;
   - usage README and ADR-011: Variant A formula, fixed-window inputs,
     exact pre-rounding emission decision, and shared opaque pace carrier;
   - docs roadmap item/index: record the phase but keep overall Desktop
     Partially implemented;
   - local roadmap/index: keep IN EXECUTION and append plan-004 log;
   - hub row 004 → DONE only after gates.
2. Run every command from "Commands", plus `cargo xtask ci --fast`.
   Before merge readiness, the PR executor runs full `cargo xtask ci`.
   After writing the DONE/status/log protocol state, rerun the docs build and
   every audit command, including `docs brand` and `env -u CI docs specs`.
3. Stage exactly:

   ```sh
   git add -- \
     crates/jackin-usage/src/usage/format.rs \
     crates/jackin-usage/src/usage/tests.rs \
     crates/jackin-usage/README.md \
     'docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx' \
     docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx \
     'docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx' \
     docs/content/docs/roadmap/index.mdx \
     plans/jackin-desktop/README.md \
     roadmap/jackin-desktop/README.md roadmap/README.md
   git diff --cached --name-only
   git diff --cached -- plans/jackin-desktop/README.md \
     roadmap/jackin-desktop/README.md roadmap/README.md
   ```

   Expected: exactly 10 paths; protocol diff contains row 004 plus narrow
   roadmap status/log only.
4. Commit once and push:

   ```sh
   PLAN004_BRANCH="$(git branch --show-current)"
   if PLAN004_UPSTREAM="$(git rev-parse --abbrev-ref '@{upstream}' 2>/dev/null)"; then
     PLAN004_REMOTE="${PLAN004_UPSTREAM%%/*}"
     PLAN004_REMOTE_HEAD="${PLAN004_UPSTREAM#*/}"
   else
     PLAN004_REMOTE=origin
     PLAN004_REMOTE_HEAD="$PLAN004_BRANCH"
   fi
   git commit -s -m "feat(usage): run-out projection in pace label" \
     -m "Co-authored-by: Codex <codex@openai.com>"
   if git rev-parse --verify '@{upstream}' >/dev/null 2>&1; then
     git push "$PLAN004_REMOTE" "HEAD:$PLAN004_REMOTE_HEAD"
   else
     git push -u "$PLAN004_REMOTE" "HEAD:$PLAN004_REMOTE_HEAD"
   fi
   ```

   `PLAN004_REMOTE`/`PLAN004_REMOTE_HEAD` are the exact values recorded in
   Preconditions; do not infer a new destination here.

**Verify**:

```sh
test "$(git log -1 --format=%s)" = \
  "feat(usage): run-out projection in pace label"
git log -1 --format=%B | grep -q '^Signed-off-by: .\+ <.\+>$'
git log -1 --format=%B |
  grep -qx 'Co-authored-by: Codex <codex@openai.com>'
test "$(git rev-parse HEAD)" = "$(git rev-parse '@{upstream}')"
test "$(git diff-tree --no-commit-id --name-only -r HEAD | wc -l | tr -d ' ')" = 10
test -z "$(git status --porcelain=v1)"
```

All gates and proofs exit 0; cached path list was exact; no out-of-scope
change exists; the exact signed commit is pushed.

## Test plan

- All in `crates/jackin-usage/src/usage/tests.rs` (single test surface for
  the module): the Step 2 update plus the eight Step 3 tests — at least one
  per spec scenario (scenario 1 → test 1; scenario 2 → test 2; scenario 3 →
  test 3), plus the window-start edge (test 4), exact behind-clock
  boundary property (test 5), depleted edge (test 6), and pre-rounding
  strictness regression (test 7).
- Expected values are hand-computed in the test comments (the arithmetic in
  Step 3 above) — an independent source of truth; never regenerate an
  expectation from the code under test.
- Structural pattern to model after:
  `quota_pace_label_uses_codexbar_reserve_deficit_onpace`
  (tests.rs:2820-2835) — direct calls with small synthetic
  `(remaining, reset_at, window, now)` tuples and `assert_eq!` on the full
  string.
- Parity gates (no new tests, existing suites must stay green unchanged):
  capsule `usage_dialog_renders_deficit_and_runout_quota_labels` +
  `usage_dialog_renders_dynamic_provider_quota_bucket_meters`; Swift
  `ArchitectureTests` / `StatusItemChipHarness` splitter fixtures.
- **Verify**: `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked`
  → all pass including the 8 new tests; `cargo nextest run -p jackin-capsule`
  → all pass.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo build -p jackin-usage` exits 0
- [ ] `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked`
      exits 0; tests for every spec scenario exist and pass
- [ ] `cargo nextest run -p jackin-usage -E 'test(quota_pace_label)' --locked`
      runs ≥ 9 tests (1 updated + 8 new), all passing
- [ ] `cargo nextest run -p jackin-capsule` exits 0 with zero capsule file
      changes (`git diff --name-only` shows nothing under
      `crates/jackin-capsule/`)
- [ ] `git diff --name-only` shows nothing under `native/` (N1: no Swift
      edits); `cargo xtask desktop test` and
      `cd native && swift test -c release` exit 0
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
      exits 0; `cargo fmt --all -- --check`, `cargo xtask ci --fast`, and
      all docs/audit gates exit 0
- [ ] Cached path list equals exact 10-path Step-6 allowlist; no new
      out-of-scope path exists
- [ ] `plans/jackin-desktop/README.md` status row for 004 updated
- [ ] Every commit is signed (`-s`), contains
      `Co-authored-by: Codex <codex@openai.com>`, and is pushed; no force
      pushes

## STOP conditions

Stop and report back (do not improvise) if:

- Any precondition fails, or "Starting state" does not match reality
  (drift check hits and an excerpt differs from live code).
- A step's verification fails twice after a reasonable fix attempt.
- The work requires touching an out-of-scope file or violating a Must NOT
  (N1: any Swift edit; N3: any price/cost/trend/history string).
- Assumption A1 turns out false. A1, verbatim from the coverage ledger:
  "Provider windows are fixed-slot; `resets_at` anchors Variant A
  (`window_start = resets_at − window_seconds`)" — falsified by: "live
  window observed rolling / usage non-zero at start breaking projections
  materially". If, while validating, a live provider window is observed
  rolling (or usage demonstrably non-zero at window start) such that the
  projections are materially wrong, STOP — the linear-from-window-start
  model itself is in question, not this plan's arithmetic.
- The capsule splitter tests (Step 4) or Swift splitter harnesses (Step 5)
  fail — that means the composite format drifted from the
  `"<pace> · Runs out in <duration>"` contract; do not "fix" the consumers,
  report instead.
- Step 1 makes any reserve or On-pace-ahead fixture gain a segment, or
  changes the ±2 band — the emission condition is wrong; report if a fix
  attempt does not restore them.

## Maintenance notes

- Plans 006/008 (popover / Usage window display) render this composite in
  Swift via the existing `splitPaceLabel`; they depend on the exact
  `" · "` separator and the `"Runs out in "` prefix. Any future wording
  change must update the capsule splitter, the Swift splitter, and their
  fixtures in one PR.
- Reviewer scrutiny: the strict exact pre-rounding integer cross-product
  decision (algebraically exact projection before reset; display rounding is
  later),
  and the band case
  `−2 ≤ delta < 0` producing `"On pace · Runs out in …"` — intentional,
  matches both Swift fixtures and the spec's emission condition; it also
  makes the TUI's previous mild over-claim ("Lasts until reset" for
  slightly-behind On-pace buckets) self-correcting, since a present
  "Runs out" detail suppresses the synthesized line.
- Depleted buckets (`remaining = 0`) emit `"Runs out in 0m"` per the spec
  letter (projection 0 precedes any future reset). If the operator finds
  this noisy next to the depleted meter, suppressing it is a spec change
  (amend the requirement's edge-case sentence), not a silent code tweak.
- Known accepted limitation (research failure mode): early-window
  jumpiness — small `elapsed` amplifies the whole-percent `u8` quantization
  of `remaining`. No smoothing is added; Variant A is deliberately
  history-free (the snapshot store retains only the latest row per bucket).
- Deferred: nothing. The Swift-side rendering of the two segments is plans
  006/008 territory, already anticipated by the shipped splitter.

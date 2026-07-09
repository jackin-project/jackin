# Plan 003: Stop deep-cloning the usage-view map on every refresh (restore the documented borrow invariant)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-usage/src/usage.rs crates/jackin-usage/src/usage/refresh.rs crates/jackin-usage/src/usage/tests.rs`
> If any of these changed, compare the "Current state" excerpts against the live
> code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

`jackin-usage`'s own contract file `crates/jackin-usage/CLAUDE.md` states:
"account materialization serializes from borrowed views/iterators, not full
clones." The code does the opposite: `materialize_accounts` deep-clones the
entire usage-view map on **every** refresh cycle, purely to serialize it to a
file. Each `FocusedUsageView` is label-heavy (multiple `Option<String>`, a
header struct, `Vec<QuotaBucketView>` with ~5 label `String`s per bucket, a
`Vec<UsageProviderTab>`), so the clone deep-copies dozens of heap strings per
account, proportional to total accounts, on a periodic background path. This is
documented decision drift (the doc says one thing, the code does another) and a
straightforward allocation win with byte-identical output.

## Current state

The offending clone — `crates/jackin-usage/src/usage.rs:527-538`:

```rust
pub(crate) fn materialize_accounts(&self, generated_at_epoch: i64) -> Result<(), String> {
    let snapshots = self
        .snapshots
        .values()
        .map(|cached| cached.view.clone())   // <-- deep-clones every FocusedUsageView
        .collect::<Vec<_>>();
    write_materialized_usage_accounts(
        Path::new(MATERIALIZED_USAGE_ACCOUNTS_PATH),
        generated_at_epoch,
        snapshots,
    )
}
```

The writer it calls — `crates/jackin-usage/src/usage/refresh.rs:382-400`:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MaterializedUsageAccounts {
    pub(crate) generated_at_epoch: i64,
    pub(crate) snapshots: Vec<FocusedUsageView>,
}

pub(crate) fn write_materialized_usage_accounts(
    path: &Path,
    generated_at_epoch: i64,
    snapshots: Vec<FocusedUsageView>,      // <-- takes owned Vec
) -> Result<(), String> {
    let document = MaterializedUsageAccounts { generated_at_epoch, snapshots };
    let contents = serde_json::to_string_pretty(&document)
        .map_err(|err| format!("usage accounts encode failed: {err}"))?;
    atomic_write_usage_json(path, &contents)
}
```

Facts you need:
- `FocusedUsageView` derives `Serialize` + `Deserialize`
  (`crates/jackin-protocol/src/control.rs:158`), so serde serializes a slice of
  references identically to a `Vec` of owned values — the on-disk JSON is
  byte-identical.
- `MaterializedUsageAccounts` is read back (Deserialize) in the test at
  `crates/jackin-usage/src/usage/tests.rs:431` — **keep it** for the read path.
- The only two callers of `write_materialized_usage_accounts` are
  `usage.rs:533` (the hot path above) and `usage/tests.rs:428`
  (`write_materialized_usage_accounts(&path, 456, vec![view])`).

Key insight: collecting `Vec<&FocusedUsageView>` (a vector of pointers) is not
the problem — the deep `.clone()` of each view is. Switch the writer to accept
borrowed views and serialize through a borrowed twin.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Targeted test | `cargo nextest run -p jackin-usage -E 'test(materializ)'` | passes |
| Crate tests | `cargo nextest run -p jackin-usage` | all pass |
| Clippy | `cargo clippy -p jackin-usage --all-targets --locked -- -D warnings` | exit 0 |

## Scope

**In scope**:
- `crates/jackin-usage/src/usage/refresh.rs` (writer signature + borrowed serialize twin)
- `crates/jackin-usage/src/usage.rs` (`materialize_accounts` call site)
- `crates/jackin-usage/src/usage/tests.rs` (update the one caller at line 428)

**Out of scope** (do NOT touch):
- `crates/jackin-protocol/src/control.rs` — do not change `FocusedUsageView`.
- The on-disk JSON format — output must stay byte-identical (the reader and the
  test at tests.rs:431 depend on it).
- `atomic_write_usage_json` and the tmp-file logic below it.

## Git workflow

- Branch: operator's active branch, or `perf/materialize-accounts-borrow`.
- One commit, conventional, signed. Example:
  `perf(usage): serialize materialized accounts from borrows, not clones`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add a borrowed serialize twin and change the writer signature

In `crates/jackin-usage/src/usage/refresh.rs`, keep `MaterializedUsageAccounts`
(owned, `Deserialize`) for the read path, and change
`write_materialized_usage_accounts` to accept a borrowed slice and serialize
through a borrowed struct:

```rust
#[derive(Serialize)]
struct MaterializedUsageAccountsRef<'a> {
    generated_at_epoch: i64,
    snapshots: &'a [&'a FocusedUsageView],
}

pub(crate) fn write_materialized_usage_accounts(
    path: &Path,
    generated_at_epoch: i64,
    snapshots: &[&FocusedUsageView],
) -> Result<(), String> {
    let document = MaterializedUsageAccountsRef { generated_at_epoch, snapshots };
    let contents = serde_json::to_string_pretty(&document)
        .map_err(|err| format!("usage accounts encode failed: {err}"))?;
    atomic_write_usage_json(path, &contents)
}
```

The field names (`generated_at_epoch`, `snapshots`) and order are identical to
the owned struct, so the serialized JSON is byte-identical.

**Verify**: `cargo check -p jackin-usage` — will fail at the two call sites
(next steps fix them). That is expected; do not "fix" by reverting.

### Step 2: Update the hot-path caller to pass borrows

In `crates/jackin-usage/src/usage.rs:527-538`, replace the clone with a borrow:

```rust
pub(crate) fn materialize_accounts(&self, generated_at_epoch: i64) -> Result<(), String> {
    let snapshots: Vec<&FocusedUsageView> =
        self.snapshots.values().map(|cached| &cached.view).collect();
    write_materialized_usage_accounts(
        Path::new(MATERIALIZED_USAGE_ACCOUNTS_PATH),
        generated_at_epoch,
        &snapshots,
    )
}
```

Ensure `FocusedUsageView` is in scope (it is already used in this crate; if the
type isn't imported in this file, add the import that `refresh.rs` uses).

**Verify**: `cargo check -p jackin-usage` — the hot-path call site now compiles;
only the test caller remains.

### Step 3: Update the test caller

In `crates/jackin-usage/src/usage/tests.rs:428`, change:

```rust
write_materialized_usage_accounts(&path, 456, vec![view]).expect("write accounts");
```

to pass a borrowed slice:

```rust
write_materialized_usage_accounts(&path, 456, &[&view]).expect("write accounts");
```

(If `view` is moved/consumed later in that test, this borrow actually makes it
available afterward — no other change needed. If the test previously relied on
`view` being moved here, keep `view` and use `&[&view]`.)

**Verify**: `cargo nextest run -p jackin-usage -E 'test(materializ)'` passes —
this test round-trips the written JSON through `MaterializedUsageAccounts`,
proving the output is still valid and byte-shape-compatible.

### Step 4: Full crate check

**Verify**: `cargo nextest run -p jackin-usage` all pass; `cargo clippy -p
jackin-usage --all-targets --locked -- -D warnings` exits 0.

## Test plan

- No new test is strictly required — the existing round-trip test at
  `usage/tests.rs:428-431` (write then `serde_json::from_str::<MaterializedUsageAccounts>`)
  already proves byte-compatible output and is the regression guard.
- Optional: add an assertion in that test that the decoded `snapshots.len()`
  matches the input count, if not already asserted.
- Verification: `cargo nextest run -p jackin-usage` → all pass.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -n 'cached.view.clone()' crates/jackin-usage/src/usage.rs` returns nothing
- [ ] `grep -n 'snapshots: &\[&FocusedUsageView\]' crates/jackin-usage/src/usage/refresh.rs` matches
- [ ] `cargo nextest run -p jackin-usage` exits 0
- [ ] `cargo clippy -p jackin-usage --all-targets --locked -- -D warnings` exits 0
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The round-trip test at `usage/tests.rs:428-431` fails after the change — that
  means the serialized JSON is NOT byte-identical; do not force it, report the
  diff.
- `materialize_accounts` no longer matches the "Current state" excerpt (the
  clone was already removed, or the function moved).
- A third caller of `write_materialized_usage_accounts` exists that the grep in
  "Current state" missed — update it the same way, but if its shape is
  incompatible, report it.

## Maintenance notes

- This restores the invariant stated in `crates/jackin-usage/CLAUDE.md` line 2;
  a reviewer should confirm the on-disk format is unchanged (the round-trip test
  is the proof).
- Related but separate: the token-monitor whole-file re-read on each poll and
  the per-row usage upsert (no cached prepared statement) are lower-leverage
  perf items recorded in `plans/code-health/README.md`, not part of this plan.

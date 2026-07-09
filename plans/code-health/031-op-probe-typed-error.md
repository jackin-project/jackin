# Plan 031: Phase 2 — typed `op` probe errors: stop classifying 1Password failures by substring across a crate boundary

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat c856acc9d..HEAD -- crates/jackin-env/src/op_cli.rs crates/jackin-console-oppicker/src/lib.rs crates/jackin-core/src/`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (error-classification behavior feeds the op-picker UI states; the existing classification tests are the contract)
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `c856acc9d`, 2026-07-09

## Why this matters

First-wave finding DEBT-op-typed-error, now with its open ownership question resolved. The 1Password probe path stringifies process errors in `jackin-env`, and `jackin-console-oppicker` re-derives their meaning by substring matching in another crate — the code itself documents the debt: "Classifies by stderr substring because the root picker receives process errors through `anyhow::Error` rather than typed variants" (`oppicker/src/lib.rs:986-987`). Substring contracts across crate boundaries break silently: a wording change in `op_cli.rs`'s error message (or in the `op` CLI's own stderr) flips the picker from a helpful "not signed in — run `op signin`" state to a generic fatal error, and no test at the boundary catches it. This is the roadmap's error-taxonomy rule in miniature (Phase 2: "machine-matchable variants; `anyhow` only at boundaries where errors are reported, not handled" — here the error IS handled, so it must be typed). **Ownership decision (was the open question):** the enum lives in `jackin-core` — the only shared ancestor of `jackin-env` (constructor side) and `jackin-console-oppicker` (consumer side), and already home to the port vocabulary (`CommandRunner`, the sink traits) — placed beside them.

## Current state

Verified at the planning commit.

- Constructor side, `crates/jackin-env/src/op_cli.rs:485-501`:

  ```rust
  fn run_op_json(
      binary: &str,
      args: &[&str],
      timeout: std::time::Duration,
  ) -> anyhow::Result<Vec<u8>> {
      let cmd_label = format!("op {}", args.join(" "));
      run_op_with_timeout(binary, args, timeout).map_err(|e| {
          let msg = e.to_string();
          if msg.contains("not currently signed") || msg.contains("no accounts") {
              anyhow::anyhow!(
                  "1Password CLI is not signed in (running `{cmd_label}` returned: {msg}). \
                   Run `op signin` in your shell, then retry."
              )
          } else {
              e
          }
      })
  }
  ```

  — the signed-in check ALSO happens here by substring, and rewrites the error into prose the consumer then re-parses. Read `run_op_with_timeout` (same file) to learn the raw failure shapes: spawn failure ("failed to spawn op…"), timeout, non-zero exit with stderr.
- Consumer side, `crates/jackin-console-oppicker/src/lib.rs:986-1001`:

  ```rust
  /// Classifies by stderr substring because the root picker receives
  /// process errors through `anyhow::Error` rather than typed variants.
  pub fn classify_probe_error_message(message: impl Into<String>) -> OpPickerError {
      let message = message.into();
      if message.contains("failed to spawn") {
          OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
      } else if message.contains("not signed in")
          || message.contains("not currently signed")
          || message.contains("no accounts")
      {
          OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
      } else {
          OpPickerError::Fatal(OpPickerFatalState::GenericFatal { message })
      }
  }
  ```

  Called from `lib.rs:129` (`OpLoadState::Error(classify_probe_error_message(message))`). Classification behavior is pinned by tests in `crates/jackin-console/src/tui/components/op_picker/tests.rs:139-148` ("failed to spawn op" → NotInstalled, "not currently signed in" → NotSignedIn, "boom" → GenericFatal).
- Message flow between the two: find how the anyhow error travels from env's probe to the oppicker's `message` (grep `classify_probe_error_message` callers upward through `crates/jackin-console/src` and `crates/jackin/src` — the probe result crosses as a string via the console's op-picker services). The typed fix must survive that transport: **anyhow preserves the typed source** — construct the typed error in env and `downcast` at the classification point; where the transport genuinely stringifies (e.g. crossing a channel as `String`), the classifier keeps the substring path as documented fallback.
- Crate relationships (all verified): env deps core; oppicker deps core (+diagnostics, tui); console deps both env and oppicker. Core is the shared home.
- Error conventions: core currently hosts small dedicated error types (e.g. `ParseMountIsolationError` with `thiserror`, `isolation.rs:5-8`) — thiserror derive is the pattern; workspace `thiserror = "2.0"` dep exists.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core | `cargo nextest run -p jackin-core` / clippy `-D warnings` | pass / exit 0 |
| Env | `cargo nextest run -p jackin-env` | all pass |
| Oppicker + console | `cargo nextest run -p jackin-console-oppicker -p jackin-console` | all pass (incl. the classification tests) |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-core/src/op_probe_error.rs` (create) + tests + lib.rs registration + README row
- `crates/jackin-env/src/op_cli.rs` (typed construction in `run_op_json`/`run_op_with_timeout`'s error path)
- `crates/jackin-console-oppicker/src/lib.rs` (`classify_probe_error_message` gains a typed path; substring becomes fallback)
- The transport call sites in `crates/jackin-console/src` ONLY if they must pass the anyhow error instead of a pre-stringified message (minimal signature adjustments)
- `plans/code-health/README.md` (row + strike DEBT-op-typed-error, recording the ownership decision)

**Out of scope**:
- The other ~40 `.contains()` classification sites workspace-wide (different subsystems; this plan is the op-probe path only — the pattern it establishes is the point)
- Changing any operator-visible message text (the "Run `op signin`…" guidance string stays verbatim)
- `OpPickerError`/`OpPickerFatalState` variant changes (consumer enum untouched; only how it's derived)
- jackin-env's broader anyhow usage (DEBT-anyhow-in-libs, separate)

## Git workflow

- Branch off `main`: `refactor/op-probe-typed-error`.
- Commits: core enum; env construction; oppicker classification + transport. `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: The typed error in core

Create `crates/jackin-core/src/op_probe_error.rs`:

```rust
//! Typed failure classes for probing the 1Password `op` CLI. Constructed at
//! the process boundary (jackin-env); consumed by pickers/UI without
//! substring matching. Attached as an anyhow source so it survives `?`
//! propagation and is recovered by `downcast_ref`.

/// Why an `op` CLI probe failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpProbeError {
    /// The `op` binary could not be spawned (not installed / not on PATH).
    #[error("failed to spawn op: {detail}")]
    NotInstalled { detail: String },
    /// The CLI ran but reports no signed-in account.
    #[error("1Password CLI is not signed in: {detail}")]
    NotSignedIn { detail: String },
    /// The probe timed out.
    #[error("op timed out after {seconds}s")]
    Timeout { seconds: u64 },
    /// Any other failure; carries the raw message.
    #[error("{message}")]
    Other { message: String },
}
```

Variant set is the shape — reconcile it against `run_op_with_timeout`'s real failure branches in Step 2 (add/drop variants to match reality; every variant must have a real construction site). Register in lib.rs; README row; `op_probe_error/tests.rs`: Display texts render the detail; the enum is `Eq`-comparable.

**Verify**: `cargo nextest run -p jackin-core` → pass; clippy clean.

### Step 2: Construct it in jackin-env

In `op_cli.rs`: read `run_op_with_timeout` fully. At each failure branch, wrap: `Err(anyhow::Error::new(OpProbeError::NotSignedIn { detail: msg }).context("1Password CLI is not signed in … Run `op signin` in your shell, then retry."))` — the typed error is the SOURCE, the operator guidance is the context, so `e.to_string()`/display chains keep today's wording (verify by comparing rendered output before/after on a forced failure in tests) while `e.downcast_ref::<OpProbeError>()` recovers the class. The substring checks inside `run_op_json` collapse into constructing the right variant at the true origin (spawn error → NotInstalled; the "not currently signed"/"no accounts" stderr detection stays — but as the single, documented place that inspects op's stderr wording).

**Verify**: `cargo nextest run -p jackin-env` → all pass (extend its op_cli tests: a forced spawn-failure and a fake not-signed-in stderr each yield an anyhow error whose `downcast_ref::<OpProbeError>()` matches the right variant AND whose display string contains the same guidance text as before).

### Step 3: Typed classification in oppicker

Trace the transport (Step-1-of-execution read): adjust the minimal set of console call sites so the oppicker receives either the `anyhow::Error` itself or `(Option<OpProbeError>, String)`. Then:

```rust
pub fn classify_probe_error(error: &anyhow::Error) -> OpPickerError {
    if let Some(probe) = error.downcast_ref::<jackin_core::op_probe_error::OpProbeError>() {
        return match probe { /* variant → OpPickerFatalState, 1:1 */ };
    }
    classify_probe_error_message(error.to_string()) // documented fallback
}
```

`classify_probe_error_message` stays public (tests + genuine string-only paths) but its doc comment changes from explaining the debt to naming itself the fallback for stringified transports. Existing classification tests keep passing; add downcast-path tests: an `anyhow::Error` wrapping each variant classifies correctly WITHOUT relying on message text (construct with a decoy message like "xyzzy" to prove the substring path wasn't used).

**Verify**: `cargo nextest run -p jackin-console-oppicker -p jackin-console` → all pass incl. the new decoy-message tests; `cargo nextest run --workspace --all-features --locked` → all pass.

### Step 4: Index + roadmap

Ledger: strike DEBT-op-typed-error, recording the ownership decision (jackin-core, beside the port vocabulary) and the residue (~40 other `.contains()` classification sites remain, different subsystems). Roadmap Phase 2 error-taxonomy item: note the pattern's first instance shipped.

**Verify**: `cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Env: 2+ construction tests (spawn-fail, not-signed-in) asserting variant + preserved display text.
- Oppicker: decoy-message downcast tests per variant + the existing substring tests untouched (they now pin the fallback).
- Workspace suite green.

## Done criteria

- [ ] `OpProbeError` in core with real construction sites for every variant
- [ ] `run_op_json`'s substring rewrite replaced by typed construction at origin; operator-visible text unchanged (asserted in tests)
- [ ] Oppicker classifies via downcast first; decoy tests prove it; substring path documented as fallback
- [ ] All suites green; `cargo xtask ci --fast` → `ci gate OK`
- [ ] Ledger updated with the decision + residue; `plans/code-health/README.md` row updated

## STOP conditions

- The transport between env and oppicker stringifies in a way that cannot carry the anyhow error without restructuring channels/services beyond minimal signature changes (report the transport shape — the fallback-only outcome may be the honest result, and that changes the plan's value).
- `run_op_with_timeout`'s failure branches don't map onto ≤5 variants.
- Any operator-visible error text changes (display-parity assertions fail).
- `OpPickerFatalState` turns out to need a new variant to represent Timeout distinctly (consumer enum is out of scope — map Timeout onto the existing state it lands in today, note it).

## Maintenance notes

- New `op` failure classes get a variant here + a construction site + a mapping arm — the compiler now walks an agent through all three.
- The ~40 other substring-classification sites are candidates for the same pattern; when one bites (defect ledger), copy this shape.
- Reviewer should scrutinize: display-text parity (operators see identical messages) and that the downcast path is provably exercised (the decoy tests).

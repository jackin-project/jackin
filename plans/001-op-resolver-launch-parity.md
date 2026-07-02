# Plan 001: op:// resolution on the launch path uses the wide launch timeout and the `--`/`op://` argument guard

> **Executor instructions**: Follow this plan step by step. Run every verification command and
> confirm the expected result before moving on. If a STOP condition occurs, stop and report — do not
> improvise. When done, update this plan's row in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 46511939d..HEAD -- crates/jackin-env/src/op_cli.rs crates/jackin-runtime/src/runtime/launch/launch_slot.rs crates/jackin-env/src/resolve.rs crates/jackin-core/src/env_value.rs`
> If any of these changed since this plan was written, compare the "Current state" excerpts against
> live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: security + bug
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

Two independent gaps on the same 1Password (`op`) resolution code, fixed together because they touch
the same files:

1. **Timeout asymmetry (bug).** Recent fix #709 widened the operator-env `op read` timeout from 30s to
   120s because a cold 1Password app/daemon wake-up can exceed 30s and abort an otherwise-successful
   launch. But the **`[github.env]`** path (`GH_TOKEN`/`GH_HOST`/`GH_ENTERPRISE_TOKEN`) still builds its
   resolver with the 30s default, so operators who put GitHub creds in `[github.env]` as `op://` refs —
   the documented canonical location — still hit the intermittent "GitHub-only launch failure" #709 set
   out to remove.
2. **Argument-injection guard asymmetry (security, defense-in-depth).** The on-demand credential
   resolver (`exec_host.rs`) validates `source` starts with `op://`, rejects flag-like segments, and
   passes `["read", "--", op_ref]` so `op` cannot interpret a `-`-leading value as a flag. The
   launch-time path (`op_cli.rs`) passes `["read", reference]` — **no `--` sentinel, no `op://`
   validation.** Today the launch refs originate from operator config (pinned at pick time), so this is
   not directly reachable by the untrusted role; it is a latent inconsistency that a hand-edited config
   or any future code constructing an `OpRef` from less-trusted input would expose.

## Current state

- `crates/jackin-env/src/op_cli.rs:11-12` — the two timeout constants:
  ```rust
  const OP_DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
  const OP_LAUNCH_ENV_TIMEOUT: std::time::Duration = std::time::Duration::from_mins(2);
  ```
- `crates/jackin-env/src/op_cli.rs:34-46` — the two constructors:
  ```rust
  pub fn new() -> Self {
      Self { binary: OP_DEFAULT_BIN.to_owned(), timeout: OP_DEFAULT_TIMEOUT, account: None }
  }
  // ...
  pub fn new_launch_env() -> Self {
      Self { binary: OP_DEFAULT_BIN.to_owned(), timeout: OP_LAUNCH_ENV_TIMEOUT, /* ... */ }
  ```
- `crates/jackin-runtime/src/runtime/launch/launch_slot.rs:209` — the github-env resolver, which builds
  its default runner with the **30s** constructor:
  ```rust
  let default_runner = jackin_env::OpCli::new();
  let runner: &dyn jackin_env::OpRunner = opts.op_runner.as_deref().unwrap_or(&default_runner);
  ```
  Compare `crates/jackin-env/src/resolve.rs:377` (operator-env, already widened):
  ```rust
  let runner = OpCli::new_launch_env();
  ```
- `crates/jackin-env/src/op_cli.rs:266-274` — the launch-time `op read` invocation, **missing** the
  `--` sentinel:
  ```rust
  let mut child = spawn_op_with_retry(|| {
      let mut cmd = Command::new(&self.binary);
      if let Some(account) = self.account.as_deref() { cmd.args(["--account", account]); }
      cmd.args(["read", reference])            // <-- no `--`, no op:// check
         .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
  ```
  The on-demand sibling to mirror is documented at `crates/jackin-runtime/src/exec_host.rs:23-25`:
  "`source` must start with `op://` and the `--` end-of-options sentinel is inserted before passing to
  `op read`."
- `crates/jackin-core/src/env_value.rs:94-97` — `OpRef.op` is a raw `String` with `deny_unknown_fields`
  but **no** `op://` prefix validation at the type boundary.

Conventions: workspace forbids `unsafe`; `unwrap`/`expect`/`panic` are lint-denied — return `Result`.
Tests live in a sibling `tests.rs` (never inline `#[cfg(test)] mod tests {}`); see
`crates/jackin-env/src/op_cli/tests.rs` for the existing style.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Build | `cargo check -p jackin-env -p jackin-runtime -p jackin-core --all-targets` | exit 0 |
| Test (env) | `cargo nextest run -p jackin-env` | all pass |
| Test (runtime slot) | `cargo nextest run -p jackin-runtime -E 'test(/github_env/)'` | all pass |
| Clippy | `cargo clippy -p jackin-env -p jackin-runtime -- -D warnings` | exit 0 |

## Scope

**In scope:**
- `crates/jackin-runtime/src/runtime/launch/launch_slot.rs` (swap constructor)
- `crates/jackin-env/src/op_cli.rs` (add `--` sentinel + `op://` validation to the read path)
- `crates/jackin-core/src/env_value.rs` (optional: enforce `op://` at deserialize — see step 3)
- `crates/jackin-env/src/op_cli/tests.rs` (new test)
- `crates/jackin-runtime/src/runtime/launch/launch_slot/tests.rs` (new test, create if absent)

**Out of scope:**
- The on-demand `exec_host.rs` resolver — it already has both guards; do not touch it.
- The `spawn_op_with_retry` retry logic itself.
- Any change to the `op://` ref *format* accepted by operator config.

## Git workflow

- Branch: `fix/op-resolver-launch-parity` (this repo forbids committing to `main`; ask the operator to
  confirm the branch name before the first commit if you are on `main`).
- Conventional Commits, signed: `git commit -s -m "fix(env): use launch timeout and op:// guard for github env"`.
- Do not push or open a PR unless the operator instructs it.

## Steps

### Step 1: Widen the github-env resolver timeout

In `crates/jackin-runtime/src/runtime/launch/launch_slot.rs:209`, change
`jackin_env::OpCli::new()` → `jackin_env::OpCli::new_launch_env()`. This is the only functional change
for finding CORRECTNESS-01.

**Verify**: `cargo check -p jackin-runtime` → exit 0. Then
`grep -n "OpCli::new()" crates/jackin-runtime/src/runtime/launch/launch_slot.rs` → **no matches**
(the only `OpCli::` call in that file is now `new_launch_env`).

### Step 2: Add the `--` sentinel and `op://` validation to the launch read path

In `crates/jackin-env/src/op_cli.rs` at the read invocation (~line 271), before spawning:
- Validate the reference: if `!reference.starts_with("op://")`, return an `anyhow::Error`
  (e.g. `anyhow::bail!("op reference must start with op://: {reference}")`) — do **not** panic.
- Change `cmd.args(["read", reference])` → `cmd.args(["read", "--", reference])`.

Match the exact wording/shape of the on-demand guard in `exec_host.rs` (read
`crates/jackin-runtime/src/exec_host.rs:205-249` first and mirror its validation message style).

**Verify**: `grep -n '"read", "--"' crates/jackin-env/src/op_cli.rs` → 1 match.
`cargo clippy -p jackin-env -- -D warnings` → exit 0.

### Step 3: Enforce `op://` at the `OpRef` type boundary (belt-and-suspenders)

In `crates/jackin-core/src/env_value.rs`, add a validation so an `OpRef` whose `op` field does not start
with `op://` cannot be constructed from deserialization. Prefer a `#[serde(deserialize_with = ...)]` on
the `op` field (or a `TryFrom<String>`) that returns a serde error on a non-`op://` value — so every
consumer inherits the guard, not just the two read sites. If the surrounding serde derive makes this
awkward, STOP and report rather than restructuring the type; step 2 already closes the immediate gap.

**Verify**: `cargo check -p jackin-core --all-targets` → exit 0.

### Step 4: Tests

- In `crates/jackin-env/src/op_cli/tests.rs`, add a test that a non-`op://` reference is rejected by the
  read path (inject via the existing `OpRunner`/test seam — see `crates/jackin-env/src/test_support.rs`).
- Add a test asserting the github-env resolver in `launch_slot.rs` uses a 120s-budget runner. If the
  code path takes an injected `op_runner` (it does — `opts.op_runner`), assert the default runner's
  timeout via a small accessor or by mirroring the existing
  `crates/jackin-env/src/op_cli/tests.rs::launch_env_runner_uses_wider_bounded_timeout` pattern.

**Verify**: `cargo nextest run -p jackin-env -p jackin-runtime -E 'test(/op|github_env/)'` → all pass,
new tests included.

## Test plan

- New: `op_cli` read path rejects a `-`-leading / non-`op://` reference (regression for SECURITY-03).
- New: github-env default runner carries the 120s launch timeout (regression for CORRECTNESS-01).
- Pattern to follow: `crates/jackin-env/src/op_cli/tests.rs` (the existing timeout-constant test).

## Done criteria

- [ ] `cargo check --workspace --all-targets` exits 0
- [ ] `cargo clippy -p jackin-env -p jackin-runtime -p jackin-core -- -D warnings` exits 0
- [ ] `grep -n "OpCli::new()" crates/jackin-runtime/src/runtime/launch/launch_slot.rs` → no matches
- [ ] `grep -n '"read", "--"' crates/jackin-env/src/op_cli.rs` → 1 match
- [ ] `cargo nextest run -p jackin-env -p jackin-runtime` passes with the 2 new tests
- [ ] Only in-scope files modified (`git status`)
- [ ] `plans/README.md` row updated

## STOP conditions

- The `launch_slot.rs` excerpt no longer shows `OpCli::new()` (someone already fixed CORRECTNESS-01).
- Step 3's serde change requires restructuring `EnvValue`/`OpRef` beyond adding a field validator.
- Any test needs a real `op` binary / real 1Password account — these tests must use the injected runner
  seam, never the real CLI.

## Maintenance notes

- The two `op read` call sites (`op_cli.rs` launch path, `exec_host.rs` on-demand path) must keep the
  same `--` + `op://` guard. A reviewer should check both when either changes.
- If a third `op read` site appears, factor the guard into one helper rather than copying it (the repo's
  DRY rule in `ENGINEERING.md` explicitly targets this class).
- Deferred: unifying the two resolvers behind one type is out of scope here; step 3's type-level guard is
  the cheap down payment on that.

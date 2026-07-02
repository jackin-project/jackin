# Plan 007: Stop blanking Claude account metadata on a `.claude.json` read error

> **Executor instructions**: Small fix on the credential-forward path. Run every verification command.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-instance/src/auth.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`copy_host_claude_json` reads the host `~/.claude.json` with
`std::fs::read_to_string(host_path).unwrap_or_else(|_| "{}".to_owned())`, so **any** non-`NotFound`
failure (EACCES, EIO, non-UTF-8) is swallowed and a `{}` account file is written into the container —
even though Sync mode has already found real credentials. The forwarded `.credentials.json` token stays
intact, but the account/onboarding metadata is wiped, which can re-trigger Claude's onboarding wizard or
drop org context inside the container, with **no diagnostic** explaining why (a genuine read error is
indistinguishable from "file absent"). This is on the auth-forward path — the behavior the product
exists to get right.

## Current state

`crates/jackin-instance/src/auth.rs:1074-1079`:
```rust
/// Copy the host's `.claude.json` into the container state, or write `{}`
/// if the host file doesn't exist.
fn copy_host_claude_json(host_path: &Path, dest_path: &Path) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(host_path).unwrap_or_else(|_| "{}".to_owned());
    write_private_file(dest_path, &content)
}
```
The intended fallback is **only** for `NotFound`. The sibling function `read_nonempty_credentials_file`
in the same file already distinguishes `NotFound` from other errors — mirror it (find it:
`grep -n "read_nonempty_credentials_file\|ErrorKind::NotFound" crates/jackin-instance/src/auth.rs`).
A `debug_log!` telemetry macro is available in this crate (`jackin_diagnostics::debug_log!`).

## Scope

**In scope:** `crates/jackin-instance/src/auth.rs` (`copy_host_claude_json`) and its `tests.rs`.
**Out of scope:** the credentials-file copy path (already correct); the Sync-vs-fresh mode decision logic.

## Steps

### Step 1: Treat only `NotFound` as the `{}` fallback

Rewrite `copy_host_claude_json` to match on the read result:
- `Ok(content)` → write it;
- `Err(e)` where `e.kind() == ErrorKind::NotFound` → write `"{}"` (the documented, intended case);
- any other `Err(e)` → `debug_log!` the error with the path and credential-file context, and **propagate**
  it (`return Err(...)`) rather than masking. Do **not** include file *contents* in the log — path +
  `io::Error` only (never a secret value).

Match the error-handling shape of `read_nonempty_credentials_file` in the same file.

**Verify**: `cargo check -p jackin-instance --all-targets` → exit 0.

### Step 2: Tests

In `auth/tests.rs`, add:
- host file present → its content is copied;
- host file absent (`NotFound`) → `{}` written, `Ok`;
- host file present but unreadable (simulate a read error — e.g. a path that is a directory, or use the
  crate's existing fs test seam) → returns `Err`, does **not** write `{}`.
Model after existing `auth/tests.rs` cases (the crate has 99 auth tests; find one that builds a temp
host home).

**Verify**: `cargo nextest run -p jackin-instance -E 'test(/claude_json|copy_host/)'` → pass.

## Done criteria

- [ ] Non-`NotFound` read errors propagate (test proves no `{}` is written on EACCES/dir)
- [ ] `NotFound` still yields `{}` (test proves)
- [ ] `grep -n "unwrap_or_else" crates/jackin-instance/src/auth.rs` no longer matches this function
- [ ] `cargo clippy -p jackin-instance -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- Propagating the error breaks a launch path that currently tolerates a `{}` write on error in a way the
  tests reveal is intentional — report the caller before changing the signature contract.

## Maintenance notes

- Reviewer: confirm the error log never contains file contents (only path + error kind).
- If Claude tolerates an empty `.claude.json` alongside valid credentials better than assumed, the
  onboarding-retrigger impact is smaller — but propagating a real IO error is still correct.

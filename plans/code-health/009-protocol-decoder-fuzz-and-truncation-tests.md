# Plan 009: Add a fuzz target and truncation/version-skew tests for the protocol wire decoders

> **Executor instructions**: Follow step by step. Run every verification command
> and confirm the expected result before moving on. If a "STOP condition" occurs,
> stop and report. When done, update the status row in
> `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-protocol/src/attach.rs`
> If `decode_client`/`decode_server` signatures changed, compare against "Current
> state"; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (additive test-only code)
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

`jackin-protocol` decodes untrusted bytes off the capsule attach/control socket
(an in-container client, i.e. agent-adjacent code, talks to the host through it).
The decoders `decode_client`/`decode_server` have only hand-written round-trip
tests; there is **no fuzz target and no truncation/version-skew coverage** for
them — the repo's only fuzz target today is the terminal parser. A malformed,
truncated, or hostile frame that panics the decoder would take down the daemon.
This plan adds the missing fuzz target (mirroring the existing `jackin-term/fuzz`
crate) plus CI-runnable unit tests asserting the decoders fail closed (clean
`Err`, never panic) on truncated payloads and unknown tags.

## Current state

`crates/jackin-protocol/src/attach.rs`:

```rust
pub fn decode_client(tag: u8, payload: Vec<u8>) -> Result<ClientFrame> { … }   // :988
pub fn decode_server(tag: u8, payload: Vec<u8>) -> Result<ServerFrame> { … }   // :1201
```

- These take an already-split `(tag, payload)` and return a `Result`. Existing
  round-trip tests are in `crates/jackin-protocol/src/attach/tests.rs` (they
  `use super::*;` and call `encode_client`/`encode_server` + the decoders).
- There are `encode_client`/`encode_server` functions in the same module (used
  by the round-trip tests) — use them to build valid frames for the seed corpus.
- The existing fuzz crate to mirror is `crates/jackin-term/fuzz/`:
  - `Cargo.toml`: `name = "jackin-term-fuzz"`, `edition = "2021"`, its own
    `[workspace]`, `[package.metadata] cargo-fuzz = true`, deps `jackin-term = {
    path = ".." }` + `libfuzzer-sys = "0.4"`, and a `[[bin]]` with `test = false`,
    `bench = false`.
  - target `src/damage_grid_process.rs` starts `#![no_main]` + `use
    libfuzzer_sys::fuzz_target;` and calls the crate's process entry with
    arbitrary bytes.
- `cargo-fuzz` is a pinned dev tool (`mise.toml`: `"cargo:cargo-fuzz" = "0.13.1"`).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Protocol unit tests | `cargo nextest run -p jackin-protocol -E 'test(decode)'` | pass |
| Crate tests | `cargo nextest run -p jackin-protocol` | all pass |
| Fuzz build check | `cargo fuzz build --fuzz-dir crates/jackin-protocol/fuzz decode_frames` | builds (exit 0) |
| Fuzz smoke run | `cargo fuzz run --fuzz-dir crates/jackin-protocol/fuzz --sanitizer none decode_frames -- -max_total_time=30` | no crash |

If `cargo fuzz` is unavailable in the environment (no nightly, no libfuzzer),
complete the crate + target + unit tests and note in your status that the fuzz
build/run could not be exercised here; the unit tests are the CI-runnable guard.

## Scope

**In scope** (all new files/additions):
- `crates/jackin-protocol/fuzz/Cargo.toml` (create)
- `crates/jackin-protocol/fuzz/src/decode_frames.rs` (create)
- `crates/jackin-protocol/fuzz/.gitignore` (create — mirror term's)
- `crates/jackin-protocol/src/attach/tests.rs` (add truncation/unknown-tag/skew tests)

**Out of scope**:
- `attach.rs` production code — do NOT change the decoders. If a fuzz run finds a
  real panic, STOP and report it as a new bug (that's a separate fix).
- CI workflow files — wiring the new target into the CI fuzz-smoke lane is a
  noted follow-up (workflow edits are gated); this plan makes the target exist
  and build.

## Git workflow

- Branch: operator's active branch, or `test/protocol-decoder-fuzz`.
- One commit, conventional, signed. Example:
  `test(protocol): add wire-decoder fuzz target and truncation/skew tests`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Create the fuzz crate mirroring `jackin-term/fuzz`

`crates/jackin-protocol/fuzz/Cargo.toml`:

```toml
[package]
name = "jackin-protocol-fuzz"
version = "0.0.0"
publish = false
edition = "2021"

[package.metadata]
cargo-fuzz = true

[workspace]

[dependencies]
jackin-protocol = { path = ".." }
libfuzzer-sys = "0.4"

[[bin]]
name = "decode_frames"
path = "src/decode_frames.rs"
test = false
bench = false
```

`crates/jackin-protocol/fuzz/.gitignore` (copy from `crates/jackin-term/fuzz/.gitignore`
— typically `target/` and `corpus/` build artifacts; keep committed seed corpus
if you add one under a tracked path).

### Step 2: Write the fuzz target — goal: zero panics on any bytes

`crates/jackin-protocol/fuzz/src/decode_frames.rs`:

```rust
//! Fuzz target: feed arbitrary bytes to the protocol wire decoders.
//! Goal: **zero panics**, ever, on any (tag, payload) split.
//!
//! Run locally (CI-suitable short budget):
//!   cargo fuzz run --fuzz-dir crates/jackin-protocol/fuzz --sanitizer none decode_frames -- -max_total_time=60
//! Run overnight:
//!   cargo fuzz run --fuzz-dir crates/jackin-protocol/fuzz --sanitizer none decode_frames -- -max_total_time=86400

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let tag = data[0];
    let payload = data[1..].to_vec();
    // A hostile/truncated frame must fail closed (Err), never panic.
    let _ = jackin_protocol::attach::decode_client(tag, payload.clone());
    let _ = jackin_protocol::attach::decode_server(tag, payload);
});
```

**Verify**: `cargo fuzz build --fuzz-dir crates/jackin-protocol/fuzz decode_frames`
builds (exit 0). If `cargo fuzz` isn't available, `cargo check --manifest-path
crates/jackin-protocol/fuzz/Cargo.toml` at least parses the crate.

### Step 3: Add CI-runnable truncation / unknown-tag / version-skew unit tests

In `crates/jackin-protocol/src/attach/tests.rs`, add tests that assert the
decoders return `Err` (never panic) on adversarial input. Use the real tag
constants (they are in scope via `use super::*;` — e.g. `TAG_OUTPUT` appears in
the existing test; find the client/server tag constants and reuse them):

```rust
#[test]
fn decode_client_rejects_truncated_payloads_without_panic() {
    // For every known client tag, a deliberately-too-short payload must Err.
    for tag in 0u8..=40 {
        // 0-byte and 1-byte payloads exercise the length-prefix / field readers.
        let _ = super::decode_client(tag, Vec::new());
        let _ = super::decode_client(tag, vec![0x00]);
        let _ = super::decode_client(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]);
    }
    // The point is no panic; reaching here is the assertion.
}

#[test]
fn decode_server_rejects_truncated_payloads_without_panic() {
    for tag in 0u8..=40 {
        let _ = super::decode_server(tag, Vec::new());
        let _ = super::decode_server(tag, vec![0x00]);
        let _ = super::decode_server(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]);
    }
}

#[test]
fn decode_rejects_unknown_tags() {
    // Pick a tag value that is not a defined frame tag (confirm against the
    // TAG_* constants in attach.rs; 0xFE is a safe unlikely value).
    assert!(super::decode_client(0xFE, Vec::new()).is_err());
    assert!(super::decode_server(0xFE, Vec::new()).is_err());
}
```

Then add one round-trip-then-truncate test that takes a *valid* encoded frame,
lops off its last byte, and asserts the decode of the truncated remainder is
`Err` — model it on the existing round-trip test in the same file (which uses
`encode_server(ServerFrame::Output(...))`), e.g.:

```rust
#[test]
fn truncated_valid_frame_fails_closed() {
    let payload = vec![0xCDu8; 64];
    let frame = super::encode_server(super::ServerFrame::Output(payload));
    // frame = [tag, len(4 bytes BE), body…]; decode the body minus its last byte.
    let tag = frame[0];
    let body = &frame[5..frame.len() - 1];
    assert!(super::decode_server(tag, body.to_vec()).is_err() || body.len() < 64);
}
```

(Adjust field offsets to the real frame layout you see in the existing
round-trip test. If a given tag's decoder tolerates a short body by design,
weaken that specific assertion and note it — the non-negotiable property is *no
panic*.)

**Verify**: `cargo nextest run -p jackin-protocol -E 'test(decode)'` and
`test(truncated)` pass.

### Step 4: Full check

**Verify**: `cargo nextest run -p jackin-protocol` all pass; `cargo clippy -p
jackin-protocol --all-targets --locked -- -D warnings` exits 0.

## Test plan

- New unit tests: `decode_client_rejects_truncated_payloads_without_panic`,
  `decode_server_rejects_truncated_payloads_without_panic`,
  `decode_rejects_unknown_tags`, `truncated_valid_frame_fails_closed`. These are
  the CI-runnable guards (nextest runs them; the fuzz target is a separate lane).
- New fuzz target `decode_frames` builds and runs a short smoke without crashing.

## Done criteria

- [ ] `crates/jackin-protocol/fuzz/Cargo.toml` and `src/decode_frames.rs` exist
- [ ] `cargo fuzz build --fuzz-dir crates/jackin-protocol/fuzz decode_frames` exits 0 (or `cargo fuzz` unavailable, noted)
- [ ] `cargo nextest run -p jackin-protocol` exits 0; the 4 new tests pass
- [ ] `cargo clippy -p jackin-protocol --all-targets --locked -- -D warnings` exits 0
- [ ] Only the in-scope files created/modified (`git status`)
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report if:

- A fuzz run or a truncation test triggers a **real panic** in a decoder — that
  is a genuine bug in `attach.rs`; report it (with the minimizing input if fuzz
  found it) as a separate finding, do NOT patch `attach.rs` under this test-only
  plan.
- The decoder signatures don't match `(tag: u8, payload: Vec<u8>) -> Result<…>`
  (the API changed) — report the new shape.
- The workspace refuses to build the nested fuzz crate (it has its own
  `[workspace]`, so it should be excluded from the parent — confirm the parent
  `Cargo.toml` `members`/`exclude` mirrors how `jackin-term/fuzz` is handled).

## Maintenance notes

- **Follow-ups (recorded in README):** commit a minimized seed corpus of valid
  frames per tag (generate via `encode_*`), promote fuzz crash finds into a
  golden corpus, wire `decode_frames` into the CI fuzz-smoke lane beside the
  terminal target, and add an explicit host↔capsule version-negotiation test
  (old-capsule/new-host is currently an assumed-identical state).
- Reviewer should confirm the fuzz target exercises *both* decoders and that the
  unit tests assert "no panic" as the load-bearing property, not just specific
  error messages.

# Plan 018: Commit real PTY render-conformance fixtures and fix the harness doc drift

> **Executor instructions**: Test-infrastructure plan. Requires a working Docker + capsule build to record
> fixtures (see STOP conditions if you lack that). Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-capsule/tests/fixtures/pty crates/jackin-capsule/src/daemon/tests.rs TESTING.md crates/jackin-xtask/src/pty_fixture.rs`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

The render-conformance harness — the guard that "emitted frames reproduce the pane model" for the
highest-churn code (daemon compositor/multiplexer) — runs on **synthetic** byte streams only. There are
**zero** committed `.bin` PTY fixtures (`crates/jackin-capsule/tests/fixtures/pty/` has only `README.md`),
so it never sees a real agent's escape-sequence stream; real-world rendering regressions (cursor/damage/OSC
handling from actual Claude/Codex output) are uncovered, and the documented fixture-regeneration workflow
(`cargo xtask pty-fixture`) has never been exercised end-to-end. Separately, `TESTING.md` points at a
**non-existent** file (`daemon/render_conformance_tests.rs`; the harness actually lives inline in
`daemon/tests.rs`), and a stale in-code comment references `#[ignore]`-tagged specs that don't exist.

## Current state

- `crates/jackin-capsule/tests/fixtures/pty/` — only `README.md`, no `.bin` fixtures.
- `crates/jackin-capsule/src/daemon/tests.rs` (~7.6K lines, 253 tests) — the harness, inline. Comment
  (~line 6624): byte streams are "synthetic"; fixtures "land in `tests/fixtures/pty/` once a Stage-0
  operator run id exists"; and a stale note about `#[ignore]` tags "naming the fixing PR" for "PR 3 / PR 4
  of the capsule rendering plan" — but there are **0 `#[ignore]` attributes** in the workspace.
- `TESTING.md:50` references `crates/jackin-capsule/src/daemon/render_conformance_tests.rs` (does not exist).
- Regeneration path exists: `crates/jackin-xtask/src/pty_fixture.rs` (127 lines);
  `TESTING.md:54-61` documents `cargo xtask pty-fixture <run.jsonl> <session-label> <out.bin>` and
  `include_bytes!` referencing.

## Scope

**In scope:** `crates/jackin-capsule/tests/fixtures/pty/*.bin` (new), a non-synthetic harness scenario in
`crates/jackin-capsule/src/daemon/tests.rs`, `TESTING.md` (fix the path), and the stale comment in
`daemon/tests.rs`. **Out of scope:** the harness engine itself; the `pty_fixture` xtask internals (you use
it, don't rewrite it).

## Steps

### Step 1: Fix the two doc-drift items (no Docker needed — do this first)

- In `TESTING.md`, replace the `crates/jackin-capsule/src/daemon/render_conformance_tests.rs` reference with
  the real location `crates/jackin-capsule/src/daemon/tests.rs` (or, if you later extract the harness to the
  documented filename, do that instead — but the cheap fix is updating the doc).
- In `daemon/tests.rs` (~line 6624), delete/refresh the stale "PR 3 / PR 4 `#[ignore]` tags" comment since
  no such attributes exist.

**Verify**: `grep -rn "render_conformance_tests.rs" TESTING.md` → no matches;
`grep -rn "#\[ignore\]" crates/jackin-capsule/src` → no matches (confirming the comment was stale).

### Step 2: Record 2–3 real-agent PTY fixtures

Per `TESTING.md:52-61`: run a `--debug` session (`cargo run --bin jackin -- console --debug`), exercise an
agent (prefer `the-architect`), note the run id, then extract one session's stream:
```sh
cargo xtask pty-fixture ~/.jackin/data/diagnostics/runs/<run-id>.jsonl <session-label> \
  crates/jackin-capsule/tests/fixtures/pty/<agent>-<scenario>.bin
```
Record at least: one Claude/Codex "normal streaming" and one "full-screen redraw / alt-screen" scenario.
This step **also smoke-tests the regeneration path itself** (a stated goal).

### Step 3: Wire fixtures into a non-synthetic harness scenario

Add a harness scenario that `include_bytes!`-references each new `.bin` and asserts emitted frames
reproduce the pane model, following the existing synthetic scenarios' structure in `daemon/tests.rs`.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/render_conformance|pty_fixture|daemon/)'` → pass.

## Done criteria

- [ ] `TESTING.md` no longer references the non-existent `render_conformance_tests.rs`
- [ ] The stale `#[ignore]`/"PR 3/4" comment is gone
- [ ] ≥2 real `.bin` fixtures committed under `crates/jackin-capsule/tests/fixtures/pty/`
- [ ] ≥1 non-synthetic harness scenario references them via `include_bytes!` and passes
- [ ] `cargo nextest run -p jackin-capsule` green
- [ ] `plans/README.md` row updated

## STOP conditions

- **No Docker / cannot build the capsule to record fixtures**: do Step 1 (doc fixes) only, mark the plan
  `BLOCKED (fixture recording needs Docker + capsule build)`, and report — Step 1 still delivers value.
- `cargo xtask pty-fixture` fails to extract from the run JSONL: report the error; the regeneration path is
  itself broken and that's a finding (the whole point of Step 2 is to prove it works).

## Maintenance notes

- Fixtures are binary; note in the fixtures `README.md` which agent/scenario each captures and the
  regeneration command, so a future maintainer can re-record after a deliberate rendering change.
- If a rendering change intentionally alters frames, fixtures must be re-recorded (not hand-edited).

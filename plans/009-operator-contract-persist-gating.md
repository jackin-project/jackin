# Plan 009: Honest operator contract — gate every diagnostics-path display on `persists()`, fix the capsule's fabricated path, print a run-end pointer

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-launch-tui/src/progress.rs crates/jackin-capsule/src/container_context.rs crates/jackin-runtime/src/runtime/launch/launch_runtime.rs crates/jackin-core/src/launch_progress.rs crates/jackin/src/app.rs`
> STOP on excerpt mismatch.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none (independent of 001–008; merges cleanly before or after)
- **Category**: bug / dx
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

When OTLP export is active and `JACKIN_DIAGNOSTICS_FILE` is unset, the run's JSONL file is **deliberately not written** (`run.rs:151`: `persist = !otlp_active || diagnostics_file_forced()`), and `RunDiagnostics::path()`'s own doc warns "Callers that read or display it must gate on `persists()` first" (`run.rs:214-220`). Exactly one caller obeys (`app.rs:240` startup banner). Three surfaces disobey and show operators a clickable path to a file that does not exist: the launch cockpit's "Diagnostics log" row, the launch failure dialog, and the capsule's Debug-info dialog — whose "Reveal diagnostics" row is built from a path the capsule **fabricates from its own `$HOME`** when the host didn't pass one, producing a `file://` link that can't resolve on the host at all. And the run id — the only key that finds the run in the backend — is printed once at startup, only under `--debug`, then scrolls away; a non-debug run never learns it. One process currently tells the operator "diagnostics file: off (OTLP active)" in the banner and offers "Reveal diagnostics" in the capsule — self-contradiction.

## Current state

- Persist gate + `path()`/`persists()` semantics: `crates/jackin-diagnostics/src/run.rs:151, 214-227` (verified firsthand).
- Cockpit row: `crates/jackin-launch-tui/src/progress.rs:53-61` (verified) — `RichDriver::spawn(..., diagnostics.path().display().to_string(), ...)` unconditional; renders via `crates/jackin-tui/src/components/container_info.rs:165-171` as a copyable `file://` row.
- Failure dialog: `progress.rs:150` (verified) — `failure.diagnostics_path = Some(self.diagnostics.path().to_path_buf())` unconditional; rendered at `crates/jackin-launch-tui/src/tui/components/failure_dialog.rs:45`.
- Capsule fabrication: `crates/jackin-capsule/src/container_context.rs:65-81` (verified firsthand):

```rust
fn resolve_run_log_location(run_id: &str, diagnostics_path: Option<&str>, home: &str) -> (String, Option<String>) {
    if run_id.is_empty() { return ("(not set)".to_owned(), None); }
    if let Some(path) = diagnostics_path { return (path.to_owned(), file_href_for_path(path)); }
    let full_path = format!("{home}/.jackin/data/diagnostics/runs/{run_id}.jsonl");
    (format!("~/.jackin/data/diagnostics/runs/{run_id}.jsonl"), file_href_for_path(&full_path))
}
```

  `home` here is the **container's** HOME. Rendered rows + "Reveal diagnostics" (host-reveal via `ServerFrame::HostRevealPath`): `crates/jackin-capsule/src/tui/components/dialog/container_info.rs:70-93`.
- Env injection: `launch_runtime.rs::debug_runtime_envs` (verified firsthand) — `JACKIN_RUN_ID` + `JACKIN_RUN_DIAGNOSTICS_PATH` injected **only when `--debug`**, path unconditional on persist; the OTLP block (`launch_runtime.rs:715-737`, verified) separately injects `OTEL_EXPORTER_OTLP_ENDPOINT`/`TRACEPARENT` and `JACKIN_RUN_ID` when not already present.
- Trait: `LaunchDiagnostics` (`crates/jackin-core/src/launch_progress.rs`) exposes `path()`/`command_output_path()` but no persist signal.
- Run-end: `emit_run_summary` (`run.rs:440-474`) writes telemetry only; no operator print anywhere at run end. Startup banner prints run id at `app.rs:229-262` under debug only. The compact-notice channel that is safe under a rich TUI: `emit_operator_notice` (`logging.rs:114-120`) buffers until teardown; `emit_teardown_notice` (`logging.rs:128-130`) direct stderr at final teardown.
- Backend pointer text conventions (research doc, "OTLP Sink Contract"): show `Run ID <id>`, `parallax run <id>` when the endpoint is Parallax-like; backend-neutral otherwise ("Use parallax.run.id=<id> in your OpenTelemetry backend"). Endpoint summary available via `configured_endpoint_summary()` (`observability.rs:349-358`).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt/clippy/check | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` ; `cargo check --all-targets --all-features` | exit 0 |
| Tests | `cargo nextest run --all-features` | pass |

## Scope

**In scope**:

- `crates/jackin-core/src/launch_progress.rs` — add `fn persists(&self) -> bool` to `LaunchDiagnostics`
- `crates/jackin-diagnostics/src/run.rs` — trait impl (method exists; wire it)
- `crates/jackin-launch-tui/src/progress.rs`, `tui/components/failure_dialog.rs`, `crates/jackin-tui/src/components/container_info.rs` — gate rows
- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` — env injection honesty
- `crates/jackin-capsule/src/container_context.rs` + `tui/components/dialog/container_info.rs` — no fabrication
- `crates/jackin/src/app.rs` — run-end pointer line
- Tests in the affected crates; `docs/content/docs/(public)/guides/run-telemetry.mdx` + `docs/content/docs/reference/runtime/diagnostics.mdx` (surface behavior)

**Out of scope**:

- TUI layout/keybinding changes beyond hiding/replacing the rows (read `docs/content/docs/reference/tui/index.mdx` BEFORE touching any dialog row — TUI hard rule; the changes here are row content/presence only, but the read is mandatory).
- `--debug` semantics (008), persist-gate logic itself (unchanged), summary content.

## Git workflow

- Propose branch `fix/diagnostics-path-honesty`; operator confirm; `git commit -s` per step; push each.

## Steps

### Step 1: `persists()` on the port trait

Add `fn persists(&self) -> bool;` to `LaunchDiagnostics`; implement on `RunDiagnostics` (delegate to the existing inherent method `run.rs:225-227`). Update every other `impl LaunchDiagnostics` (grep — expect test fakes in `jackin-launch-tui`; return `true` there unless a test needs otherwise).

**Verify**: `cargo check --all-targets --all-features` → 0.

### Step 2: Gate the host surfaces

1. `progress.rs:57`: pass `diagnostics.persists().then(|| diagnostics.path().display().to_string())` (an `Option<String>`) into `RichDriver::spawn`; adjust the cockpit view model + `container_info.rs:165-171` renderer: when `None`, render `Telemetry` row instead — value `run <run_id> → <endpoint summary>` (use `configured_endpoint_summary()`; plain `run <run_id> (no export, no file — check startup banner)` should be impossible since `persists()==false` implies OTLP active, but handle it with the neutral text).
2. `progress.rs:150`: `failure.diagnostics_path = self.diagnostics.persists().then(|| self.diagnostics.path().to_path_buf());` — `failure_dialog.rs:45` already handles `Option` (it's already `Option<PathBuf>`; verify — if it unwraps, fix the render to skip the row when `None` and show `run <id>` instead).
3. `command_output_path` probe (`progress.rs:151-156`): already existence-checked (`docker_output.exists()`) — leave.

**Verify**: `cargo nextest run -p jackin-launch-tui -p jackin-tui --all-features` → pass (update snapshot/view tests that pinned the old row — search `rg -rn "Diagnostics log" crates/jackin-launch-tui crates/jackin-tui`).

### Step 3: Honest env injection

In `launch_runtime.rs`:

1. `debug_runtime_envs`: inject `JACKIN_RUN_DIAGNOSTICS_PATH` **only when the active run `persists()`**. Keep `JACKIN_DEBUG=1` + `JACKIN_RUN_ID` injection as-is (run id is always valid).
2. Inject `JACKIN_RUN_ID` unconditionally whenever a run is active (not only under debug or OTLP): move the run-id injection out of both conditional blocks into one site (the OTLP block's dedup check then simplifies). Rationale: the run id is the correlation key for bug reports regardless of mode; it is not a path and can't dangle.

**Verify**: unit-test `debug_runtime_envs` (it is a pure-ish fn over the active run — add a test via a persisting and a non-persisting `RunDiagnostics` if constructible; otherwise extract `fn runtime_envs(debug: bool, run: Option<(&str, Option<&Path>)>) -> Vec<String>` pure core and test that). Grep: `rg -n "JACKIN_RUN_DIAGNOSTICS_PATH" crates/jackin-runtime` shows the persist gate.

### Step 4: Capsule stops fabricating

`container_context.rs::resolve_run_log_location`: delete the fabrication branch — when `diagnostics_path` is `None`, return `("(backend only — no local file)".to_owned(), None)` when `run_id` is non-empty, keeping the `run_id.is_empty()` arm as-is. The dialog (`dialog/container_info.rs:70-93`): when `run_log_href` is `None`, drop the "Reveal diagnostics" row entirely and show the run id row with the display text above. TUI docs note: cross-check `docs/content/docs/reference/tui/` for the container-info dialog page and update the row list there in the same PR (RULES.md TUI gate).

**Verify**: new tests in `crates/jackin-capsule/src/container_context/tests.rs` (create; `#[cfg(test)] mod tests;` in container_context.rs): (a) empty run id → "(not set)", no href; (b) explicit path → passthrough + `file://` href; (c) no path + run id → backend-only text, `None` href. `cargo nextest run -p jackin-capsule --all-features` → pass.

### Step 5: Run-end pointer

In `app.rs`, at run teardown (immediately before `ActiveRunGuard` drops / after `emit_run_summary()` at `app.rs:196` — find the exact teardown sequence and place it so it prints on both success and failure paths), emit one line through `jackin_diagnostics::emit_operator_notice`:

- persists: `telemetry: run <id> — file <path>`
- OTLP active: `telemetry: run <id> — query your backend for parallax.run.id=<id>` (append endpoint summary when available)

Not gated on `--debug`. The notice channel already defers under a rich TUI and flushes at teardown (verified `logging.rs:94-120`), so it cannot spew over a live surface.

**Verify**: manual smoke per TESTING.md — `cargo run --bin jackin -- console --debug`, quit; last stderr lines include `telemetry: run <id> …`. Automated: if an e2e harness asserts stderr today (grep `crates/jackin/tests/` for stderr assertions), extend it; else record the manual check in the PR body.

### Step 6: Docs + gate

run-telemetry.mdx: "Where to find your run" section — run id always printed at exit; file path only when a file exists. diagnostics.mdx: surfaces table (banner/cockpit/failure dialog/capsule dialog/exit line × file-mode/OTLP-mode). Update the TUI reference page for the container-info dialog rows.

**Verify**: fmt/clippy/`cargo nextest run --all-features` → exit 0; `cargo xtask docs repo-links` → exit 0.

## Test plan

Named in steps: trait-impl compile coverage, `runtime_envs` pure test (path only when persisting; run id always), `resolve_run_log_location` three-branch test, view tests for the replaced rows. Pattern for capsule dialog tests: existing `crates/jackin-capsule/src/tui/components/dialog/tests.rs` (it pins row content — search `jk-run` fixtures there and update).

## Done criteria

- [ ] `rg -n "diagnostics.path\(\)" crates/jackin-launch-tui/src/progress.rs` → every use behind `persists()`
- [ ] `rg -n "\.jackin/data/diagnostics/runs" crates/jackin-capsule/src/container_context.rs` → no matches
- [ ] `JACKIN_RUN_DIAGNOSTICS_PATH` injected only when persisting (grep + test)
- [ ] Run-end pointer emitted on success and failure paths (code inspection + manual smoke noted in PR)
- [ ] Capsule dialog shows no Reveal row when no host path was injected (test)
- [ ] fmt/clippy/nextest green; TUI docs page + run-telemetry.mdx updated; `plans/README.md` updated

## STOP conditions

- `RichDriver::spawn`'s signature change fans out beyond `progress.rs` + its view/renderer modules (grep callers first).
- The failure dialog's `diagnostics_path` is load-bearing `PathBuf` (non-Option) in more types than `LaunchFailure` — report the type web instead of forcing it.
- Any place *reads* the synthesized capsule path for logic (not display) — fabrication removal would break it; report.
- Run-end notice prints mid-TUI in the smoke test (deferral not working on some path) — report; do not ship a spewing notice.

## Maintenance notes

- Plan 010's config surface and plan 008's banner line both touch `app.rs` startup output — small merge coordination.
- Reviewer: check both modes manually (OTLP env set vs unset) — this plan's whole point is the two modes telling one consistent story.
- Deferred: `jackin diagnostics` CLI ergonomics for backend-mode runs (e.g. `jackin diagnostics <run-id>` hinting `parallax run <id>` when no file) — separate UX decision.

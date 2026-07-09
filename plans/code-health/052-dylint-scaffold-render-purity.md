# Plan 052: dylint scaffold — `crates/jackin-lints` with the render-thread-purity lint, advisory nightly lane

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-lints .github/workflows/hygiene.yml Cargo.toml`
> `crates/jackin-lints` must not exist yet (`ls crates/ | grep lints` → empty);
> if it does, STOP — someone started this. hygiene.yml appends from
> 022/035/046/048 are expected; append beside.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED (dylint compiles against nightly-internal rustc APIs — isolation + advisory posture contain it; the lane can break on nightly churn without ever blocking a PR)
- **Depends on**: none
- **Category**: dx / tech-debt
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

The roadmap's "Custom project lints via dylint" section: "When a jackin❯-specific invariant cannot be expressed by any shipped lint, encode it as a workspace-owned dylint library run from CI, not as a prose rule an agent will drift from," naming render-thread purity as the first candidate and prescribing the delivery shape verbatim: "Author the lint library under `crates/jackin-lints/`, gate it advisory first (warn in a scheduled lane), and promote to PR-blocking once the false-positive rate is measured. dylint lints compile against a nightly-internal rustc API, so isolate them in their own crate and run them in the scheduled lane or a pinned-toolchain CI job." The render-thread rule today is prose (capsule AGENTS: "No blocking on the render/control path") plus a method-list lint (`clippy::disallowed_methods` blocks `Command::output`, `std::thread::sleep`, `File::open`, `OpenOptions::open`) — the list can't see one call deep. A dylint lint walks the call graph from render entry points and catches what the list structurally cannot. The old index deferred this pending "a second concrete lint candidate"; a dylint crate ships fine with one lint, and the roadmap already names three follow-up candidates.

## Current state

Verified at `fabe88406`.

- `crates/jackin-lints/` does not exist. The roadmap reserves the name.
- The invariant's current enforcement: workspace `clippy::disallowed_methods` (crates/AGENTS.md lint section — blocks the four blocking calls, with `#[expect]` carve-outs for non-render contexts) + capsule AGENTS prose. Gap: a render-path function calling a helper that calls `std::fs::read` passes today.
- Render entry points (the lint's roots): implementations of `jackin_tui::runtime::View::render` — the trait at `crates/jackin-tui/src/runtime.rs:266-268` (`fn render(&self, model: &Model, frame: &mut ratatui::Frame<'_>, area: Rect)`), implemented at `crates/jackin-console/src/tui/runtime.rs:23`, `crates/jackin-capsule/src/tui/runtime.rs:16`, `crates/jackin-launch-tui/src/tui/model.rs:113` — plus the capsule compositor's frame path (`crates/jackin-capsule/src/daemon/compositor.rs`, e.g. `compose_pending_frame`/`compose_ratatui_frame` around :75-103).
- Denied-on-render-path call set (start narrow, grow later): `std::fs::*` open/read/write fns, `std::net`, `std::thread::sleep`, `std::process::Command` spawn/output/status, `std::sync::Mutex::lock`/`RwLock::{read,write}` (tokio primitives excluded this wave — `std::sync` only, matching the roadmap's wording "no blocking I/O … or `std::sync` locks reachable from the … draw path").
- Toolchain: the workspace pins stable via `rust-toolchain.toml`; dylint needs its own pinned nightly → `crates/jackin-lints/rust-toolchain.toml` with a fixed `nightly-YYYY-MM-DD` and the crate EXCLUDED from the workspace (`[workspace] exclude` in root Cargo.toml, or simply not a member + its own `[workspace]` table) so `cargo check --workspace` and every PR lane never compile it.
- Lane shape: `.github/workflows/hygiene.yml` job pattern (:49-92) — mise-action for tools, own rust-cache key. dylint runs via `cargo dylint` (driver auto-managed); install `cargo:cargo-dylint` + `cargo:dylint-link` through mise `install_args` (both are cargo crates — the mise cargo-backend rule in `.github/AGENTS.md` applies).
- Upstream references (roadmap "Research and prior art"): dylint repo <https://github.com/trailofbits/dylint> — `cargo dylint new` scaffolds a library with `ui/` tests; lints use `clippy_utils` for path matching. Re-verify the current template API before coding (the roadmap's own instruction for prior-art links).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Scaffold (inside crates/) | `cargo dylint new jackin-lints` (then adjust per Scope) | template compiles |
| Build the lint lib | `cd crates/jackin-lints && cargo build` | exit 0 (its own nightly) |
| UI tests | `cd crates/jackin-lints && cargo test` | ui tests pass |
| Run against workspace | `cargo dylint --all -- --workspace` (from root, with DYLINT_LIBRARY_PATH or workspace metadata) | findings listed, exit per mode |
| Main workspace unaffected | `cargo check --workspace --all-targets --locked` | exit 0 (never compiles jackin-lints) |

## Scope

**In scope**:
- `crates/jackin-lints/` (new, workspace-EXCLUDED): lint library `render_thread_purity`, its `ui/` tests, own `rust-toolchain.toml` (pinned nightly), own README.md + AGENTS.md + CLAUDE.md symlink (the `cargo xtask lint agents` gate scans `crates/*/` — check whether it walks non-workspace dirs; if it does, the three files are mandatory; add them regardless, the convention is per-directory)
- Root `Cargo.toml` workspace `exclude` entry (if needed for the layout chosen)
- `.github/workflows/hygiene.yml` — one `dylint-advisory` job (pinned nightly, `continue-on-error: true`)
- `[workspace.metadata.dylint]` table in root Cargo.toml pointing at the library (so `cargo dylint --all` finds it)
- TESTING.md row; roadmap "Custom project lints" status note

**Out of scope** (do NOT touch):
- The other three roadmap lint candidates (foundational-crate Debug/sealed, config-field consistency, telemetry discipline) — recorded follow-ups.
- PR-blocking promotion (needs measured false-positive rate — maintenance note).
- Any fix to code the lint flags (findings are the lane's output; fixing them is separate work — list findings in the PR body).
- `clippy.toml` / the disallowed-methods list (they stay; the dylint lint is the structural complement).

## Git workflow

- Branch off `main`: `feature/jackin-lints-scaffold`.
- Conventional Commits (`feat(lints): …`, `ci: …`), `-s`, push per commit. PR to `main`; do not merge. Schedule-gated job → dispatch-smoke `gh workflow run Hygiene --ref <branch>`, URL in PR body.

## Steps

### Step 1: Scaffold, isolated

Install locally: `cargo install cargo-dylint dylint-link` (or mise equivalents). `cargo dylint new` the library into `crates/jackin-lints`; pin its `rust-toolchain.toml` to the nightly the current dylint release documents; ensure root workspace excludes it; add the `[workspace.metadata.dylint]` libraries entry. Add README/AGENTS/CLAUDE per crate convention (AGENTS content: the nightly-pin rule + "never make this a workspace member").

**Verify**: `cd crates/jackin-lints && cargo build` → exit 0; `cargo check --workspace --all-targets --locked` from root → exit 0 AND does not compile jackin-lints (confirm no `jackin-lints` line in its output); `cargo xtask lint agents` → pass.

### Step 2: The render-thread-purity lint

Implement `render_thread_purity` as a `LateLintPass`: identify render roots — fns that are (a) impls of a trait method named `render` on a trait named `View` in a `jackin_tui`-pathed module, or (b) an attribute opt-in `#[jackin_lints::render_path]`-style marker if trait resolution across crate boundaries proves unreliable in dylint's model (decide by testing (a) first; fall back to (b) + seed the three impl sites and the compositor fns with the marker — report which). From each root, walk the local call graph (callee HIR within the same crate; cross-crate calls check the path against the deny list directly) and emit a warning naming the full call chain when a denied path (`std::fs::*`, `std::net::*`, `std::thread::sleep`, `std::process::Command::{output,status,spawn}`, `std::sync::{Mutex::lock, RwLock::read, RwLock::write}`) is reachable. Depth-limit the walk (e.g. 5) to bound false paths; dedupe reports per root+sink.

`ui/` tests: a fake render fn calling `std::fs::read` transitively (flagged, chain shown), the same call behind `spawn_blocking`-style indirection (not flagged — the deny walk must not cross closure-spawn boundaries: skip callees inside `tokio::spawn`/`spawn_blocking` args), a clean render fn (silent).

**Verify**: `cd crates/jackin-lints && cargo test` → ui tests pass.

### Step 3: Run against the real workspace, record findings

`cargo dylint --all -- --workspace` from the root (nightly driver builds the workspace under the lint). Record every finding in the PR body with a triage guess (real / false-positive / expected-carve-out). Zero findings is a valid outcome (the disallowed-methods list may have kept the paths clean); the lane's value is regression coverage.

**Verify**: the command completes and its findings (or "0 findings") are recorded; main workspace `cargo check` still green.

### Step 4: Advisory hygiene lane

Append a `dylint-advisory` job to hygiene.yml: checkout, mise-action (`install_args: "cargo-binstall rust cargo:cargo-dylint cargo:dylint-link"`), rustup install of the crate's pinned nightly (`rustup toolchain install $(cat crates/jackin-lints/rust-toolchain.toml | grep channel | cut -d'"' -f2)` — or simpler, let rustup auto-install via the directory override when dylint builds), own rust-cache key, run `cargo dylint --all -- --workspace`, `continue-on-error: true`, findings to `$GITHUB_STEP_SUMMARY`.

**Verify**: `gh workflow run Hygiene --ref <branch>` → job runs; summary shows findings or clean; URL in PR body.

### Step 5: Docs

TESTING.md row (what the lane checks, how to run locally, the nightly-pin caveat). Roadmap "Custom project lints" section → scaffold + first lint shipped, three candidates remaining, promotion criteria (measured FP rate) restated.

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- dylint `ui/` tests are the lint's spec (3 cases minimum per Step 2).
- The workspace run (Step 3) is the integration test; its findings list is a deliverable.
- Main-workspace isolation proof: `cargo check --workspace` never touches the lints crate.

## Done criteria

- [ ] `crates/jackin-lints` exists, workspace-excluded, own pinned nightly, README/AGENTS/CLAUDE present
- [ ] `render_thread_purity` implemented; ui tests green; call-chain in the diagnostic
- [ ] Workspace run recorded (findings or clean) in PR body
- [ ] `dylint-advisory` hygiene job green on dispatch (URL in PR body); `continue-on-error: true`
- [ ] Main workspace builds/gates unaffected; `cargo xtask ci --fast` → `ci gate OK`
- [ ] TESTING.md + roadmap updated; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- `cargo dylint new`'s current template fails to build on its documented nightly (upstream churn) — report versions tried; do not hand-roll a rustc-driver harness.
- Cross-crate trait-impl detection AND the marker fallback both prove unworkable in dylint's late pass — report what IS resolvable; a lint that can't find its roots is not worth shipping half-blind.
- The workspace-under-nightly-driver build fails on stable-only code paths (some crates may not compile under the nightly driver) — report the crate + error; scoping the run to the TUI crates (`-p` flags) is an acceptable fallback, say so.
- Anything requires making jackin-lints a workspace member.

## Maintenance notes

- Follow-up lint candidates (each its own small plan once this scaffold exists): foundational-crate `Debug`-on-pub + sealed-extension traits; config/manifest field-consistency; telemetry discipline (post-041: flag `tracing::`/`log::` calls outside the facade/renderers).
- Nightly churn is the tax: when the lane reds on a rustc API break, the fix is bumping the pinned nightly + mechanical API chasing — a chore, never a PR blocker (advisory posture).
- Promotion path: 2-4 weeks of advisory runs → measured FP rate in the README index → PR-blocking via a pinned-toolchain job if FP ≈ 0.

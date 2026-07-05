# Plan 011: Extract agent-status detection into an independent `jackin-agent-status` crate

> **Executor instructions**: Behavior-preserving crate extraction. Verify the workspace builds and all tests
> pass after the move — nothing should change except where the code lives. Do plan 008 (the `EvidenceSnapshot`
> / `ProcessSampler` seam) first; it makes this extraction clean. Run every verification command. Update the
> README row when done.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/agent_status crates/jackin-capsule/src/session.rs Cargo.toml`

## Status

- **Priority**: P2 (structural — the home for 001/005/006/007/010; do early)
- **Implementation status**: DONE in PR 714 (`crates/jackin-agent-status/` now owns the pure detection core; capsule keeps reporter installation)
- **Effort**: L
- **Risk**: MED (wide mechanical move; low logic risk — the boundary is already clean)
- **Depends on**: 008 (the injectable seam)
- **Category**: tech-debt / architecture
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

**Verified**: there is **no** independent agent-status crate today — all detection lives inside the 50K-line
`jackin-capsule` crate at `crates/jackin-capsule/src/agent_status/`, and the workspace `Cargo.toml` has no
`jackin-agent-status` member. Detection (a self-contained, security-relevant concern) is entangled with the
container multiplexer. Isolating it into its own crate is correct for several independent reasons:
- **Testability** — the detection core is already **sync and pure** (no tokio/async, no container), so as a
  crate it is unit-testable on any host without the capsule/Docker (this *is* plan 008's goal, delivered at
  the crate boundary).
- **Reuse** — host-side consumers (console, daemon, Desktop Agent Hub) can depend on the same detection crate
  instead of re-deriving state; today they can't reach into capsule internals.
- **Clean home for pack provenance** — plan 010's signed remote-pack channel and plan 005's pack loading want
  one crate that owns pack loading/verification, not logic woven through `Session`.
- **Enforced layering** — the compiler stops detection from reaching into capsule internals (today
  `advance_status` is a `Session` method).
- **Shrinks the capsule monolith** — removes ~5K lines from the workspace's largest crate.

The boundary is already clean (see below), so this is a mechanical move, not a redesign.

## Current state — the boundary is already clean (verified)

- Location: `crates/jackin-capsule/src/agent_status.rs` + `agent_status/{arbitrate,policy,gating,evidence,rules,process,hook_installer}.rs` + `agent_status/screen/fixtures/**` + `agent_status/screen/transcripts/**`.
- **The detection core imports nothing from capsule internals** (verified — no `jackin_term`, no
  `crate::session`, no `crate::tui`/`crate::daemon`). Its only imports:
  - `crate::protocol::AgentState` (a re-export of `jackin_protocol::control::AgentState`, defined at
    `crates/jackin-protocol/src/control.rs:468`),
  - `jackin_protocol::agent_status::{AgentRawState, AgentStatusConfidence, AgentStatusReport, …}`,
  - `jackin_core::agent::Agent` (`process.rs`),
  - `regex`, `semver`, `toml`, `serde`, `anyhow` (utility crates).
- **The engine already operates on plain data.** `Session::advance_status` (`session.rs:969-1042`) is the seam:
  it converts the screen to `Vec<String>` (`visible_screen_rows()`, `session.rs:893,988`), calls
  `registry.evaluate_with_virtuals(agent, &rows, virtuals)` (`session.rs:994`), builds a plain
  `EvidenceSnapshot { … }` (`session.rs:1002`), and calls `apply_watchdog(arbitrate(&snapshot, raw, now), now)`
  (`session.rs:1013`). The detection functions never see a `Session` or a `jackin-term` type.
- Packs: `docker/runtime/agent-status/packs/*.toml`, `include_str!`'d by `rules.rs:363-375`.
- Conventions to honor (`crates/AGENTS.md`): self-named module files (no `mod.rs`), tests in sibling
  `tests.rs`, `[lints] workspace = true`, no per-crate edition/lint copies.

## Recommended crate shape (the "how")

**New crate `crates/jackin-agent-status/`** — pure, sync detection library.

- **Depends on:** `jackin-protocol` (wire types incl. `AgentState`), `jackin-core` (`Agent`), `regex`,
  `semver`, `toml`, `serde`, `anyhow`. **No** `tokio`, `jackin-term`, or `jackin-capsule`.
- **Owns (move from capsule):**
  - `evidence.rs` (the `EvidenceSnapshot`, `ScreenEvidence`, `ActivityEvidence`, `AuthorityEvidence`,
    `EvidenceNote`, `RawAgentState` re-export, notes/summary types) — the crate's public input/output vocabulary.
  - `rules.rs` (the `RulePack`/`RulePackRegistry`/gate grammar/pack loading engine) + the packs (move
    `docker/runtime/agent-status/packs/*.toml` into `crates/jackin-agent-status/packs/`; update `include_str!`
    paths; keep the image-bake list in `jackin-image` pointing at the new location) + the fixtures/transcripts.
  - `arbitrate.rs`, `policy.rs` (debounce/watchdog + constants), `gating.rs` (event table).
  - `process.rs`'s **pure** parts: `identify_agent` + the `ProcessSampler` trait + a `LinuxProcSampler` impl
    (it's `std::fs` `/proc` reads — sync, no capsule dep; it can live in the crate behind a `cfg(linux)` guard
    with the in-memory test double from plan 008).
- **Public API (the seam the capsule calls):** `EvidenceSnapshot` (built by the caller), `RulePackRegistry`
  (with `evaluate_with_virtuals`), `arbitrate(&snapshot, prev_raw, now) -> …`, `apply_watchdog(…)`,
  `debounce`/policy, and the `ProcessSampler` trait. The crate exposes *pure functions over plain data*.

**Stays in `jackin-capsule`:**
- `Session::advance_status` — it assembles the `EvidenceSnapshot` from the capsule's own `jackin-term` screen
  (`visible_screen_rows`), its `ProcessSampler` (injected), its `osc`/`authority` fields, then calls the crate.
  The capsule depends on `jackin-agent-status`.
- The daemon tick, the tui render (plan 001), the jackin-term screen→rows conversion.
- **`hook_installer.rs` — decision:** it writes reporter/plugin files into the *container* agent home (a
  provisioning concern, not detection). Recommend it **stays in capsule** (or moves to `runtime_setup`), so the
  detection crate stays free of filesystem-provisioning and container coupling. If the maintainer prefers a
  single "agent-status" home, it can move too — flag the decision, don't silently bundle it.

## Steps

Do this as one behavior-preserving move; the tree must build and pass tests at the end (ideally at each sub-step).

1. **Scaffold the crate.** Create `crates/jackin-agent-status/` with `Cargo.toml` (`[lints] workspace = true`,
   no edition/lint copies), add it to the workspace `members` in root `Cargo.toml`, and declare the deps above.
2. **Move the pure modules** (`evidence`, `rules`, `arbitrate`, `policy`, `gating`, pure `process`) into the
   crate's `src/`, following self-named-module layout. Replace `crate::protocol::AgentState` with
   `jackin_protocol::control::AgentState`; replace `crate::agent_status::…` internal paths with `crate::…`.
   Move the packs + fixtures + transcripts into the crate; fix `include_str!` paths.
3. **Wire the seam.** In `jackin-capsule`, add the `jackin-agent-status` dependency; have `Session::advance_status`
   call the crate's `RulePackRegistry`/`arbitrate`/`apply_watchdog` (it already does, just across the crate
   boundary now). Inject the crate's `ProcessSampler` (plan 008's trait) rather than calling `/proc` inline.
4. **Keep the image-bake correct.** Update `crates/jackin-image/src/derived_image.rs` `AGENT_STATUS_ASSETS` and
   any `docker/runtime/agent-status/packs` references to the new pack location so the derived image still ships
   the packs (they must still land at `/jackin/runtime/agent-status/packs` in-container).
5. **Move the tests with their modules** (sibling `tests.rs`, no `mod.rs`); the pure detection tests now run in
   the crate on any host.

**Verify (after the move)**:
- `cargo check --workspace --all-targets --all-features` → exit 0
- `cargo nextest run -p jackin-agent-status -p jackin-capsule` → all pass (behavior preserved)
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0 (the CI gate)
- `cargo shear` (via mise) → no unused/misplaced deps (the CI dead-code-deps gate)
- `grep -rn "agent_status" crates/jackin-capsule/src` → only the `advance_status` seam + hook_installer remain
  (detection logic is gone from capsule)

## Done criteria

- [x] `crates/jackin-agent-status/` exists, is a workspace member, depends only on protocol/core + utility crates (no tokio/term/capsule)
- [x] All detection logic (evidence/rules/arbitrate/policy/gating/pure-process + packs + fixtures) lives in the crate
- [x] `jackin-capsule` depends on the crate; `Session::advance_status` calls it across the boundary; behavior unchanged
- [x] The derived image still ships the packs to `/jackin/runtime/agent-status/packs`
- [x] Pure detection tests run in the crate on the dev host (not container-gated)
- [x] `cargo nextest run --workspace` green; clippy + `cargo shear` clean
- [x] `PROJECT_STRUCTURE.md` + codebase-map doc updated for the new crate (structural-change docs gate)
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- The move surfaces a hidden dependency on a capsule type not visible in the import scan (e.g. an
  `EvidenceSnapshot` field typed as a `jackin-term`/capsule type) — that field must be lowered to a plain type
  (or the type moved) before the crate can compile; report the specific field before widening scope.
- `hook_installer` turns out to be imported by the pure modules (it shouldn't be) — keep it in capsule and
  report the coupling.
- Plan 008's seam isn't in place — the extraction still works but the `/proc` sampler won't be injectable; do
  008 first so the crate's `ProcessSampler` trait is the clean boundary.

## Maintenance notes

- This is the structural home the whole subsystem wanted: after it, plans 001 (render maps from the crate's
  `AgentState`), 005/007 (packs live in the crate), 010 (remote packs load through the crate), and 009
  (reporters feed the crate's gating) all have one clear owner.
- A reviewer should confirm the crate has **zero** capsule/tokio/jackin-term deps — that purity is the whole
  point and the guarantee that detection is independently testable and reusable.
- Sequence: do 008 → 011 early, then land 003/004/005/007 inside the crate rather than moving them twice. 001
  (capsule tui) and 002 (identification — moves into the crate) can proceed in parallel.

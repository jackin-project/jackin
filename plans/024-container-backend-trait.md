# Plan 024: Extract a `ContainerBackend` trait and finish the apple-container lifecycle dispatch

> **Executor instructions**: Architecture plan bridging a tech-debt fix and a direction bet. Do the trait
> extraction (mechanical, testable) as the deliverable; the Phase-0 hardware validation is flagged as a
> dependency you do NOT attempt without macOS 26 ARM hardware. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-runtime/src/runtime/apple_container.rs crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs crates/jackin-runtime/src/runtime/cleanup.rs crates/jackin-runtime/src/apple_container_client.rs`

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED (touches the central launch pipeline)
- **Depends on**: none
- **Category**: tech-debt / direction (DEBT-03 + DIRECTION-01)
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

jackin has **two** container backends — Docker (bollard) and Apple Container (`container` CLI) — with **no
unifying trait**. Dispatch exists only in the launch path (`launch_core.rs:907` `match backend`), while
`cleanup.rs`/`load_cleanup.rs` (eject/exile/purge) and the hardline/reconnect paths are **Docker-only** —
they have no `Backend` arm. The Apple Container variant is also knowingly incomplete: finalization is "not
yet wired" (`apple_container.rs:176`) and DinD-in-VM is disabled pending Phase 0. This is the
symmetric-variant drift class `ENGINEERING.md` warns about, but at module scale: any launch/attach/finalize
change must be mirrored across two backends by hand, and one side is silently incomplete. It's also the
enabler the roadmap keeps deferring to ("non-Docker backend parity" recurs across egress/limits/cache
items). A shared trait makes "one side is incomplete" a **compile error** instead of a silent gap.

## Current state

- `crates/jackin-runtime/src/apple_container_client.rs` — full `AppleContainerApi` trait +
  `AppleContainerClient` + `FakeAppleContainerClient`; `BACKEND_NAME = "apple-container"`.
- `crates/jackin-runtime/src/runtime/apple_container.rs` — parallel free functions `attach`/`launch`/
  `reconnect`/`eject`/`purge` mirroring the Docker path; `:176` "apple-container finalization is not yet
  wired"; `inner_docker_enabled` defaults `false` (Phase 0).
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs:905-912` — dispatch is a bare
  `match backend { Backend::Docker => {}, Backend::AppleContainer => { … return apple_container::launch(…) } }`
  (exhaustive — good — but the *operations* are not trait-unified).
- `enum Backend { Docker, AppleContainer }` at `launch/mounts.rs:177`; `resolve_backend` reads
  `ws.runtime.backend` then `config.runtime.default_backend`.
- **Cleanup/hardline are Docker-only**: `grep -rn "Backend::" crates/jackin-runtime/src/runtime/cleanup.rs crates/jackin-runtime/src/runtime/host_attach.rs`
  → confirm no backend arm exists there.
- Roadmap: `docs/content/docs/reference/roadmap/apple-container-backend/` (Partially implemented, "Phase 0").

## Scope

**In scope:** a new `ContainerBackend` trait (in `jackin-runtime`), impls for Docker + Apple Container,
and routing the eject/exile/purge/hardline/reconnect paths through it. **Out of scope:** the Phase-0
hardware validation (mounts/DinD-in-VM on real macOS 26 ARM); completing apple-container finalization logic
*correctness* (the trait makes its absence a compile error — wiring the real finalize behavior is a
follow-up needing hardware).

## Steps

### Step 1: Define the `ContainerBackend` trait

Extract the five lifecycle ops both backends already implement as free functions into a trait:
`launch`, `attach`, `reconnect`, `purge`/`eject`, and `finalize`. Make `finalize` a **required** method so
"not yet wired" surfaces as an unimplemented trait method (a compile error / explicit `todo!`-free
`unimplemented` guarded return) rather than a silent skip. Keep signatures compatible with the existing
free functions.

### Step 2: Implement the trait for both backends

Provide `DockerBackend` and `AppleContainerBackend` impls that delegate to the existing logic. For the
apple-container `finalize`, if the real behavior can't be completed without hardware, have it return a clear
`Err("apple-container finalize not yet implemented — Phase 0")` **explicitly** (so it's visible and typed),
not a silent no-op.

### Step 3: Route ALL lifecycle paths through the trait

Replace the bare `match backend` sites — including the cleanup/eject/exile/purge and hardline/reconnect
paths that are currently Docker-only — with dispatch through the trait object/enum. After this, an
apple-container instance is fully *manageable* through the abstraction (today it launches but can't be
cleanly torn down through the same path).

**Verify**: `cargo check -p jackin-runtime --all-targets` → exit 0;
`cargo nextest run -p jackin-runtime` → all pass (use `FakeAppleContainerClient` for backend-agnostic tests).

### Step 4: Record the decision as an ADR

Write an ADR under `docs/content/docs/reference/adrs/` (next number after adr-005) capturing: two backends,
the `ContainerBackend` trait, Docker-CLI-vs-bollard choice, and the Phase-0 open questions. (Ties to plan
037 ADR consolidation.)

## Done criteria

- [ ] `ContainerBackend` trait exists with `finalize` **required**
- [ ] Both backends impl it; eject/purge/hardline/reconnect all dispatch through it (no Docker-only lifecycle path remains)
- [ ] apple-container `finalize` returns an explicit typed error, not a silent no-op
- [ ] `cargo nextest run -p jackin-runtime` green with backend-agnostic tests using the fake client
- [ ] ADR written; roadmap `apple-container-backend` Status/Related-Files updated (docs gate)
- [ ] `plans/README.md` row updated

## STOP conditions

- Unifying the ops reveals the two backends have genuinely divergent signatures that can't share a trait
  without a lowest-common-denominator that loses Docker capability — report; a trait with associated types
  may be needed and that's a design decision for the operator.
- Routing cleanup through the trait uncovers apple-container teardown logic that doesn't exist at all (not
  just "not wired") — report; that's real missing functionality, not a refactor.

## Maintenance notes

- The whole point: after this, adding a lifecycle op forces both backends to implement it (compile error
  otherwise). A reviewer should confirm no `match backend` with an empty/defaulted arm survives.
- Phase-0 hardware validation and real apple-container `finalize` are explicit follow-ups — record them in
  the roadmap item, not this plan.

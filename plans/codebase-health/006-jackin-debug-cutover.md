# Plan 006: `JACKIN_DEBUG` cutover — one shared reader + dated removal boundary

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-usage/src/logging.rs crates/jackin-diagnostics/src/logging.rs crates/jackin-diagnostics/src/observability.rs crates/jackin-runtime/src/runtime/launch/launch_runtime.rs crates/jackin-runtime/src/runtime/apple_container.rs DEPRECATED.md`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (coordinate with plan 004 if concurrent — both touch `jackin-usage/src/logging.rs`)
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Telemetry item 3: remove `JACKIN_DEBUG` as a telemetry control and stop injecting it into containers after the image-skew transition, "or define a dated compatibility boundary with a test that prevents permanent retention. Use one shared telemetry-resolution reader apart from the documented transition injection." Today the alias resolution is documented as centralized in `jackin_diagnostics::telemetry_level`, but two additional direct readers parse `JACKIN_DEBUG` themselves (they can drift on accepted truthy values and precedence), and the dual container injection has a prose condition ("after the capsule image floor moves past this release") with no date, no tracked deprecation entry, and no failing test — so it can be retained forever with green CI, which the contract forbids.

## Current state

- Canonical reader: `crates/jackin-diagnostics/src/logging.rs:46-75` (`telemetry_level`, alias handling; module doc says "only read here").
- Bypass reader 1: `crates/jackin-usage/src/logging.rs:95-102`:

```rust
let debug = std::env::var("JACKIN_DEBUG").is_ok_and(|v| {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}) || telemetry_level.as_deref().is_some_and(|level| matches!(level, "debug" | "trace"));
```

- Bypass reader 2: `crates/jackin-diagnostics/src/observability.rs:964-971` (`capsule_debug`).
- Injection sites (the documented transition — keep, but bound): `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs:1210` and `crates/jackin-runtime/src/runtime/apple_container.rs:273`; tests asserting presence at `launch/tests.rs:1915,1935,6319`.
- `DEPRECATED.md:30-37` — "JACKIN_DEBUG as telemetry control (alias only)" note sits OUTSIDE the tracked active-deprecations table (line 22 says "Active deprecations: _None._") with the undated removal condition.
- Capsule image version floor source: `jackin-build-meta` crate stamps versions (`JACKIN_CAPSULE_VERSION` per CONTRIBUTING.md); find the exact accessor with `grep -rn "CAPSULE_VERSION" crates/jackin-build-meta/src crates/jackin-runtime/src | head`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Usage + diagnostics | `cargo nextest run -p jackin-usage -p jackin-diagnostics --all-features` | pass |
| Runtime | `cargo nextest run -p jackin-runtime` | pass |
| Lint | `cargo clippy -p jackin-usage -p jackin-diagnostics -p jackin-runtime --all-targets --all-features -- -D warnings` | exit 0 |

## Scope

**In scope**: the two bypass readers; `DEPRECATED.md`; one new boundary test (in `jackin-runtime` launch tests or `jackin-build-meta` — wherever the version floor is readable); the injection-site comments.

**Out of scope**: removing the dual injection itself (that happens when the boundary trips); capsule macro restructure (004); operator docs for `JACKIN_TELEMETRY_LEVEL` (already preferred per DEPRECATED.md).

## Git workflow

Branch `chore/jackin-debug-cutover`; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Route bypass readers through the shared resolver

Replace both direct `JACKIN_DEBUG` parses with calls into `jackin_diagnostics::telemetry_level`/`sink_level`/`is_debug_mode` (choose the accessor that preserves current behavior — read `logging.rs:46-75` semantics first; usage crate already depends on nothing from diagnostics? CHECK: `grep jackin-diagnostics crates/jackin-usage/Cargo.toml`. If jackin-usage cannot depend on jackin-diagnostics due to tier direction (diagnostics may sit above usage), invert: move the single parser into the lower crate (`jackin-usage` or `jackin-core`) and have diagnostics re-export/consume it — the contract cares that ONE parser exists, not where it lives; `cargo xtask lint arch --strict` decides placement.)

**Verify**: `cargo nextest run -p jackin-usage -p jackin-diagnostics --all-features` → pass; `grep -rn 'env::var("JACKIN_DEBUG")' crates --include='*.rs' | grep -v tests` → exactly one non-injection match (the shared resolver) plus the two documented injection sites.

### Step 2: Dated boundary test

Add a test that reads the capsule image floor version and FAILS (with a message naming this plan and `DEPRECATED.md`) once the floor passes the current release — i.e. assert `capsule_image_floor <= <current pinned version>`; when a future bump violates the assertion, the failure message instructs: "remove the JACKIN_DEBUG dual-inject at launch_runtime.rs:1210 / apple_container.rs:273 and delete this test." If no machine-readable floor exists, use a dated boundary instead: the test fails after `2026-10-01` (compare against a build-time date from `jackin-build-meta`, never wall-clock in test logic if the repo forbids it — check how existing dated tests work; if none exist, the version-floor form is required).

**Verify**: test passes today; temporarily flip the constant locally to prove it fails with the instructive message (do not commit the flip).

### Step 3: Track the deprecation

Move the `JACKIN_DEBUG` note into `DEPRECATED.md`'s active-deprecations table with a concrete "Remove when: capsule image floor > <version> (guarded by test `<test name>`)".

**Verify**: `cargo xtask ci --fast` → exit 0 (docs gates included).

## Test plan

Boundary test (step 2); resolver-equivalence tests: same truthy values (`1|true|yes|on`) and `JACKIN_TELEMETRY_LEVEL` precedence produce identical levels through the shared path (port the value matrix from the deleted parser into the shared resolver's tests if not already covered).

## Done criteria

- [x] One non-injection `JACKIN_DEBUG` reader in the workspace (grep-proven)
- [x] Boundary test exists, passes now, and demonstrably fails post-boundary with removal instructions
- [x] `DEPRECATED.md` active table tracks the entry with the removal trigger
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Arch tier gate blocks every reasonable placement of the single parser — report the tier conflict and the options.
- The resolver semantics differ between the three readers in a way users could observe (e.g. one treats bare `JACKIN_DEBUG=` empty as on) — enumerate the difference; behavior choice is the operator's.
- No version floor or build-date source exists — report; do not invent wall-clock reads (see roadmap deterministic-time item).

## Maintenance notes

- When the boundary trips, removal is: delete dual-inject lines + their presence tests + boundary test + DEPRECATED.md row; `JACKIN_TELEMETRY_LEVEL` remains the sole control.
- Plan 009's conformance capture should run with `JACKIN_TELEMETRY_LEVEL` only, proving the alias is not needed on the export path.

## Execution notes

- Boundary uses package version floor `0.6.0-dev` (not wall-clock).

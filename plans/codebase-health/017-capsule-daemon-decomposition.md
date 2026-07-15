# Plan 017: Capsule daemon decomposition — owned subsystems + injectable boundary ports

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-capsule/src/daemon.rs crates/jackin-capsule/src/daemon/ crates/jackin-capsule/src/session.rs`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: L (multi-PR program; subsystem-at-a-time slices)
- **Risk**: MED (live attach/render loop)
- **Depends on**: none
- **Category**: tech-debt (ownership) + tests (control-plane determinism)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Ownership item 2: "Reduce the daemon shell to event dispatch and owned subsystems for client registry, session supervision, active-client policy, status, clipboard, git/PR watch, usage, and control routing." Today `struct Multiplexer` holds 100+ inline fields spanning all of those; the daemon submodules are behavior files operating on the one god-struct, so every subsystem edit touches it. Roadmap Characterization item 5 adds the testing half: ports must sit "at the real daemon/session-supervisor boundary, with fake-port tests proving observable behavior rather than only helper predicates" — the current port traits take pre-computed booleans and can't drive attach/displace/reattach against faked edges. The characterization precondition is satisfied (daemon suite: 321 tests / 7843 lines), so decomposition is unblocked. Splitting `daemon/tests.rs` happens per-subsystem WITH each extraction (mega-test rule: production responsibility first).

## Current state

- God-struct: `crates/jackin-capsule/src/daemon.rs:174-339` — `Multiplexer` fields include `sessions: HashMap<u64, Session>` (:175), `status_bar` (:180), clipboard state (:243-246), PR/git watch state (`pull_request_context`/`git_branch_lookup`/`pull_request_lookup`/`pull_request_context_cache`, :307-324), `provider_keys` (:335), `UsageCache` (import :137).
- Behavior modules on the struct: `daemon/input_dispatch.rs` (1115 lines), `daemon/mouse_input.rs` (899), `daemon/compositor.rs` (780), `daemon/session_lifecycle.rs` (491).
- Predicate-only ports: `daemon/ports.rs:8-30` — `ControlPort`/`AttachPort`/`StatusPort`/`PersistencePort` take primitives (`session_known: bool`, …) and return `bool`; sole impl `DefaultDaemonPorts` (:34-64); no fakes exist.
- `session.rs` — 1689 lines, and (per module-contract audit) lacks a leading `//!` contract (plan 022 sweeps contracts; add one here for whatever this plan touches).
- Preserve-behavior contract from the roadmap: range-snapshot equivalence across scrollback/live boundary, clamp/empty behavior, selection behavior, hover/click narrow-range use; borrowed row accessor only if the selection-copy benchmark demonstrates value.
- Characterization suites: `daemon/tests.rs` (7843 lines, 321 tests), `session/tests.rs` (1476).
- No sim/failpoint deps exist (`turmoil`/`madsim`/`proptest-state-machine`/`fail`: zero hits) — evaluation is a step here, not adoption.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Capsule tests | `cargo nextest run -p jackin-capsule` | pass |
| Snapshots | `cargo nextest run -p jackin-capsule` (insta lives here) | no pending snaps |
| Lint | `cargo clippy -p jackin-capsule --all-targets -- -D warnings` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-capsule/src/daemon.rs` + `daemon/**`, `session.rs` where supervision extraction demands, new subsystem modules + their tests (moved from `daemon/tests.rs`), `daemon/ports.rs` (real boundary ports + fakes), `crates/jackin-capsule/README.md`.

**Out of scope**: TUI rendering internals (`tui/`), attach protocol wire format, telemetry call sites (plans 004/008), scrollback data structures (preserve exactly), the borrowed-row-accessor perf idea (bench first — roadmap conditions it).

## Git workflow

Branch `refactor/daemon-subsystems`; Conventional Commits; `git commit -s`; push per commit. One subsystem per PR-sized slice if the operator prefers; at minimum one commit per subsystem.

## Steps

### Step 1: Extraction order + inventory

Map every `Multiplexer` field to its target subsystem: `ClientRegistry` (client set, active-client policy), `SessionSupervisor` (sessions map + lifecycle), `StatusState`, `ClipboardState`, `PrWatch` (git/PR lookups + cache), `UsageState`, `ControlRouting`. Post the field→owner table in the PR. Extraction order: start with the least-coupled (likely `PrWatch` or `ClipboardState`), end with `SessionSupervisor`.

**Verify**: table covers 100% of fields (count in PR description).

### Step 2: Extract one subsystem (repeatable slice)

Move the fields into an owned struct with methods; `Multiplexer` holds the struct and dispatches events to it; move that subsystem's tests from `daemon/tests.rs` into the new module's `tests.rs` (test-layout rule). No behavior change; snapshot suite guards rendering.

**Verify per slice**: `cargo nextest run -p jackin-capsule` → pass; moved tests run under the new module path; `daemon/tests.rs` shrinks by the moved block.

### Step 3: Real boundary ports + fakes

Once `SessionSupervisor` exists: reshape `ports.rs` traits to own effectful operations at the daemon/supervisor boundary (attach/displace decision + execution, spawn, persist, status read) instead of boolean predicates; implement `DefaultDaemonPorts` over the real subsystems and a `FakeDaemonPorts` for tests. Add fake-port tests proving observable behavior: attach/displace outcome, reattach after disconnect, PTY-failure path, persistence round-trip — driving the daemon event loop, not predicates.

**Verify**: new fake-port tests pass; predicate-shaped trait methods deleted (`grep -n "session_known: bool" crates/jackin-capsule/src/daemon/ports.rs` → none).

### Step 4: Sim/state-machine evaluation (decision, not adoption)

With ports in place, run a short evaluation: does `proptest-state-machine` over `SessionSupervisor` (attach/displace/reattach transitions) find anything the fixed tests don't? Record adopt/defer per tool (turmoil/madsim/proptest-state-machine/fail) in the plan's PR or the roadmap item — the roadmap asks to "evaluate", so a written verdict closes it.

**Verify**: verdict recorded; any adopted dev-dependency has at least one meaningful property test.

### Step 5: Gates

`cargo clippy -p jackin-capsule --all-targets -- -D warnings`; `cargo xtask ci --fast`; README structure table updated; `session.rs`/new modules get `//!` contracts.

## Test plan

Existing 321-test daemon suite is the characterization harness — it must stay green un-weakened through every slice; step 3 adds the fake-port behavioral suite; snapshot tests guard compositor output.

## Done criteria

- [ ] `Multiplexer` reduced to event dispatch over owned subsystem structs (field count in the struct itself < ~15, all state in subsystems)
- [ ] Ports own effects; `FakeDaemonPorts` drives attach/displace/reattach/PTY-failure/persistence tests
- [ ] Each subsystem's tests live with it; `daemon/tests.rs` correspondingly reduced
- [x] Sim-tooling evaluation verdict recorded
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- A slice forces changes to range-snapshot/selection/clamp behavior (snapshot or behavioral tests redden in a way that needs assertion edits) — revert, report.
- Borrow-checker coupling between two subsystems (e.g. status needs &mut sessions during compositor borrow) resists extraction without an event-queue redesign — report the specific cycle; that design call is the operator's.
- `daemon/tests.rs` tests depend on cross-subsystem internals so heavily that moving them requires rewrites (not moves) — flag before rewriting characterization.

## Maintenance notes

- New daemon state goes into a subsystem, never back onto the shell — reviewer rule.
- Plan 009's host-to-capsule conformance scenario can later drive the fake ports for telemetry assertions.
- The `session.rs` split (supervision vs PTY plumbing) becomes tractable after `SessionSupervisor` exists; treat as follow-up evidence-driven work.

**Index deviation (audit 2026-07-15)**: demoted from DONE to IN PROGRESS — Done criteria not fully met; see implementer audit rollup.

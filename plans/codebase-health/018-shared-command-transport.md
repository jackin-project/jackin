# Plan 018: One shared command-transport model for xtask, capsule probes, and runtime shell execution

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-runtime/src/exec_host.rs crates/jackin-capsule/src/exec.rs crates/jackin-xtask/src/cmd.rs`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: MED (process boundaries on every surface)
- **Depends on**: none
- **Category**: tech-debt (shared boundaries)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Ownership item 3: "Establish one transport-level command capture/timeout/retry/status model for xtask, capsule probes, and runtime shell execution. Its API may carry ordinary bytes, timing, and exit status; protected-value classification, environment exposure, redaction, and policy enforcement remain exclusively owned by [sensitive-boundary work]." Today ~55 `Command::new` sites across three crates re-implement capture/timeout/status independently and already drift (the capsule exec deliberately has no read timeout while runtime helpers do), so timeout/retry policy can't be reasoned about or hardened in one place.

## Current state

- Site counts (re-census before starting): jackin-xtask ~20 (`src/cmd.rs` is its helper), jackin-capsule ~20 (`src/exec.rs:174` uses `tokio::process::Command`; comments at `:114,:134` note the bespoke no-read-timeout policy), jackin-runtime ~15 (`src/exec_host.rs:61,85`).
- The telemetry choke point already exists for host subprocesses: `crates/jackin-docker/src/shell_runner.rs` (`process.execute` spans) — read it first; the transport crate must compose with (not replace) that instrumentation.
- Arch tiers: check `crates/jackin-xtask/src/arch.rs` TIERS to pick the new crate's tier — it must sit low enough for xtask, capsule, and runtime to depend on it. Boundary rule from the roadmap: NO redaction/classification/env-policy in this crate — bytes, timing, exit status, timeout, retry only.
- Async split: capsule/runtime are tokio; xtask is sync — the API needs both faces (sync wrapper over the async core, or two entry points; xtask already depends on tokio? check `crates/jackin-xtask/Cargo.toml`).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| New crate tests | `cargo nextest run -p jackin-transport` (final name per step 1) | pass |
| Consumers | `cargo nextest run -p jackin-xtask -p jackin-capsule -p jackin-runtime` | pass |
| Arch gate | `cargo xtask lint arch --strict` | exit 0 |
| Agents gate | `cargo xtask lint agents` | exit 0 (new crate needs README/AGENTS/CLAUDE per crates rule) |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: one new low-tier crate (name it per repo convention — e.g. `jackin-exec`; confirm no collision) with README/AGENTS.md/CLAUDE.md symlink; migration of the three owners' call sites; `PROJECT_STRUCTURE.md` + codebase-map docs row (docs gates will demand it).

**Out of scope**: redaction/env/protected-value logic (sensitive-boundary ownership — the API must not even accept a "redact" hook); HTTP transports; changing any command's actual timeout/retry semantics during migration (preserve current per-site behavior via explicit per-call options; policy convergence is a later decision).

## Git workflow

Branch `refactor/shared-exec-transport`; Conventional Commits; `git commit -s`; push per commit. Crate-introduction commit first, then one migration commit per consumer crate.

## Steps

### Step 1: Crate skeleton + API

New crate exposing: `ExecRequest { program, args, cwd, stdin, timeout: Option<Duration>, retry: RetryPolicy }` → `ExecResult { status, stdout, stderr, duration, timed_out: bool }`; async core + sync facade. Explicitly NO env-map manipulation beyond pass-through, no redaction, no logging (callers instrument — `ShellRunner` keeps owning telemetry). Unit tests with real short-lived processes (`true`/`false`/`sleep` — but `std::thread::sleep` in-test only per clippy rules; use the disallowed-methods test escape valves as configured).

**Verify**: `cargo nextest run -p <crate>` → pass; `cargo xtask lint arch --strict` + `cargo xtask lint agents` → exit 0.

### Step 2: Migrate runtime (`exec_host.rs`), then capsule (`exec.rs`), then xtask (`cmd.rs`)

Per consumer: map each call site's current semantics (timeout? capture? kill-on-drop?) into explicit `ExecRequest` options — byte-preserving behavior, no policy change; capsule's no-read-timeout stays as `timeout: None` with its existing WHY comment moved along. Keep thin per-crate wrappers where ergonomics demand (xtask's `cmd.rs` may become a shim over the crate).

**Verify per consumer**: that crate's full suite passes; `grep -c "Command::new" crates/<consumer>/src -r` shrinks to ~0 outside the shim (enumerate any survivor with a reason).

### Step 3: Gates + docs

`PROJECT_STRUCTURE.md` + codebase-map entry; full CI.

**Verify**: `cargo xtask ci --fast` → exit 0; `cargo xtask docs repo-links` → exit 0.

## Test plan

Transport unit tests (exit codes, capture, timeout firing, timeout-none, retry policy); consumer suites as characterization. Add one regression test per consumer for its previously-bespoke semantic (capsule no-read-timeout; runtime timeout kill).

## Done criteria

- [x] One transport crate; three consumers migrated; surviving direct `Command::new` sites enumerated with reasons
- [x] No redaction/env-policy/logging in the transport crate (review + grep `redact\|secret` → none)
- [x] Per-site semantics preserved (regression tests)
- [x] Arch/agents/docs gates green; `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Tier placement impossible (some consumer sits below every viable tier) — report the tier math.
- A call site's semantics are load-bearing and unclear (e.g. relies on inherited fds/pty details the API doesn't model) — leave it direct, enumerate, continue; >10 such sites = stop and report the API gap.
- Sensitive-boundary code (protected-value paths) turns out to flow through a site being migrated — do not move it; that migration belongs to the sensitive-boundary program.

## Maintenance notes

- Retry/timeout policy convergence (making sites share defaults) is a deliberate later decision with this crate as the lever.
- New subprocess call sites use the transport crate — reviewer rule; consider a disallowed-methods entry for `std::process::Command::new` outside the transport crate once migration stabilizes (plan 011's inventory owns that decision).

## Execution notes

- Transport crate `jackin-process` with `ExecRequest`/`ExecResult`, sync+async, timeout/retry/stdio modes, spawn lifecycle helpers.
- Migrated consumers: runtime (exec_host, apple container, attach, git pull, snapshot, host_daemon), capsule (exec, exit_assess, firewall, runtime_setup, git/util probes), xtask (`cmd` shim + all call sites), isolation git_inspect, build-meta git SHA, jackin preflight, pr-trailers, host desktop openers, instance gh/keychain probes, e2e profile_matrix.
- Remaining direct `Command` sites are intentional special cases: docker/shell_runner streaming attach, `op_cli`/host_claude (sensitive-boundary + interactive PTY), host clipboard osascript image pipes, usage agent child sessions, image download verify, build-jackin-capsule zigbuild, help man/pager, daemon residual. Documented for follow-on streaming/PTY API extension — not deferred acceptance blockers; primary three consumers + broad production probes are on the shared transport.

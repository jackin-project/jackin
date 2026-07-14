# Plan 013: Executable boundary gates — Turso sole-owner + forbidden-root container-path audit

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-xtask/src/container_paths_gate.rs crates/jackin-xtask/src/arch.rs container-path-allowlist.toml`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (new gate false positives; tune before enforcing)
- **Depends on**: none
- **Category**: tech-debt (executable policy)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Two roadmap items require decisions that currently hold only by fact, not by enforcement. Rust-enforcement item 8: `jackin-usage` is the sole Turso SDK owner today, but nothing stops another crate from adding the dependency — "A dependency pin or exception is not closed merely because it has a comment." Item 9 requires "a token-aware absolute-container-path audit for forbidden `/run`, `/var`, `/opt`, `/etc`, and `/tmp/jackin*` paths … each prohibited-root fixture fails with file/line and a fix"; the existing gate counts only `"/jackin` substring occurrences (its own tests assert a `/run/x` fixture is NOT counted), is not token-aware (comments count the same as code), and reports per-file counts without line numbers. The `/jackin` chokepoint ledger and shrink-only allowlist are already in place and stay.

## Current state

- Container gate: `crates/jackin-xtask/src/container_paths_gate.rs:161` — `let n = text.matches("\"/jackin").count();`; skips `tests/` dirs (`:155-157`); reports file+count (`:100-113`); shrink-only allowlist logic `:92-114` against `container-path-allowlist.toml`. Tests `container_paths_gate/tests.rs:10,12` assert the `/run/x` fixture is ignored.
- Turso: only `crates/jackin-usage/Cargo.toml:35` depends on `turso`; imports only in `crates/jackin-usage/src/store_backend.rs:7,11`; `crates/jackin-xtask/src/arch.rs` (dependency gate, TIERS table at `:34+`) has no turso rule. Pin rationale at root `Cargo.toml:128-130`; deny.toml exceptions + `.cargo/audit.toml` sync already in order.
- Legitimate forbidden-root strings exist in production code that EMITS host-side or configures Docker (e.g. `crates/jackin-runtime/src/runtime/docker_profile.rs`, `crates/jackin-image/src/derived_image.rs` write Dockerfiles/paths for the container image build) — the roadmap requires distinguishing "host-only and test fixtures from container emitters" and routing production container paths through the owned chokepoint (`jackin_core` container-paths module — locate with `grep -rn "container_paths\|/jackin" crates/jackin-core/src --include='*.rs' -l | head`) or an audited exception.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| xtask tests | `cargo nextest run -p jackin-xtask` | pass |
| Gate | `cargo xtask lint container-paths` (confirm subcommand name in `crates/jackin-xtask/src/main.rs`) | exit 0 |
| Arch gate | `cargo xtask lint arch --strict` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-xtask/src/container_paths_gate.rs` + tests (extend), `crates/jackin-xtask/src/arch.rs` or a small new xtask module for the Turso rule + tests, `container-path-allowlist.toml` (new forbidden-root exception section), README of jackin-xtask if modules change.

**Out of scope**: moving any production path to the chokepoint beyond what the new audit flags as trivially fixable (big moves = follow-up with the audit output as evidence); host-write policy (sensitive-boundary roadmap).

## Git workflow

Branch `feat/boundary-gates`; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Turso sole-owner rule

Add an xtask check (natural home: beside the arch gate) that fails if (a) any `crates/*/Cargo.toml` other than `crates/jackin-usage/Cargo.toml` declares `turso` (or `libsql`) in any dependency table, or (b) any `.rs` outside `crates/jackin-usage/src` contains `use turso::` / `turso::` path tokens (token-level: parse with syn or match on identifier boundaries; exclude comments/strings to avoid the plan-010 defect class — the diagnostics test at `crates/jackin-diagnostics/src/observability/otlp/tests.rs:423` has a `turso` log-string that must NOT trip it). Failure message names the owner rule and the file:line.

**Verify**: `cargo nextest run -p jackin-xtask -E 'test(/turso/)'` → new tests pass (violation fixture detected, string mention ignored, owner crate exempt); gate exits 0 on the real tree.

### Step 2: Forbidden-root scanner

Extend `container_paths_gate.rs`: parse each production `.rs` (reuse plan-010's syn approach if landed; else a lexer that honors comments/strings — string literals are exactly where paths live, so scan string literal CONTENTS but skip comments) for string literals beginning with `/run`, `/var`, `/opt`, `/etc`, or matching `/tmp/jackin`. Classify each hit: (a) routed through the chokepoint (call-site file is the chokepoint module) → OK; (b) container-emitter (path flows into container config/Dockerfile/env — undecidable statically in general, so classify by allowlist) → violation unless allowlisted; (c) host-only/test fixture → allowlisted with reason. Report every violation as `file:line: <literal> — route through <chokepoint> or add a reasoned exception in container-path-allowlist.toml`. Add a `[forbidden-roots]` section to the allowlist file with per-entry `path`, `file`, `reason` — shrink-only, same mechanics as the existing ledger.

**Verify**: gate run over the tree produces a finite reviewed list; every entry either fixed (trivial reroutes only) or allowlisted with a reason; `cargo nextest run -p jackin-xtask` → pass including new fixtures (a `/run/x` production literal now FAILS with file:line — inverting the old test's assertion, update it deliberately).

### Step 3: Line-number reporting for the existing `/jackin` ledger

While in the file: upgrade the existing chokepoint report from file+count to file:line (the roadmap acceptance asks for file/line + fix on prohibited-root failures; give the `/jackin` ledger the same treatment for consistency).

**Verify**: `cargo xtask lint container-paths` output shows file:line; `cargo xtask ci --fast` → exit 0.

## Test plan

Fixture-driven tests in `container_paths_gate/tests.rs`: forbidden root in production literal (fails, file:line), same in comment (ignored), same in `tests/` path (ignored), allowlisted entry (passes, shrink-only enforced — removing the source line then requires allowlist shrink), `/tmp/jackin-foo` matches the wildcard. Turso rule tests per step 1.

## Done criteria

- [ ] Turso rule: non-owner dependency or import fails with file:line (fixture-proven); real tree passes
- [ ] Forbidden-root audit: token-aware, file:line + fix message, shrink-only exceptions, real tree green with reviewed allowlist
- [ ] `/run/x`-style fixture now fails (old ignore-assertion consciously inverted)
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Forbidden-root sweep over the real tree yields >40 unclassifiable hits — the emitter-vs-host distinction needs an operator-reviewed classification pass; deliver the raw list instead of a 40-entry allowlist.
- The chokepoint module in jackin-core doesn't exist or is named differently than expected — locate it first; if there is genuinely no owned chokepoint for container paths, that's a missing prerequisite to report.
- Turso identifier scan cannot avoid the diagnostics test-string false positive without syn — wait for plan 010's parser or use syn directly here.

## Maintenance notes

- Both allowlists are shrink-only; reviewers reject growth without a reasoned entry.
- If a second crate ever legitimately needs Turso, the rule's owner constant is the single place to change — with the roadmap decision updated in the same PR.

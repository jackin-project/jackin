# Code-health parallel dispatch playbook

How to execute plans and residual ledger rows **faster without cutting
corners**: maximize concurrent independent work; serialize only on shared
write sets; keep Done criteria and gates.

Related:

- Plan ledger + historical batches: [README.md](README.md)
- Residual dispositions: [RESIDUAL_LEDGER.md](RESIDUAL_LEDGER.md)
- Re-verify evidence: [VERIFICATION.md](VERIFICATION.md)
- Inventory format: [plan-inventory.md](plan-inventory.md)

## Goals

| Goal | Non-goal |
|------|----------|
| Wall-clock throughput via fan-out | Skipping Done criteria |
| One integration `ci --fast` per merge wave | Full suite on every worker |
| Disjoint write allowlists | Two owners on `Cargo.toml` lint table |
| Foreground package-scoped verify | Ending a turn to “wait on background CI” |

## Roles

### Orchestrator (main session)

- Owns the **integration branch** (usually one PR branch; branch lock applies).
- Builds the ready set from the DAG + file ownership map.
- Spawns **N workers in one turn** (parallel tool calls); never waits for worker A before starting B when both are ready.
- Merges worktree results in dependency order.
- Runs **batch integration verify** once per wave (see tiers below).
- Updates README status, residual ledger, inventory, roadmap freshness.
- Commits with DCO (`-s`) and pushes immediately after each integration commit.

### Worker (sub-agent)

- Owns **exactly one** plan or residual row (or a tightly declared pair).
- Touches only its **write allowlist**.
- Runs **narrow** verify only.
- Exits with a structured report; does not own full-workspace CI.
- Uses `isolation: worktree` when the harness supports it so edits do not fight.

## Hard serialization (single owner at a time)

Regardless of batch labels, only **one** in-flight worker may write:

| Resource | Typical owners (historical) | Rule |
|----------|----------------------------|------|
| Root `Cargo.toml` lint / clippy table | 011 → 019 → 034 → 047 | Queue; never parallel |
| `crates/jackin-xtask/src/ci.rs` + `.github/workflows/ci.yml` | 011 → 022 → 036 | Queue |
| `suppression-budget.toml` / `ratchet.toml` floors | 011 / 017 / budget refresh | Orchestrator or single lint owner |
| `hygiene.yml` **restructure** | — | Forbidden mid-wave; **append-only** jobs may parallel if job ids unique |
| Capsule smoke path (full product path) | 026, 042 | Code may parallel; smoke can parallel if Docker slots allow |

Everything else with **disjoint crates/paths** is eligible for concurrent start.

## Historical safe-parallel batches (003–054)

These batches remain the reference DAG for re-runs or audits (all DONE on
`chore/rust-code-health-roadmap`). Full plan titles live in the README.

1. **Batch 1** — spine openers + independents: 010, 018, 003+014, 004, 007, 008, 009, 023, 024, 028, 029, 031, 049, 053.
2. **Batch 2** — after batch 1: 011 (serial spine), 041∥042∥043 (after 018), 015, 025, 026, 027, 040, 048.
3. **Batch 3** — after batch 2: 012 then 016; 019 then 021; 034 then 047; 022 then 036; 044 after 041+042; plus independents 013, 020, 030, 032+033, 035, 037, 038, 039, 045, 046, 050, 051, 052.
4. **Last** — 017 after 010+011; residual SEQ debt → DEFER via 055–056 + residual ledger.

Telemetry chain (must respect): **018 → (041 ∥ 042) → 043 → 044**.

## Next residual waves (from RESIDUAL_LEDGER)

Only **DEFER** rows with a concrete next trigger. Group by parallel safety.

### Wave R1 — small independent closes (fan-out OK)

| Residual | Scope | Write allowlist (typical) | Narrow verify |
|----------|-------|---------------------------|---------------|
| R-014-materialize-bench | **CLOSED** plan 057 | — | — |
| R-038-env-console-tail (env slice) | WorkspaceName at env sites | `crates/jackin-env/**`, maybe `jackin-core` | `cargo nextest -p jackin-env` |
| R-038-env-console-tail (console slice) | WorkspaceName at console sites | `crates/jackin-console/**` | `cargo nextest -p jackin-console` |
| R-snapshot-helpers | helpers into test-support | `crates/jackin-test-support/**`, consumer test paths | `cargo nextest -p jackin-test-support -p <consumer>` |
| R-map-metadata-gate | **CLOSED** plan 057 (`docs map-check`) | — | — |
| R-export-volume-ratchet | **CLOSED** plan 057 (`export-volume` family) | — | — |
| R-complexity-threshold | lower one clippy floor after census | `clippy.toml` / baseline / budget (orchestrator if conflict) | `cargo clippy -p <hot crates>` then lint strict |

**Dispatch:** spawn all ready R1 rows in one turn with disjoint allowlists.
Env and console WorkspaceName slices are **two workers**, not one.

### Wave R2 — thiserror mid-tranches (one crate per worker)

| Residual slice | Crate | Measured sites (ledger) | After |
|----------------|-------|-------------------------|-------|
| config | `jackin-config` | ~66 | 037 idiom |
| isolation | `jackin-isolation` | ~14 | 037 |
| docker | `jackin-docker` | ~17 | 037 |
| image | `jackin-image` | ~23 | 037 |
| instance | `jackin-instance` | ~7 | 037 |

**Dispatch:** all five **parallel** (disjoint crates). Orchestrator only
refreshes shared budgets if clippy floors change.

### Wave R3 — design-gated (serialize or single design owner)

Do **not** fan out blindly; need a design spike or port extract first:

| Residual | Why serial / design-first |
|----------|---------------------------|
| R-launch-typestate / R-typestate-general | LaunchCore extract; large blast radius |
| R-daemon-decomp / R-daemon-char-remainder | Ports + MISSING worklists from 032 |
| R-033-suite-a | Blocked on LaunchCore fixture cheapness |
| R-sim-turmoil | Needs daemon ports |
| R-self-tightening / R-agent-hygiene / R-health-history | Ops/bot/product decisions |
| R-014-launch-pipeline-bench | After launch-core extract |
| R-iai-callgrind / R-perf-budgets / R-dhat-budgets-ratchet | After stable bench lane + ratchet family design |

**Dispatch:** one design owner or a short design doc PR, then parallel
characterization slices.

### Wave R4 — product / policy gated (no agent spend until trigger)

| Residual | Trigger |
|----------|---------|
| R-023-usage-scope | Product reintroduces workspace usage CLI |
| R-023-apple-container | Backend ships |
| R-045-hello-skew | Protocol softens Hello (pinned fail-closed) |
| Golden agent tasks (matrix DEFER) | Operator spend framing |

## Verification tiers (quality floor)

| Tier | Who | Commands | When |
|------|-----|----------|------|
| **T0 worker** | Worker | `cargo check -p <crates…>`; `cargo nextest -p <crates…>` (or plan-named test filter) | Before worker exit |
| **T1 merge** | Orchestrator | `cargo run -p jackin-xtask -- lint --strict` | After merging a wave into the integration branch |
| **T2 batch** | Orchestrator | `cargo xtask ci --fast` | Once per merge wave (not per plan) |
| **T3 program** | Orchestrator | Inventory rebuild; residual ledger audit; waiver-only reds documented | End of program / major wave |

Waivers (executor env) stay explicit: Docker-missing `manager_flow` disk
tests; `RUSTSEC-2026-0204` via turso. Non-waiver reds block the wave.

## File ownership map (how to decide parallel)

Before spawn, each plan/residual gets:

```text
plan_id | write_allowlist | read_only_ok | conflicts_with
```

Two workers may run together iff their `write_allowlist` sets are **disjoint**
and neither holds a hard-serialization resource above.

Suggested allowlist patterns:

| Work class | Allowlist |
|------------|-----------|
| Single-crate fix | `crates/<crate>/**` |
| Host CLI only | `crates/jackin/**` (not usage unless needed) |
| Xtask gate | `crates/jackin-xtask/src/<module>.rs` only |
| Docs gate | `crates/jackin-xtask/src/*docs*`, `docs/**` as needed |
| Hygiene job append | **only** the new job block at end of `.github/workflows/hygiene.yml` |
| Lint table | root `Cargo.toml` **orchestrator-owned** |

## Worker prompt template (copy into every spawn)

```text
You are a code-health WORKER, not the orchestrator.

PLAN: plans/code-health/<NNN-title>.md   (or RESIDUAL_LEDGER row <R-id>)
INTEGRATION BRANCH: <branch>   (do not create a second remote feature branch
  unless isolation=worktree requires a local branch name)
WRITE ALLOWLIST: <paths>
FORBIDDEN: root Cargo.toml lint table; ci.rs/ci.yml; hygiene restructure;
  other plans' crates; force-push; skipping Done criteria

RULES:
1. Implement only this plan/residual. No drive-by refactors.
2. Touch only WRITE ALLOWLIST. If you need a forbidden file, STOP and report BLOCKED.
3. Meet every Done criterion (or residual CLOSED criteria) with evidence.
4. Verify FOREGROUND with narrow commands only:
   cargo check -p … ; cargo nextest -p …  (or plan-specified filter)
   Do NOT run cargo xtask ci --fast. Do NOT end the turn waiting on CI.
5. Prefer worktree isolation; leave a clean tree or a single logical commit.
6. Commits: Conventional Commits + DCO (-s) if you commit; push only to the
   branch the orchestrator named.
7. Exit report (required):
   - status: DONE | BLOCKED | PARTIAL
   - files changed
   - commands + exit codes
   - Done-criteria checklist with pass/fail
   - residuals / follow-ups for the ledger
   - merge notes (conflicts expected?)

Brand: jackin❯ in prose. Identifiers stay jackin without chevron.
```

## Orchestrator loop (one wave)

```text
1. Load RESIDUAL_LEDGER + README statuses
2. ready = deps satisfied ∧ write sets disjoint ∧ not hard-serialized
3. Spawn ALL ready workers in ONE turn (parallel)
4. While workers run: do serial spine work OR prep inventory greps
5. On each DONE: merge worktree → integration branch
6. When wave ready set empty or merge batch full:
     lint --strict → fix non-waiver reds → ci --fast once
7. Update ledger/inventory/roadmap; commit -s; push
8. Repeat until no executable ready rows (only design/product DEFERs left)
```

**Never** do: start worker 1 → wait → start worker 2 when both were ready
at step 3.

## Anti-patterns (from past tranches)

| Pattern | Cost | Fix |
|---------|------|-----|
| Worker ends turn to await background CI | Dead agents, dirty trees | Foreground T0 only; T2 is orchestrator |
| Harness worktree on wrong base | Drift, re-dispatch | Checkout plan-base / integration tip first |
| N remote `exec-plan-*` branches | Merge hell | Worktrees → one integration branch |
| Full CI per plan | Wall clock ≈ sum(CI) | T2 once per wave |
| Parallel lint-table editors | Conflict + wrong floors | Serial owners only |
| Docs-only residual close without ledger | Skeptic refute | CLOSED in tree **or** DEFER measured in ledger |

## Inventory evidence format (orchestrator T3)

Every plan/residual row in re-verify:

```text
id | ledger_status | in_tree_pass|fail|reject | evidence
```

Include all numbered plans present under `plans/code-health/` (e.g. 051).
Evidence = path:line, gate exit, or residual ledger ID.

## Capsule / smoke note

Plans that touch capsule runtime (historical 026, 042) require the project
smoke block at PR time. Workers still use narrow tests; orchestrator schedules
smoke once per capsule-affecting merge wave when Docker is available.

## Quick start (next session)

```sh
# 1) Confirm branch lock
git branch --show-current

# 2) List executable residuals
rg '^\| R-' plans/code-health/RESIDUAL_LEDGER.md | rg DEFER

# 3) Pick Wave R1 disjoint set → spawn workers in parallel
# 4) Merge → cargo run -p jackin-xtask -- lint --strict
# 5) cargo xtask ci --fast once; document waivers only
```

When opening new numbered plans for residual slices, start at **057+**, keep
one residual ID ↔ one plan or one plan section, and register the plan in
README status + inventory the same PR.

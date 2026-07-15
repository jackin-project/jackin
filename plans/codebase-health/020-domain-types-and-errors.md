# Plan 020: Domain newtype census + typed error taxonomy at real boundaries

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-core/src/workspace_name.rs crates/jackin-capsule/src/daemon.rs`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P3
- **Effort**: L (census is M; adoption is rolling)
- **Risk**: MED (serde must stay schema-preserving)
- **Depends on**: none (coordinates with 017 — session ids live in the daemon)
- **Category**: tech-debt (type boundaries)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Ownership item 4: "Census `WorkspaceName`, `RoleRef`, `SessionId`, `ContainerId`, `TokenCount`, and `MountPath`, and record the intended named type or explicit primitive decision at every boundary." Only `WorkspaceName` exists (`crates/jackin-core/src/workspace_name.rs:12`); the other five have no definition, and raw `session_id: u64`/`container_id: String` cross ~59 boundary sites unvalidated. The same item requires libraries to use "typed `thiserror` enums with machine-matchable variants" with `anyhow` at reporting boundaries — the census shows `jackin-runtime` at 193 anyhow sites / 2 thiserror files and `jackin-capsule` at 48/0, so callers string-match failures the type system should distinguish. The roadmap's own bar is a *decision at every boundary*, not blanket newtyping — census first, adopt where a real boundary is protected.

## Current state

- Exemplar newtype: `crates/jackin-core/src/workspace_name.rs` — read fully; it shows the repo's validated-constructor + serde idiom to replicate.
- Absent: `RoleRef`, `SessionId`, `ContainerId`, `TokenCount`, `MountPath` (no struct defs anywhere in `crates/*/src`).
- Boundary census starting points: `grep -rn "session_id: u64\|session_id: String" crates --include='*.rs' | grep -v tests | wc -l` (~daemon/attach/protocol); `container_id: String` (~runtime/docker/instance); token counts in `jackin-usage`; mount paths in `jackin-isolation`/`jackin-config`.
- Error census (coarse; re-verify per crate): runtime 193 anyhow/2 thiserror, env 115/2, instance 60/1, config 56/2, capsule 48/0. `Box<dyn Error>`: zero (compliant). Structured-error INV example resting on string matching: the runtime-launch spec's `verify_credential_api_key_missing_returns_structured_error`.
- Roadmap adjuncts to record (not necessarily implement): Result-first public constructors, Send-contract decision before broader `async fn`-in-trait, deliberate `#[non_exhaustive]`, typestate only where a pipeline benefits (plan 016 is doing the launch one).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Workspace | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Targeted tests | `cargo nextest run -p <crate>` per touched crate | pass |
| Schema gate | `cargo xtask schema-check --base origin/main` | exit 0 (serde stays schema-preserving) |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: the census document (contributor reference, same home as plan 011's policy doc or a sibling page + `meta.json`); new newtypes in their owning tier (likely `jackin-core`); boundary threading for the two highest-value types first (`SessionId`, `ContainerId`); one library-crate thiserror pilot (`jackin-config` — smallest anyhow census with a real public API).

**Out of scope**: converting all five types everywhere in one PR (rolling adoption — the census records each boundary's decision including "primitive, because…"); rewriting runtime/capsule error types wholesale (pilot first, measure ergonomics); `anyhow` at binary/reporting boundaries (stays, per contract).

## Git workflow

Branch `refactor/domain-newtypes` (census + first types), `refactor/config-error-taxonomy` (pilot); Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Boundary census

For each of the six names: enumerate boundaries (constructor args, launch/attach APIs, serde structs, protocol frames) via targeted greps; record per boundary: intended type or explicit-primitive decision + reason. Publish as the census table in the reference doc.

**Verify**: doc exists; all six names covered; docs gates green (`cargo xtask docs repo-links`).

### Step 2: `SessionId` + `ContainerId` newtypes

Define in `jackin-core` following the `WorkspaceName` idiom: validated Result-first constructor (`SessionId` wraps the daemon's u64 — validation may be trivial; the win is signature-level confusion-proofing; `ContainerId` validates docker-id shape), schema-preserving serde (`#[serde(transparent)]`), `Display`/`FromStr` as needed. Thread through the highest-value boundaries per the census (daemon session APIs — coordinate with plan 017's `SessionSupervisor`; runtime/docker container-id params). Leave lower-value sites as recorded primitive decisions.

**Verify**: workspace clippy green; touched crates' suites pass; `cargo xtask schema-check --base origin/main` → exit 0 (no wire/persisted schema drift).

### Step 3: `jackin-config` thiserror pilot

Define a typed `ConfigError` enum (machine-matchable variants + `#[source]` chains) for the crate's public fallible API; keep `anyhow` internal/reporting only. Measure the ergonomic cost (how many `?` sites needed `map_err`? how many callers matched variants?) and record the verdict + rollout recommendation for runtime/capsule in the census doc.

**Verify**: `cargo nextest run -p jackin-config` + dependents pass; at least one caller demonstrably matches a variant instead of string-matching (find one current string-match consumer and convert it as proof).

### Step 4: Record the adjunct decisions

Add to the census doc: Result-first constructor audit outcome for the newtyped boundaries; the Send-contract decision for `async fn`-in-trait (survey current usage: `grep -rn "async fn" crates/*/src --include='*.rs' | grep "trait" | head`); `#[non_exhaustive]` policy (which public enums are deliberately exhaustive).

**Verify**: `cargo xtask ci --fast` → exit 0.

## Test plan

Newtype unit tests (constructor validation, serde round-trip preserving schema — model on `workspace_name/tests.rs` if present, else the crate's test idiom); config-error tests asserting variant matching; boundary-threading rides existing suites.

## Done criteria

- [x] Census doc: all six names, per-boundary type-or-primitive decision recorded
- [x] `SessionId`/`ContainerId` exist, validated, serde-transparent, threaded at census-designated boundaries
- [x] `jackin-config` public API returns typed errors; one caller converted from string-matching; rollout verdict recorded
- [x] Adjunct decisions (Result-first, Send, non_exhaustive) recorded
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Schema-check reddens on a serde change (a newtype broke a persisted/wire shape) — revert that site; transparent wrapping should never do this, so investigate before proceeding.
- Threading `SessionId` through the daemon collides with in-flight plan 017 — rebase onto its subsystem shape or defer that boundary.
- The config pilot shows variant enums fighting the crate's error flow badly (>30% of sites need adapters) — record the negative verdict; do NOT force runtime/capsule conversion.

## Maintenance notes

- The census doc is the standing decision record: new boundaries for these six concepts must pick a row (type or reasoned primitive) at review time.
- `RoleRef`/`TokenCount`/`MountPath` adoption proceeds boundary-by-boundary using the same recipe when their census rows justify it.

**Index deviation (audit 2026-07-15)**: demoted from DONE to IN PROGRESS — Done criteria not fully met; see implementer audit rollup.

## Execution notes

- Every public fallible `jackin-config` function now returns `ConfigResult<T>`. `ConfigError` is non-exhaustive and retains specific lookup/validation variants plus transparent source variants; internal helpers may use `anyhow` and convert at the public boundary.
- The workspace missing-removal test now matches `ConfigError::UnknownWorkspace` instead of string-matching. The workspace-wide all-target/all-feature compile proves downstream reporting boundaries convert explicitly. The adapter rate stayed below the plan's 30% stop threshold, so the census records a positive rollout verdict.

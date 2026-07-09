# Plan 037: Typed thiserror errors for jackin-core and the jackin-env resolution path

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**: `git diff --stat 0971da66d..HEAD -- crates/jackin-core/src/ crates/jackin-env/src/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (public signature changes in two foundational crates; mitigated by the fact that every measured downstream caller only `?`-propagates into `anyhow`, which absorbs any `std::error::Error` automatically)
- **Depends on**: none (**coordination**: plan 031 owns `crates/jackin-env/src/op_cli.rs` and creates `jackin-core/src/op_probe_error.rs` — do not touch `op_cli.rs`'s error paths here; if 031 landed, match its enum/module style)
- **Category**: tech-debt (error taxonomy)
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

Roadmap Phase 2, "Type-system, newtype, and error-handling discipline", item 2 (`docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` line 150): library crates define concrete `thiserror` enums; `anyhow` is permitted only at binary boundaries. The measured reality (census 2026-07-09): `anyhow` saturates 21 library crates; **jackin-env has zero typed error types** — every failure is a stringly `bail!`, which is why `jackin-console-oppicker` classifies op errors by substring matching (`lib.rs:986-1001`, the debt plan 031 fixes for probes). This plan starts the conversion where the roadmap says to start — the foundational crates: jackin-core's four concrete fallible surfaces and jackin-env's launch-critical resolution path. Port traits (`DockerApi`, `CommandRunner`, sink traits) deliberately keep `anyhow` — they are boundary seams whose errors are reported, not handled; converting them is a breaking 8-implementor change recorded as residue, not silently attempted here.

## Current state

### jackin-core (has `thiserror = { workspace = true }` already)

- **Existing exemplars to match** (all in-crate):
  - `crates/jackin-core/src/isolation.rs:5-8`:

```rust
#[derive(Debug, thiserror::Error)]
#[error("invalid isolation `{0}`; expected one of: shared, worktree, clone")]
pub struct ParseMountIsolationError(String);
```

  - `crates/jackin-core/src/selector.rs:37-44` — `SelectorError` thiserror enum, `#[non_exhaustive]`.
  - `crates/jackin-core/src/agent.rs:141-146` — `ParseAgentError { got: String }` with named-field message.
- **Conversion targets**:
  1. `crates/jackin-core/src/docker_security.rs:58-71` — `ParseProfileError(String)` is the crate's one **hand-rolled** pre-thiserror error (manual `Display` + empty `impl std::error::Error`); `FromStr for DockerSecurityProfile` returns it (`:44-56`, `other => Err(ParseProfileError(other.to_owned()))`). Message text: `"unknown docker profile {:?} - valid values: locked, hardened, standard, compat"`.
  2. `crates/jackin-core/src/env_model.rs:140-142` — `pub fn topological_env_order(…) -> anyhow::Result<…>`; single failure: `bail!("env var dependency cycle detected")` (`:185`). External callers: `crates/jackin-manifest/src/validate.rs:168` and `crates/jackin-env/src/env_resolver.rs:91` (both `?`-propagate).
  3. `crates/jackin-core/src/paths.rs` — `JackinPaths::detect` (`:28`, fails with `anyhow!("Cannot resolve home directory")` at `:30`) and `ensure_base_dirs` (`:84`, propagates `std::fs::create_dir_all` io errors). Callers `?`-propagate (`crates/jackin/src/app.rs:116`, `bin/build_jackin_capsule.rs:65`).
- **NOT targets** (keep `anyhow`, document why in the module headers where you touch them):
  - Port traits: `DockerApi` (`docker.rs:95`), `CommandRunner` (`runner.rs:56`), `StandaloneDialogSink` (`standalone_dialog.rs:24`) — failures are implementor-defined; trait change = lockstep break of 8 implementors.
  - `LaunchCancelled` (`launch_progress.rs:203-224`) — a deliberate concrete sentinel carried **inside** `anyhow::Error`, recovered by `downcast_ref` at the binary boundary (`crates/jackin/src/main.rs:142`, `app.rs:217`). This is the repo's established typed-source-inside-anyhow idiom; leave it.
  - `worktree_dirty::assess_worktree` (`worktree_dirty.rs:121-126`) — doc states it never returns `Err` today (every git failure maps to fail-closed `Unpushed`); its innards call the anyhow-returning `CommandRunner`. Leave; recorded residue.

### jackin-env (NO thiserror dependency yet; zero typed errors)

- `crates/jackin-env/Cargo.toml`: `anyhow` present, `thiserror` **absent** — must be added (workspace dep exists at root `Cargo.toml:103`, `thiserror = "2.0"`).
- **Conversion target 1 — `resolve.rs`** (the operator-env resolution path; 12 files in the crate use anyhow, this one carries the taxonomy). Distinct failure kinds measured at: reserved runtime names (`:51`), not an `op://` ref (`:86`, `:100`), shell-var substitution in `op://` ref (`:89`), malformed URI segments (`:106`), item-not-found-in-vault (`:154`, `:175`), ambiguous items needing disambiguation (`:161`), per-var aborted resolution (`:501` "operator env resolution aborted: {e}"), aggregated failure (`:557` "operator env resolution failed for N var(s)").
- **Conversion target 2 — `env_resolver.rs`**: `bail!("env var {name}: required prompt cannot be skipped")` (`:148`); propagates `topological_env_order` (`:91`).
- **NOT targets this wave** (recorded residue): `op_cli.rs` (plan 031), `token_setup.rs` (26 pub items, its own future tranche), `host_claude.rs`, `picker.rs`, `op_struct.rs`/`op_runner.rs` trait signatures (same port-seam rule as core).
- **Downstream error handling reality** (why signature changes are safe): every measured consumer `?`-propagates into an `anyhow::Result` context — `launch_pipeline.rs:87,124,133,157`, `launch_slot.rs:232`, `manifest/validate.rs:168`, console `services/launch.rs:227`. Rust's `?` auto-converts any `E: std::error::Error + Send + Sync + 'static` into `anyhow::Error`, so `anyhow::Result<T>` → `Result<T, ConcreteError>` at a leaf compiles without caller edits **unless** a caller used `.context(…)` (none do — census: zero `.context()` calls in either crate's callers on these paths) or stored the error.
- Convention note: no `pub use anyhow` or `Result` alias exists in either crate; don't introduce one.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core+env tests | `cargo nextest run -p jackin-core -p jackin-env` | all pass |
| Downstream compile proof | `cargo check --workspace --all-targets --locked` | exit 0 |
| Full lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify):
- `crates/jackin-core/src/docker_security.rs`, `env_model.rs`, `paths.rs` (+ their sibling `tests.rs` files)
- `crates/jackin-env/Cargo.toml`, `crates/jackin-env/src/resolve.rs`, `env_resolver.rs`, `crates/jackin-env/src/lib.rs` (re-export the new error types) (+ sibling `tests.rs` files)
- Downstream files ONLY where `cargo check` forces an edit (expected: none; STOP condition 3 if more than trivial)
- `crates/jackin-core/README.md`, `crates/jackin-env/README.md` (Public API rows)

**Out of scope** (do NOT touch):
- `op_cli.rs` (plan 031), `token_setup.rs`, `host_claude.rs`, `picker.rs`, `op_struct.rs`, `op_runner.rs`, `test_support.rs`
- All port traits (`docker.rs`, `runner.rs`, `standalone_dialog.rs`) and `launch_progress.rs`
- `worktree_dirty.rs`
- Any error-message TEXT change beyond what thiserror formatting requires — messages are load-bearing (operators read them; some flow into diagnostics); keep wording identical.

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `refactor/thiserror-core-env` and wait for confirmation.
- Conventional Commits, signed, push after each: `refactor(core): thiserror for profile/env-cycle/paths errors`, `refactor(env): typed operator-env resolution errors`.

## Steps

### Step 1: Mechanical — `ParseProfileError` to thiserror

In `docker_security.rs`, replace the hand-rolled `Display`/`Error` impls (`:61-71`) with:

```rust
#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown docker profile {0:?} - valid values: locked, hardened, standard, compat")]
pub struct ParseProfileError(String);
```

Keep the struct's field private (it already is), keep `Clone`, keep message text byte-identical.

**Verify**: `cargo nextest run -p jackin-core docker_security` → existing tests pass (a test asserting the message string proves parity; if none exists, add one in `docker_security/tests.rs`).

### Step 2: `EnvCycleError` in env_model

Define next to the fn, matching `ParseMountIsolationError`'s shape:

```rust
#[derive(Debug, thiserror::Error)]
#[error("env var dependency cycle detected")]
pub struct EnvCycleError;
```

Change `topological_env_order` to `-> Result<Vec<…>, EnvCycleError>` (keep the exact success type), replace the `bail!` with `return Err(EnvCycleError)`. Re-export from `lib.rs` if env_model items are re-exported there (check; match existing style).

**Verify**: `cargo check --workspace --all-targets --locked` → exit 0 (proves manifest/env callers absorb it via `?`). `cargo nextest run -p jackin-core -p jackin-manifest` → pass.

### Step 3: `PathsError` in paths

```rust
#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("Cannot resolve home directory")]
    HomeDirUnresolved,
    #[error("failed to create {path}")]
    CreateDir { path: std::path::PathBuf, #[source] source: std::io::Error },
}
```

`JackinPaths::detect` → `Result<Self, PathsError>`; `ensure_base_dirs` → `Result<(), PathsError>` with each `create_dir_all` wrapped to name the dir it was creating (today the bare `?` loses which dir failed — the one permitted message improvement, since it adds the path context the io error lacks; note it in the commit body).

**Verify**: `cargo check --workspace --all-targets --locked` → exit 0. `cargo nextest run -p jackin-core paths` → pass.

### Step 4: jackin-env dependency + `OperatorEnvError`

Add `thiserror = { workspace = true }` to `crates/jackin-env/Cargo.toml` (match dep-line style). In `resolve.rs`, define one enum covering the measured kinds (names indicative — read each site and make variants carry what the message interpolates):

```rust
#[derive(Debug, thiserror::Error)]
pub enum OperatorEnvError {
    #[error("env name {name:?} is reserved by the runtime")]           ReservedName { name: String },
    #[error("not an op:// reference: {value}")]                        NotOpRef { value: String },
    #[error("op:// references do not support shell-variable substitution: {value}")] ShellVarInRef { value: String },
    #[error("malformed op:// reference {value:?}: {detail}")]          MalformedRef { value: String, detail: String },
    #[error("item {item:?} not found in vault {vault:?}")]             ItemNotFound { item: String, vault: String },
    #[error("multiple items named {item:?} in vault {vault:?}; disambiguate")] AmbiguousItem { item: String, vault: String },
    #[error("operator env resolution aborted: {source}")]              Aborted { #[source] source: anyhow::Error },
    #[error("operator env resolution failed for {count} var(s): {summary}")] Aggregated { count: usize, summary: String },
}
```

**CRITICAL — message parity**: before writing each variant, read the live `bail!`/`anyhow!` at the cited line and copy its exact wording into the `#[error]` string (the shapes above are from the census, not verbatim). The `Aborted` variant wrapping `anyhow::Error` is legitimate: the per-var inner failure comes from the `OpRunner` port (anyhow by design). Convert the `pub` fns in `resolve.rs` whose only failures are these kinds to `Result<_, OperatorEnvError>`; fns whose failures mix port errors keep `anyhow::Result` but construct typed variants as the **source** (the plan-031 idiom: `anyhow::Error::new(OperatorEnvError::…)`), so classification survives `downcast_ref`. Read each of the 12 pub fns and decide per-fn; record the split in the commit body.

**Verify**: `cargo check --workspace --all-targets --locked` → exit 0. `cargo nextest run -p jackin-env` → pass (resolve tests assert message text; parity failures surface here — fix the variant string, not the test).

### Step 5: `env_resolver.rs` + re-exports

`bail!("env var {name}: required prompt cannot be skipped")` (`:148`) becomes a typed variant — either a small `ResolveEnvError` enum in `env_resolver.rs` (`PromptRequired { name } | Cycle(#[from] EnvCycleError)`) or, if Step 4's enum fits naturally, reuse it; prefer the local enum (different concern than operator-env). Update `lib.rs` re-export blocks to export the new error types alongside their fns (match the existing grouped `pub use` style at `lib.rs:29-55`).

**Verify**: `cargo nextest run -p jackin-env` → pass. `cargo doc -p jackin-env --no-deps` → exit 0 (doc links resolve).

### Step 6: README rows + full gates

Both crates' README "Public API" sections gain the new error types. Full gates.

**Verify**: `cargo fmt && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo nextest run -p jackin-core -p jackin-env -p jackin-manifest -p jackin-runtime && cargo xtask ci --fast` → all exit 0.

## Test plan

- New tests (in the sibling `tests.rs` of each touched module): message-parity assertions for `ParseProfileError`, `EnvCycleError`, `PathsError::HomeDirUnresolved`; one `OperatorEnvError` test per variant family asserting the rendered message matches the pre-conversion string (write these FIRST against the current strings — red/green proves parity); a `downcast_ref::<OperatorEnvError>` test for one typed-source-inside-anyhow site.
- Pattern exemplar: existing `resolve/tests.rs` and `env_resolver/tests.rs` test shapes in jackin-env.
- Regression net: jackin-manifest + jackin-runtime crate tests (their launch/validate paths consume the converted fns).

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -n "impl std::fmt::Display for ParseProfileError\|impl std::error::Error for ParseProfileError" crates/jackin-core/src/docker_security.rs` → empty (derive replaced hand-rolls)
- [ ] `grep -n "anyhow" crates/jackin-core/src/env_model.rs crates/jackin-core/src/paths.rs` → empty
- [ ] `grep -n "thiserror" crates/jackin-env/Cargo.toml` → 1 hit
- [ ] `grep -c "thiserror::Error" crates/jackin-env/src/resolve.rs` ≥ 1
- [ ] `cargo check --workspace --all-targets --locked` exits 0 with zero downstream edits outside the in-scope list (`git diff --name-only`)
- [ ] `cargo nextest run -p jackin-core -p jackin-env -p jackin-manifest -p jackin-runtime` exits 0
- [ ] `cargo xtask ci --fast` exits 0
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

1. Cited lines don't match the excerpts (drift).
2. A downstream caller turns out to `.context()`/store/match one of the converted errors (census says none) — that caller's behavior is then load-bearing; report the site before changing its semantics.
3. `cargo check --workspace` demands edits in more than 3 files outside the in-scope list — the blast radius premise is wrong; report the file list.
4. Step 4's per-fn split leaves fewer than half of `resolve.rs`'s failure paths typed — the enum design doesn't fit the code's real structure; report with the actual failure-flow map instead of forcing it.
5. Any test fails on message text you believe you copied verbatim — diff the strings; if the OLD text was dynamic in a way thiserror can't express (e.g. embedded `{e}` chains), report rather than approximating.

## Maintenance notes

- This establishes the pattern for the remaining anyhow-saturated library crates (config 14 files, isolation 5, docker 3, image 7, instance 3 are the natural next tranche; runtime 39/console 35/capsule 22 are the long tail). Each conversion is now mechanical: enum per module, message parity tests first, `?`-absorption proof via `cargo check --workspace`.
- Port-trait signatures (`DockerApi`, `CommandRunner`, `OpRunner`, `EnvPrompter`, sinks) keeping `anyhow` is a **decision recorded here**: they are seams whose errors are reported, not handled. Revisit only with an AFIT/Send-story plan (roadmap Phase 2 item 5) since both changes break the same implementors.
- Reviewer scrutiny: message parity (operators and diagnostics consume these strings) and that no `unwrap`/`expect` crept into conversions (workspace-denied).
- Plan 031 interaction: if 031 lands after this, its `OpProbeError` slots into the same convention; nothing here blocks it.

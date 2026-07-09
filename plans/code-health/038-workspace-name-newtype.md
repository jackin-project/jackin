# Plan 038: `WorkspaceName` newtype — validated at construction, adopted at the config/instance/launch boundaries

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**: `git diff --stat 0971da66d..HEAD -- crates/jackin-core/src/ crates/jackin-config/src/ crates/jackin-instance/src/naming.rs crates/jackin-runtime/src/runtime/launch/launch_slot.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M-L
- **Risk**: MED (cross-crate signature changes; bounded by compiler-driven migration and a hard rule against touching persisted-serde shapes)
- **Depends on**: none
- **Category**: tech-debt (type-system discipline)
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

Roadmap Phase 2, type-system item 1 (`docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` line 149): replace bare primitives at contract boundaries with validated newtypes, naming `WorkspaceName` first. The census (2026-07-09) measured **117 bare workspace-name sites** across 9 crates, exactly **one** validation function (applied only at config-file load), and two launch functions whose adjacent `workspace: &str, role: &str` params are silently transposable — a transposition compiles and misreports auth errors (`launch_slot.rs:175-180`, `:345-353`). A `WorkspaceName` that can only be constructed validated makes "is this string a legal workspace?" a question `cargo check` answers, per the roadmap's "make the compiler the oracle" principle. This plan introduces the type in jackin-core and adopts it through the highest-value boundary chain: config mint → editor → instance naming → launch verification. The long tail (env resolve cluster, console TUI plumbing) converts later behind the established pattern.

## Current state

- **No newtype exists**: zero `struct WorkspaceName`/`SessionId`/`#[serde(transparent)]` anywhere in `crates/` (verified).
- **The exemplar to match** — `crates/jackin-core/src/selector.rs:21-87` (`RoleSelector`): derive `Debug, Clone, PartialEq, Eq`; fallible `parse(&str) -> Result<Self, SelectorError>` with a thiserror error; `Display` for the canonical string; private validation predicate; `TryFrom<&str>` wrapper. jackin-core's rule (its `AGENTS.md`): types + pure helpers only — a validation-carrying newtype belongs there.
- **The only existing validation** — `crates/jackin-config/src/persist.rs:18-44` `validate_workspace_file_stem(name: &str) -> anyhow::Result<()>`:

```rust
pub fn validate_workspace_file_stem(name: &str) -> anyhow::Result<()> {
    if name.is_empty() { anyhow::bail!("workspace name cannot be empty"); }
    if name == "." || name == ".." { anyhow::bail!("workspace name {name:?} is reserved"); }
    if name.contains('/') || name.contains('\\') { anyhow::bail!("workspace name {name:?} cannot contain path separators"); }
    #[cfg(windows)] { /* CON/PRN/COM1…/trailing dot-or-space rejections */ }
    Ok(())
}
```

  The name is the config-file stem — these rules ARE the domain invariant.
- **Mint boundary** — `crates/jackin-config/src/app_config/persist.rs`: `load_workspace_files` (`:55`) takes each `<name>.toml`'s `file_stem()` (`:78`), validates (`:81`), inserts into `pub workspaces: BTreeMap<String, WorkspaceConfig>` (`app_config.rs:61`) at `:88`. Other entry points: `editor.rs:550` `create_workspace(name: &str, …)`, `editor.rs:519` `rename_workspace(old: &str, new: &str)` (adjacent swappable pair), CLI clap fields (`jackin/src/cli/workspace.rs` etc.), console TUI input (`commit_workspace_name_input`).
- **Load-bearing signatures to convert** (from the census's top-10):
  - `crates/jackin-instance/src/naming.rs:19,23` — `new_container_name(workspace_name: Option<&str>, selector: &RoleSelector)` / `container_name_with_id(…)`.
  - `crates/jackin-config/src/app_config/roles.rs:30,79` — `resolve_mode(cfg, agent, workspace: &str, role: &str)` / `resolve_github_mode(…)`.
  - `crates/jackin-config/src/editor.rs:506,519,550` — `set_last_agent(workspace: &str, agent_key: &str)`, `rename_workspace`, `create_workspace`.
  - `crates/jackin-config/src/validation.rs:48` — `validate_workspace_config(name: &str, …)`.
  - `crates/jackin-runtime/src/runtime/launch/launch_slot.rs:175-180` (excerpt below) and `:345-353` — the swappable pairs:

```rust
pub(crate) fn verify_github_token_present(
    github_mode: jackin_config::GithubAuthMode,
    resolved_token: Option<&str>,
    workspace: &str,
    role: &str,
) -> anyhow::Result<()> {
```

- **Serde exposure — the hazard**: `InstanceIndexEntry.workspace_name: Option<String>` (`jackin-core/src/instance.rs:89`) and `InstanceManifest.workspace_name: Option<String>` (`jackin-instance/src/manifest.rs:89`) are **persisted JSON read back from disk**. Old manifests may hold values a validating deserializer would reject → a newtype there breaks reattach to existing containers. **These persisted fields stay `String` in this plan** (hard out-of-scope).
- Session ids are explicitly NOT in scope: the `u64` mux id is wire-format (protocol frames) and the `String` `SessionRecord.session_id` has no in-Rust mint site (populated from outside via manifest JSON) — both carry design questions this plan must not improvise on.
- 25-crate workspace; `unreachable_pub`/`dead_code` deny; tests in sibling `tests.rs`; Conventional Commits.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Compile-driven migration loop | `cargo check --workspace --all-targets --locked` | exit 0 when a tranche is complete |
| Affected-crate tests | `cargo nextest run -p jackin-core -p jackin-config -p jackin-instance -p jackin-runtime` | all pass |
| Full lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Frontier count (progress metric) | `grep -rn "workspace_name: Option<&str>\|workspace: &str" crates/ --include="*.rs" | grep -v tests | wc -l` | shrinks vs. the 117-site baseline |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify):
- `crates/jackin-core/src/workspace_name.rs` (create) + `workspace_name/tests.rs` (create) + `lib.rs` (module + re-export)
- `crates/jackin-config/src/persist.rs`, `app_config.rs`, `app_config/persist.rs`, `app_config/roles.rs`, `app_config/workspaces.rs`, `editor.rs`, `validation.rs` (+ sibling tests)
- `crates/jackin-instance/src/naming.rs` (+ tests)
- `crates/jackin-runtime/src/runtime/launch/launch_slot.rs` (+ call sites the compiler forces, expected in `launch_pipeline.rs`/`launch_core.rs`)
- Call-site conversions (`.as_str()` / `WorkspaceName::parse(…)?`) in `crates/jackin/`, `crates/jackin-console/`, `crates/jackin-env/` ONLY where the compiler forces them — minimal edits, no opportunistic conversion
- `crates/jackin-core/README.md`, `crates/jackin-config/README.md`, `crates/jackin-instance/README.md`

**Out of scope** (do NOT touch):
- `SessionRecord.session_id`, `InstanceManifest.workspace_name`, `InstanceIndexEntry.workspace_name` and every other **persisted serde field** — they stay `String` (see hazard above). Conversion happens at the read boundary via `.as_str()`/parse where needed.
- The protocol crate and the `u64` session ids — wire format.
- `BTreeMap<String, WorkspaceConfig>` key type — **decision**: the map key stays `String` this wave (TOML round-trip + `Borrow<str>` lookups everywhere); the newtype guards the *entrances* to that map (`create_workspace`/`rename_workspace`/`load_workspace_files` mint a `WorkspaceName`, then store `.into_inner()`). Converting the key type is the natural follow-up once entrances are typed.
- The env `resolve.rs` 12-fn cluster and console TUI state — recorded follow-up tranche.

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `refactor/workspace-name-newtype` and wait for confirmation.
- Conventional Commits, signed, push per tranche: `feat(core): WorkspaceName newtype with construction validation`, `refactor(config): mint WorkspaceName at load/editor boundaries`, `refactor(runtime): typed workspace param in launch_slot verification`.

## Steps

### Step 1: The type

`crates/jackin-core/src/workspace_name.rs`, modeled on `RoleSelector`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceName(String);

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceNameError {
    #[error("workspace name cannot be empty")]                                Empty,
    #[error("workspace name {0:?} is reserved")]                              Reserved(String),
    #[error("workspace name {0:?} cannot contain path separators")]           PathSeparator(String),
    #[cfg(windows)]
    #[error("workspace name {0:?} is reserved on Windows")]                   WindowsReserved(String),
    #[cfg(windows)]
    #[error("workspace name {0:?} cannot end with a dot or space on Windows")] WindowsTrailing(String),
}

impl WorkspaceName {
    pub fn parse(input: &str) -> Result<Self, WorkspaceNameError> { /* the persist.rs:18-44 rules, moved */ }
    pub fn as_str(&self) -> &str { … }
    pub fn into_inner(self) -> String { … }
}
impl std::fmt::Display for WorkspaceName { … }
impl std::borrow::Borrow<str> for WorkspaceName { … }
impl TryFrom<&str> for WorkspaceName { type Error = WorkspaceNameError; … }
```

Move the validation LOGIC from `validate_workspace_file_stem` verbatim, including the `#[cfg(windows)]` arm and message wording. No serde impls (nothing persisted carries the type this wave). Register the module + `pub use workspace_name::{WorkspaceName, WorkspaceNameError};` in `lib.rs` matching existing style. Port the existing validation tests (find them near `persist.rs`'s tests) into `workspace_name/tests.rs` and extend: empty, `.`, `..`, `/`, `\`, valid names, `Display` round-trip, `Borrow<str>` map-lookup compile check.

**Verify**: `cargo nextest run -p jackin-core workspace_name` → all pass.

### Step 2: Config mints it

- `persist.rs`: `validate_workspace_file_stem` becomes a thin delegate — `WorkspaceName::parse(name).map(drop).map_err(Into::into)` (keep the fn and its anyhow signature; its callers don't all need the type). Message parity is inherited from Step 1.
- `app_config/persist.rs:78-88`: `load_workspace_files` parses `WorkspaceName` from the stem (replacing the bare validate call) and inserts with `name.into_inner()`.
- `editor.rs`: `create_workspace(name: &WorkspaceName, …)`, `rename_workspace(old: &WorkspaceName, new: &WorkspaceName)`, `set_last_agent(workspace: &WorkspaceName, agent_key: &str)`; `validation.rs:48` `validate_workspace_config(name: &WorkspaceName, …)`; `app_config/roles.rs:30,79` `workspace: &WorkspaceName`. Let `cargo check --workspace` drive every caller: TUI/CLI entry points parse once at their boundary (`WorkspaceName::parse(&input)?` where user input arrives; `.map_err(anyhow::Error::from)?` where the fn is anyhow) and pass the typed value down; map lookups use `Borrow<str>` (`workspaces.get(name.as_str())` also fine).

**Verify**: `cargo check --workspace --all-targets --locked` → exit 0; `cargo nextest run -p jackin-config -p jackin-console -p jackin` → pass.

### Step 3: Instance naming + launch_slot

- `naming.rs:19,23`: `workspace_name: Option<&WorkspaceName>`. The body's `compact_component` sanitization keeps operating on `.as_str()` (container names have their own DNS rules — the newtype does NOT absorb them).
- `launch_slot.rs:175-180` and `:345-353`: `workspace: &WorkspaceName, role: &str` — the pair is no longer transposable (different types). Fix the forced call sites in the launch pipeline; where the pipeline only has a `&str` from config-map iteration, parse is NOT allowed to fail silently — the value came from the validated map, so `WorkspaceName::parse(...).expect(…)` is forbidden (workspace denies expect); instead thread the typed value from where Step 2 minted it. If threading requires touching more than ~6 pipeline files, STOP (condition 3).

**Verify**: `cargo check --workspace --all-targets --locked` → exit 0; `cargo nextest run -p jackin-instance -p jackin-runtime` → pass.

### Step 4: Frontier count + READMEs + gates

Record the new frontier: run the frontier-count grep from the Commands table; it must be **strictly below** the 117 baseline (expect roughly 60-75 remaining, concentrated in jackin-env/jackin-console — those are the recorded next tranche). Put the number in the commit body and in the plans/README status note. Update the three crate READMEs (new module row + Public API entry). Full gates.

**Verify**: `cargo fmt && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo xtask ci --fast` → all exit 0.

## Test plan

- `workspace_name/tests.rs`: the validation matrix (5 reject cases + valid + Display + TryFrom), ported message-parity assertions from the old `validate_workspace_file_stem` tests.
- Regression net: jackin-config's persist/editor tests (rename/create paths), jackin-instance naming tests, jackin-runtime launch tests, `cargo nextest run` on jackin/jackin-console for the forced boundary parses.
- One new test in `launch_slot`'s tests: constructing the auth-error path with a typed workspace and asserting the error names the right workspace (locks in the transposition fix's value).

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -n "pub struct WorkspaceName" crates/jackin-core/src/workspace_name.rs` → 1 hit; re-exported from `lib.rs`
- [ ] `grep -n "workspace: &str" crates/jackin-runtime/src/runtime/launch/launch_slot.rs` → empty
- [ ] `grep -rn "fn validate_workspace_file_stem" crates/jackin-config/src/persist.rs` still exists (delegate) and contains no duplicated rule logic (`grep -c "bail!" `on it → ≤1)
- [ ] Persisted-serde untouched: `git diff --name-only` does NOT include `crates/jackin-core/src/instance.rs` or `crates/jackin-instance/src/manifest.rs` (unless purely a call-site `.as_str()` — then `git diff` on them shows no `struct`/field changes)
- [ ] Frontier grep count < 117 and recorded in the plans/README status row
- [ ] `cargo nextest run -p jackin-core -p jackin-config -p jackin-instance -p jackin-runtime -p jackin -p jackin-console` exits 0
- [ ] `cargo xtask ci --fast` exits 0
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

1. Excerpts drifted (especially `persist.rs:18-44` — if validation rules changed, the newtype must carry the NEW rules; reconcile first).
2. Any persisted-serde struct would need the type to make a tranche compile — the boundary design is then wrong; report the chain.
3. Step 3's threading forces edits in more than ~6 launch-pipeline files — the pipeline may need plan-033's characterization + decomposition first; ship Steps 1-2 alone and report.
4. A `WorkspaceName::parse` failure path appears at a boundary where failure has no clean surface (e.g. deep in TUI render) — do not `unwrap`/`expect`; report the site so the boundary can be redesigned.
5. Windows-reserved behavior can't be test-verified on Linux CI (`#[cfg(windows)]` tests don't run) — keep the cfg'd logic verbatim-moved and note it; do NOT "fix" it blind.

## Maintenance notes

- Next tranches behind this pattern (recorded, not planned): the jackin-env `resolve.rs` cluster (12 fns, 25 sites), console TUI state, then the `BTreeMap` key type itself; later candidates from the roadmap list: `RoleRef`, `ContainerId`, `MountPath`. SessionId needs a design decision first (u64 wire form vs String record form — see census note in plans/README).
- Reviewer scrutiny: no `expect`/`unwrap` at parse boundaries; message parity on the error enum; that `naming.rs` sanitization still applies (the newtype validates file-stem legality, NOT DNS-label legality — two different invariants, deliberately).
- Future agents: mint `WorkspaceName` at input boundaries (CLI parse, TUI commit, config load); functions below the boundary take `&WorkspaceName`. Never re-validate below the boundary.

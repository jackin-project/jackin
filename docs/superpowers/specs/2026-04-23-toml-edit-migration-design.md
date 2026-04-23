# `toml_edit` Migration — Comment-Preserving Config Writes

**Status:** Proposed
**Date:** 2026-04-23
**Scope:** `jackin` crate only
**PR:** 1 of 3 in the launcher-workspace-manager series

## Problem

Every programmatic save of `~/.config/jackin/config.toml` today discards the user's hand-written comments, blank lines, and key ordering. `AppConfig::save()` at `src/config/persist.rs:46` serializes the whole struct through `toml::to_string_pretty(self)`, which cannot round-trip anything that isn't modeled by the serde schema.

This bites anyone who hand-annotates their config. A user who writes

```toml
[env]
# Production token; rotate quarterly (last: 2026-Q1)
API_TOKEN = "op://Personal/api/token"
```

and then runs any command that triggers a save (trust toggle, builtin agent sync, deprecated `auth_forward = "copy"` migration, last-used-agent update) loses the comment silently.

The upcoming launcher-workspace-manager work (specs 2 and 3 of this series) adds a secrets screen that annotates ID-form 1Password references with their human-readable name as a mid-document comment. That feature cannot ship safely on top of the current writer: the first unrelated save would erase the annotations. The writer must preserve mid-document comments before we can build anything that emits them.

## Goals

1. Every programmatic save of `config.toml` preserves hand-written comments, blank lines, and key ordering in sections the write does not touch.
2. The read path and `AppConfig` schema stay unchanged; no migration for existing config files.
3. Write sites go through a single new module (`src/config/editor.rs`) that exposes typed setters for each mutation the app actually performs. No public "blind re-serialize" path survives.
4. Atomic-write semantics match today's: `.tmp` + fsync + rename, `0o600` on unix.
5. Round-trip behavior is pinned by tests: an `open → save` with no mutations produces a byte-identical file.

## Non-Goals

- New TUI or CLI surface. This spec is pure plumbing; the UX work lives in specs 2 and 3.
- Any schema change to `AppConfig` or its sub-structs. Field names, defaults, serde derives all stay.
- Multi-process write safety / file locking. The current code does not coordinate concurrent writers; adding that is a separate concern.
- Secret-at-rest encryption, config validation expansion, or new key types. Out of scope.
- Touching `CHANGELOG.md`. The operator curates the changelog manually; this spec adds no changelog entry.

## Design

### Read vs. write split

Reads stay in `AppConfig::load_or_init` (unchanged — `toml` + serde into the typed struct). Writes go through a new `ConfigEditor` that owns a `toml_edit::DocumentMut` — a comment-preserving editable representation of the TOML source. The struct is the app's read model; the document is the on-disk model. They are kept consistent only across a save boundary, not continuously.

### `ConfigEditor` API surface

```rust
// src/config/editor.rs

pub struct ConfigEditor {
    doc: toml_edit::DocumentMut,
    path: PathBuf,
}

pub enum EnvScope {
    Global,
    Agent(String),
    Workspace(String),
    WorkspaceAgent { workspace: String, agent: String },
}

impl ConfigEditor {
    /// Loads the existing config file as a `DocumentMut`. If the file
    /// does not exist, delegates to `AppConfig::load_or_init` to
    /// materialize defaults, then reopens the resulting file.
    pub fn open(paths: &JackinPaths) -> anyhow::Result<Self>;

    /// Writes the mutated document atomically. Returns a freshly-loaded
    /// `AppConfig` so callers that still need the in-memory shape get
    /// it without a second manual `load_or_init`.
    pub fn save(self) -> anyhow::Result<AppConfig>;

    // — Mutations required by today's 13 write sites —
    pub fn add_mount(&mut self, scope: MountScope, name: &str, mount: MountConfig);
    pub fn remove_mount(&mut self, scope: MountScope, name: &str) -> bool;
    pub fn set_agent_trust(&mut self, agent_key: &str, trusted: bool);
    pub fn set_agent_auth_forward(&mut self, agent_key: &str, mode: AuthForwardMode);
    pub fn set_global_auth_forward(&mut self, mode: AuthForwardMode);
    pub fn upsert_builtin_agent(&mut self, agent_key: &str, source: AgentSource);
    pub fn set_last_agent(&mut self, workspace: &str, agent_key: &str);
    pub fn add_workspace(&mut self, name: &str, ws: WorkspaceConfig) -> anyhow::Result<()>;
    pub fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()>;
    pub fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()>;

    // — Forward-looking, consumed by spec #3 (secrets screen) —
    pub fn set_env_var(&mut self, scope: EnvScope, key: &str, value: &str);
    pub fn remove_env_var(&mut self, scope: EnvScope, key: &str) -> bool;
    /// Sets a `# comment` line immediately above the env var entry.
    /// `None` removes just that annotation line, leaving the key intact.
    pub fn set_env_comment(&mut self, scope: EnvScope, key: &str, comment: Option<&str>);
}
```

All mutators that target nested tables (`agents.X.claude`, `workspaces.X.agents.Y.env`, etc.) create intermediate tables as needed via `doc.entry(...).or_insert(toml_edit::table())`. A mutation on an absent path is not an error — it's an upsert.

The env-related methods (`set_env_var`, `remove_env_var`, `set_env_comment`) are included in this spec's API surface even though nothing outside the secrets work will call them. They ship with this migration so spec #3 builds on a stable, tested editor; we do not want to re-open `editor.rs` every time a new mutation category lands.

### `AppConfig` mutator methods become internal

Methods like `AppConfig::add_mount`, `trust_agent`, `untrust_agent`, `set_agent_auth_forward`, `add_workspace`, `edit_workspace`, `remove_workspace` are moved behind the editor module (or deleted outright when the editor has an equivalent). Nothing outside `src/config/` calls them today that cannot instead call the corresponding editor method.

Result: the rest of the app can read `AppConfig` but cannot mutate it in-place. The only way to change persisted state is through `ConfigEditor::open → mutate → save`. This eliminates the current footgun where a caller mutates the struct but forgets to `.save()`.

### Call site migration

Every existing `config.X(...); config.save(&paths)?;` pair becomes:

```rust
let mut editor = ConfigEditor::open(&paths)?;
editor.X(...);
config = editor.save()?;   // if the caller still reads AppConfig after
// or just `editor.save()?;` when it does not
```

There are 13 such pairs:

| File | Line | Mutation |
|---|---|---|
| `src/app/mod.rs` | 210 | add mount |
| `src/app/mod.rs` | 216 | remove mount |
| `src/app/mod.rs` | 269 | trust agent |
| `src/app/mod.rs` | 282 | untrust agent |
| `src/app/mod.rs` | 318 | set per-agent auth_forward |
| `src/app/mod.rs` | 322 | set global auth_forward |
| `src/app/mod.rs` | 386 | add workspace |
| `src/app/mod.rs` | 655 | edit workspace |
| `src/app/mod.rs` | 705 | edit workspace (remove destinations) |
| `src/app/mod.rs` | 714 | remove workspace |
| `src/app/context.rs` | 327 | set last-used agent |
| `src/config/persist.rs` | 34 | builtin sync + deprecated-copy migration |
| `src/runtime/launch.rs` | 600 | upsert newly-trusted or newly-registered agent |

The migration lands as one atomic PR — leaving half the project on the old writer and half on the new one creates two sources of truth for on-disk state.

### Load-time migrations

`AppConfig::load_or_init` currently runs builtin sync and deprecated-`copy`-to-`sync` migration in-memory, then calls `config.save()` if anything changed. Under the new model this becomes: load via serde, compute the needed changes, open a `ConfigEditor`, apply the changes as targeted patches, save. The user-visible behavior (single deprecation notice on first run after upgrade) is identical.

### Fate of `src/config/persist.rs`

- `AppConfig::save()` is **removed**. Its atomic-write logic (`.tmp` + fsync + rename, `0o600` on unix) moves into `ConfigEditor::save`. There is no public "serialize the whole struct" method after this migration.
- `AppConfig::load_or_init` **stays** in `persist.rs`, unchanged in signature. Its internal migration branch switches to using `ConfigEditor` instead of `self.save()`.
- The `contains_deprecated_copy_auth_forward` helper and the module layout otherwise stay put.

### Error handling

- `ConfigEditor::open` on a missing file: creates defaults via `load_or_init`, reopens the resulting file. First-run UX is unchanged.
- `ConfigEditor::open` on an unparseable file: returns the `toml_edit` parse error with source line/column.
- Mutators on absent nested paths: create intermediate tables silently.
- `ConfigEditor::save` failure: the `.tmp` file is cleaned up; the original `config.toml` is untouched because rename happens last.
- Atomic-write parity: `.tmp` + `sync_all` + `rename`, `0o600` on unix — lifted from today's `persist.rs::save`.

## Testing

New tests co-located in `src/config/editor.rs`:

1. **Idempotent save (tripwire).** `open → save` with no mutations produces a byte-identical file. If this ever fails, `toml_edit` is round-tripping lossily and something in our usage is wrong.
2. **Preserves comments above mutated sibling.** Load doc with `# note` above `FOO = "x"` → `set_env_var(same scope, "BAR", "y")` → `# note` is still attached to `FOO`.
3. **Preserves comments in untouched tables.** Mutation in `[workspaces.ws-a]` does not disturb comments in `[workspaces.ws-b]`.
4. **Preserves blank lines and key ordering.** Diff of the surrounding bytes is empty for untouched regions.
5. **`set_env_comment` contract.** Adding `Some("foo")` places `# foo` on the line above the key; passing `Some("bar")` replaces the previous annotation; passing `None` removes just the annotation line and leaves the key alone.
6. **Upsert creates intermediate tables.** `set_env_var(EnvScope::WorkspaceAgent { workspace: "new-ws", agent: "new-a" }, "K", "v")` on a doc that has neither the workspace nor the agent entry creates both tables and writes the key.
7. **Atomic-write parity.** On unix, the produced file is `0o600`; the `.tmp` path does not outlive a successful save; a simulated rename failure leaves the original file intact.
8. **Load-then-save on a real fixture.** A checked-in `config.fixture.toml` with mixed comments, blank lines, quoted keys, and nested tables survives an `open → no-op → save` byte-for-byte.

Existing tests that assert on post-save file contents via `toml::to_string_pretty` output shape need to be rewritten against `toml_edit`'s output. The shape is stable but not identical (minor whitespace differences around table headers, for example).

## Rollout

- Add `toml_edit` dependency (latest `0.22.x` at time of writing — check Cargo.lock for the actual chosen version at implementation time). `toml` stays as a read-side dep; removing it is not in scope.
- Land as one PR off `main`. CI runs the full test suite; the idempotent-save test is the smoke test that the migration did not regress anything.
- No operator-facing change. A user upgrading from the old jackin to the new jackin sees no difference unless they have hand-written comments in their config — in which case those comments now persist across saves. No migration notice needed.
- Rollback plan: revert the single migration PR. The read path is untouched, so an older binary reads the newer-format file (same TOML, same schema) without issue.

## Open questions

None. The A/B decision on write model (targeted patch vs. full-reserialize-with-preserved-header) was settled during brainstorming in favor of targeted patches; the A/B decision on comment storage (TOML comments vs. structured sibling field vs. no storage) was settled in favor of TOML comments.

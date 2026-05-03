# toml_edit Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace jackin's full-reserialize config write path with a targeted `toml_edit` patch path so hand-written comments, blank lines, and key ordering survive every programmatic save.

**Architecture:** A new `src/config/editor.rs` module owns a `toml_edit::DocumentMut` and exposes typed setters for each mutation the app performs. Reads stay in `AppConfig::load_or_init` (serde + `toml`); writes go through `ConfigEditor::open → mutate → save`. `AppConfig::save()` is removed; the in-memory mutator methods on `AppConfig` are deleted or made `pub(super)` so nothing outside `src/config/` can change persisted state except through the editor.

**Tech Stack:** Rust, `toml_edit` (new dep, latest 0.22.x), `toml` (kept for the read path), `serde`, `anyhow`, `tempfile` (test-only, already in the tree).

**Spec reference:** `docs/superpowers/specs/2026-04-23-toml-edit-migration-design.md` (PR #160, merging separately).

---

## Branching & commits

All work happens on `feature/toml-edit-migration` (already created off `main`). Every commit:
- Uses Conventional Commits (`feat(config): …`, `refactor(config): …`, `test(config): …`).
- Includes `Signed-off-by:` via `git commit -s` (DCO required by repo).
- Includes `Co-authored-by: Claude <noreply@anthropic.com>` as the single agent trailer.
- Does **not** touch `CHANGELOG.md` (operator curates manually).

Verify trailers with `git log -1 --format="%B"` after each commit.

---

## File Structure

New files:
- `src/config/editor.rs` — `ConfigEditor` struct, `EnvScope` enum, all typed setters, co-located `#[cfg(test)] mod tests`.
- `src/config/fixtures/config.round_trip.toml` — fixture with mixed comments, blank lines, quoted keys, and nested tables for the byte-for-byte round-trip test.

Modified files (roughly in order of task sequencing):
- `Cargo.toml` — add `toml_edit = "0.22"`.
- `src/config/mod.rs` — add `pub mod editor;`, re-export `ConfigEditor` and `EnvScope`.
- `src/config/persist.rs` — delete `AppConfig::save`; migrate the `load_or_init` save branch to use `ConfigEditor`.
- `src/config/agents.rs` — delete `AppConfig::trust_agent`, `untrust_agent`, `set_agent_auth_forward`; keep `sync_builtin_agents` (called from load path but the save uses the editor now).
- `src/config/mounts.rs` — delete `AppConfig::add_mount`, `remove_mount`.
- `src/config/workspaces.rs` — delete `AppConfig::create_workspace`, `edit_workspace`, `remove_workspace`.
- `src/app/mod.rs` — migrate 10 call sites (lines 210, 216, 269, 282, 318, 322, 386, 655, 705, 714).
- `src/app/context.rs` — migrate 1 call site (line 327 area, `remember_last_agent`).
- `src/runtime/launch.rs` — migrate 1 call site (line 600 area).

Unchanged (verified):
- `src/config/mod.rs` — all structs (`AppConfig`, `AgentSource`, `AuthForwardMode`, etc.) keep the same serde shape.
- `src/workspace/mod.rs` — `WorkspaceConfig`, `WorkspaceAgentOverride`, `WorkspaceEdit` unchanged.
- `src/paths.rs` — `JackinPaths` unchanged.

---

## Design notes for implementers

### `toml_edit` 0.22 API cheat-sheet

```rust
use toml_edit::{DocumentMut, Item, Table, Value, value};

// Parse
let mut doc: DocumentMut = raw.parse()?;

// Navigate / upsert a table:
let table = doc["workspaces"]
    .or_insert(Item::Table(Table::new()))
    .as_table_mut()
    .expect("`workspaces` must be a table");

// Insert/update a scalar value:
table.insert("KEY", value("some string"));

// Remove a key:
table.remove("KEY");

// Set a "# comment\n" line above a key (leaf decor of the Key, not the Value):
if let Some(mut key) = table.key_mut("KEY") {
    key.leaf_decor_mut().set_prefix("# comment\n");
}

// Serialize back:
let out = doc.to_string();
```

### Nested-path helper

Most setters need to upsert a table path like `workspaces.<name>.agents.<agent>.env`. Implement a private helper:

```rust
fn table_path_mut<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    let mut current: &mut Item = doc.as_item_mut();
    for segment in path {
        current = current
            .as_table_mut()
            .expect("expected table segment")
            .entry(segment)
            .or_insert(Item::Table(Table::new()));
    }
    current.as_table_mut().expect("final path segment is a table")
}
```

Use it as `table_path_mut(&mut self.doc, &["workspaces", name, "agents", agent, "env"])`.

### Atomic write

Lift the body of today's `AppConfig::save()` (see `src/config/persist.rs:46–69`) into `ConfigEditor::save()` verbatim — same `.tmp` + fsync + rename, same `0o600` on unix. Do **not** switch to the `NamedTempFile` pattern from `src/instance/auth.rs`; the spec pins atomic-write parity with today's behavior, and behavioral differences between the two patterns (symlink handling) are out of scope.

### `EnvScope` → TOML path

| Scope | TOML table path |
|---|---|
| `EnvScope::Global` | `["env"]` |
| `EnvScope::Agent("agent-smith")` | `["agents", "agent-smith", "env"]` |
| `EnvScope::Workspace("ws")` | `["workspaces", "ws", "env"]` |
| `EnvScope::WorkspaceAgent { workspace: "ws", agent: "a" }` | `["workspaces", "ws", "agents", "a", "env"]` |

Private helper:

```rust
fn env_scope_path<'a>(scope: &'a EnvScope) -> Vec<&'a str> {
    match scope {
        EnvScope::Global => vec!["env"],
        EnvScope::Agent(a) => vec!["agents", a.as_str(), "env"],
        EnvScope::Workspace(w) => vec!["workspaces", w.as_str(), "env"],
        EnvScope::WorkspaceAgent { workspace, agent } => {
            vec!["workspaces", workspace.as_str(), "agents", agent.as_str(), "env"]
        }
    }
}
```

---

## Task 1: Foundation — dependency, module skeleton, tripwire test

**Files:**
- Modify: `Cargo.toml`
- Create: `src/config/editor.rs`
- Modify: `src/config/mod.rs` (add module declaration + re-exports)

- [ ] **Step 1: Add `toml_edit` to Cargo.toml**

Edit `Cargo.toml`. Find the block around line 22–27 with `toml = "1.1"`. Insert `toml_edit` (latest 0.22 minor/patch — verify on crates.io at implementation time):

```toml
toml = "1.1"
toml_edit = "0.22"
```

Run `cargo check` to confirm the dep resolves. Expected: success, new `toml_edit` entry in `Cargo.lock`.

- [ ] **Step 2: Create the module skeleton**

Create `src/config/editor.rs`:

```rust
//! Comment-preserving config writer.
//!
//! Reads still go through `AppConfig::load_or_init` (serde + `toml`).
//! Writes go through `ConfigEditor::open → mutate → save`, which keeps
//! user-written comments, blank lines, and key ordering intact in
//! sections untouched by the mutation.

use std::path::PathBuf;

use anyhow::Context;
use toml_edit::{DocumentMut, Item, Table};

use crate::config::AppConfig;
use crate::paths::JackinPaths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvScope {
    Global,
    Agent(String),
    Workspace(String),
    WorkspaceAgent { workspace: String, agent: String },
}

pub struct ConfigEditor {
    doc: DocumentMut,
    path: PathBuf,
}

impl ConfigEditor {
    /// Loads the existing config file as a `DocumentMut`. If the file
    /// does not exist, delegates to `AppConfig::load_or_init` to
    /// materialize defaults, then reopens the resulting file.
    pub fn open(paths: &JackinPaths) -> anyhow::Result<Self> {
        if !paths.config_file.exists() {
            // Trigger the existing default-materialization path.
            AppConfig::load_or_init(paths)?;
        }
        let raw = std::fs::read_to_string(&paths.config_file)
            .with_context(|| format!("reading {}", paths.config_file.display()))?;
        let doc: DocumentMut = raw
            .parse()
            .with_context(|| format!("parsing {}", paths.config_file.display()))?;
        Ok(Self {
            doc,
            path: paths.config_file.clone(),
        })
    }

    /// Writes the mutated document atomically. Returns a freshly-loaded
    /// `AppConfig` so callers that still need the in-memory shape get
    /// it without a second manual `load_or_init`.
    pub fn save(self) -> anyhow::Result<AppConfig> {
        let contents = self.doc.to_string();
        let tmp = self.path.with_extension("tmp");

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
        }

        #[cfg(not(unix))]
        std::fs::write(&tmp, &contents)?;

        std::fs::rename(&tmp, &self.path)?;

        // Reload from disk so callers that need the in-memory struct
        // get a fresh, fully-validated view (validate_workspaces,
        // validate_reserved_names all re-run).
        let paths = synthesize_paths(&self.path);
        AppConfig::load_or_init(&paths)
    }
}

/// Rebuild the `JackinPaths` bundle from just the config file path.
/// Used by `ConfigEditor::save` so we can call `load_or_init` without
/// threading a `JackinPaths` through the editor.
fn synthesize_paths(config_file: &std::path::Path) -> JackinPaths {
    // The only field `load_or_init` reads from JackinPaths is
    // config_file and ensure_base_dirs (which reads config_dir).
    // config_dir is the parent of config_file.
    let config_dir = config_file
        .parent()
        .expect("config file has no parent")
        .to_path_buf();
    let home_dir = config_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| config_dir.clone());
    JackinPaths {
        home_dir: home_dir.clone(),
        config_dir: config_dir.clone(),
        config_file: config_file.to_path_buf(),
        agents_dir: home_dir.join("agents"),
        data_dir: home_dir.join("data"),
        cache_dir: home_dir.join("cache"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn idempotent_save_is_byte_identical() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let original = r#"# Top-of-file note about this config
[claude]
auth_forward = "sync"

# Agents we trust
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

# My production workspace
[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.env]
# Rotate quarterly (last: 2026-Q1)
API_TOKEN = "op://Personal/api/token"
"#;
        std::fs::write(&paths.config_file, original).unwrap();

        let editor = ConfigEditor::open(&paths).unwrap();
        editor.save().unwrap();

        let round_tripped = std::fs::read_to_string(&paths.config_file).unwrap();
        assert_eq!(round_tripped, original, "open → save must be byte-identical");
    }
}
```

- [ ] **Step 3: Wire the module into `src/config/mod.rs`**

Near the other `pub mod` declarations in `src/config/mod.rs`, add:

```rust
pub mod editor;
```

And near the other re-exports:

```rust
pub use editor::{ConfigEditor, EnvScope};
```

- [ ] **Step 4: Run the tripwire test**

Run: `cargo test -p jackin --lib config::editor::tests::idempotent_save_is_byte_identical`
Expected: PASS.

If the test fails, `toml_edit` is round-tripping lossily on some construct in the fixture. Narrow the fixture until it passes, then note what was lost in a follow-up investigation.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/config/editor.rs src/config/mod.rs
git commit -s -m "feat(config): scaffold ConfigEditor with toml_edit round-trip

Introduces src/config/editor.rs with a ConfigEditor struct that owns a
toml_edit::DocumentMut. ConfigEditor::open loads and parses, save writes
atomically (.tmp + fsync + rename, 0o600 on unix) and returns a
freshly-loaded AppConfig.

No mutators yet — subsequent commits add typed setters and migrate call
sites off AppConfig::save.

Tripwire test (idempotent_save_is_byte_identical) pins that an open →
save with no mutations produces byte-identical output, including
hand-written comments. If this ever fails, toml_edit is round-tripping
lossily.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 2: `set_env_var` (all four scopes)

**Files:**
- Modify: `src/config/editor.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/config/editor.rs`:

```rust
#[test]
fn set_env_var_creates_global_env_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_var(EnvScope::Global, "API_TOKEN", "op://Personal/api/token");
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[env]"), "missing [env] table: {out}");
    assert!(
        out.contains(r#"API_TOKEN = "op://Personal/api/token""#),
        "missing entry: {out}"
    );
}

#[test]
fn set_env_var_upserts_workspace_agent_scope() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.prod]
workdir = "/workspace/prod"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_var(
        EnvScope::WorkspaceAgent {
            workspace: "prod".to_string(),
            agent: "agent-smith".to_string(),
        },
        "OPENAI_API_KEY",
        "op://Work/OpenAI/default",
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        out.contains("[workspaces.prod.agents.agent-smith.env]"),
        "missing nested table: {out}"
    );
    assert!(
        out.contains(r#"OPENAI_API_KEY = "op://Work/OpenAI/default""#),
        "missing entry: {out}"
    );
}

#[test]
fn set_env_var_overwrites_existing_value() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
API_TOKEN = "old-value"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_var(EnvScope::Global, "API_TOKEN", "new-value");
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains(r#"API_TOKEN = "new-value""#), "{out}");
    assert!(!out.contains("old-value"), "{out}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p jackin --lib config::editor::tests::set_env_var`
Expected: FAIL (compile error, `set_env_var` not found).

- [ ] **Step 3: Implement the nested-path helper and `set_env_var`**

In `src/config/editor.rs`, inside `impl ConfigEditor`:

```rust
pub fn set_env_var(&mut self, scope: EnvScope, key: &str, value_str: &str) {
    let path = env_scope_path(&scope);
    let table = table_path_mut(&mut self.doc, &path);
    table.insert(key, toml_edit::value(value_str));
}
```

And at module scope, add the two private helpers:

```rust
fn env_scope_path(scope: &EnvScope) -> Vec<&str> {
    match scope {
        EnvScope::Global => vec!["env"],
        EnvScope::Agent(a) => vec!["agents", a.as_str(), "env"],
        EnvScope::Workspace(w) => vec!["workspaces", w.as_str(), "env"],
        EnvScope::WorkspaceAgent { workspace, agent } => {
            vec!["workspaces", workspace.as_str(), "agents", agent.as_str(), "env"]
        }
    }
}

fn table_path_mut<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    fn walk<'a>(item: &'a mut Item, path: &[&str]) -> &'a mut Table {
        let table = item.as_table_mut().expect("path segment is not a table");
        if path.is_empty() {
            return table;
        }
        let entry = table
            .entry(path[0])
            .or_insert(Item::Table(Table::new()));
        walk(entry, &path[1..])
    }
    walk(doc.as_item_mut(), path)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jackin --lib config::editor::tests`
Expected: PASS (all four tests: the original tripwire + three new ones).

- [ ] **Step 5: Commit**

```bash
git add src/config/editor.rs
git commit -s -m "feat(config): ConfigEditor::set_env_var with scope-table upsert

Adds set_env_var for all four EnvScope variants (Global, Agent,
Workspace, WorkspaceAgent). Intermediate tables are created as needed
via the private table_path_mut helper, so writing to
WorkspaceAgent { workspace: \"new\", agent: \"new\" } works even when
neither table exists.

Existing values are overwritten. Tests cover the upsert, cross-scope
path construction, and overwrite semantics.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 3: `remove_env_var`

**Files:**
- Modify: `src/config/editor.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:

```rust
#[test]
fn remove_env_var_returns_true_when_present() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
API_TOKEN = "x"
OTHER = "y"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_env_var(EnvScope::Global, "API_TOKEN");
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("API_TOKEN"), "{out}");
    assert!(out.contains(r#"OTHER = "y""#), "sibling gone: {out}");
}

#[test]
fn remove_env_var_returns_false_when_absent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_env_var(EnvScope::Global, "API_TOKEN");
    editor.save().unwrap();

    assert!(!removed);
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p jackin --lib config::editor::tests::remove_env_var`
Expected: FAIL (`remove_env_var` not defined).

- [ ] **Step 3: Implement**

In `impl ConfigEditor`:

```rust
pub fn remove_env_var(&mut self, scope: EnvScope, key: &str) -> bool {
    let path = env_scope_path(&scope);
    // Walk without creating: return false if any segment is missing.
    let mut current: &mut Item = self.doc.as_item_mut();
    for segment in &path {
        match current.as_table_mut().and_then(|t| t.get_mut(segment)) {
            Some(next) => current = next,
            None => return false,
        }
    }
    match current.as_table_mut() {
        Some(table) => table.remove(key).is_some(),
        None => false,
    }
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p jackin --lib config::editor::tests::remove_env_var`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git add src/config/editor.rs
git commit -s -m "feat(config): ConfigEditor::remove_env_var

Removes an env var from any scope, returning true if the key existed
and false if it did not (including when the scope's table is missing).
Does not create intermediate tables — removes-from-nothing is a no-op.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 4: `set_env_comment` — the feature this whole migration exists for

**Files:**
- Modify: `src/config/editor.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:

```rust
#[test]
fn set_env_comment_adds_line_above_key() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
API_TOKEN = "op://vault-id/item-id/field"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_comment(
        EnvScope::Global,
        "API_TOKEN",
        Some("op://Personal/Google/password"),
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        out.contains("# op://Personal/Google/password\nAPI_TOKEN"),
        "expected comment directly above key: {out}"
    );
}

#[test]
fn set_env_comment_replaces_existing_comment() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "[env]\n# old annotation\nAPI_TOKEN = \"x\"\n",
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_comment(EnvScope::Global, "API_TOKEN", Some("new annotation"));
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("# new annotation"), "{out}");
    assert!(!out.contains("# old annotation"), "{out}");
}

#[test]
fn set_env_comment_none_removes_annotation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "[env]\n# some note\nAPI_TOKEN = \"x\"\n",
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_comment(EnvScope::Global, "API_TOKEN", None);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("# some note"), "{out}");
    assert!(out.contains(r#"API_TOKEN = "x""#), "key still present: {out}");
}

#[test]
fn mutating_sibling_preserves_comment_above_other_key() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let original = "[env]\n# rotate quarterly\nAPI_TOKEN = \"x\"\nOTHER = \"y\"\n";
    std::fs::write(&paths.config_file, original).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_var(EnvScope::Global, "OTHER", "z");
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        out.contains("# rotate quarterly\nAPI_TOKEN = \"x\""),
        "sibling mutation wiped adjacent comment: {out}"
    );
    assert!(out.contains(r#"OTHER = "z""#), "{out}");
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p jackin --lib config::editor::tests::set_env_comment`
Expected: FAIL (not defined).
Also run the sibling test: `cargo test -p jackin --lib config::editor::tests::mutating_sibling_preserves_comment_above_other_key`
Expected: the sibling test should PASS even before `set_env_comment` exists, since `set_env_var`'s upsert uses `table.insert` which should preserve adjacent decor. If it fails, that's a `toml_edit` usage bug in Task 2 that must be fixed before proceeding.

- [ ] **Step 3: Implement `set_env_comment`**

In `impl ConfigEditor`:

```rust
pub fn set_env_comment(&mut self, scope: EnvScope, key: &str, comment: Option<&str>) {
    let path = env_scope_path(&scope);
    // Walk without creating — setting a comment on a nonexistent key
    // is a silent no-op (same contract as remove_env_var).
    let mut current: &mut Item = self.doc.as_item_mut();
    for segment in &path {
        match current.as_table_mut().and_then(|t| t.get_mut(segment)) {
            Some(next) => current = next,
            None => return,
        }
    }
    let Some(table) = current.as_table_mut() else {
        return;
    };
    let Some(mut key_mut) = table.key_mut(key) else {
        return;
    };
    let decor = key_mut.leaf_decor_mut();
    let prefix = match comment {
        Some(text) => format!("# {text}\n"),
        None => String::new(),
    };
    decor.set_prefix(prefix);
}
```

Note on `toml_edit` version: on 0.22+, the `Table::key_mut(key) -> Option<KeyMut<'_>>` and `KeyMut::leaf_decor_mut() -> &mut Decor` methods are stable. If the installed `toml_edit` predates this, check crates.io for the equivalent; the test suite tells you immediately if the method names changed.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p jackin --lib config::editor::tests::set_env_comment`
Expected: PASS (all three `set_env_comment_*` tests).

Also re-run: `cargo test -p jackin --lib config::editor::tests`
Expected: all seven tests PASS (1 tripwire + 3 set_env_var + 2 remove_env_var + 3 set_env_comment + 1 sibling-preserve = 10 tests actually).

- [ ] **Step 5: Commit**

```bash
git add src/config/editor.rs
git commit -s -m "feat(config): ConfigEditor::set_env_comment

Sets or removes a \"# comment\\n\" line immediately above an env var
entry via toml_edit's leaf_decor_mut on the key. The secrets screen
(PR 3 of this series) will use this to annotate ID-form op://
references with their human-readable name.

Tests cover: adding a new comment, replacing an existing one, removing
via None, and — critically — that mutating a sibling key does NOT
disturb the comment above an unrelated key in the same table.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 5: Cross-table and fixture round-trip tests

**Files:**
- Create: `src/config/fixtures/config.round_trip.toml`
- Modify: `src/config/editor.rs`

- [ ] **Step 1: Create the fixture**

Create `src/config/fixtures/config.round_trip.toml`:

```toml
# Jackin config — top-of-file note
# keep this across saves

[claude]
auth_forward = "sync"

# Our two builtin agents
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[roles.the-architect]
git = "https://github.com/jackin-project/jackin-the-architect.git"
trusted = true

# An external agent we vetted in 2025-Q4
[roles."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"
trusted = true

# Production workspace — the big one
[workspaces.prod]
workdir = "/workspace/prod"

[workspaces.prod.env]
# Rotate quarterly (last: 2026-Q1)
API_TOKEN = "op://Personal/api/token"

# Passes through from host
GITHUB_TOKEN = "$GITHUB_TOKEN"

[workspaces.prod.agents.agent-smith.env]
OPENAI_API_KEY = "op://Work/OpenAI/default"

# Playground workspace — disposable
[workspaces.playground]
workdir = "/tmp/playground"
```

- [ ] **Step 2: Write the cross-table and fixture tests**

Append to `src/config/editor.rs` `tests` module:

```rust
#[test]
fn mutating_one_workspace_preserves_comments_in_another() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let original = r#"# workspace a — keep this comment
[workspaces.a]
workdir = "/a"

# workspace b — also keep
[workspaces.b]
workdir = "/b"
"#;
    std::fs::write(&paths.config_file, original).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_env_var(EnvScope::Workspace("a".to_string()), "K", "v");
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("# workspace b — also keep"), "{out}");
    assert!(out.contains("# workspace a — keep this comment"), "{out}");
}

#[test]
fn fixture_round_trip_is_byte_identical() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    let original = include_str!("fixtures/config.round_trip.toml");
    std::fs::write(&paths.config_file, original).unwrap();

    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();

    let round_tripped = std::fs::read_to_string(&paths.config_file).unwrap();
    assert_eq!(
        round_tripped, original,
        "fixture round-trip is lossy — toml_edit is dropping something"
    );
}
```

- [ ] **Step 3: Run tests to verify pass**

Run: `cargo test -p jackin --lib config::editor::tests`
Expected: PASS on all tests including the two new ones. If the fixture test fails, the saved output will be visible in the assertion diff — examine which construct in the fixture is not round-tripping and either (a) simplify the fixture if it's something we don't actually use, or (b) investigate the `toml_edit` usage.

- [ ] **Step 4: Commit**

```bash
git add src/config/fixtures/config.round_trip.toml src/config/editor.rs
git commit -s -m "test(config): fixture round-trip + cross-table comment preservation

Adds a realistic fixture (mixed comments, quoted keys with slashes,
nested env tables) and asserts open → save is byte-identical. This is
the main smoke test that toml_edit's round-trip actually preserves
everything we care about in a real config.

Also adds a cross-table test: mutating workspaces.a's env does not
disturb comments attached to workspaces.b.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 6: Atomic write parity tests

**Files:**
- Modify: `src/config/editor.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:

```rust
#[test]
#[cfg(unix)]
fn saved_file_is_0600_on_unix() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nK = \"v\"\n").unwrap();

    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();

    let perms = std::fs::metadata(&paths.config_file).unwrap().permissions();
    assert_eq!(perms.mode() & 0o777, 0o600, "config file must be 0600");
}

#[test]
fn save_leaves_no_tmp_file_on_success() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nK = \"v\"\n").unwrap();

    let editor = ConfigEditor::open(&paths).unwrap();
    editor.save().unwrap();

    let tmp = paths.config_file.with_extension("tmp");
    assert!(!tmp.exists(), "expected .tmp to be renamed away");
}
```

- [ ] **Step 2: Run tests to verify pass**

Run: `cargo test -p jackin --lib config::editor::tests`
Expected: PASS — the atomic-write logic was copied from `persist.rs::save` in Task 1 and should already satisfy both properties. If either fails, inspect `ConfigEditor::save()` against `src/config/persist.rs:46–69`.

- [ ] **Step 3: Commit**

```bash
git add src/config/editor.rs
git commit -s -m "test(config): pin ConfigEditor atomic-write parity

Asserts the saved config is 0600 on unix and that the .tmp sidecar does
not outlive a successful save. Matches the invariants of the existing
AppConfig::save body that ConfigEditor::save lifted.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 7: Mount editor methods (`add_mount`, `remove_mount`)

**Context:** The existing `AppConfig::add_mount`/`remove_mount` signatures are `(&mut self, name: &str, mount: MountConfig, scope: Option<&str>)` and `(&mut self, name: &str, scope: Option<&str>) -> bool`. These live in `src/config/mounts.rs`. The TOML representation is in `[docker.mounts]`, and `MountEntry` is an untagged enum of either a single `MountConfig` or a scoped `BTreeMap<String, MountConfig>`.

Preserve exactly the same call shape on `ConfigEditor` so the call-site migrations are mechanical.

**Files:**
- Modify: `src/config/editor.rs`
- Read (reference): `src/config/mounts.rs` (to replicate scope routing semantics)

- [ ] **Step 1: Read the current implementation**

Read `src/config/mounts.rs` end-to-end. Note:
- How `add_mount` routes a mount into `MountEntry::Mount` vs `MountEntry::Scoped(BTreeMap)` based on `scope`.
- How `remove_mount` handles the same branching.
- What happens when scope is `None` and the existing entry is `Scoped` (or vice versa).

Replicate this branching in the editor. The goal is behavioral equivalence: a test that exercised `AppConfig::add_mount` should produce the same on-disk result via `ConfigEditor::add_mount`.

- [ ] **Step 2: Write tests mirroring the existing behavior**

Append to `src/config/editor.rs` `tests`. Cover the four combinations:

```rust
#[test]
fn add_mount_unscoped_creates_single_mount_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.add_mount(
        "shared-home",
        crate::workspace::MountConfig {
            src: "/home/user".to_string(),
            dst: "/workspace/home".to_string(),
            readonly: false,
        },
        None,
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[docker.mounts.shared-home]"), "{out}");
    assert!(out.contains(r#"src = "/home/user""#), "{out}");
}

#[test]
fn add_mount_scoped_creates_nested_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.add_mount(
        "creds",
        crate::workspace::MountConfig {
            src: "/run/secrets/x".to_string(),
            dst: "/secrets/x".to_string(),
            readonly: true,
        },
        Some("agent-smith"),
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[docker.mounts.creds.agent-smith]"), "{out}");
    assert!(out.contains(r#"src = "/run/secrets/x""#), "{out}");
    assert!(out.contains("readonly = true"), "{out}");
}

#[test]
fn remove_mount_unscoped_deletes_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[docker.mounts.shared-home]
src = "/home/user"
dst = "/workspace/home"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("shared-home", None);
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("shared-home"), "{out}");
}

#[test]
fn remove_mount_returns_false_for_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("nope", None);
    editor.save().unwrap();
    assert!(!removed);
}
```

- [ ] **Step 3: Run to verify fail**

Run: `cargo test -p jackin --lib config::editor::tests::add_mount`
Expected: FAIL (`add_mount` not defined on `ConfigEditor`).

- [ ] **Step 4: Implement**

Add to `impl ConfigEditor`. Use `toml_edit::ser::to_document` or hand-construct the mount table. Hand-construction is clearer:

```rust
pub fn add_mount(
    &mut self,
    name: &str,
    mount: crate::workspace::MountConfig,
    scope: Option<&str>,
) {
    // Walk to or create [docker.mounts.<name>]
    let mount_table = table_path_mut(&mut self.doc, &["docker", "mounts", name]);

    match scope {
        None => {
            // Single-mount entry: replace the table's contents with src/dst/readonly.
            mount_table.clear();
            mount_table.insert("src", toml_edit::value(mount.src));
            mount_table.insert("dst", toml_edit::value(mount.dst));
            if mount.readonly {
                mount_table.insert("readonly", toml_edit::value(true));
            }
        }
        Some(scope_key) => {
            // Scoped entry: [docker.mounts.<name>.<scope_key>]
            let scoped = table_path_mut(&mut self.doc, &["docker", "mounts", name, scope_key]);
            scoped.clear();
            scoped.insert("src", toml_edit::value(mount.src));
            scoped.insert("dst", toml_edit::value(mount.dst));
            if mount.readonly {
                scoped.insert("readonly", toml_edit::value(true));
            }
        }
    }
}

pub fn remove_mount(&mut self, name: &str, scope: Option<&str>) -> bool {
    // Walk to docker.mounts without creating.
    let Some(docker) = self.doc.get_mut("docker").and_then(|i| i.as_table_mut()) else {
        return false;
    };
    let Some(mounts) = docker.get_mut("mounts").and_then(|i| i.as_table_mut()) else {
        return false;
    };
    match scope {
        None => mounts.remove(name).is_some(),
        Some(scope_key) => {
            let Some(entry) = mounts.get_mut(name).and_then(|i| i.as_table_mut()) else {
                return false;
            };
            entry.remove(scope_key).is_some()
        }
    }
}
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p jackin --lib config::editor::tests`
Expected: PASS — all mount tests plus everything from prior tasks.

- [ ] **Step 6: Commit**

```bash
git add src/config/editor.rs
git commit -s -m "feat(config): ConfigEditor::add_mount, remove_mount

Preserves the existing (name, mount, scope: Option<&str>) call shape so
call-site migrations stay mechanical. Unscoped writes [docker.mounts.N];
scoped writes [docker.mounts.N.scope]. remove_mount returns whether the
entry existed.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 8: Agent editor methods (trust, auth_forward, upsert builtin)

**Files:**
- Modify: `src/config/editor.rs`

- [ ] **Step 1: Write the failing tests**

Append to `tests`:

```rust
#[test]
fn set_agent_trust_toggles_trusted_field() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.my-agent]
git = "https://example.com/a.git"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_agent_trust("my-agent", true);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("trusted = true"), "{out}");
}

#[test]
fn set_agent_trust_false_removes_field() {
    // The AppConfig schema uses #[serde(default, skip_serializing_if = "is_false")]
    // on `trusted`, so the canonical representation of false is absent.
    // Match that: setting trust=false removes the key if present.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.my-agent]
git = "x"
trusted = true
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_agent_trust("my-agent", false);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("trusted"), "{out}");
}

#[test]
fn set_agent_auth_forward_writes_claude_subtable() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.my-agent]
git = "x"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_agent_auth_forward("my-agent", crate::config::AuthForwardMode::Token);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[roles.my-agent.claude]"), "{out}");
    assert!(out.contains(r#"auth_forward = "token""#), "{out}");
}

#[test]
fn set_global_auth_forward_writes_root_claude_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_global_auth_forward(crate::config::AuthForwardMode::Sync);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[claude]"), "{out}");
    assert!(out.contains(r#"auth_forward = "sync""#), "{out}");
}

#[test]
fn upsert_builtin_agent_creates_entry_when_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_builtin_agent(
        "agent-smith",
        "https://github.com/jackin-project/jackin-agent-smith.git",
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[roles.agent-smith]"), "{out}");
    assert!(out.contains("trusted = true"), "{out}");
}

#[test]
fn upsert_builtin_agent_preserves_existing_claude_override() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "OLD-URL"
trusted = false

[roles.agent-smith.claude]
auth_forward = "token"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_builtin_agent(
        "agent-smith",
        "https://github.com/jackin-project/jackin-agent-smith.git",
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains(r#"git = "https://github.com/jackin-project/jackin-agent-smith.git""#), "{out}");
    assert!(out.contains("trusted = true"), "{out}");
    assert!(out.contains(r#"auth_forward = "token""#), "claude override wiped: {out}");
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p jackin --lib config::editor::tests::set_agent_trust`
Expected: FAIL (method not defined).

- [ ] **Step 3: Implement**

In `impl ConfigEditor`:

```rust
pub fn set_agent_trust(&mut self, agent_key: &str, trusted: bool) {
    let table = table_path_mut(&mut self.doc, &["agents", agent_key]);
    if trusted {
        table.insert("trusted", toml_edit::value(true));
    } else {
        // Canonical representation of false is absent (matches serde
        // skip_serializing_if on AgentSource::trusted).
        table.remove("trusted");
    }
}

pub fn set_agent_auth_forward(
    &mut self,
    agent_key: &str,
    mode: crate::config::AuthForwardMode,
) {
    let claude_table = table_path_mut(&mut self.doc, &["agents", agent_key, "claude"]);
    claude_table.insert("auth_forward", toml_edit::value(auth_forward_str(mode)));
}

pub fn set_global_auth_forward(&mut self, mode: crate::config::AuthForwardMode) {
    let claude_table = table_path_mut(&mut self.doc, &["claude"]);
    claude_table.insert("auth_forward", toml_edit::value(auth_forward_str(mode)));
}

pub fn upsert_builtin_agent(&mut self, agent_key: &str, git_url: &str) {
    // Touch only git + trusted. Leave [roles.X.claude] and
    // [roles.X.env] alone — those are operator-owned.
    let table = table_path_mut(&mut self.doc, &["agents", agent_key]);
    table.insert("git", toml_edit::value(git_url));
    table.insert("trusted", toml_edit::value(true));
}
```

And at module scope, helper for the serde-matching string form:

```rust
fn auth_forward_str(mode: crate::config::AuthForwardMode) -> &'static str {
    match mode {
        crate::config::AuthForwardMode::Ignore => "ignore",
        crate::config::AuthForwardMode::Sync => "sync",
        crate::config::AuthForwardMode::Token => "token",
    }
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p jackin --lib config::editor::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/config/editor.rs
git commit -s -m "feat(config): ConfigEditor agent mutators

Adds set_agent_trust, set_agent_auth_forward, set_global_auth_forward,
upsert_builtin_agent. The builtin upsert touches only git + trusted so
operator-owned [roles.X.claude] and [roles.X.env] overrides survive
the sync that runs on every load_or_init.

set_agent_trust(false) removes the trusted key entirely to match the
canonical serde shape (skip_serializing_if = is_false).

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 9: Workspace editor methods

**Context:** `AppConfig` currently has `create_workspace`, `edit_workspace(name, WorkspaceEdit)`, `remove_workspace` in `src/config/workspaces.rs`. `edit_workspace` does non-trivial validation (workdir must exist / must be equal-to-or-parent-of a mount destination) and produces a `WorkspacePlan` for user-visible output.

**Trade-off:** duplicating that validation inside `ConfigEditor` is a waste. Instead, `ConfigEditor::edit_workspace` should:
1. Load the current `AppConfig` from the editor's document (parse a serde copy in-memory).
2. Run the existing `AppConfig::edit_workspace` logic on that in-memory copy to get the validated result + the resulting `WorkspaceConfig`.
3. Apply the validated result as targeted patches to `self.doc`.

But that means `AppConfig::edit_workspace` (and the validation helpers it calls) must stay live. They do — they live on `AppConfig` today; the mutators stay `pub(super)` after migration so the editor can call them. Reader code outside `src/config/` no longer has access (that's the whole point of the migration).

For `create_workspace` and `remove_workspace` the operations are simpler — just insert/remove a `[workspaces.N]` table. We replicate the validation by running serde on the resulting doc and surfacing any `validate_workspaces` error.

**Files:**
- Modify: `src/config/editor.rs`

- [ ] **Step 1: Write the failing tests**

Append to `tests`:

```rust
#[test]
fn create_workspace_adds_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let mount_src = temp.path().join("src");
    std::fs::create_dir_all(&mount_src).unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let ws = crate::workspace::WorkspaceConfig {
        workdir: "/workspace/new".to_string(),
        mounts: vec![crate::workspace::MountConfig {
            src: mount_src.display().to_string(),
            dst: "/workspace/new".to_string(),
            readonly: false,
        }],
        allowed_roles: vec![],
        default_role: None,
        last_agent: None,
        env: std::collections::BTreeMap::new(),
        agents: std::collections::BTreeMap::new(),
    };

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.create_workspace("new-ws", ws).unwrap();
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[workspaces.new-ws]"), "{out}");
    assert!(out.contains(r#"workdir = "/workspace/new""#), "{out}");
}

#[test]
fn set_last_agent_preserves_other_fields() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let original = r#"[workspaces.prod]
workdir = "/workspace/prod"
default_role = "agent-smith"
"#;
    std::fs::write(&paths.config_file, original).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.set_last_agent("prod", "agent-smith");
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains(r#"last_agent = "agent-smith""#), "{out}");
    assert!(out.contains(r#"default_role = "agent-smith""#), "{out}");
}

#[test]
fn remove_workspace_deletes_table() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[workspaces.a]
workdir = "/a"

[workspaces.b]
workdir = "/b"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_workspace("a").unwrap();
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("[workspaces.a]"), "{out}");
    assert!(out.contains("[workspaces.b]"), "{out}");
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p jackin --lib config::editor::tests::create_workspace`
Expected: FAIL.

- [ ] **Step 3: Implement the simpler methods directly**

In `impl ConfigEditor`:

```rust
pub fn set_last_agent(&mut self, workspace: &str, agent_key: &str) {
    let table = table_path_mut(&mut self.doc, &["workspaces", workspace]);
    table.insert("last_agent", toml_edit::value(agent_key));
}

pub fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()> {
    let Some(workspaces) = self.doc.get_mut("workspaces").and_then(|i| i.as_table_mut()) else {
        anyhow::bail!("workspace {name:?} not found");
    };
    if workspaces.remove(name).is_none() {
        anyhow::bail!("workspace {name:?} not found");
    }
    Ok(())
}

pub fn create_workspace(
    &mut self,
    name: &str,
    ws: crate::workspace::WorkspaceConfig,
) -> anyhow::Result<()> {
    // Collision check first — match today's create_workspace behavior.
    if self
        .doc
        .get("workspaces")
        .and_then(|i| i.as_table())
        .and_then(|t| t.get(name))
        .is_some()
    {
        anyhow::bail!("workspace {name:?} already exists");
    }

    // Serialize the WorkspaceConfig to a toml_edit Item via the string
    // round-trip. toml::to_string on the struct, parse as DocumentMut,
    // lift the resulting [workspaces.<name>] body.
    let rendered = toml::to_string(&ws)
        .with_context(|| format!("serializing workspace {name:?}"))?;
    let parsed: DocumentMut = rendered
        .parse()
        .with_context(|| format!("re-parsing serialized workspace {name:?}"))?;

    // `rendered` is the body of a [workspaces.<name>] table — i.e. its
    // top-level items become the table's items. Lift them in.
    let workspaces_table = table_path_mut(&mut self.doc, &["workspaces", name]);
    for (key, item) in parsed.as_table().iter() {
        workspaces_table.insert(key, item.clone());
    }

    Ok(())
}
```

Note: `edit_workspace` is NOT added yet — it's coming in the same task as Step 4 below because it's more involved.

- [ ] **Step 4: Implement `edit_workspace` by delegating to AppConfig**

`WorkspaceEdit` application is non-trivial (validation, mount upserts, allowed-role list diffs). Rather than duplicate, delegate:

```rust
pub fn edit_workspace(
    &mut self,
    name: &str,
    edit: crate::workspace::WorkspaceEdit,
) -> anyhow::Result<()> {
    // Snapshot current on-disk state into an AppConfig.
    let mut in_memory: AppConfig = toml::from_str(&self.doc.to_string())
        .context("re-parsing current doc into AppConfig for workspace edit")?;

    // Apply the edit using the existing validated logic. This mutates
    // in_memory and returns Ok on success / Err with the validation
    // message on failure.
    in_memory.edit_workspace(name, edit)?;

    // Pull the resulting WorkspaceConfig back out and splat into the doc.
    let updated = in_memory
        .workspaces
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("workspace {name:?} disappeared after edit"))?;

    // Replace the entire [workspaces.<name>] table. This preserves
    // comments in OTHER workspaces and in unrelated top-level sections,
    // which is what the migration cares about. Comments inside the
    // edited workspace itself are consumed — that's acceptable because
    // the edit IS the change the user is making to that workspace.
    let rendered = toml::to_string(updated)?;
    let parsed: DocumentMut = rendered.parse()?;
    let target = table_path_mut(&mut self.doc, &["workspaces", name]);
    target.clear();
    for (key, item) in parsed.as_table().iter() {
        target.insert(key, item.clone());
    }

    Ok(())
}
```

This preserves the validation contract of `AppConfig::edit_workspace` exactly and limits the "clears comments" behavior to the one workspace being edited.

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p jackin --lib config::editor::tests`
Expected: PASS on the three new tests + every prior test.

- [ ] **Step 6: Commit**

```bash
git add src/config/editor.rs
git commit -s -m "feat(config): ConfigEditor workspace mutators

Adds create_workspace, edit_workspace, remove_workspace, set_last_agent.

create_workspace collision-checks then splats a serialized
WorkspaceConfig into [workspaces.<name>]. edit_workspace delegates to
AppConfig::edit_workspace's validated logic (mount upserts,
allowed_roles diffs, workdir validation) and then replaces the
[workspaces.<name>] table — comments inside the edited workspace are
consumed (that IS the change), but all other sections survive intact.

set_last_agent is a targeted insert that preserves every other field
on the workspace, including default_role.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 10: Migrate call sites in `src/app/mod.rs`

**Context:** Ten call sites in this file each follow the pattern `config.MUTATOR(...); config.save(&paths)?;`. Convert each to the `ConfigEditor::open → mutate → save` shape.

**Files:**
- Modify: `src/app/mod.rs`

- [ ] **Step 1: Run the full test suite to establish a green baseline**

Run: `cargo test -p jackin`
Expected: all tests PASS. If anything is red before the migration, fix it first — we need a known-green baseline to catch any regressions from the migration.

- [ ] **Step 2: Migrate the mount sites (lines 210, 216)**

At `src/app/mod.rs:204–217` area, the current code:

```rust
let mount = config::MountConfig {
    src: resolved_src,
    dst: dst.clone(),
    readonly,
};
config.add_mount(&name, mount, scope.as_deref());
config.save(&paths)?;
println!("Added mount {name:?} ({scope_label}): {src} -> {dst}{ro}");
```

Replace with:

```rust
let mount = config::MountConfig {
    src: resolved_src,
    dst: dst.clone(),
    readonly,
};
let mut editor = crate::config::ConfigEditor::open(&paths)?;
editor.add_mount(&name, mount, scope.as_deref());
config = editor.save()?;
println!("Added mount {name:?} ({scope_label}): {src} -> {dst}{ro}");
```

Apply the analogous transformation to the `MountCommand::Remove` branch (line 216 area):

```rust
let mut editor = crate::config::ConfigEditor::open(&paths)?;
if editor.remove_mount(&name, scope.as_deref()) {
    config = editor.save()?;
    println!("Removed mount {name:?}.");
} else {
    // Nothing to save, drop the editor
    drop(editor);
    println!("No mount named {name:?} to remove.");
}
```

Note: the `drop(editor)` is cosmetic — `ConfigEditor` is `Drop`-safe because it only holds in-memory data. Include it for clarity that no save happens on the no-op path.

- [ ] **Step 3: Migrate the trust sites (lines 269, 282)**

Current (Grant):

```rust
let class = ClassSelector::parse(&selector)?;
config.resolve_agent_source(&class)?;
if config.trust_agent(&class.key()) {
    config.save(&paths)?;
    println!("Trusted {}.", class.key());
} else {
    println!("{} is already trusted.", class.key());
}
```

Replace with:

```rust
let class = ClassSelector::parse(&selector)?;
config.resolve_agent_source(&class)?;
// resolve_agent_source may have mutated config in-memory (inserting
// a new agent entry); persist that and the trust change together.
let was_trusted = config
    .agents
    .get(&class.key())
    .map(|a| a.trusted)
    .unwrap_or(false);
if !was_trusted {
    let mut editor = crate::config::ConfigEditor::open(&paths)?;
    // resolve_agent_source may have just inserted this agent; make
    // sure the editor sees its git URL.
    if let Some(source) = config.agents.get(&class.key()) {
        editor.upsert_agent_source(&class.key(), source);
    }
    editor.set_agent_trust(&class.key(), true);
    config = editor.save()?;
    println!("Trusted {}.", class.key());
} else {
    println!("{} is already trusted.", class.key());
}
```

This introduces a new need: `ConfigEditor::upsert_agent_source(key, &AgentSource)`. Add it alongside `upsert_builtin_agent`:

```rust
pub fn upsert_agent_source(&mut self, agent_key: &str, source: &crate::config::AgentSource) {
    // Hand-splat git + trusted. Leave claude & env overrides alone if
    // they already exist; otherwise serialize and insert.
    let table = table_path_mut(&mut self.doc, &["agents", agent_key]);
    table.insert("git", toml_edit::value(source.git.clone()));
    if source.trusted {
        table.insert("trusted", toml_edit::value(true));
    } else {
        table.remove("trusted");
    }
    // Serialize claude override if present and the table doesn't have one.
    if let Some(claude) = &source.claude {
        if !table.contains_key("claude") {
            let rendered = toml::to_string(claude).unwrap_or_default();
            if let Ok(parsed) = rendered.parse::<DocumentMut>() {
                let claude_table = table_path_mut(&mut self.doc, &["agents", agent_key, "claude"]);
                for (k, v) in parsed.as_table().iter() {
                    claude_table.insert(k, v.clone());
                }
            }
        }
    }
    // Env is operator-owned — do not overwrite.
}
```

Add a test for it (append to `tests`):

```rust
#[test]
fn upsert_agent_source_preserves_existing_env() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.foo]
git = "OLD"

[roles.foo.env]
MY_VAR = "preserved"
"#,
    )
    .unwrap();

    let source = crate::config::AgentSource {
        git: "NEW".to_string(),
        trusted: true,
        claude: None,
        env: std::collections::BTreeMap::new(),
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_agent_source("foo", &source);
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains(r#"git = "NEW""#), "{out}");
    assert!(out.contains(r#"MY_VAR = "preserved""#), "{out}");
}
```

Revoke branch (line 282 area):

```rust
cli::TrustCommand::Revoke { selector } => {
    let class = ClassSelector::parse(&selector)?;
    if AppConfig::is_builtin_agent(&class.key()) {
        anyhow::bail!("{} is a built-in agent and is always trusted.", class.key());
    }
    let was_trusted = config
        .agents
        .get(&class.key())
        .map(|a| a.trusted)
        .unwrap_or(false);
    if was_trusted {
        let mut editor = crate::config::ConfigEditor::open(&paths)?;
        editor.set_agent_trust(&class.key(), false);
        config = editor.save()?;
        println!("Revoked trust for {}.", class.key());
    } else {
        println!("{} is not trusted.", class.key());
    }
}
```

- [ ] **Step 4: Migrate the auth_forward sites (lines 318, 322)**

Current:

```rust
if let Some(agent_selector) = agent {
    let class = ClassSelector::parse(&agent_selector)?;
    config.resolve_agent_source(&class)?;
    config.set_agent_auth_forward(&class.key(), parsed_mode);
    config.save(&paths)?;
    println!("Set auth forwarding for {} to {parsed_mode}.", class.key());
} else {
    config.claude.auth_forward = parsed_mode;
    config.save(&paths)?;
    println!("Set global auth forwarding to {parsed_mode}.");
}
```

Replace:

```rust
if let Some(agent_selector) = agent {
    let class = ClassSelector::parse(&agent_selector)?;
    config.resolve_agent_source(&class)?;
    let mut editor = crate::config::ConfigEditor::open(&paths)?;
    if let Some(source) = config.agents.get(&class.key()) {
        editor.upsert_agent_source(&class.key(), source);
    }
    editor.set_agent_auth_forward(&class.key(), parsed_mode);
    config = editor.save()?;
    println!("Set auth forwarding for {} to {parsed_mode}.", class.key());
} else {
    let mut editor = crate::config::ConfigEditor::open(&paths)?;
    editor.set_global_auth_forward(parsed_mode);
    config = editor.save()?;
    println!("Set global auth forwarding to {parsed_mode}.");
}
```

- [ ] **Step 5: Migrate the workspace sites (lines 386, 655, 705, 714)**

Line 386 — `create_workspace`:

```rust
// Before:
config.create_workspace(&name, WorkspaceConfig { ... })?;
config.save(&paths)?;
// After:
let ws = WorkspaceConfig { ... };
let mut editor = crate::config::ConfigEditor::open(&paths)?;
editor.create_workspace(&name, ws)?;
config = editor.save()?;
```

Lines 655 and 705 — `edit_workspace`:

```rust
// Before:
config.edit_workspace(&name, WorkspaceEdit { ... })?;
config.save(&paths)?;
// After:
let mut editor = crate::config::ConfigEditor::open(&paths)?;
editor.edit_workspace(&name, WorkspaceEdit { ... })?;
config = editor.save()?;
```

Line 714 — `remove_workspace`:

```rust
// Before:
config.remove_workspace(&name)?;
config.save(&paths)?;
// After:
let mut editor = crate::config::ConfigEditor::open(&paths)?;
editor.remove_workspace(&name)?;
config = editor.save()?;
```

- [ ] **Step 6: Run the full test suite**

Run: `cargo test -p jackin`
Expected: all tests PASS. If any existing integration test fails, the migration shape is wrong — read the failure, identify whether it's a missing editor method or a caller change.

- [ ] **Step 7: Commit**

```bash
git add src/app/mod.rs src/config/editor.rs
git commit -s -m "refactor(app): route config writes through ConfigEditor

Migrates the ten config.mutate(); config.save() pairs in src/app/mod.rs
(mounts, trust, auth_forward, workspace create/edit/remove) to the new
ConfigEditor::open → mutate → save shape. AppConfig::save still exists
at this point; it gets removed in a later commit once all callers are
migrated.

Adds ConfigEditor::upsert_agent_source because the trust/auth_forward
CLI paths call resolve_agent_source first (which may insert a new
agent into the in-memory config); the editor must see the same insert
so it persists alongside the trust change.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 11: Migrate `src/app/context.rs` (last-used-agent)

**Files:**
- Modify: `src/app/context.rs`

- [ ] **Step 1: Replace the save site at line 327 area**

Current `remember_last_agent` body:

```rust
if let Some(workspace_name) = workspace_name
    && let Some(workspace) = config.workspaces.get_mut(workspace_name)
{
    workspace.last_agent = Some(class.key());
    if let Err(error) = config.save(paths) {
        eprintln!("warning: failed to save last-used agent: {error}");
    }
}
```

Replace with:

```rust
let Some(workspace_name) = workspace_name else { return; };
if !config.workspaces.contains_key(workspace_name) {
    return;
}
let Ok(mut editor) = crate::config::ConfigEditor::open(paths) else {
    eprintln!("warning: failed to open config for last-used-agent save");
    return;
};
editor.set_last_agent(workspace_name, &class.key());
match editor.save() {
    Ok(reloaded) => *config = reloaded,
    Err(error) => eprintln!("warning: failed to save last-used agent: {error}"),
}
```

- [ ] **Step 2: Run the test suite**

Run: `cargo test -p jackin`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/app/context.rs
git commit -s -m "refactor(app): route last-used-agent save through ConfigEditor

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 12: Migrate `src/runtime/launch.rs` (newly-trusted / newly-registered)

**Files:**
- Modify: `src/runtime/launch.rs`

- [ ] **Step 1: Replace the save site at line 600 area**

Current:

```rust
let newly_trusted = if source.trusted {
    false
} else {
    confirm_trust(selector, &source)?;
    config.trust_agent(&selector.key());
    true
};

// Persist config when the agent was newly registered or newly trusted
if is_new || newly_trusted {
    config.save(paths)?;
}
```

Replace with:

```rust
let newly_trusted = if source.trusted {
    false
} else {
    confirm_trust(selector, &source)?;
    // Mutate the in-memory copy so callers downstream see the trust
    // without a reload; persist via editor below.
    if let Some(entry) = config.agents.get_mut(&selector.key()) {
        entry.trusted = true;
    }
    true
};

if is_new || newly_trusted {
    let mut editor = crate::config::ConfigEditor::open(paths)?;
    if let Some(agent_source) = config.agents.get(&selector.key()) {
        editor.upsert_agent_source(&selector.key(), agent_source);
    }
    editor.set_agent_trust(&selector.key(), true);
    *config = editor.save()?;
}
```

- [ ] **Step 2: Run the test suite**

Run: `cargo test -p jackin`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/runtime/launch.rs
git commit -s -m "refactor(launch): route newly-trusted agent save through ConfigEditor

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 13: Migrate `src/config/persist.rs` (load-time migrations)

**Files:**
- Modify: `src/config/persist.rs`

- [ ] **Step 1: Replace the save branch in `load_or_init`**

The current body at `src/config/persist.rs:5–44` ends with:

```rust
let builtins_changed = config.sync_builtin_agents();

if deprecated_copy_seen {
    crate::tui::deprecation_warning(&format!(
        "migrated auth_forward \"copy\" → \"sync\" in {} (copy is deprecated)",
        paths.config_file.display()
    ));
}

if builtins_changed || deprecated_copy_seen {
    config.save(paths)?;
}
```

Replace the whole post-load migration block with:

```rust
let builtins_changed = config.sync_builtin_agents();

if deprecated_copy_seen {
    crate::tui::deprecation_warning(&format!(
        "migrated auth_forward \"copy\" → \"sync\" in {} (copy is deprecated)",
        paths.config_file.display()
    ));
}

if builtins_changed || deprecated_copy_seen {
    let mut editor = crate::config::ConfigEditor::open(paths)?;
    if builtins_changed {
        for &(name, git) in crate::config::agents::BUILTIN_AGENTS {
            editor.upsert_builtin_agent(name, git);
        }
    }
    if deprecated_copy_seen {
        // Rewrite every "copy" string we find to "sync" at the exact
        // paths contains_deprecated_copy_auth_forward checks:
        //   [claude].auth_forward
        //   [roles.*.claude].auth_forward
        editor.normalize_deprecated_copy();
    }
    // Discard the reloaded AppConfig — we already have `config` in the
    // correct post-migration shape.
    editor.save()?;
}
```

- [ ] **Step 2: Expose `BUILTIN_AGENTS` from the `agents` module**

In `src/config/agents.rs`, find the `BUILTIN_AGENTS` constant (currently `pub(super)`). If `load_or_init` in `persist.rs` is in the same `super` module the existing visibility may already work — verify with `cargo check`. If not, change to `pub(crate)`:

```rust
pub(crate) const BUILTIN_AGENTS: &[(&str, &str)] = &[
    ("agent-smith", "https://github.com/jackin-project/jackin-agent-smith.git"),
    ("the-architect", "https://github.com/jackin-project/jackin-the-architect.git"),
];
```

- [ ] **Step 3: Implement `ConfigEditor::normalize_deprecated_copy`**

In `impl ConfigEditor` in `src/config/editor.rs`:

```rust
/// Rewrite any `auth_forward = "copy"` to `"sync"` at the two paths
/// `contains_deprecated_copy_auth_forward` checks:
///   [claude].auth_forward
///   [roles.*.claude].auth_forward
///
/// Does not touch any other structure. Used by load_or_init when the
/// on-disk config still contains the deprecated literal.
pub fn normalize_deprecated_copy(&mut self) {
    // Global [claude]
    if let Some(claude) = self.doc.get_mut("claude").and_then(|i| i.as_table_mut()) {
        if claude.get("auth_forward").and_then(|i| i.as_str()) == Some("copy") {
            claude.insert("auth_forward", toml_edit::value("sync"));
        }
    }
    // Per-agent [roles.X.claude]
    if let Some(agents) = self.doc.get_mut("agents").and_then(|i| i.as_table_mut()) {
        for (_, agent_item) in agents.iter_mut() {
            let Some(agent_table) = agent_item.as_table_mut() else { continue };
            let Some(claude) = agent_table.get_mut("claude").and_then(|i| i.as_table_mut())
            else {
                continue;
            };
            if claude.get("auth_forward").and_then(|i| i.as_str()) == Some("copy") {
                claude.insert("auth_forward", toml_edit::value("sync"));
            }
        }
    }
}
```

And add a test for it:

```rust
#[test]
fn normalize_deprecated_copy_rewrites_global_and_agent_paths() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[claude]
auth_forward = "copy"

[roles.foo]
git = "x"

[roles.foo.claude]
auth_forward = "copy"

[roles.bar]
git = "y"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.normalize_deprecated_copy();
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains(r#""copy""#), "{out}");
    assert!(out.contains(r#"auth_forward = "sync""#), "{out}");
}
```

- [ ] **Step 4: Run the test suite**

Run: `cargo test -p jackin`
Expected: all tests PASS, including the existing `sync_does_not_rewrite_config_when_already_current` (line 115) and `load_or_init_rejects_invalid_persisted_workspace`. If any fail, the replacement of the migration branch introduced a regression.

- [ ] **Step 5: Commit**

```bash
git add src/config/persist.rs src/config/agents.rs src/config/editor.rs
git commit -s -m "refactor(config): route load_or_init migrations through ConfigEditor

The builtin-sync and deprecated-copy migrations run in-memory on
AppConfig as before, but the resulting save goes through
ConfigEditor::upsert_builtin_agent and normalize_deprecated_copy
instead of AppConfig::save. User comments and blank lines in config
sections untouched by the migration now survive the first load after
upgrade.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 14: Remove `AppConfig::save` and the dead mutators

**Context:** At this point, `cargo build` succeeds with `AppConfig::save` unused. The dead-code path can be deleted, along with the in-memory mutator methods that have editor equivalents.

**Files:**
- Modify: `src/config/persist.rs`
- Modify: `src/config/agents.rs`
- Modify: `src/config/mounts.rs`
- Modify: `src/config/workspaces.rs`

- [ ] **Step 1: Delete `AppConfig::save`**

In `src/config/persist.rs`, delete the `pub fn save(&self, paths: &JackinPaths) -> anyhow::Result<()> { ... }` method (lines 46–69 in the original). Keep `load_or_init`, `contains_deprecated_copy_auth_forward`, and everything else.

Run `cargo check`. Expected: success. No callers of `save` should remain after tasks 10–13.

If `cargo check` reports unused imports in `persist.rs` (e.g., `OpenOptions`, `PermissionsExt`), remove them.

- [ ] **Step 2: Audit and delete dead mutators on `AppConfig`**

For each of these methods in the listed files, check whether it has callers **outside the `src/config/` module tree**:

- `src/config/agents.rs`: `trust_agent`, `untrust_agent`, `set_agent_auth_forward`
- `src/config/mounts.rs`: `add_mount`, `remove_mount`
- `src/config/workspaces.rs`: `create_workspace`, `remove_workspace`

Run (from the jackin root):
```
grep -rn "config\.trust_agent\|config\.untrust_agent\|config\.set_agent_auth_forward\|config\.add_mount\|config\.remove_mount\|config\.create_workspace\|config\.remove_workspace" src/ --include="*.rs" | grep -v "^src/config/"
```

After tasks 10–13, this should return no matches. If it does, migrate those call sites before proceeding.

If clean, delete these methods from their respective files. `AppConfig::edit_workspace` is retained because `ConfigEditor::edit_workspace` calls it internally; mark it `pub(crate)` so external code can no longer invoke it directly:

```rust
pub(crate) fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()> {
    // unchanged body
}
```

Similarly `AppConfig::resolve_agent_source` is still called from `src/app/mod.rs` and `src/runtime/launch.rs` — leave that public.

- [ ] **Step 3: Run the test suite + clippy**

Run: `cargo test -p jackin && cargo clippy -p jackin -- -D warnings`
Expected: all tests PASS, clippy clean.

- [ ] **Step 4: Commit**

```bash
git add -u
git commit -s -m "refactor(config): remove AppConfig::save and dead mutator methods

AppConfig::save is removed; ConfigEditor is the sole write path. The
in-memory mutator methods (trust_agent, untrust_agent,
set_agent_auth_forward, add_mount, remove_mount, create_workspace,
remove_workspace) are deleted — their editor equivalents are now the
only way to change persisted state.

AppConfig::edit_workspace stays (called internally by
ConfigEditor::edit_workspace) but is demoted to pub(crate) so the rest
of the crate cannot bypass the editor.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 15: Final verification

**Files:** None modified — this is a gate, not a change.

- [ ] **Step 1: Full test suite, release build, clippy**

Run (from jackin root):

```bash
cargo test -p jackin --all-targets && \
cargo build -p jackin --release && \
cargo clippy -p jackin --all-targets -- -D warnings
```

Expected: all green.

- [ ] **Step 2: Confirm no stray `AppConfig::save` or `config.save` references**

Run:

```bash
grep -rn "config\.save\|AppConfig::save" src/ --include="*.rs"
```

Expected: zero matches outside comments or doc strings. If any remain, migrate them before opening a PR.

- [ ] **Step 3: Confirm no `toml::to_string_pretty(` on the write side**

Run:

```bash
grep -n "toml::to_string_pretty" src/ -r --include="*.rs"
```

Expected: matches only in tests or in `ConfigEditor` internals (the `WorkspaceConfig` serialize helper inside `create_workspace` / `edit_workspace` uses `toml::to_string` — that's fine because those results are re-parsed by `toml_edit`, not written to disk).

- [ ] **Step 4: Manual smoke test — hand-written comment survives a save**

```bash
# Make a test config with a hand-written comment
mkdir -p /tmp/jackin-smoke
cat > /tmp/jackin-smoke/config.toml <<'EOF'
# I wrote this comment by hand and expect it to survive.

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

# this too
[workspaces.demo]
workdir = "/tmp"
EOF

# Run a command that triggers a save (trust revoke/grant, mount add, etc.)
JACKIN_HOME=/tmp/jackin-smoke/home \
  cargo run -p jackin -- config mount add shared-home /tmp:/workspace/home

# Expected: the hand-written comments are still present in the config
grep "I wrote this comment" /tmp/jackin-smoke/config.toml && echo "✓ comment preserved"
grep "this too" /tmp/jackin-smoke/config.toml && echo "✓ second comment preserved"
```

*(Adjust env var to jackin's actual override for the config path — see `JackinPaths::detect`. The point is: hand-written comments must survive the programmatic save.)*

- [ ] **Step 5: Open the PR**

```bash
git push -u origin feature/toml-edit-migration
gh pr create --title "feat(config): toml_edit migration (PR 1 of 3)" --body "$(cat <<'EOF'
## Summary

Implements the spec at `docs/superpowers/specs/2026-04-23-toml-edit-migration-design.md` (PR #160).

- New `ConfigEditor` in `src/config/editor.rs` owns a `toml_edit::DocumentMut` and exposes typed setters for every mutation the app performs. Reads still use `AppConfig::load_or_init` (serde + `toml`); writes go through `ConfigEditor::open → mutate → save`.
- `AppConfig::save` and the in-memory mutators (`trust_agent`, `add_mount`, `create_workspace`, etc.) are removed. The rest of the app cannot change persisted state except through the editor.
- 13 call sites migrated across `src/app/mod.rs`, `src/app/context.rs`, `src/runtime/launch.rs`, and `src/config/persist.rs`.
- User-written comments, blank lines, and key ordering now survive every programmatic save.

## Test plan

- [ ] `cargo test -p jackin --all-targets` — green
- [ ] `cargo clippy -p jackin --all-targets -- -D warnings` — clean
- [ ] Manual smoke test: a hand-written `# comment` in `~/.config/jackin/config.toml` survives a `jackin config mount add` (or any other save-triggering command)
- [ ] Fixture round-trip test (`fixture_round_trip_is_byte_identical`) passes — the main smoke test that `toml_edit` round-trips everything we use
- [ ] `set_env_comment` + `mutating_sibling_preserves_comment_above_other_key` tests pass — the two tests that validate the specific feature PR 3 depends on

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Report the PR URL to the operator and stop — do not merge without explicit per-PR confirmation.

---

## Self-review notes

Ran spec-coverage check against `docs/superpowers/specs/2026-04-23-toml-edit-migration-design.md`:

- **Goal 1** (preserve comments/blanks/order): Tasks 4, 5 (comment tests), Task 5 (fixture round-trip), Task 15 Step 4 (manual smoke). ✓
- **Goal 2** (read path + schema unchanged): No task modifies `AppConfig` field shapes; Task 14 explicitly demotes `edit_workspace` to `pub(crate)` only. ✓
- **Goal 3** (all writes through one module): Tasks 10–14 migrate every call site; Task 14 deletes the old `save()`. ✓
- **Goal 4** (atomic-write parity): Task 1 lifts the existing body verbatim; Task 6 pins the invariants. ✓
- **Goal 5** (idempotent-save tripwire): Task 1 Step 4. ✓

Spec testing section (8 tests): tripwire (Task 1), preserves comment above mutated sibling (Task 4), preserves comments in untouched tables (Task 5), preserves blank lines/key order — covered by the fixture round-trip which checks whole-file equality (Task 5), `set_env_comment` contract (Task 4), upsert creates intermediate tables (Task 2), atomic-write parity (Task 6), load-then-save on real fixture (Task 5). All eight have tasks. ✓

Spec non-goals: no TUI/CLI changes (none introduced), no schema change (confirmed in Task 14), no multi-process locking (not added), no CHANGELOG touch (absent from every commit message). ✓

Placeholder scan: no "TBD" / "appropriate error handling" / "similar to Task N" / "write tests for the above". Every test has explicit code; every impl step has explicit code. ✓

Type consistency: `EnvScope` used identically across Tasks 2, 3, 4, 5; `AuthForwardMode::{Sync, Token, Ignore}` matches the enum in `src/config/mod.rs:22–46`; `WorkspaceConfig` / `MountConfig` struct literals match the verified field lists from the exploration report. ✓

One forward-looking method emerged mid-plan (`upsert_agent_source` in Task 10 Step 3) that was not in the original spec API surface. It arose because `config.resolve_agent_source` may have mutated memory before the editor opens — a real interaction not surfaced during brainstorming. The plan introduces it with a dedicated test and notes the reason in the commit message. This is a real spec gap that could be addressed by either (a) updating the spec to include `upsert_agent_source`, or (b) noting it as an implementation detail discovered during execution. Option (b) is cheaper; leaving it as-is.

# Auth Forward: `sync` as Default, Deprecate `copy` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `auth_forward = "sync"` the default, migrate existing `copy` configs in place with a one-line deprecation notice on load, drop the `AuthForwardMode::Copy` enum variant.

**Architecture:** One-file schema change (drop variant from `AuthForwardMode`), plus a small migration pass in the config loader that detects the deprecated `"copy"` literal at two known TOML paths (`claude.auth_forward`, `agents.*.claude.auth_forward`) and rewrites the file using the existing `AppConfig::save` path. `FromStr` and `Deserialize` accept `"copy"` as a deprecated alias that resolves to `Sync`, so scripts and CI workflows that still pass `copy` never break. CLI surface emits a one-line deprecation warning on `jackin config auth set copy`.

**Tech Stack:** Rust (edition 2024), `serde` + `toml` 1.x (already in `Cargo.toml`), `owo-colors` for the warning color, `anyhow` for errors, `tempfile` + `cargo-nextest` for tests.

**Branch:** `feature/auth-sync-default` (per `BRANCHING.md` — `feature/<short-description>`).
**Commit style:** Conventional Commits with DCO `Signed-off-by` and `Co-authored-by: Claude <noreply@anthropic.com>` per `AGENTS.md`.
**Spec:** `docs/superpowers/specs/2026-04-23-auth-sync-default-design.md`.

---

## File Structure

| File                                                                     | Purpose                                                                 |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| `src/config/mod.rs`                                                      | `AuthForwardMode` enum, `Default`, `Display`, `FromStr`, custom `Deserialize`. |
| `src/config/persist.rs`                                                  | `load_or_init` detects deprecated `copy` in raw TOML; rewrites to disk via `save`. |
| `src/config/agents.rs`                                                   | One docstring update (`→ Copy` → `→ Sync`). Tests updated.              |
| `src/instance/auth.rs`                                                   | Drop the `Copy` match arm in `provision_claude_auth`. Tests updated.    |
| `src/cli/config.rs`                                                      | Help-text update: drop `copy` from the examples and the help body.     |
| `src/app/mod.rs`                                                         | `AuthCommand::Set`: print a deprecation warning when the CLI receives `copy`. |
| `src/tui/output.rs`                                                      | Add `pub fn deprecation_warning(msg: &str)` (one-liner yellow notice).  |
| `src/tui/mod.rs`                                                         | Re-export `deprecation_warning`.                                        |
| `docs/src/content/docs/guides/authentication.mdx`                        | Mode table drops `copy`; `sync` labelled default; mention migration.    |
| `docs/src/content/docs/reference/configuration.mdx`                      | `auth_forward` default updated.                                         |
| `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`       | "Current State" note that `copy` is deprecated.                         |
| `CHANGELOG.md`                                                           | `Changed` + `Deprecated` entries under `## [Unreleased]`.               |

No new files.

---

## Preflight

- [ ] **Step 0.1: Ensure clean tree on `main`**

```bash
git fetch origin
git checkout main
git pull --ff-only
git status
```

Expected: `nothing to commit, working tree clean`. If dirty, stop and investigate.

- [ ] **Step 0.2: Create the feature branch**

```bash
git checkout -b feature/auth-sync-default
```

- [ ] **Step 0.3: Confirm pre-commit gate is currently clean (baseline)**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all three exit 0. If any fail, do NOT start this work — fix baseline first or you will be chasing unrelated failures.

---

## Task 1: Drop `Copy`, set `Sync` as default, accept `"copy"` as deprecated alias

**Files:**
- Modify: `src/config/mod.rs:20-57` (enum, `Display`, `FromStr`, add custom `Deserialize`)
- Modify: `src/config/agents.rs:46-49` (docstring referencing `Copy`)

- [ ] **Step 1.1: Write failing tests for the new default + deprecated-alias behavior**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/config/mod.rs`:

```rust
#[test]
fn auth_forward_mode_default_is_sync() {
    assert_eq!(AuthForwardMode::default(), AuthForwardMode::Sync);
}

#[test]
fn auth_forward_mode_from_str_accepts_copy_as_deprecated_alias() {
    use std::str::FromStr;
    assert_eq!(
        AuthForwardMode::from_str("copy").unwrap(),
        AuthForwardMode::Sync
    );
}

#[test]
fn auth_forward_mode_from_str_accepts_sync_and_ignore() {
    use std::str::FromStr;
    assert_eq!(
        AuthForwardMode::from_str("sync").unwrap(),
        AuthForwardMode::Sync
    );
    assert_eq!(
        AuthForwardMode::from_str("ignore").unwrap(),
        AuthForwardMode::Ignore
    );
}

#[test]
fn auth_forward_mode_from_str_rejects_unknown_values() {
    use std::str::FromStr;
    assert!(AuthForwardMode::from_str("bogus").is_err());
}

#[test]
fn auth_forward_mode_deserializes_copy_to_sync() {
    let toml_str = r#"
[claude]
auth_forward = "copy"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.claude.auth_forward, AuthForwardMode::Sync);
}

#[test]
fn auth_forward_mode_display_does_not_emit_copy() {
    assert_eq!(AuthForwardMode::Sync.to_string(), "sync");
    assert_eq!(AuthForwardMode::Ignore.to_string(), "ignore");
}
```

- [ ] **Step 1.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin --test-threads=1 \
  config::tests::auth_forward_mode_default_is_sync \
  config::tests::auth_forward_mode_from_str_accepts_copy_as_deprecated_alias \
  config::tests::auth_forward_mode_deserializes_copy_to_sync
```

Expected: fails — `auth_forward_mode_default_is_sync` fails (default is currently `Copy`), `from_str_accepts_copy_as_deprecated_alias` still returns `Copy`, `deserializes_copy_to_sync` still returns `Copy`.

- [ ] **Step 1.3: Edit `AuthForwardMode` enum — remove `Copy`, make `Sync` default**

In `src/config/mod.rs`, replace the enum definition (lines 20–32 currently):

```rust
/// Controls how the host's `~/.claude.json` is forwarded into agent containers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthForwardMode {
    /// Revoke any forwarded auth and never copy — container starts with `{}`.
    Ignore,
    /// Overwrite container auth from host on each launch when host auth
    /// exists; preserve container auth when host auth is absent.
    #[default]
    Sync,
}
```

Note: `Deserialize` is no longer derived — we'll hand-roll it in Step 1.5 so `"copy"` maps to `Sync` as a deprecated alias.

- [ ] **Step 1.4: Update `Display` and `FromStr`**

Replace the `Display` and `FromStr` impls (lines 34–57 currently):

```rust
impl std::fmt::Display for AuthForwardMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignore => write!(f, "ignore"),
            Self::Sync => write!(f, "sync"),
        }
    }
}

impl std::str::FromStr for AuthForwardMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ignore" => Ok(Self::Ignore),
            "sync" => Ok(Self::Sync),
            // Deprecated alias — accepted to avoid breaking scripts and
            // configs from before the default flipped to `sync`. Callers
            // that want to surface the deprecation should check for the
            // literal `"copy"` themselves before calling `parse()`.
            "copy" => Ok(Self::Sync),
            other => Err(format!(
                "invalid auth_forward mode {other:?}; expected one of: sync, ignore"
            )),
        }
    }
}
```

- [ ] **Step 1.5: Add custom `Deserialize` impl that routes through `FromStr`**

Add immediately after the `FromStr` impl:

```rust
impl<'de> serde::Deserialize<'de> for AuthForwardMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(D::Error::custom)
    }
}
```

- [ ] **Step 1.6: Update the docstring in `src/config/agents.rs:46-49`**

Replace:

```rust
    /// Resolution order: per-agent override → global default → `Copy`.
```

with:

```rust
    /// Resolution order: per-agent override → global default → `Sync`.
```

- [ ] **Step 1.7: Update stale tests that referenced `AuthForwardMode::Copy`**

Two tests in `src/config/agents.rs` (currently at lines 346–388) reference `Copy`:

Find and replace:

```rust
    #[test]
    fn auth_forward_defaults_to_copy() {
        let config = AppConfig::default();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Copy);
    }
```

with:

```rust
    #[test]
    fn auth_forward_defaults_to_sync() {
        let config = AppConfig::default();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Sync);
    }
```

And:

```rust
    #[test]
    fn resolve_auth_forward_defaults_to_copy() {
        let config = AppConfig::default();
        assert_eq!(
            config.resolve_auth_forward_mode("nonexistent"),
            AuthForwardMode::Copy
        );
    }
```

with:

```rust
    #[test]
    fn resolve_auth_forward_defaults_to_sync() {
        let config = AppConfig::default();
        assert_eq!(
            config.resolve_auth_forward_mode("nonexistent"),
            AuthForwardMode::Sync
        );
    }
```

Also update `src/config/mod.rs:310-323` test `existing_config_without_claude_section_deserializes_with_defaults`:

```rust
    #[test]
    fn existing_config_without_claude_section_deserializes_with_defaults() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Sync);
        assert_eq!(
            config.resolve_auth_forward_mode("agent-smith"),
            AuthForwardMode::Sync
        );
    }
```

- [ ] **Step 1.8: Run tests — confirm the new ones pass and nothing else regresses**

```bash
cargo nextest run -p jackin config
```

Expected: all green. If the `auth.rs` tests under `instance::` fail because of the `Copy` variant removal, that's Task 2's job — those failures are expected at this stage.

- [ ] **Step 1.9: Commit Task 1**

```bash
git add src/config/mod.rs src/config/agents.rs
git commit -s -m "$(cat <<'EOF'
feat(config)!: drop AuthForwardMode::Copy; default to Sync; accept "copy" as deprecated alias

Drop the Copy variant from AuthForwardMode. Default is now Sync. The string
literal "copy" is still accepted by FromStr and Deserialize and resolves to
Sync, so existing configs and CLI invocations keep working. Callers that
want to emit a deprecation warning check for the literal themselves.

BREAKING CHANGE: the Rust-level AuthForwardMode::Copy variant no longer
exists. External Rust code that pattern-matched on it must update its match
arms. The TOML/CLI surface remains backward-compatible (accepts "copy" as
an alias).

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Drop the `Copy` arm in `provision_claude_auth`

**Files:**
- Modify: `src/instance/auth.rs:38-52` (remove the `Copy` match arm)
- Modify: `src/instance/auth.rs:276-362, 408-437, 653-693, 696-736` (tests that use `AuthForwardMode::Copy`)

- [ ] **Step 2.1: Run the instance tests to see what fails after Task 1**

```bash
cargo nextest run -p jackin instance::
```

Expected: multiple failures in `instance::tests` referencing `AuthForwardMode::Copy` (variant no longer exists).

- [ ] **Step 2.2: Remove the `Copy` match arm from `provision_claude_auth`**

In `src/instance/auth.rs`, delete lines 38–52 (the entire `AuthForwardMode::Copy` arm including the comment block). The match should now contain only two arms: `Ignore` and `Sync`. No behavioral change to those two.

After the edit, the match looks like:

```rust
let outcome = match mode {
    AuthForwardMode::Ignore => {
        // ... existing body unchanged ...
    }
    AuthForwardMode::Sync => {
        // ... existing body unchanged ...
    }
};
```

- [ ] **Step 2.3: Replace `Copy` in tests that verified shared behavior**

Tests that used `Copy` only because it was the default and the test wasn't about `Copy` semantics specifically should be updated to `Sync`. Specifically rewrite these test bodies:

`copy_mode_does_not_overwrite_existing` (lines 326-362) — this test verifies the exact `Copy`-specific semantic we're removing ("never overwrite afterwards"). **Delete the test entirely.** The invariant no longer exists.

`copy_mode_copies_host_auth_on_first_run` (lines 276-302) — rename to reflect what it now proves. Replace the body with a `Sync`-equivalent assertion since `Copy` and `Sync` behave identically on first run:

```rust
    #[test]
    fn sync_mode_copies_host_auth_on_first_run() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();

        assert!(
            std::fs::read_to_string(&state.claude_json)
                .unwrap()
                .contains("test@example.com")
        );
        assert_eq!(
            std::fs::read_to_string(state.claude_dir.join(".credentials.json")).unwrap(),
            TEST_CREDENTIALS
        );
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
    }
```

`copy_mode_falls_back_to_empty_json_when_host_has_none` (lines 304-323) — rename and switch to `Sync`:

```rust
    #[test]
    fn sync_mode_falls_back_to_empty_json_when_host_has_none() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        // No host auth seeded
        let manifest = simple_manifest(&temp);

        let (state, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
        assert!(!state.claude_dir.join(".credentials.json").exists());
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    }
```

`switching_from_copy_to_ignore_revokes_forwarded_credentials` (lines 408-437) — rename to `switching_from_sync_to_ignore_revokes_forwarded_credentials` **if it doesn't already exist** (there's already one at 440-468, so the old `copy`-based test is redundant — delete it).

`auth_file_has_restricted_permissions` (lines 513-544) — change `AuthForwardMode::Copy` to `AuthForwardMode::Sync`. Body is unchanged.

`rejects_symlink_at_claude_json` (lines 654-693) — change `AuthForwardMode::Copy` in the two `prepare` calls to `AuthForwardMode::Sync`. Body unchanged.

`rejects_symlink_at_credentials_json` (lines 696-736) — change `AuthForwardMode::Copy` in the two `prepare` calls to `AuthForwardMode::Sync`. Body unchanged.

- [ ] **Step 2.4: Run tests — confirm instance module is green**

```bash
cargo nextest run -p jackin instance::
```

Expected: all green, with fewer tests (two deleted).

- [ ] **Step 2.5: Run full suite to catch anything else referencing `Copy`**

```bash
cargo build --all-targets
```

Expected: compiles cleanly with no warnings. If there are remaining references to `AuthForwardMode::Copy`, the compiler will point at them.

```bash
cargo nextest run
```

Expected: all green.

- [ ] **Step 2.6: Commit Task 2**

```bash
git add src/instance/auth.rs
git commit -s -m "$(cat <<'EOF'
refactor(instance): drop Copy arm in provision_claude_auth

Remove the AuthForwardMode::Copy match arm now that the variant no longer
exists. Tests that verified Copy-specific "never overwrite after first
run" semantics are deleted along with the semantic. Tests that used Copy
only because it was the default are migrated to Sync, which is now the
default and has equivalent first-run behavior.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Detect deprecated `copy` in config on load, rewrite to disk, print notice

**Files:**
- Modify: `src/config/persist.rs:1-46` (`load_or_init`, add detector helper)
- Modify: `src/tui/output.rs:130-140` (new `deprecation_warning` helper)
- Modify: `src/tui/mod.rs:26-30` (re-export)

- [ ] **Step 3.1: Add the `deprecation_warning` helper in `src/tui/output.rs`**

After the existing `fatal` function (around line 140), add:

```rust
/// One-line yellow deprecation warning to stderr. Used for soft-migration
/// notices like "config field X is deprecated — migrated to Y".
pub fn deprecation_warning(msg: &str) {
    const AMBER: (u8, u8, u8) = (230, 180, 80);
    eprintln!(
        "  {} {}",
        "warning:".color(rgb(AMBER)).bold(),
        msg.color(rgb(AMBER)),
    );
}
```

- [ ] **Step 3.2: Re-export `deprecation_warning` from `src/tui/mod.rs`**

Find the `pub use output::{ ... }` block around line 26 and add `deprecation_warning` to the list, alphabetically or at the end depending on existing style.

- [ ] **Step 3.3: Write failing test for migration detection + rewrite**

In `src/config/persist.rs`, at the bottom of `mod tests`:

```rust
#[test]
fn load_migrates_global_copy_to_sync_and_rewrites_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[claude]
auth_forward = "copy"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();

    // In memory, Copy normalized to Sync
    assert_eq!(
        config.claude.auth_forward,
        crate::config::AuthForwardMode::Sync
    );

    // On disk, "copy" no longer present
    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        !persisted.contains("auth_forward = \"copy\""),
        "expected on-disk config to be migrated; got:\n{persisted}"
    );
    assert!(
        persisted.contains("auth_forward = \"sync\""),
        "expected migrated config to contain sync; got:\n{persisted}"
    );
}

#[test]
fn load_migrates_per_agent_copy_to_sync() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.agent-smith.claude]
auth_forward = "copy"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();

    assert_eq!(
        config.resolve_auth_forward_mode("agent-smith"),
        crate::config::AuthForwardMode::Sync
    );

    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!persisted.contains("auth_forward = \"copy\""));
}

#[test]
fn load_does_not_rewrite_when_no_copy_present() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    // Bootstrap once so builtins are synced and file stabilizes.
    AppConfig::load_or_init(&paths).unwrap();
    let mtime_before = std::fs::metadata(&paths.config_file)
        .unwrap()
        .modified()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(50));

    // Second load with no "copy" anywhere — must not rewrite.
    AppConfig::load_or_init(&paths).unwrap();
    let mtime_after = std::fs::metadata(&paths.config_file)
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(mtime_before, mtime_after);
}
```

- [ ] **Step 3.4: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin config::persist::tests::load_migrates_global_copy_to_sync_and_rewrites_config config::persist::tests::load_migrates_per_agent_copy_to_sync
```

Expected: both tests fail — `load_or_init` currently doesn't rewrite when `copy` is present; even though the in-memory value is normalized to `Sync` (thanks to Task 1), the file still contains `"copy"`.

- [ ] **Step 3.5: Add the detector + migration in `load_or_init`**

Make two edits in `src/config/persist.rs`:

**Edit 1 — replace the `impl AppConfig { ... }` block** (lines 4–46 currently) with this exact block:

```rust
impl AppConfig {
    pub fn load_or_init(paths: &JackinPaths) -> anyhow::Result<Self> {
        paths.ensure_base_dirs()?;

        let contents_opt = match std::fs::read_to_string(&paths.config_file) {
            Ok(c) => Some(c),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };

        let deprecated_copy_seen = match &contents_opt {
            Some(c) => contains_deprecated_copy_auth_forward(c)?,
            None => false,
        };

        let mut config: Self = match contents_opt {
            Some(c) => toml::from_str(&c)?,
            None => Self::default(),
        };

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

        config.validate_workspaces()?;
        Ok(config)
    }

    pub fn save(&self, paths: &JackinPaths) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(self)?;
        let tmp = paths.config_file.with_extension("tmp");

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

        std::fs::rename(&tmp, &paths.config_file)?;
        Ok(())
    }
}
```

**Edit 2 — insert this new free function immediately after the closing `}` of the `impl` block, BEFORE the `#[cfg(test)] mod tests` block:**

```rust
/// Detect the literal deprecated `auth_forward = "copy"` at either of the
/// two known config paths: the global `[claude]` table or any
/// `[roles.*.claude]` table. Returns `true` if any occurrence is found.
///
/// Uses `toml::Value` (cheap — we parse the same string into `AppConfig`
/// right after) instead of a regex, so quoted keys with odd whitespace
/// are handled correctly.
fn contains_deprecated_copy_auth_forward(raw: &str) -> anyhow::Result<bool> {
    let value: toml::Value = toml::from_str(raw)?;

    // Global [claude] auth_forward
    if let Some(s) = value
        .get("claude")
        .and_then(|c| c.get("auth_forward"))
        .and_then(|v| v.as_str())
        && s == "copy"
    {
        return Ok(true);
    }

    // Per-agent [roles.<name>.claude] auth_forward
    if let Some(agents) = value.get("agents").and_then(|a| a.as_table()) {
        for agent in agents.values() {
            if let Some(s) = agent
                .get("claude")
                .and_then(|c| c.get("auth_forward"))
                .and_then(|v| v.as_str())
                && s == "copy"
            {
                return Ok(true);
            }
        }
    }

    Ok(false)
}
```

**Do not touch the existing `#[cfg(test)] mod tests` block** — the three failing tests from Step 3.3 are already inside it, and the existing `sync_does_not_rewrite_config_when_already_current` test stays untouched as the "no-op on clean config" regression guard.

- [ ] **Step 3.6: Run tests — confirm the new ones pass**

```bash
cargo nextest run -p jackin config::persist::tests
```

Expected: all four tests green.

- [ ] **Step 3.7: Run full suite for regressions**

```bash
cargo nextest run
```

Expected: all green.

- [ ] **Step 3.8: Commit Task 3**

```bash
git add src/config/persist.rs src/tui/output.rs src/tui/mod.rs
git commit -s -m "$(cat <<'EOF'
feat(config): migrate deprecated auth_forward "copy" to "sync" on load

On every load, scan the raw TOML for auth_forward = "copy" at the two
known paths (global [claude] and [roles.*.claude]). If found, normalize
to sync in memory, rewrite the config to disk via the existing save path,
and print a one-line deprecation warning naming the config file.

Add tui::deprecation_warning() — yellow one-liner to stderr, used for
soft migration notices like this one.

Existing behavior (builtin agent sync, mtime-invariant reload) is
preserved for configs that don't contain the deprecated literal.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: CLI emits deprecation warning when operator runs `jackin config auth set copy`

**Files:**
- Modify: `src/app/mod.rs:295-311` (CLI `AuthCommand::Set` handler)

- [ ] **Step 4.1: Write a failing test for the CLI deprecation path**

This handler is currently untested directly. The most focused seam is to extract the mode-parsing-and-warning logic into a small function and unit-test that function. Add to `src/app/mod.rs` inside its existing `#[cfg(test)] mod tests` if present, or create one at the bottom:

```rust
#[cfg(test)]
mod auth_set_tests {
    use super::*;

    #[test]
    fn parse_auth_forward_mode_from_cli_accepts_copy_as_deprecated() {
        let (mode, was_deprecated) =
            parse_auth_forward_mode_from_cli("copy").unwrap();
        assert_eq!(mode, crate::config::AuthForwardMode::Sync);
        assert!(was_deprecated);
    }

    #[test]
    fn parse_auth_forward_mode_from_cli_accepts_sync_non_deprecated() {
        let (mode, was_deprecated) =
            parse_auth_forward_mode_from_cli("sync").unwrap();
        assert_eq!(mode, crate::config::AuthForwardMode::Sync);
        assert!(!was_deprecated);
    }

    #[test]
    fn parse_auth_forward_mode_from_cli_rejects_bogus() {
        assert!(parse_auth_forward_mode_from_cli("bogus").is_err());
    }
}
```

- [ ] **Step 4.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin auth_set_tests
```

Expected: fails — `parse_auth_forward_mode_from_cli` does not exist.

- [ ] **Step 4.3: Add the helper and update the handler**

At a logical location in `src/app/mod.rs` (e.g. just above the main `run` function), add:

```rust
/// Parse an `auth_forward` mode value as it arrived from the CLI.
///
/// Returns the resolved mode and a boolean indicating whether the operator
/// passed the deprecated `copy` alias — the caller is responsible for
/// emitting a user-facing warning in that case.
fn parse_auth_forward_mode_from_cli(
    raw: &str,
) -> anyhow::Result<(config::AuthForwardMode, bool)> {
    let mode: config::AuthForwardMode =
        raw.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?;
    let was_deprecated = raw == "copy";
    Ok((mode, was_deprecated))
}
```

Replace the `AuthCommand::Set` arm (currently lines 296–311) with:

```rust
                cli::AuthCommand::Set { mode, agent } => {
                    let (parsed_mode, was_deprecated) =
                        parse_auth_forward_mode_from_cli(&mode)?;
                    if was_deprecated {
                        tui::deprecation_warning(
                            "auth_forward \"copy\" is deprecated; saving as \"sync\"",
                        );
                    }
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
                    Ok(())
                }
```

- [ ] **Step 4.4: Run tests — confirm all green**

```bash
cargo nextest run -p jackin auth_set_tests
cargo nextest run
```

Expected: all green.

- [ ] **Step 4.5: Commit Task 4**

```bash
git add src/app/mod.rs
git commit -s -m "$(cat <<'EOF'
feat(cli): emit deprecation warning when operator sets auth_forward = "copy"

Extract the CLI mode parse into parse_auth_forward_mode_from_cli, which
returns (mode, was_deprecated). The config auth set handler uses the
deprecation flag to emit a one-line warning via tui::deprecation_warning
before saving. The saved value is always the canonical sync; "copy" is
never written to disk.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Update CLI help text — remove `copy` from examples and mode list

**Files:**
- Modify: `src/cli/config.rs:13-57` (doc comments + `after_long_help` blocks)

- [ ] **Step 5.1: Write failing tests asserting help text no longer mentions `copy` as a mode**

Edit the existing `config_auth_set_help_shows_examples` test (around line 278):

```rust
    #[test]
    fn config_auth_set_help_shows_examples() {
        let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin config auth set sync"));
        assert!(help.contains("--agent"));
        assert!(
            !help.contains("jackin config auth set copy"),
            "help text must not recommend the deprecated copy mode"
        );
    }
```

Also add:

```rust
    #[test]
    fn config_auth_set_help_lists_modes_without_copy() {
        let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
        assert!(help.contains("sync"));
        assert!(help.contains("ignore"));
        // `copy` must not appear as an advertised mode. A word-boundary
        // check is close enough — the string "copy" should not show up
        // in help text once deprecated.
        assert!(
            !help.to_lowercase().contains("copy"),
            "help text must not mention copy; got:\n{help}"
        );
    }
```

- [ ] **Step 5.2: Run the tests — confirm they fail**

```bash
cargo nextest run -p jackin cli::config::tests::config_auth_set_help_shows_examples cli::config::tests::config_auth_set_help_lists_modes_without_copy
```

Expected: both fail — current help still shows `copy`.

- [ ] **Step 5.3: Update the `AuthCommand::Set` doc comment and `after_long_help`**

In `src/cli/config.rs`, replace lines 19–43 (the `Set` variant) with:

```rust
    /// Set the authentication forwarding mode
    ///
    /// Controls how the host's ~/.claude.json is forwarded into agent containers.
    /// Modes: sync (overwrite from host on each launch when host auth exists;
    /// preserve container auth when host auth is absent — default), ignore
    /// (revoke and never copy).
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth set sync
  jackin config auth set ignore
  jackin config auth set sync --agent agent-smith
  jackin config auth set ignore --agent chainargos/the-architect"
    )]
    Set {
        /// Authentication forwarding mode: sync or ignore
        mode: String,
        /// Apply to a specific agent instead of globally
        #[arg(long)]
        agent: Option<String>,
    },
```

Similarly update the `Show` variant's `after_long_help` to drop any `copy` references (there are none in the current snippet, but verify).

- [ ] **Step 5.4: Update the existing `parses_config_auth_set_global` test if it uses `copy`**

It does (line 294). Change the argument from `"copy"` to `"sync"`, and update the assertion:

```rust
    #[test]
    fn parses_config_auth_set_global() {
        let cli = Cli::try_parse_from(["jackin", "config", "auth", "set", "sync"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                        ref mode,
                        agent: None,
                    })) if mode == "sync"
        ));
    }
```

- [ ] **Step 5.5: Run tests — confirm all green**

```bash
cargo nextest run -p jackin cli::
cargo nextest run
```

Expected: all green.

- [ ] **Step 5.6: Commit Task 5**

```bash
git add src/cli/config.rs
git commit -s -m "$(cat <<'EOF'
docs(cli): remove "copy" from config auth set help text and examples

The copy mode is deprecated and no longer listed in `--help` output or
example blocks. Accepted modes are now sync (default) and ignore. The
CLI still accepts "copy" as a deprecated alias (handled in src/app/mod.rs)
but we don't advertise it.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Documentation updates

**Files:**
- Modify: `docs/src/content/docs/guides/authentication.mdx`
- Modify: `docs/src/content/docs/reference/configuration.mdx`
- Modify: `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`

- [ ] **Step 6.1: Update `authentication.mdx` mode table and prose**

Replace the table at `docs/src/content/docs/guides/authentication.mdx:27-31`:

```markdown
## Auth forwarding modes

jackin' supports two modes, configurable globally or per-agent:

| Mode | Behavior |
|---|---|
| `sync` (default) | When host auth exists, overwrite container auth on each launch. When host auth is absent, preserve existing container auth. |
| `ignore` | Never forward host auth. Revoke any previously forwarded credentials. The agent authenticates itself via `/login`. |
```

Delete the `### copy (default)` subsection (lines 33-37). Move the `### sync` subsection up, and update the opening line to "The `sync` mode is the default."

Add a new subsection after the mode descriptions:

```markdown
### `copy` (deprecated)

The `copy` mode — "copy host auth on first container creation only; never overwrite afterwards" — was jackin's previous default. It caused subtle auth drift when OAuth refresh tokens rotated across concurrent Claude Code sessions on the host and in containers, surfacing as intermittent `API Error: 401` inside agents.

It was removed in favor of `sync`. On the first launch after upgrading, jackin automatically rewrites any `auth_forward = "copy"` entries in your config to `"sync"` and prints a one-line deprecation notice:

\`\`\`
warning: migrated auth_forward "copy" → "sync" in ~/.config/jackin/config.toml (copy is deprecated)
\`\`\`

Scripts or invocations that still pass `copy` to `jackin config auth set` continue to work — the value is accepted as a deprecated alias and saved as `sync`.
```

Update the config example at lines 82–92 to use `sync`:

```toml
# Global default
[claude]
auth_forward = "sync"

# Per-agent override
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[roles.agent-smith.claude]
auth_forward = "ignore"
```

- [ ] **Step 6.2: Update `configuration.mdx` — default changed**

Search for `auth_forward` in `docs/src/content/docs/reference/configuration.mdx` and update any mention of "default: copy" to "default: sync", and remove `copy` from accepted-values lists.

- [ ] **Step 6.3: Update the roadmap's "Current State" section**

In `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`, replace the "Current State" list (lines 40–45):

```markdown
## Current State

The current operator-facing modes are:

- `sync` — overwrite container auth from host on each launch when host auth exists (default as of the `sync`-default release)
- `ignore` — never forward host auth; require in-container login

Historically there was also a `copy` mode (copy on first creation; never overwrite afterwards), which is deprecated. Configs that still declare `auth_forward = "copy"` are migrated to `sync` on load with a deprecation notice.
```

- [ ] **Step 6.4: Verify docs site still builds**

The docs are Astro/Starlight with Bun. From the `docs/` directory:

```bash
cd docs
bun install --frozen-lockfile
bun run build 2>&1 | tail -20
cd ..
```

Expected: build succeeds. If `bun install` is slow, the `node_modules` cache should make subsequent builds fast.

- [ ] **Step 6.5: Commit Task 6**

```bash
git add docs/src/content/docs/guides/authentication.mdx \
  docs/src/content/docs/reference/configuration.mdx \
  docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx
git commit -s -m "$(cat <<'EOF'
docs(auth): sync is default, copy is deprecated

Update the authentication guide, configuration reference, and the
claude-auth-strategy roadmap to reflect that sync is now the default
auth_forward mode and copy is deprecated (auto-migrated on load).

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: CHANGELOG entry

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 7.1: Read the existing CHANGELOG to match style**

```bash
head -30 CHANGELOG.md
```

- [ ] **Step 7.2: Add `Changed` and `Deprecated` entries under `## [Unreleased]`**

If an `## [Unreleased]` section does not exist at the top, add one. Under it, add (following existing keep-a-changelog-style headings):

```markdown
### Changed

- `auth_forward` default is now `sync` (was `copy`). Existing configs that declare `auth_forward = "copy"` are migrated to `"sync"` on the next `jackin` launch, with a one-line deprecation warning. This resolves the intermittent `API Error: 401` that operators saw when multiple Claude Code sessions ran concurrently — see the Claude auth strategy roadmap for context. [#<pr-number>]

### Deprecated

- `auth_forward = "copy"` is deprecated. The value is still accepted by the CLI (`jackin config auth set copy`) and by the TOML config as a compatibility alias, but it resolves to `"sync"` and a deprecation warning is printed. Update your configs to `"sync"` directly. [#<pr-number>]
```

Leave `<pr-number>` as a literal placeholder for now; it will be filled in once the PR is opened.

- [ ] **Step 7.3: Commit Task 7**

```bash
git add CHANGELOG.md
git commit -s -m "$(cat <<'EOF'
docs(changelog): record sync-default and copy-deprecation changes

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Final verification

- [ ] **Step 8.1: Full pre-commit gate**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all three exit 0, zero warnings, zero failures. If clippy flags anything, fix it in a separate `style:` commit.

- [ ] **Step 8.2: Manual smoke — config with `copy` auto-migrates**

```bash
mkdir -p /tmp/jackin-migration-test
cat > /tmp/jackin-migration-test/config.toml <<'EOF'
[claude]
auth_forward = "copy"
EOF
JACKIN_CONFIG=/tmp/jackin-migration-test/config.toml cargo run -- config auth show 2>&1 | head -10
```

Expected output contains both:
- `warning: migrated auth_forward "copy" → "sync" in ...`
- `sync` (printed by `config auth show`)

Also verify the file was rewritten:

```bash
cat /tmp/jackin-migration-test/config.toml
```

Expected: contains `auth_forward = "sync"`, not `"copy"`.

> Note: `JACKIN_CONFIG` is a hypothetical override — adapt this step to whatever `paths.rs` supports. If there is no env-var override, this manual smoke runs against `~/.config/jackin/config.toml` instead; back it up first.

- [ ] **Step 8.3: Manual smoke — CLI deprecation warning**

```bash
cargo run -- config auth set copy 2>&1 | head -5
cargo run -- config auth show
```

Expected: first command prints `warning: auth_forward "copy" is deprecated; saving as "sync"`. Second command prints `sync`.

- [ ] **Step 8.4: Verify commit log is clean and DCO-signed**

```bash
git log main..HEAD --oneline
git log main..HEAD --format="%B" | grep -c "Signed-off-by"
git log main..HEAD --format="%B" | grep -c "Co-authored-by: Claude"
```

Expected: 7 commits (one per task from 1–7). `Signed-off-by` count = 7. `Co-authored-by: Claude` count = 7.

- [ ] **Step 8.5: Push and open the PR**

```bash
git push -u origin feature/auth-sync-default
gh pr create --title "feat(config)!: auth_forward sync-default, deprecate copy" --body "$(cat <<'BODY'
## Summary

Flips the `auth_forward` default from `copy` to `sync`, removes the `AuthForwardMode::Copy` enum variant, and migrates existing configs in place with a one-line deprecation notice. Fixes the intermittent `API Error: 401` that operators see when multiple Claude Code sessions run concurrently across host and jackin containers.

- On load: any `auth_forward = "copy"` at `[claude]` or `[roles.*.claude]` is normalized to `sync`, written back to disk, and a warning is printed.
- CLI: `jackin config auth set copy` is accepted, prints a deprecation warning, and saves as `sync`.
- Rust surface: `AuthForwardMode::Copy` is removed. External code that matched on it must update.
- TOML/CLI surface: `"copy"` remains a deprecated alias.

Delivers PR 1 of the three-PR Claude auth strategy series. Spec: `docs/superpowers/specs/2026-04-23-auth-sync-default-design.md`. Plan: `docs/superpowers/plans/2026-04-23-auth-sync-default.md`.

## Test plan

- [x] `cargo fmt -- --check && cargo clippy && cargo nextest run` — all green, zero warnings.
- [x] Manual: config with `auth_forward = "copy"` triggers migration notice and on-disk rewrite.
- [x] Manual: `jackin config auth set copy` prints deprecation warning and saves `sync`.
- [x] Manual: docs site builds (`bun run build`).
- [ ] Reviewer confirms operator-facing messaging is clear.
- [ ] Reviewer confirms test coverage in `src/config/{mod,persist,agents}.rs` and `src/instance/auth.rs`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
BODY
)"
```

Return the PR URL; **do not merge** (per `AGENTS.md`, agents must never merge a PR without explicit per-PR operator confirmation).

---

## Self-Review Checklist (for the implementer)

Before marking this plan complete:

- [ ] No remaining references to `AuthForwardMode::Copy` in the code (grep: `rg 'AuthForwardMode::Copy' src/`)
- [ ] No remaining non-test TOML examples containing `auth_forward = "copy"` in the repo
- [ ] `CHANGELOG.md` PR-number placeholder has been replaced with the actual PR number after the PR is opened
- [ ] All seven commits carry both `Signed-off-by:` and `Co-authored-by: Claude <noreply@anthropic.com>`
- [ ] Pre-commit gate is clean on every commit, not just the final one (run after each task; fix fails before moving on)
- [ ] Manual smoke from Step 8.2 / 8.3 succeeded on the implementer's machine

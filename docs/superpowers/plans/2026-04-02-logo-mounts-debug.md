# Logo, Global Mounts, Debug Output & UX Polish — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add agent repo logo display, configurable Docker mounts with scoped matching, debug output wiring, and UX polish (suppressed git noise, deploying message timing).

**Architecture:** Four independent features layered onto the existing load lifecycle. The mount system extends `AppConfig` with a new `DockerConfig` section and adds `config mount` CLI subcommands. Logo, debug, and UX changes are localized to `runtime.rs` and `tui.rs`. The `CommandRunner` trait gains a `capture_silent` method so git clone/pull output can be suppressed without changing existing callers.

**Tech Stack:** Rust, clap (CLI), serde/toml (config), owo-colors (TUI), tempfile (tests)

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `src/tui.rs` | Add `print_logo` function | Modify |
| `src/config.rs` | Add `DockerConfig`, `MountConfig`, mount resolution and CRUD logic | Modify |
| `src/cli.rs` | Add `Config` subcommand with `mount add/remove/list` | Modify |
| `src/lib.rs` | Route `config mount` subcommands | Modify |
| `src/runtime.rs` | Logo display, step 1 "Resolving agent identity", debug flag wiring, deploying timing, mount injection into `docker run` | Modify |
| `src/docker.rs` | No changes needed — git clone/pull already uses `capture` which suppresses stdout. The `run` method inherits stdout which caused the "Already up to date" leak; fix is to switch those calls to `capture` in `runtime.rs`. | No change |

---

### Task 1: Add `print_logo` to TUI

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Write the `print_logo` function**

Add to the end of `src/tui.rs`, before the `// ── Utility` section:

```rust
// ── Logo ─────────────────────────────────────────────────────────────

pub fn print_logo(logo_path: &std::path::Path) {
    let contents = match std::fs::read_to_string(logo_path) {
        Ok(c) if !c.trim().is_empty() => c,
        _ => return,
    };

    eprintln!();
    for line in contents.lines() {
        eprintln!("  {}", line.color(rgb(PHOSPHOR_GREEN)));
    }
    eprintln!();
}
```

- [ ] **Step 2: Run `cargo check`**

Run: `cargo check`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "feat: add print_logo to TUI module"
```

---

### Task 2: Display logo during load

**Files:**
- Modify: `src/runtime.rs`

- [ ] **Step 1: Call `print_logo` after Matrix intro, before config table**

In `load_agent`, after `tui::set_terminal_title(&agent_display_name);` and before the config table block, add:

```rust
    // Logo (if present in agent repo)
    tui::print_logo(&cached_repo.repo_dir.join("logo.txt"));
```

This goes right before the existing line:

```rust
    // Configuration summary
    let config_rows = build_config_rows(
```

- [ ] **Step 2: Run existing tests**

Run: `cargo nextest run`
Expected: all tests pass (logo file won't exist in test repos, so `print_logo` silently skips)

- [ ] **Step 3: Commit**

```bash
git add src/runtime.rs
git commit -m "feat: display agent repo logo during load"
```

---

### Task 3: Add `MountConfig` and `DockerConfig` structs to config

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing test for mount config deserialization**

Add to the `#[cfg(test)] mod tests` block in `src/config.rs`:

```rust
    #[test]
    fn deserializes_global_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/claude/.gradle/caches" }
gradle-wrapper = { src = "~/.gradle/wrapper", dst = "/home/claude/.gradle/wrapper", readonly = true }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();

        let global = config.docker.mounts.get("").unwrap();
        assert_eq!(global.len(), 2);

        let cache = global.get("gradle-cache").unwrap();
        assert_eq!(cache.src, "~/.gradle/caches");
        assert_eq!(cache.dst, "/home/claude/.gradle/caches");
        assert!(!cache.readonly);

        let wrapper = global.get("gradle-wrapper").unwrap();
        assert!(wrapper.readonly);
    }

    #[test]
    fn deserializes_scoped_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "~/.chainargos/brown", dst = "/config" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();

        let wildcard = config.docker.mounts.get("chainargos/*").unwrap();
        assert_eq!(wildcard.len(), 1);
        assert!(wildcard.get("chainargos-secrets").unwrap().readonly);

        let exact = config.docker.mounts.get("chainargos/agent-brown").unwrap();
        assert_eq!(exact.get("brown-config").unwrap().dst, "/config");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run deserializes_global_docker_mounts deserializes_scoped_docker_mounts`
Expected: FAIL — `DockerConfig` field does not exist on `AppConfig`

- [ ] **Step 3: Implement the structs**

Add these structs before `impl AppConfig` in `src/config.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default, flatten)]
    pub mounts: BTreeMap<String, BTreeMap<String, MountConfig>>,
}
```

Wait — TOML flattening with serde is tricky here. The TOML layout is:

```toml
[docker.mounts]
gradle-cache = { src = "...", dst = "..." }

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "...", dst = "...", readonly = true }
```

In TOML, `[docker.mounts]` is a table where keys are mount names (global scope), and `[docker.mounts."chainargos/*"]` is a sub-table. This naturally deserializes as a nested map where the scope key `""` doesn't exist — the global mounts are at the top level of `docker.mounts` alongside the scope keys.

This is a problem: global mounts like `gradle-cache = { ... }` and scope keys like `"chainargos/*" = { ... }` live at the same level but have different value types (MountConfig vs BTreeMap<String, MountConfig>).

We need a custom approach. The cleanest TOML-native approach: use a dedicated scope key for globals too. But that changes the user-facing format. Instead, let's use a custom deserializer.

Actually, let's use a simpler TOML structure that avoids the ambiguity:

```toml
[docker.mounts.global]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/claude/.gradle/caches" }

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }
```

No — the user explicitly wanted `[docker.mounts]` for global. Let's implement this with an untagged enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountEntry {
    Mount(MountConfig),
    Scoped(BTreeMap<String, MountConfig>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts {
    #[serde(flatten)]
    pub entries: BTreeMap<String, MountEntry>,
}
```

With this, `gradle-cache = { src, dst }` parses as `MountEntry::Mount` and `"chainargos/*" = { chainargos-secrets = { src, dst } }` parses as `MountEntry::Scoped`.

Actually, `MountEntry::Scoped` won't work because `{ chainargos-secrets = { src, dst } }` has a nested table, while `MountConfig` has `src`+`dst`+`readonly`. Serde's untagged enum tries each variant in order — `MountConfig` will fail (no `src` field), then `BTreeMap<String, MountConfig>` will succeed. This should work.

Here's the corrected implementation:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountEntry {
    Mount(MountConfig),
    Scoped(BTreeMap<String, MountConfig>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts(pub BTreeMap<String, MountEntry>);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: DockerMounts,
}
```

And add to `AppConfig`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSource>,
    #[serde(default)]
    pub docker: DockerConfig,
}
```

Now update the tests accordingly. Global mounts are `MountEntry::Mount`, scoped mounts are `MountEntry::Scoped`:

Replace the two tests from Step 1 with:

```rust
    #[test]
    fn deserializes_global_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/claude/.gradle/caches" }
gradle-wrapper = { src = "~/.gradle/wrapper", dst = "/home/claude/.gradle/wrapper", readonly = true }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();

        let mounts = &config.docker.mounts.0;
        match mounts.get("gradle-cache").unwrap() {
            MountEntry::Mount(m) => {
                assert_eq!(m.src, "~/.gradle/caches");
                assert_eq!(m.dst, "/home/claude/.gradle/caches");
                assert!(!m.readonly);
            }
            _ => panic!("expected MountEntry::Mount"),
        }
        match mounts.get("gradle-wrapper").unwrap() {
            MountEntry::Mount(m) => assert!(m.readonly),
            _ => panic!("expected MountEntry::Mount"),
        }
    }

    #[test]
    fn deserializes_scoped_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "~/.chainargos/brown", dst = "/config" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();

        let mounts = &config.docker.mounts.0;
        match mounts.get("chainargos/*").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("chainargos-secrets").unwrap();
                assert_eq!(m.dst, "/secrets");
                assert!(m.readonly);
            }
            _ => panic!("expected MountEntry::Scoped"),
        }
        match mounts.get("chainargos/agent-brown").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("brown-config").unwrap();
                assert_eq!(m.dst, "/config");
                assert!(!m.readonly);
            }
            _ => panic!("expected MountEntry::Scoped"),
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run deserializes_global_docker_mounts deserializes_scoped_docker_mounts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add MountConfig and DockerConfig to AppConfig"
```

---

### Task 4: Mount resolution logic

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing test for `resolve_mounts`**

Add to tests in `src/config.rs`:

```rust
    #[test]
    fn resolve_mounts_collects_global_and_matching_scopes() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "/tmp/gradle-caches", dst = "/home/claude/.gradle/caches" }

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "/tmp/chainargos-secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "/tmp/chainargos-brown", dst = "/config" }

[docker.mounts."other/*"]
other-data = { src = "/tmp/other", dst = "/other" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");

        let resolved = config.resolve_mounts(&selector);

        assert_eq!(resolved.len(), 3);
        assert!(resolved.iter().any(|m| m.dst == "/home/claude/.gradle/caches"));
        assert!(resolved.iter().any(|m| m.dst == "/secrets" && m.readonly));
        assert!(resolved.iter().any(|m| m.dst == "/config" && !m.readonly));
    }

    #[test]
    fn resolve_mounts_exact_overrides_global_with_same_name() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
shared = { src = "/tmp/global", dst = "/data" }

[docker.mounts."chainargos/agent-brown"]
shared = { src = "/tmp/specific", dst = "/data" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");

        let resolved = config.resolve_mounts(&selector);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].src, "/tmp/specific");
    }

    #[test]
    fn resolve_mounts_returns_empty_when_no_mounts_configured() {
        let config = AppConfig::default();
        let selector = ClassSelector::new(None, "agent-smith");

        let resolved = config.resolve_mounts(&selector);

        assert!(resolved.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run resolve_mounts`
Expected: FAIL — `resolve_mounts` method does not exist

- [ ] **Step 3: Implement `resolve_mounts`**

Add this method to `impl AppConfig`:

```rust
    pub fn resolve_mounts(&self, selector: &ClassSelector) -> Vec<MountConfig> {
        let mut by_name: BTreeMap<String, MountConfig> = BTreeMap::new();

        // Priority order: global < wildcard < exact (later inserts override earlier)
        let scopes = [
            None,                                                          // global
            selector.namespace.as_ref().map(|ns| format!("{ns}/*")),       // wildcard
            Some(selector.key()),                                          // exact
        ];

        for scope in &scopes {
            let entries = match scope {
                None => {
                    // Collect global mounts (MountEntry::Mount at top level)
                    let mut map = BTreeMap::new();
                    for (name, entry) in &self.docker.mounts.0 {
                        if let MountEntry::Mount(m) = entry {
                            map.insert(name.clone(), m.clone());
                        }
                    }
                    map
                }
                Some(scope_key) => {
                    match self.docker.mounts.0.get(scope_key) {
                        Some(MountEntry::Scoped(scope_map)) => scope_map.clone(),
                        _ => continue,
                    }
                }
            };

            for (name, mount) in entries {
                by_name.insert(name, mount);
            }
        }

        by_name.into_values().collect()
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run resolve_mounts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add mount resolution with global/wildcard/exact scoping"
```

---

### Task 5: Mount validation

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing tests for validation**

Add to tests in `src/config.rs`:

```rust
    #[test]
    fn validate_mounts_rejects_missing_src() {
        let mounts = vec![MountConfig {
            src: "/nonexistent/path/that/does/not/exist".to_string(),
            dst: "/data".to_string(),
            readonly: false,
        }];

        let err = AppConfig::validate_mounts(&mounts).unwrap_err();

        assert!(err.to_string().contains("/nonexistent/path/that/does/not/exist"));
    }

    #[test]
    fn validate_mounts_rejects_relative_dst() {
        let temp = tempdir().unwrap();
        let mounts = vec![MountConfig {
            src: temp.path().display().to_string(),
            dst: "relative/path".to_string(),
            readonly: false,
        }];

        let err = AppConfig::validate_mounts(&mounts).unwrap_err();

        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn validate_mounts_rejects_duplicate_dst() {
        let temp = tempdir().unwrap();
        let src = temp.path().display().to_string();
        let mounts = vec![
            MountConfig { src: src.clone(), dst: "/data".to_string(), readonly: false },
            MountConfig { src, dst: "/data".to_string(), readonly: true },
        ];

        let err = AppConfig::validate_mounts(&mounts).unwrap_err();

        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_mounts_expands_tilde_in_src() {
        let home = std::env::var("HOME").unwrap();
        let mounts = vec![MountConfig {
            src: "~".to_string(),
            dst: "/home-mount".to_string(),
            readonly: true,
        }];

        let validated = AppConfig::validate_mounts(&mounts).unwrap();

        assert_eq!(validated[0].src, home);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run validate_mounts`
Expected: FAIL — `validate_mounts` does not exist

- [ ] **Step 3: Implement `validate_mounts`**

Add to `impl AppConfig`:

```rust
    pub fn validate_mounts(mounts: &[MountConfig]) -> anyhow::Result<Vec<MountConfig>> {
        let mut validated = Vec::new();
        let mut seen_dst = std::collections::HashSet::new();

        for mount in mounts {
            let expanded_src = expand_tilde(&mount.src);

            if !std::path::Path::new(&expanded_src).exists() {
                anyhow::bail!(
                    "mount source does not exist: {} (expanded from {:?})",
                    expanded_src,
                    mount.src,
                );
            }

            if !mount.dst.starts_with('/') {
                anyhow::bail!(
                    "mount destination must be an absolute path: {}",
                    mount.dst,
                );
            }

            if !seen_dst.insert(mount.dst.clone()) {
                anyhow::bail!(
                    "duplicate mount destination: {}",
                    mount.dst,
                );
            }

            validated.push(MountConfig {
                src: expanded_src,
                dst: mount.dst.clone(),
                readonly: mount.readonly,
            });
        }

        Ok(validated)
    }
```

Add the `expand_tilde` helper as a free function in `src/config.rs`:

```rust
fn expand_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run validate_mounts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add mount validation with tilde expansion"
```

---

### Task 6: Inject mounts into `docker run` args

**Files:**
- Modify: `src/runtime.rs`

- [ ] **Step 1: Write failing test for mount injection**

Add to tests in `src/runtime.rs`:

```rust
    #[test]
    fn load_agent_injects_configured_mounts() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");
        let mut runner = FakeRunner::with_capture_queue([
            String::new(),
            "jackin-chainargos-agent-brown".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("chainargos").join("agent-brown");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        // Create a mount source directory
        let mount_src = temp.path().join("test-mount");
        std::fs::create_dir_all(&mount_src).unwrap();

        // Write config with a scoped mount
        let config_content = format!(
            r#"[agents."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"

[docker.mounts."chainargos/*"]
test-mount = {{ src = "{}", dst = "/test-data", readonly = true }}
"#,
            mount_src.display()
        );
        std::fs::write(&paths.config_file, &config_content).unwrap();
        config = AppConfig::load_or_init(&paths).unwrap();

        load_agent(&paths, &mut config, &selector, &mut runner, &LoadOptions::default()).unwrap();

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -it"))
            .unwrap();
        assert!(run_cmd.contains(&format!("{}:/test-data:ro", mount_src.display())));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run load_agent_injects_configured_mounts`
Expected: FAIL — no mount injection in `docker run` args yet

- [ ] **Step 3: Implement mount injection in `load_agent`**

In `src/runtime.rs` inside `load_agent`, after the `AgentState::prepare` call and before the `docker run` args are built, add mount resolution and validation:

```rust
    let resolved_mounts = config.resolve_mounts(selector);
    let validated_mounts = AppConfig::validate_mounts(&resolved_mounts)?;
```

Then in the `docker run` args construction (the closure), after the existing `-v` entries for `.jackin/plugins.json:ro` and before `image.clone()`, add:

```rust
                // User-configured mounts
                for mount in &validated_mounts {
                    let suffix = if mount.readonly { ":ro" } else { "" };
                    args.extend([
                        "-v".into(),
                        format!("{}:{}{}", mount.src, mount.dst, suffix),
                    ]);
                }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run load_agent_injects_configured_mounts`
Expected: PASS

- [ ] **Step 5: Run all tests**

Run: `cargo nextest run`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/runtime.rs
git commit -m "feat: inject configured mounts into docker run"
```

---

### Task 7: CLI `config mount` subcommands

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/lib.rs`
- Modify: `src/config.rs`

- [ ] **Step 1: Add `Config` subcommand to CLI**

Replace the `Command` enum in `src/cli.rs`:

```rust
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Load {
        selector: String,
        /// Skip the Matrix intro/outro animations
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        /// Show verbose output (e.g. Docker build logs)
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    Hardline { container: String },
    Eject {
        selector: String,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        purge: bool,
    },
    Exile,
    Purge {
        selector: String,
        #[arg(long)]
        all: bool,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    Mount {
        #[command(subcommand)]
        command: MountCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum MountCommand {
    Add {
        /// Mount name (used as identifier for removal)
        name: String,
        /// Host source path
        #[arg(long)]
        src: String,
        /// Container destination path
        #[arg(long)]
        dst: String,
        /// Mount as read-only
        #[arg(long, default_value_t = false)]
        readonly: bool,
        /// Scope pattern (e.g. "chainargos/*" or "chainargos/agent-brown")
        #[arg(long)]
        scope: Option<String>,
    },
    Remove {
        /// Mount name to remove
        name: String,
        /// Scope pattern to remove from
        #[arg(long)]
        scope: Option<String>,
    },
    List,
}
```

- [ ] **Step 2: Add mount CRUD methods to `AppConfig`**

Add to `impl AppConfig` in `src/config.rs`:

```rust
    pub fn add_mount(
        &mut self,
        name: &str,
        mount: MountConfig,
        scope: Option<&str>,
    ) {
        let scope_key = scope.unwrap_or("");
        if scope_key.is_empty() {
            // Global: insert as MountEntry::Mount at top level
            self.docker.mounts.0.insert(name.to_string(), MountEntry::Mount(mount));
        } else {
            // Scoped: insert into MountEntry::Scoped map
            match self.docker.mounts.0.entry(scope_key.to_string()) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if let MountEntry::Scoped(ref mut map) = entry.get_mut() {
                        map.insert(name.to_string(), mount);
                    }
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let mut map = BTreeMap::new();
                    map.insert(name.to_string(), mount);
                    entry.insert(MountEntry::Scoped(map));
                }
            }
        }
    }

    pub fn remove_mount(&mut self, name: &str, scope: Option<&str>) -> bool {
        let scope_key = scope.unwrap_or("");
        if scope_key.is_empty() {
            self.docker.mounts.0.remove(name).is_some()
        } else {
            match self.docker.mounts.0.get_mut(scope_key) {
                Some(MountEntry::Scoped(map)) => {
                    let removed = map.remove(name).is_some();
                    if map.is_empty() {
                        self.docker.mounts.0.remove(scope_key);
                    }
                    removed
                }
                _ => false,
            }
        }
    }

    pub fn list_mounts(&self) -> Vec<(String, String, &MountConfig)> {
        let mut result = Vec::new();
        for (key, entry) in &self.docker.mounts.0 {
            match entry {
                MountEntry::Mount(m) => {
                    result.push(("(global)".to_string(), key.clone(), m));
                }
                MountEntry::Scoped(map) => {
                    for (name, m) in map {
                        result.push((key.clone(), name.clone(), m));
                    }
                }
            }
        }
        result
    }
```

- [ ] **Step 3: Route `config mount` commands in `lib.rs`**

Add the `Config` match arm inside the `match cli.command` block in `src/lib.rs`:

```rust
        Command::Config { command: config_cmd } => match config_cmd {
            cli::ConfigCommand::Mount { command: mount_cmd } => {
                match mount_cmd {
                    cli::MountCommand::Add { name, src, dst, readonly, scope } => {
                        let mount = config::MountConfig { src, dst, readonly };
                        config.add_mount(&name, mount, scope.as_deref());
                        config.save(&paths)?;
                        Ok(())
                    }
                    cli::MountCommand::Remove { name, scope } => {
                        if config.remove_mount(&name, scope.as_deref()) {
                            config.save(&paths)?;
                        }
                        Ok(())
                    }
                    cli::MountCommand::List => {
                        let mounts = config.list_mounts();
                        if mounts.is_empty() {
                            println!("No mounts configured.");
                        } else {
                            for (scope, name, m) in &mounts {
                                let ro = if m.readonly { " (ro)" } else { "" };
                                println!("{scope}  {name}  {} -> {}{ro}", m.src, m.dst);
                            }
                        }
                        Ok(())
                    }
                }
            }
        },
```

Add `use config::MountConfig;` is not needed since we qualify through `config::`.

- [ ] **Step 4: Update CLI test**

Add to tests in `src/cli.rs`:

```rust
    #[test]
    fn parses_config_mount_add() {
        let cli = Cli::try_parse_from([
            "jackin", "config", "mount", "add", "gradle-cache",
            "--src", "~/.gradle/caches",
            "--dst", "/home/claude/.gradle/caches",
            "--readonly",
            "--scope", "chainargos/*",
        ]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config { command: ConfigCommand::Mount { command: MountCommand::Add { .. } } }
        ));
    }

    #[test]
    fn parses_config_mount_remove() {
        let cli = Cli::try_parse_from([
            "jackin", "config", "mount", "remove", "gradle-cache",
        ]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config { command: ConfigCommand::Mount { command: MountCommand::Remove { .. } } }
        ));
    }

    #[test]
    fn parses_config_mount_list() {
        let cli = Cli::try_parse_from([
            "jackin", "config", "mount", "list",
        ]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Config { command: ConfigCommand::Mount { command: MountCommand::List } }
        ));
    }
```

- [ ] **Step 5: Run all tests**

Run: `cargo nextest run`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/config.rs src/lib.rs
git commit -m "feat: add jackin config mount CLI subcommands"
```

---

### Task 8: Suppress git noise and add "Resolving agent identity" step

**Files:**
- Modify: `src/runtime.rs`

- [ ] **Step 1: Write failing test that verifies git clone/pull uses `capture`**

Add to tests in `src/runtime.rs`:

```rust
    #[test]
    fn load_agent_uses_capture_for_git_operations() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let mut runner = FakeRunner::with_capture_queue([
            // git pull output (captured, not displayed)
            "Already up to date.".to_string(),
            // docker ps output
            "jackin-agent-smith".to_string(),
        ]);

        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        load_agent(&paths, &mut config, &selector, &mut runner, &LoadOptions::default()).unwrap();

        // Verify git operation used capture (appears in recorded commands)
        // and the first docker command is the build (not git — git uses capture too)
        let git_cmd = runner.recorded.iter().find(|c| c.contains("git")).unwrap();
        assert!(git_cmd.contains("git -C") || git_cmd.contains("git clone"));
    }
```

- [ ] **Step 2: Switch git clone/pull from `run` to `capture` in `load_agent`**

In `src/runtime.rs`, inside `load_agent`, replace the git clone/pull block:

```rust
    // Step 1: Resolve agent identity (clone or update repo)
    if !opts.no_intro {
        tui::step_shimmer(step, "Resolving agent identity");
    }
    step += 1;

    let cached_repo = CachedRepo::new(paths, selector);
    std::fs::create_dir_all(cached_repo.repo_dir.parent().unwrap())?;

    if cached_repo.repo_dir.exists() {
        runner.capture(
            "git",
            &[
                "-C".into(),
                cached_repo.repo_dir.display().to_string(),
                "pull".into(),
                "--ff-only".into(),
            ],
            None,
        )?;
    } else {
        runner.capture(
            "git",
            &[
                "clone".into(),
                source.git.clone(),
                cached_repo.repo_dir.display().to_string(),
            ],
            None,
        )?;
    }
```

Note: The `step` counter needs to be initialized before this block. Move `let mut step = 1u32;` up to right after the Matrix intro block, before the git operations.

Also move the shimmer steps: the existing steps start at 1 for "Building Docker image". Now "Resolving agent identity" is step 1, and the old steps shift by 1.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo nextest run`
Expected: all tests pass (existing tests use `FakeRunner` which treats `run` and `capture` identically for recording)

- [ ] **Step 4: Commit**

```bash
git add src/runtime.rs
git commit -m "feat: add 'Resolving agent identity' step, suppress git noise"
```

---

### Task 9: Wire debug flag to Docker commands

**Files:**
- Modify: `src/docker.rs`
- Modify: `src/runtime.rs`

- [ ] **Step 1: Add `run_visible` method to `CommandRunner` trait**

In `src/docker.rs`, add a new method to the trait that inherits stdout/stderr (for debug mode). Add it to the trait:

```rust
pub trait CommandRunner {
    fn run(&mut self, program: &str, args: &[String], cwd: Option<&Path>) -> anyhow::Result<()>;
    fn capture(
        &mut self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
}
```

Actually, `run` already inherits stdout/stderr (it uses `.status()` not `.output()`). The issue is that `runtime.rs` uses `runner.run(...)` for docker commands and it shows output. For non-debug mode, docker build should use `capture` to suppress output, and for debug mode it should use `run`.

Looking at the current code: `runner.run("docker", &["build"...])` already shows output. So the fix is:

- In non-debug mode (current behavior needs changing): use `capture` for docker build to suppress output
- In debug mode: use `run` for docker build to show output

Let's pass `debug` into the load closure and conditionally choose:

```rust
        // Step 2: Build Docker image
        tui::step_shimmer(step, "Building Docker image");
        step += 1;
        let build_args = [
            "build".into(),
            "-t".into(),
            image.clone(),
            "-f".into(),
            build.dockerfile_path.display().to_string(),
            build.context_dir.display().to_string(),
        ];
        if opts.debug {
            runner.run("docker", &build_args, None)?;
        } else {
            runner.capture("docker", &build_args, None)?;
        }
```

Apply the same pattern for network creation, DinD start, and DinD readiness polling. For network and DinD, they already use `runner.run(...)` which shows output. Change the non-debug path to `capture`:

```rust
        // Step 3: Create Docker network
        tui::step_shimmer(step, "Creating Docker network");
        step += 1;
        let net_args = ["network".into(), "create".into(), network.clone()];
        if opts.debug {
            runner.run("docker", &net_args, None)?;
        } else {
            runner.capture("docker", &net_args, None)?;
        }

        // Step 4: Start Docker-in-Docker
        tui::step_shimmer(step, "Starting Docker-in-Docker container");
        step += 1;
        let dind_args = [
            "run".into(), "-d".into(), "--name".into(), dind.clone(),
            "--network".into(), network.clone(),
            "--privileged".into(), "docker:dind".into(),
        ];
        if opts.debug {
            runner.run("docker", &dind_args, None)?;
        } else {
            runner.capture("docker", &dind_args, None)?;
        }
```

- [ ] **Step 2: Update `wait_for_dind` for debug mode**

Change the `wait_for_dind` function signature to accept `debug`:

```rust
fn wait_for_dind(dind_name: &str, runner: &mut impl CommandRunner, debug: bool) -> anyhow::Result<()> {
    for _ in 0..30 {
        let result = runner.capture(
            "docker",
            &[
                "exec".into(),
                dind_name.to_string(),
                "docker".into(),
                "info".into(),
            ],
            None,
        );
        if result.is_ok() {
            return Ok(());
        }
        if debug {
            if let Err(ref e) = result {
                eprintln!("  DinD not ready: {e}");
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    anyhow::bail!("timed out waiting for Docker-in-Docker sidecar {dind_name}")
}
```

Update the call site: `wait_for_dind(&dind, runner, opts.debug)?;`

- [ ] **Step 3: Run all tests**

Run: `cargo nextest run`
Expected: all tests pass (FakeRunner's `run` and `capture` both record the same way)

- [ ] **Step 4: Commit**

```bash
git add src/runtime.rs
git commit -m "feat: wire --debug flag to show Docker command output"
```

---

### Task 10: Fix deploying message timing

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Increase the sleep duration in `print_deploying`**

In `src/tui.rs`, change the `print_deploying` function:

```rust
pub fn print_deploying(agent_name: &str) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("Deploying {agent_name} into the Matrix...")
            .color(rgb(PHOSPHOR_GREEN))
            .bold()
    );
    eprintln!();

    std::thread::sleep(std::time::Duration::from_millis(1500));
    clear_screen();
}
```

Change `800` to `1500`.

- [ ] **Step 2: Run `cargo check`**

Run: `cargo check`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "fix: increase deploying message visibility to 1500ms"
```

---

### Task 11: Final integration test and cleanup

**Files:**
- No new files

- [ ] **Step 1: Run the full test suite**

Run: `cargo nextest run`
Expected: all tests pass

- [ ] **Step 2: Run `cargo clippy`**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Final commit if any clippy fixes were needed**

```bash
git add -A
git commit -m "chore: fix clippy warnings"
```

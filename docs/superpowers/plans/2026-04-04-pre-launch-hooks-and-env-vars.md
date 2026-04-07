# Pre-Launch Hooks and Runtime Environment Variables Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add pre-launch hook support and interactive runtime environment variable declarations to jackin agent manifests, unblocking the ChainArgos migration.

**Architecture:** The agent manifest (`jackin.agent.toml`) gains `[hooks]` and `[env.*]` sections. A new `env_resolver` module resolves env var declarations into concrete values via interactive prompting. Pre-launch hooks are bash scripts copied into the derived image and executed by the entrypoint before Claude starts. All manifest structs enforce strict parsing with `deny_unknown_fields`.

**Tech Stack:** Rust, serde (with `deny_unknown_fields`), dialoguer (new dependency for interactive prompts), cargo-nextest (test runner)

---

### Task 1: Add `deny_unknown_fields` to Existing Manifest Structs

**Files:**
- Modify: `src/manifest.rs:1-35`
- Test: `src/manifest.rs` (inline tests)

- [ ] **Step 1: Write a failing test for unknown field rejection**

Add to the `mod tests` block in `src/manifest.rs`:

```rust
#[test]
fn rejects_unknown_top_level_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\nunknown_field = true\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    let error = AgentManifest::load(temp.path()).unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn rejects_unknown_claude_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\ntypo = \"oops\"\n",
    )
    .unwrap();

    let error = AgentManifest::load(temp.path()).unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn rejects_unknown_identity_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[identity]\nname = \"Smith\"\ntypo = true\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    let error = AgentManifest::load(temp.path()).unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(/manifest::tests::rejects_unknown/)'`
Expected: 3 FAIL — serde currently allows unknown fields silently.

- [ ] **Step 3: Add `deny_unknown_fields` to all manifest structs**

In `src/manifest.rs`, add the attribute to each struct:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentManifest {
    pub dockerfile: String,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    pub claude: ClaudeConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub plugins: Vec<String>,
}
```

- [ ] **Step 4: Run all tests to verify they pass**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/manifest.rs
git commit -m "feat: enforce strict manifest parsing with deny_unknown_fields"
```

---

### Task 2: Add `HooksConfig` and `[hooks]` to Manifest

**Files:**
- Modify: `src/manifest.rs`
- Test: `src/manifest.rs` (inline tests)

- [ ] **Step 1: Write failing tests for hooks parsing**

Add to the `mod tests` block in `src/manifest.rs`:

```rust
#[test]
fn loads_manifest_with_hooks() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"hooks/pre-launch.sh\"\n",
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    assert_eq!(
        manifest.hooks.as_ref().unwrap().pre_launch.as_deref(),
        Some("hooks/pre-launch.sh")
    );
}

#[test]
fn loads_manifest_without_hooks() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    assert!(manifest.hooks.is_none());
}

#[test]
fn rejects_unknown_hooks_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"hooks/pre-launch.sh\"\npost_launch = \"bad\"\n",
    )
    .unwrap();

    let error = AgentManifest::load(temp.path()).unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(/manifest::tests::loads_manifest_with_hooks/) or test(/manifest::tests::loads_manifest_without_hooks/) or test(/manifest::tests::rejects_unknown_hooks_field/)'`
Expected: FAIL — `hooks` field doesn't exist on `AgentManifest` yet.

- [ ] **Step 3: Add `HooksConfig` struct and `hooks` field**

In `src/manifest.rs`, add the struct and field:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    pub pre_launch: Option<String>,
}
```

And add to `AgentManifest`:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentManifest {
    pub dockerfile: String,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    pub claude: ClaudeConfig,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/manifest.rs
git commit -m "feat: add [hooks] section to agent manifest with pre_launch field"
```

---

### Task 3: Add `EnvVarDecl` and `[env.*]` to Manifest

**Files:**
- Modify: `src/manifest.rs`
- Test: `src/manifest.rs` (inline tests)

- [ ] **Step 1: Write failing tests for env var parsing**

Add to the `mod tests` block in `src/manifest.rs`:

```rust
use std::collections::BTreeMap;

#[test]
fn loads_manifest_with_static_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.CLAUDE_ENV]
default = "docker"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    assert_eq!(manifest.env.len(), 1);
    let var = &manifest.env["CLAUDE_ENV"];
    assert_eq!(var.default_value.as_deref(), Some("docker"));
    assert!(!var.interactive);
}

#[test]
fn loads_manifest_with_interactive_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Select a project:"
options = ["project1", "project2"]
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    let var = &manifest.env["PROJECT"];
    assert!(var.interactive);
    assert_eq!(var.prompt.as_deref(), Some("Select a project:"));
    assert_eq!(var.options, vec!["project1", "project2"]);
}

#[test]
fn loads_manifest_with_env_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Select:"
options = ["a", "b"]

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    let var = &manifest.env["BRANCH"];
    assert_eq!(var.depends_on, vec!["env.PROJECT"]);
}

#[test]
fn loads_manifest_with_skippable_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.API_KEY]
interactive = true
skippable = true
prompt = "API key (optional):"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    let var = &manifest.env["API_KEY"];
    assert!(var.skippable);
}

#[test]
fn loads_manifest_without_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    assert!(manifest.env.is_empty());
}

#[test]
fn rejects_unknown_env_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
typo = true
"#,
    )
    .unwrap();

    let error = AgentManifest::load(temp.path()).unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(/manifest::tests::loads_manifest_with_static_env/) or test(/manifest::tests::loads_manifest_with_interactive_env/) or test(/manifest::tests::loads_manifest_with_env_depends_on/) or test(/manifest::tests::loads_manifest_with_skippable_env/) or test(/manifest::tests::loads_manifest_without_env/) or test(/manifest::tests::rejects_unknown_env_field/)'`
Expected: FAIL — `env` field and `EnvVarDecl` don't exist yet.

- [ ] **Step 3: Add `EnvVarDecl` struct and `env` field**

In `src/manifest.rs`, add:

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvVarDecl {
    #[serde(rename = "default")]
    pub default_value: Option<String>,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub skippable: bool,
    pub prompt: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}
```

And add to `AgentManifest`:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentManifest {
    pub dockerfile: String,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    pub claude: ClaudeConfig,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/manifest.rs
git commit -m "feat: add [env.*] section to agent manifest with interactive var support"
```

---

### Task 4: Manifest Validation — Post-Deserialization Rules

**Files:**
- Modify: `src/manifest.rs`
- Test: `src/manifest.rs` (inline tests)

- [ ] **Step 1: Write failing tests for validation rules**

Add to the `mod tests` block in `src/manifest.rs`:

```rust
#[test]
fn validate_rejects_non_interactive_without_default() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let result = manifest.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("FOO"));
}

#[test]
fn validate_rejects_options_without_interactive() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
options = ["a", "b"]
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let result = manifest.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("options"));
}

#[test]
fn validate_rejects_dangling_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.BRANCH]
interactive = true
depends_on = ["env.NONEXISTENT"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let result = manifest.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NONEXISTENT"));
}

#[test]
fn validate_rejects_self_referencing_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = ["env.FOO"]
prompt = "Value:"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let result = manifest.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("self"));
}

#[test]
fn validate_rejects_dependency_cycle() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.A]
interactive = true
depends_on = ["env.B"]
prompt = "A:"

[env.B]
interactive = true
depends_on = ["env.A"]
prompt = "B:"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let result = manifest.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cycle"));
}

#[test]
fn validate_rejects_depends_on_without_env_prefix() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Project:"

[env.BRANCH]
interactive = true
depends_on = ["PROJECT"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let result = manifest.validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("env."));
}

#[test]
fn validate_accepts_valid_manifest_with_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.CLAUDE_ENV]
default = "docker"

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Pick:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch:"
default = "main"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let warnings = manifest.validate().unwrap();

    assert!(warnings.is_empty());
}

#[test]
fn validate_warns_on_prompt_without_interactive() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
prompt = "This is ignored"
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let warnings = manifest.validate().unwrap();

    assert!(!warnings.is_empty());
    assert!(warnings[0].message.contains("prompt"));
}

#[test]
fn validate_warns_on_skippable_without_interactive() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
skippable = true
"#,
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();
    let warnings = manifest.validate().unwrap();

    assert!(!warnings.is_empty());
    assert!(warnings[0].message.contains("skippable"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(/manifest::tests::validate_/)'`
Expected: FAIL — `validate()` method doesn't exist.

- [ ] **Step 3: Implement `validate()` method**

Add to `src/manifest.rs`:

```rust
#[derive(Debug, Clone)]
pub struct ManifestWarning {
    pub message: String,
}

impl AgentManifest {
    pub fn validate(&self) -> anyhow::Result<Vec<ManifestWarning>> {
        let mut warnings = Vec::new();

        for (name, decl) in &self.env {
            // Non-interactive without default is an error
            if !decl.interactive && decl.default_value.is_none() {
                anyhow::bail!(
                    "env var {name}: non-interactive variable must have a default value"
                );
            }

            // options without interactive is an error
            if !decl.interactive && !decl.options.is_empty() {
                anyhow::bail!(
                    "env var {name}: options requires interactive = true"
                );
            }

            // prompt without interactive is a warning
            if !decl.interactive && decl.prompt.is_some() {
                warnings.push(ManifestWarning {
                    message: format!("env var {name}: prompt is ignored without interactive = true"),
                });
            }

            // skippable without interactive is a warning
            if !decl.interactive && decl.skippable {
                warnings.push(ManifestWarning {
                    message: format!(
                        "env var {name}: skippable is meaningless without interactive = true"
                    ),
                });
            }

            // Validate depends_on entries
            for dep in &decl.depends_on {
                // Must have env. prefix
                let Some(dep_name) = dep.strip_prefix("env.") else {
                    anyhow::bail!(
                        "env var {name}: depends_on entry \"{dep}\" must use env. prefix (e.g., \"env.{dep}\")"
                    );
                };

                // Self-reference
                if dep_name == name {
                    anyhow::bail!("env var {name}: depends_on cannot reference self");
                }

                // Dangling reference
                if !self.env.contains_key(dep_name) {
                    anyhow::bail!(
                        "env var {name}: depends_on references unknown env var \"{dep_name}\""
                    );
                }
            }
        }

        // Cycle detection via topological sort (Kahn's algorithm)
        self.detect_env_cycles()?;

        Ok(warnings)
    }

    fn detect_env_cycles(&self) -> anyhow::Result<()> {
        use std::collections::{HashMap, VecDeque};

        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

        for name in self.env.keys() {
            in_degree.entry(name.as_str()).or_insert(0);
            adjacency.entry(name.as_str()).or_default();
        }

        for (name, decl) in &self.env {
            for dep in &decl.depends_on {
                if let Some(dep_name) = dep.strip_prefix("env.") {
                    adjacency.entry(dep_name).or_default().push(name.as_str());
                    *in_degree.entry(name.as_str()).or_insert(0) += 1;
                }
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();

        let mut visited = 0usize;

        while let Some(node) = queue.pop_front() {
            visited += 1;
            if let Some(neighbors) = adjacency.get(node) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        if visited != self.env.len() {
            anyhow::bail!("env var dependency cycle detected");
        }

        Ok(())
    }
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/manifest.rs
git commit -m "feat: add manifest validation with cross-field rules and cycle detection"
```

---

### Task 5: Pre-Launch Hook Path Validation in `repo.rs`

**Files:**
- Modify: `src/repo.rs`
- Test: `src/repo.rs` (inline tests)

- [ ] **Step 1: Write failing tests for hook path validation**

Add to the `mod tests` block in `src/repo.rs`:

```rust
#[test]
fn accepts_manifest_with_valid_pre_launch_hook() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
    std::fs::write(
        temp.path().join("hooks/pre-launch.sh"),
        "#!/bin/bash\necho hello\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"hooks/pre-launch.sh\"\n",
    )
    .unwrap();

    let validated = validate_agent_repo(temp.path()).unwrap();

    assert!(validated.manifest.hooks.as_ref().unwrap().pre_launch.is_some());
}

#[test]
fn rejects_pre_launch_hook_outside_repo() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"../escape.sh\"\n",
    )
    .unwrap();

    let error = validate_agent_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("must stay inside the repo"));
}

#[test]
fn rejects_pre_launch_hook_that_does_not_exist() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"hooks/missing.sh\"\n",
    )
    .unwrap();

    let error = validate_agent_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("missing"));
}

#[test]
fn rejects_absolute_pre_launch_hook_path() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"/etc/evil.sh\"\n",
    )
    .unwrap();

    let error = validate_agent_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("must be relative"));
}

#[test]
fn rejects_empty_pre_launch_hook() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
    std::fs::write(temp.path().join("hooks/pre-launch.sh"), "").unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"hooks/pre-launch.sh\"\n",
    )
    .unwrap();

    let error = validate_agent_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("empty"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(/repo::tests::accepts_manifest_with_valid_pre_launch/) or test(/repo::tests::rejects_pre_launch/) or test(/repo::tests::rejects_absolute_pre_launch/) or test(/repo::tests::rejects_empty_pre_launch/)'`
Expected: FAIL — no hook validation exists.

- [ ] **Step 3: Extract path validation helper and add hook validation**

In `src/repo.rs`, extract the path validation logic into a reusable function and call it for both the dockerfile and the pre-launch hook:

```rust
fn validate_relative_path(
    repo_dir: &Path,
    path_str: &str,
    label: &str,
) -> anyhow::Result<PathBuf> {
    let path = Path::new(path_str);

    if path.is_absolute() {
        anyhow::bail!("invalid agent repo: {label} path must be relative");
    }

    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            anyhow::bail!("invalid agent repo: {label} path must stay inside the repo");
        }
    }

    let resolved = repo_dir.join(path);
    if !resolved.is_file() {
        anyhow::bail!("invalid agent repo: missing {}", resolved.display());
    }

    let canonical_repo = repo_dir.canonicalize()?;
    let canonical_resolved = resolved.canonicalize()?;
    if !canonical_resolved.starts_with(&canonical_repo) {
        anyhow::bail!("invalid agent repo: {label} path escapes the repo boundary");
    }

    Ok(canonical_resolved)
}
```

Refactor `resolve_manifest_dockerfile_path` to use `validate_relative_path`:

```rust
fn resolve_manifest_dockerfile_path(
    repo_dir: &Path,
    manifest: &AgentManifest,
) -> anyhow::Result<PathBuf> {
    validate_relative_path(repo_dir, &manifest.dockerfile, "dockerfile")
}
```

Add hook validation to `validate_agent_repo`:

```rust
pub fn validate_agent_repo(repo_dir: &Path) -> anyhow::Result<ValidatedAgentRepo> {
    let manifest_path = repo_dir.join("jackin.agent.toml");

    if !manifest_path.is_file() {
        anyhow::bail!("invalid agent repo: missing {}", manifest_path.display());
    }

    let manifest = AgentManifest::load(repo_dir)?;
    let dockerfile_path = resolve_manifest_dockerfile_path(repo_dir, &manifest)?;
    let dockerfile = validate_agent_dockerfile(&dockerfile_path)?;

    // Validate pre-launch hook path if declared
    if let Some(ref hooks) = manifest.hooks {
        if let Some(ref pre_launch) = hooks.pre_launch {
            let hook_path = validate_relative_path(repo_dir, pre_launch, "pre_launch hook")?;
            let contents = std::fs::read_to_string(&hook_path)?;
            if contents.is_empty() {
                anyhow::bail!(
                    "invalid agent repo: pre_launch hook is empty: {}",
                    hook_path.display()
                );
            }
        }
    }

    // Validate env var declarations
    let warnings = manifest.validate()?;
    for warning in &warnings {
        eprintln!("warning: {}", warning.message);
    }

    Ok(ValidatedAgentRepo {
        manifest,
        dockerfile,
    })
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/repo.rs
git commit -m "feat: validate pre-launch hook paths and manifest env vars in agent repo"
```

---

### Task 6: Env Resolver Module — Core Logic

**Files:**
- Create: `src/env_resolver.rs`
- Modify: `src/lib.rs:1` (add module registration)
- Test: `src/env_resolver.rs` (inline tests)

- [ ] **Step 1: Register the new module**

In `src/lib.rs`, add the module declaration after the existing list:

```rust
pub mod env_resolver;
```

- [ ] **Step 2: Write failing tests for env resolution**

Create `src/env_resolver.rs` with the test module:

```rust
use crate::manifest::EnvVarDecl;
use std::collections::BTreeMap;

pub struct ResolvedEnv {
    pub vars: Vec<(String, String)>,
}

pub enum PromptResult {
    Value(String),
    Skipped,
}

pub trait EnvPrompter {
    fn prompt_text(&self, title: &str, default: Option<&str>, skippable: bool) -> PromptResult;
    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> PromptResult;
}

pub fn resolve_env(
    _declarations: &BTreeMap<String, EnvVarDecl>,
    _prompter: &impl EnvPrompter,
) -> anyhow::Result<ResolvedEnv> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPrompter {
        responses: std::cell::RefCell<Vec<PromptResult>>,
    }

    impl MockPrompter {
        fn new(responses: Vec<PromptResult>) -> Self {
            Self {
                responses: std::cell::RefCell::new(responses),
            }
        }
    }

    impl EnvPrompter for MockPrompter {
        fn prompt_text(&self, _title: &str, _default: Option<&str>, _skippable: bool) -> PromptResult {
            self.responses.borrow_mut().remove(0)
        }

        fn prompt_select(
            &self,
            _title: &str,
            _options: &[String],
            _default: Option<&str>,
            _skippable: bool,
        ) -> PromptResult {
            self.responses.borrow_mut().remove(0)
        }
    }

    fn static_var(default: &str) -> EnvVarDecl {
        EnvVarDecl {
            default_value: Some(default.to_string()),
            interactive: false,
            skippable: false,
            prompt: None,
            options: vec![],
            depends_on: vec![],
        }
    }

    fn interactive_text(prompt: &str) -> EnvVarDecl {
        EnvVarDecl {
            default_value: None,
            interactive: true,
            skippable: false,
            prompt: Some(prompt.to_string()),
            options: vec![],
            depends_on: vec![],
        }
    }

    fn interactive_select(prompt: &str, options: Vec<&str>) -> EnvVarDecl {
        EnvVarDecl {
            default_value: None,
            interactive: true,
            skippable: false,
            prompt: Some(prompt.to_string()),
            options: options.into_iter().map(String::from).collect(),
            depends_on: vec![],
        }
    }

    #[test]
    fn resolves_static_vars_without_prompting() {
        let mut decls = BTreeMap::new();
        decls.insert("CLAUDE_ENV".to_string(), static_var("docker"));
        let prompter = MockPrompter::new(vec![]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(resolved.vars, vec![("CLAUDE_ENV".to_string(), "docker".to_string())]);
    }

    #[test]
    fn resolves_interactive_text_var() {
        let mut decls = BTreeMap::new();
        decls.insert("BRANCH".to_string(), interactive_text("Branch:"));
        let prompter = MockPrompter::new(vec![PromptResult::Value("main".to_string())]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(resolved.vars, vec![("BRANCH".to_string(), "main".to_string())]);
    }

    #[test]
    fn resolves_interactive_select_var() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "PROJECT".to_string(),
            interactive_select("Pick:", vec!["a", "b"]),
        );
        let prompter = MockPrompter::new(vec![PromptResult::Value("b".to_string())]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(resolved.vars, vec![("PROJECT".to_string(), "b".to_string())]);
    }

    #[test]
    fn skippable_var_can_be_skipped() {
        let mut decls = BTreeMap::new();
        let mut var = interactive_text("API key:");
        var.skippable = true;
        decls.insert("API_KEY".to_string(), var);
        let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert!(resolved.vars.is_empty());
    }

    #[test]
    fn skip_cascades_to_dependents() {
        let mut decls = BTreeMap::new();
        let mut project = interactive_select("Pick:", vec!["a", "b"]);
        project.skippable = true;
        decls.insert("PROJECT".to_string(), project);

        let mut branch = interactive_text("Branch:");
        branch.depends_on = vec!["env.PROJECT".to_string()];
        decls.insert("BRANCH".to_string(), branch);

        let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert!(resolved.vars.is_empty());
    }

    #[test]
    fn skip_cascades_through_chain() {
        let mut decls = BTreeMap::new();

        let mut a = interactive_text("A:");
        a.skippable = true;
        decls.insert("A".to_string(), a);

        let mut b = interactive_text("B:");
        b.depends_on = vec!["env.A".to_string()];
        decls.insert("B".to_string(), b);

        let mut c = interactive_text("C:");
        c.depends_on = vec!["env.B".to_string()];
        decls.insert("C".to_string(), c);

        let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert!(resolved.vars.is_empty());
    }

    #[test]
    fn dependency_order_is_respected() {
        let mut decls = BTreeMap::new();

        let mut branch = interactive_text("Branch:");
        branch.depends_on = vec!["env.PROJECT".to_string()];
        decls.insert("BRANCH".to_string(), branch);

        decls.insert(
            "PROJECT".to_string(),
            interactive_select("Pick:", vec!["a", "b"]),
        );

        let prompter = MockPrompter::new(vec![
            PromptResult::Value("a".to_string()),
            PromptResult::Value("main".to_string()),
        ]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(resolved.vars[0].0, "PROJECT");
        assert_eq!(resolved.vars[1].0, "BRANCH");
    }

    #[test]
    fn empty_declarations_returns_empty() {
        let decls = BTreeMap::new();
        let prompter = MockPrompter::new(vec![]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert!(resolved.vars.is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(/env_resolver::tests/)'`
Expected: FAIL — `resolve_env` has `todo!()`.

- [ ] **Step 4: Implement `resolve_env`**

Replace the `todo!()` in `resolve_env` with the full implementation:

```rust
pub fn resolve_env(
    declarations: &BTreeMap<String, EnvVarDecl>,
    prompter: &impl EnvPrompter,
) -> anyhow::Result<ResolvedEnv> {
    let order = topological_sort(declarations)?;
    let mut vars = Vec::new();
    let mut skipped: std::collections::HashSet<String> = std::collections::HashSet::new();

    for name in &order {
        let decl = &declarations[name];

        // Check if any dependency was skipped — cascade skip
        let dep_skipped = decl.depends_on.iter().any(|dep| {
            dep.strip_prefix("env.")
                .is_some_and(|dep_name| skipped.contains(dep_name))
        });

        if dep_skipped {
            skipped.insert(name.clone());
            continue;
        }

        if !decl.interactive {
            // Static var — use default
            if let Some(ref default) = decl.default_value {
                vars.push((name.clone(), default.clone()));
            }
            continue;
        }

        // Interactive var — prompt
        let title = decl
            .prompt
            .as_deref()
            .unwrap_or(name.as_str());

        let result = if decl.options.is_empty() {
            prompter.prompt_text(title, decl.default_value.as_deref(), decl.skippable)
        } else {
            prompter.prompt_select(
                title,
                &decl.options,
                decl.default_value.as_deref(),
                decl.skippable,
            )
        };

        match result {
            PromptResult::Value(value) => {
                vars.push((name.clone(), value));
            }
            PromptResult::Skipped => {
                skipped.insert(name.clone());
            }
        }
    }

    Ok(ResolvedEnv { vars })
}

fn topological_sort(
    declarations: &BTreeMap<String, EnvVarDecl>,
) -> anyhow::Result<Vec<String>> {
    use std::collections::{HashMap, VecDeque};

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for name in declarations.keys() {
        in_degree.entry(name.as_str()).or_insert(0);
        adjacency.entry(name.as_str()).or_default();
    }

    for (name, decl) in declarations {
        for dep in &decl.depends_on {
            if let Some(dep_name) = dep.strip_prefix("env.") {
                adjacency.entry(dep_name).or_default().push(name.as_str());
                *in_degree.entry(name.as_str()).or_insert(0) += 1;
            }
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut result = Vec::new();

    while let Some(node) = queue.pop_front() {
        result.push(node.to_string());
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    if result.len() != declarations.len() {
        anyhow::bail!("env var dependency cycle detected");
    }

    Ok(result)
}
```

- [ ] **Step 5: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 6: Commit**

```bash
git add src/env_resolver.rs src/lib.rs
git commit -m "feat: add env_resolver module with topological sort and skip cascade"
```

---

### Task 7: Pre-Launch Hook in Derived Image

**Files:**
- Modify: `src/derived_image.rs`
- Test: `src/derived_image.rs` (inline tests)

- [ ] **Step 1: Write failing tests for hook in derived Dockerfile**

Add to the `mod tests` block in `src/derived_image.rs`:

```rust
#[test]
fn renders_derived_dockerfile_with_pre_launch_hook() {
    let dockerfile =
        render_derived_dockerfile("FROM projectjackin/construct:trixie\n", Some("hooks/pre-launch.sh"));

    assert!(dockerfile.contains(
        "COPY hooks/pre-launch.sh /home/claude/.jackin-runtime/pre-launch.sh"
    ));
    assert!(dockerfile.contains(
        "RUN chmod +x /home/claude/.jackin-runtime/pre-launch.sh"
    ));
}

#[test]
fn renders_derived_dockerfile_without_pre_launch_hook() {
    let dockerfile =
        render_derived_dockerfile("FROM projectjackin/construct:trixie\n", None);

    assert!(!dockerfile.contains("pre-launch.sh"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(/derived_image::tests::renders_derived_dockerfile_with_pre_launch/) or test(/derived_image::tests::renders_derived_dockerfile_without_pre_launch/)'`
Expected: FAIL — `render_derived_dockerfile` doesn't accept a second parameter.

- [ ] **Step 3: Update `render_derived_dockerfile` signature and implementation**

In `src/derived_image.rs`, change the function signature and add the hook COPY:

```rust
pub fn render_derived_dockerfile(base_dockerfile: &str, pre_launch_hook: Option<&str>) -> String {
    let hook_section = pre_launch_hook.map_or_else(String::new, |hook_path| {
        format!(
            "\
USER root
COPY {hook_path} /home/claude/.jackin-runtime/pre-launch.sh
RUN chmod +x /home/claude/.jackin-runtime/pre-launch.sh
USER claude
"
        )
    });

    format!(
        "\
{base_dockerfile}
USER root
ARG JACKIN_HOST_UID=1000
ARG JACKIN_HOST_GID=1000
RUN current_gid=\"$(id -g claude)\" \
    && current_uid=\"$(id -u claude)\" \
    && if [ \"$current_gid\" != \"$JACKIN_HOST_GID\" ]; then \
         groupmod -o -g \"$JACKIN_HOST_GID\" claude \
         && usermod -g \"$JACKIN_HOST_GID\" claude; \
       fi \
    && if [ \"$current_uid\" != \"$JACKIN_HOST_UID\" ]; then \
         usermod -o -u \"$JACKIN_HOST_UID\" claude; \
       fi \
    && chown -R claude:claude /home/claude
USER claude
ARG JACKIN_CACHE_BUST=0
RUN curl -fsSL https://claude.ai/install.sh | bash
RUN claude --version
{hook_section}USER root
COPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh
RUN chmod +x /home/claude/entrypoint.sh
USER claude
ENTRYPOINT [\"/home/claude/entrypoint.sh\"]
"
    )
}
```

- [ ] **Step 4: Update all callers and existing tests**

Update `create_derived_build_context` to pass the hook path:

```rust
pub fn create_derived_build_context(
    repo_dir: &Path,
    validated: &ValidatedAgentRepo,
) -> anyhow::Result<DerivedBuildContext> {
    let temp_dir = tempfile::tempdir()?;
    let context_dir = temp_dir.path().join("context");
    copy_dir_all(repo_dir, &context_dir)?;

    let runtime_dir = context_dir.join(".jackin-runtime");
    std::fs::create_dir_all(&runtime_dir)?;
    std::fs::write(runtime_dir.join("entrypoint.sh"), ENTRYPOINT_SH)?;

    let pre_launch_hook = validated
        .manifest
        .hooks
        .as_ref()
        .and_then(|h| h.pre_launch.as_deref());

    let dockerfile_path = context_dir.join(".jackin-runtime/DerivedDockerfile");
    std::fs::write(
        &dockerfile_path,
        render_derived_dockerfile(&validated.dockerfile.dockerfile_contents, pre_launch_hook),
    )?;
    ensure_runtime_assets_are_included(&context_dir, pre_launch_hook)?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}
```

Update `ensure_runtime_assets_are_included` to add negation rule for hook path:

```rust
fn ensure_runtime_assets_are_included(
    context_dir: &Path,
    pre_launch_hook: Option<&str>,
) -> anyhow::Result<()> {
    let dockerignore_path = context_dir.join(".dockerignore");
    let mut dockerignore = if dockerignore_path.exists() {
        std::fs::read_to_string(&dockerignore_path)?
    } else {
        String::new()
    };

    let mut rules = vec![
        "!.jackin-runtime/".to_string(),
        "!.jackin-runtime/entrypoint.sh".to_string(),
        "!.jackin-runtime/DerivedDockerfile".to_string(),
    ];
    if let Some(hook_path) = pre_launch_hook {
        rules.push(format!("!{hook_path}"));
    }

    for rule in &rules {
        if !dockerignore.lines().any(|line| line == rule) {
            if !dockerignore.is_empty() && !dockerignore.ends_with('\n') {
                dockerignore.push('\n');
            }
            dockerignore.push_str(rule);
            dockerignore.push('\n');
        }
    }

    std::fs::write(dockerignore_path, dockerignore)?;
    Ok(())
}
```

Update existing tests that call `render_derived_dockerfile` to pass `None`:

- `renders_derived_dockerfile_with_workspace_and_entrypoint` → `render_derived_dockerfile("FROM projectjackin/construct:trixie\n", None)`
- `renders_derived_dockerfile_installs_claude_as_claude_user` → same
- `renders_derived_dockerfile_rewrites_claude_uid_and_gid` → same

- [ ] **Step 5: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 6: Commit**

```bash
git add src/derived_image.rs
git commit -m "feat: include pre-launch hook in derived Docker image when declared"
```

---

### Task 8: Pre-Launch Hook in Entrypoint

**Files:**
- Modify: `docker/runtime/entrypoint.sh`
- Test: Manual verification (shell script, no unit test framework)

- [ ] **Step 1: Add pre-launch hook execution to entrypoint**

In `docker/runtime/entrypoint.sh`, add between plugin installation and screen clear:

```bash
run_maybe_quiet /home/claude/install-plugins.sh

# Run pre-launch hook if present
if [ -x /home/claude/.jackin-runtime/pre-launch.sh ]; then
    echo "Running pre-launch hook..."
    /home/claude/.jackin-runtime/pre-launch.sh
fi

printf '\033[2J\033[H'
```

- [ ] **Step 2: Run all tests to verify no regressions**

Run: `cargo clippy && cargo nextest run`
Expected: All pass (entrypoint is `include_str!`'d so build picks up the change).

- [ ] **Step 3: Commit**

```bash
git add docker/runtime/entrypoint.sh
git commit -m "feat: execute pre-launch hook in entrypoint before Claude starts"
```

---

### Task 9: Add `dialoguer` Dependency and Terminal Prompter

**Files:**
- Modify: `Cargo.toml`
- Create: `src/terminal_prompter.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `dialoguer` to dependencies**

In `Cargo.toml`, add under `[dependencies]`:

```toml
dialoguer = "0.11"
```

- [ ] **Step 2: Run cargo check to verify dependency resolves**

Run: `cargo check`
Expected: Compiles successfully.

- [ ] **Step 3: Create terminal prompter implementation**

Create `src/terminal_prompter.rs`:

```rust
use crate::env_resolver::{EnvPrompter, PromptResult};
use dialoguer::{Input, Select};

pub struct TerminalPrompter;

impl EnvPrompter for TerminalPrompter {
    fn prompt_text(&self, title: &str, default: Option<&str>, skippable: bool) -> PromptResult {
        let mut input = Input::<String>::new().with_prompt(title);

        if let Some(d) = default {
            input = input.default(d.to_string());
        }

        if skippable {
            input = input.allow_empty(true);
        }

        match input.interact_text() {
            Ok(value) if value.is_empty() && skippable => PromptResult::Skipped,
            Ok(value) => PromptResult::Value(value),
            Err(_) => PromptResult::Skipped,
        }
    }

    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> PromptResult {
        let mut items: Vec<&str> = options.iter().map(String::as_str).collect();
        if skippable {
            items.push("(skip)");
        }

        let mut select = Select::new().with_prompt(title).items(&items);

        if let Some(d) = default {
            if let Some(idx) = options.iter().position(|o| o == d) {
                select = select.default(idx);
            }
        }

        match select.interact() {
            Ok(idx) if skippable && idx == options.len() => PromptResult::Skipped,
            Ok(idx) => PromptResult::Value(options[idx].clone()),
            Err(_) => PromptResult::Skipped,
        }
    }
}
```

- [ ] **Step 4: Register module in `src/lib.rs`**

Add to `src/lib.rs`:

```rust
pub mod terminal_prompter;
```

- [ ] **Step 5: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/terminal_prompter.rs src/lib.rs
git commit -m "feat: add dialoguer-based terminal prompter for interactive env vars"
```

---

### Task 10: Wire Env Resolution and Hook into Runtime

**Files:**
- Modify: `src/runtime.rs`
- Test: Integration verification via existing test patterns

- [ ] **Step 1: Add `resolved_env` to `LaunchContext`**

In `src/runtime.rs`, update the `LaunchContext` struct:

```rust
struct LaunchContext<'a> {
    container_name: &'a str,
    image: &'a str,
    network: &'a str,
    dind: &'a str,
    selector: &'a ClassSelector,
    agent_display_name: &'a str,
    workspace: &'a crate::workspace::ResolvedWorkspace,
    state: &'a AgentState,
    git: &'a GitIdentity,
    debug: bool,
    resolved_env: &'a crate::env_resolver::ResolvedEnv,
}
```

- [ ] **Step 2: Add env resolution call in `load_agent`**

In `load_agent()`, after the config summary block and before the `build_agent_image` call, add env resolution:

```rust
// Resolve env vars (interactive prompts happen here, before build)
let resolved_env = if validated_repo.manifest.env.is_empty() {
    crate::env_resolver::ResolvedEnv { vars: vec![] }
} else {
    let prompter = crate::terminal_prompter::TerminalPrompter;
    crate::env_resolver::resolve_env(&validated_repo.manifest.env, &prompter)?
};
```

Thread `resolved_env` into the `LaunchContext` construction:

```rust
let ctx = LaunchContext {
    container_name: &container_name,
    image: &image,
    network: &network,
    dind: &dind,
    selector,
    agent_display_name: &agent_display_name,
    workspace,
    state: &state,
    git: &git,
    debug: opts.debug,
    resolved_env: &resolved_env,
};
```

- [ ] **Step 3: Pass resolved env vars as `-e` flags in `launch_agent_runtime`**

In `launch_agent_runtime()`, after the existing `-e` flags and before the volume mounts, add:

```rust
let mut env_strings: Vec<String> = Vec::new();
for (key, value) in &ctx.resolved_env.vars {
    env_strings.push(format!("{key}={value}"));
}
for env_str in &env_strings {
    run_args.push("-e");
    run_args.push(env_str);
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/runtime.rs
git commit -m "feat: wire env resolution and resolved vars into container launch"
```

---

### Task 11: Update Documentation — Agent Manifest

**Files:**
- Modify: `docs/src/content/docs/developing/agent-manifest.mdx`

- [ ] **Step 1: Update agent-manifest.mdx with hooks and env sections**

Rewrite `docs/src/content/docs/developing/agent-manifest.mdx` to include the full updated schema. The file should contain:

```mdx
---
title: Agent Manifest
description: "The jackin.agent.toml file that defines an agent"
---

import { Aside } from '@astrojs/starlight/components';

## Overview

Every agent repo must contain a `jackin.agent.toml` file at the repository root. This manifest tells jackin' how to build, configure, and identify the agent.

jackin' enforces **strict parsing** — unknown fields are rejected with an error. This catches typos and prevents silent misconfiguration.

## Full schema

```toml title="jackin.agent.toml"
dockerfile = "Dockerfile"

[identity]
name = "The Architect"

[claude]
plugins = ["code-review@claude-plugins-official"]

[hooks]
pre_launch = "hooks/pre-launch.sh"

[env.CLAUDE_ENV]
default = "docker"

[env.PROJECT]
interactive = true
options = ["project1", "project2"]
prompt = "Select a project:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch name:"
default = "main"
```

## Top-level fields

| Field | Required | Description |
|---|---|---|
| `dockerfile` | Yes | Relative path to the Dockerfile within the repo |

The Dockerfile path must:

- Be relative (no absolute paths)
- Stay inside the repository (no `../` escapes)
- Point to a valid Dockerfile
- Have a final stage that starts with `FROM projectjackin/construct:trixie`

## `[claude]`

| Field | Required | Description |
|---|---|---|
| `plugins` | No | List of Claude plugin identifiers to install at runtime |

Example values:

```toml title="jackin.agent.toml"
dockerfile = "Dockerfile"

[claude]
plugins = [
  "code-review@claude-plugins-official",
  "feature-dev@claude-plugins-official"
]
```

## `[identity]`

| Field | Required | Description |
|---|---|---|
| `name` | No | Human-readable display name for the agent |

When omitted, jackin' uses the class selector name.

## `[hooks]`

| Field | Required | Description |
|---|---|---|
| `pre_launch` | No | Relative path to a bash script run before Claude starts |

The pre-launch hook runs inside the container after plugin installation but before Claude Code launches. It has access to all resolved environment variables.

The script path must:

- Be relative (no absolute paths)
- Stay inside the repository (no `../` escapes)
- Point to an existing, non-empty file
- Not be a symlink

Example:

```toml title="jackin.agent.toml"
[hooks]
pre_launch = "hooks/pre-launch.sh"
```

```bash title="hooks/pre-launch.sh"
#!/bin/bash
set -euo pipefail

# Configure Context7 MCP if API key is available
if [ -n "${CONTEXT7_API_KEY:-}" ]; then
    ctx7 setup --claude --mcp --api-key "$CONTEXT7_API_KEY" -y
fi
```

## `[env.<NAME>]`

Declare environment variables that the agent needs at runtime. Each variable is a TOML table under `[env]`.

### Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `default` | String | — | Default value (used if no prompt or user accepts default) |
| `interactive` | bool | `false` | Whether to prompt the user at launch time |
| `skippable` | bool | `false` | Whether the user can skip this prompt |
| `prompt` | String | Variable name | Text shown when prompting |
| `options` | String[] | `[]` | Options for a select-style prompt |
| `depends_on` | String[] | `[]` | Variables that must be resolved first (use `env.` prefix) |

### Validation rules

- A non-interactive variable **must** have a `default` value
- `options` requires `interactive = true`
- `depends_on` entries must use the `env.` prefix (e.g., `"env.PROJECT"`)
- `depends_on` must reference variables declared in the same manifest
- Circular dependencies are rejected

### Static variables

Set automatically with no user interaction:

```toml
[env.CLAUDE_ENV]
default = "docker"
```

### Interactive text input

Prompt the user for a free-text value:

```toml
[env.GIT_BRANCH]
interactive = true
prompt = "Branch name:"

[env.BRANCH_WITH_DEFAULT]
interactive = true
prompt = "Branch name:"
default = "main"
```

### Interactive select

Present a list of options:

```toml
[env.PROJECT]
interactive = true
options = ["frontend", "backend", "infra"]
prompt = "Select a project:"
```

### Skippable prompts

Allow the user to skip a prompt. The variable won't be set:

```toml
[env.API_KEY]
interactive = true
skippable = true
prompt = "API key (optional):"
```

### Dependencies

Control prompt ordering and skip cascading:

```toml
[env.PROJECT]
interactive = true
skippable = true
options = ["frontend", "backend"]
prompt = "Select a project:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch to work on:"
default = "main"
```

If a skippable variable is skipped, all variables that depend on it are also skipped — regardless of their own `skippable` setting.

<Aside type="tip">
Interactive prompts happen before the Docker image build, so you won't wait through a build before being asked questions. If you cancel, no build resources are wasted.
</Aside>

## Minimal example

The smallest valid manifest:

```toml title="jackin.agent.toml"
dockerfile = "Dockerfile"

[claude]
plugins = []
```

## Complete example

```toml title="jackin.agent.toml"
dockerfile = "docker/Dockerfile.agent"

[identity]
name = "The Architect"

[claude]
plugins = ["code-review@claude-plugins-official"]

[hooks]
pre_launch = "hooks/pre-launch.sh"

[env.CLAUDE_ENV]
default = "docker"

[env.CONTEXT7_API_KEY]
interactive = true
skippable = true
prompt = "Context7 API key:"

[env.PROJECT]
interactive = true
options = ["frontend", "backend", "infra"]
prompt = "Select a project to clone"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch name:"
default = "main"
```

<Aside type="tip">
Keep manifests focused. The Dockerfile installs tools, the manifest declares what configuration the agent needs at launch time.
</Aside>
```

- [ ] **Step 2: Verify docs build**

Run: `cd docs && bun run build`
Expected: Build succeeds without errors.

- [ ] **Step 3: Commit**

```bash
git add docs/src/content/docs/developing/agent-manifest.mdx
git commit -m "docs: update agent manifest reference with hooks and env var sections"
```

---

### Task 12: Update Documentation — Architecture and Load Command

**Files:**
- Modify: `docs/src/content/docs/reference/architecture.mdx`
- Modify: `docs/src/content/docs/commands/load.mdx`

- [ ] **Step 1: Update architecture.mdx**

In `docs/src/content/docs/reference/architecture.mdx`, update the "Image layers" diagram to include pre-launch hooks:

```
│  Derived Layer (jackin-managed) │
│  - UID/GID remapping            │
│  - Claude Code installation     │
│  - Pre-launch hook (if declared)│
│  - Runtime entrypoint           │
│  - Plugin bootstrap             │
```

Update the "Loading an agent" lifecycle:

```
1. **Resolve agent class** — map the selector to a repo, clone or update it, and reject dirty cached checkouts
2. **Validate the repo contract** — require `jackin.agent.toml` and a valid Dockerfile path, validate manifest strictly
3. **Resolve environment variables** — prompt the user for interactive env vars declared in the manifest
4. **Generate a derived build context** — copy the repo, inject runtime assets (entrypoint + pre-launch hook), and render a derived Dockerfile
5. **Build the image on the host Docker engine** — reusing host-side Docker cache where possible
6. **Create a per-agent Docker network**
7. **Start a privileged `docker:dind` sidecar**
8. **Start the agent container** — mounts, resolved env vars, labels, and `DOCKER_HOST` all point at the sidecar
9. **Run Claude Code with full permissions inside that boundary**
```

- [ ] **Step 2: Update load.mdx**

In `docs/src/content/docs/commands/load.mdx`, update the "What happens" section to match the architecture lifecycle above (same 9 steps).

Add an Aside after the steps:

```mdx
<Aside type="note">
If the agent manifest declares interactive environment variables, `jackin load` prompts for their values before building the Docker image. This ensures no build time is wasted if you cancel.
</Aside>
```

- [ ] **Step 3: Verify docs build**

Run: `cd docs && bun run build`
Expected: Build succeeds without errors.

- [ ] **Step 4: Commit**

```bash
git add docs/src/content/docs/reference/architecture.mdx docs/src/content/docs/commands/load.mdx
git commit -m "docs: update architecture lifecycle and load command for env vars and hooks"
```

---

### Task 13: Final Verification

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo clippy && cargo nextest run`
Expected: All pass, zero warnings.

- [ ] **Step 2: Verify docs build**

Run: `cd docs && bun run build`
Expected: Build succeeds.

- [ ] **Step 3: Test with a sample manifest**

Create a temporary test manifest and verify it parses and validates:

Run:
```bash
cd /tmp && mkdir -p test-agent/hooks && \
cat > test-agent/hooks/pre-launch.sh << 'HOOK'
#!/bin/bash
echo "Pre-launch hook running"
HOOK
cat > test-agent/Dockerfile << 'DF'
FROM projectjackin/construct:trixie
DF
cat > test-agent/jackin.agent.toml << 'TOML'
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"

[env.CLAUDE_ENV]
default = "docker"

[env.PROJECT]
interactive = true
skippable = true
options = ["frontend", "backend"]
prompt = "Select project:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch:"
default = "main"
TOML
echo "Sample manifest created. Verify with: cargo run -- load test-agent /tmp/test-agent (manual)"
```

- [ ] **Step 4: Verify no regressions in existing behavior**

Run: `cargo nextest run`
Expected: All existing tests still pass — manifests without `[hooks]` and `[env]` sections continue to work unchanged.

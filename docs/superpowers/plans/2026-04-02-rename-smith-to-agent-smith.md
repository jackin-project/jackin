# Rename Smith to Agent-Smith Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the default agent class from `smith` to `agent-smith`, change container prefix from `agent-` to `jackin-`, simplify network naming, add `[identity]` support to manifests, and rename the smith repo to `jackin-agent-smith`.

**Architecture:** The rename touches naming functions in `instance.rs`, selector parsing/validation in `selector.rs`, network naming in `runtime.rs`, default config in `config.rs`, and manifest parsing in `manifest.rs`. The smith repo gets renamed locally and on GitHub. All tests and docs are updated to match.

**Tech Stack:** Rust, TOML (serde), Docker, GitHub CLI (`gh`)

---

### Task 1: Update selector.rs — Change container detection from `agent-` to `jackin-` prefix

**Files:**
- Modify: `src/selector.rs:60-98`

- [ ] **Step 1: Update `is_valid_container_name` to use `jackin-` prefix**

```rust
fn is_valid_container_name(value: &str) -> bool {
    value.starts_with("jackin-") && is_valid_class_segment(&value["jackin-".len()..])
}
```

- [ ] **Step 2: Update `is_reserved_builtin_class_name` to use `jackin-` prefix**

```rust
fn is_reserved_builtin_class_name(value: &str) -> bool {
    value.starts_with("jackin-")
        || value
            .rsplit_once("-clone-")
            .is_some_and(|(base, suffix)| is_valid_class_segment(base) && suffix.chars().all(|ch| ch.is_ascii_digit()))
}
```

- [ ] **Step 3: Update `Selector::parse` clone shorthand to use `jackin-` prefix**

In the `Selector::parse` method, change the clone shorthand expansion from `agent-` to `jackin-`:

```rust
if !input.contains('/') {
    if let Some((base, suffix)) = input.rsplit_once("-clone-") {
        if is_valid_class_segment(base) && suffix.chars().all(|ch| ch.is_ascii_digit()) {
            return Ok(Self::Container(format!("jackin-{input}")));
        }
    }
}
```

- [ ] **Step 4: Update all tests in selector.rs**

Replace the test module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_class_selector() {
        let selector = Selector::parse("agent-smith").unwrap();
        assert_eq!(selector, Selector::Class(ClassSelector::new(None, "agent-smith")));
    }

    #[test]
    fn class_parser_rejects_reserved_builtin_names() {
        assert!(matches!(
            ClassSelector::parse("jackin-agent-smith"),
            Err(SelectorError::Invalid(_))
        ));
        assert!(matches!(
            ClassSelector::parse("agent-smith-clone-1"),
            Err(SelectorError::Invalid(_))
        ));
    }

    #[test]
    fn parses_namespaced_class_selector() {
        let selector = Selector::parse("chainargos/the-architect").unwrap();
        assert_eq!(
            selector,
            Selector::Class(ClassSelector::new(Some("chainargos"), "the-architect"))
        );
    }

    #[test]
    fn parses_container_selector() {
        let selector = Selector::parse("jackin-chainargos-the-architect-clone-1").unwrap();
        assert_eq!(
            selector,
            Selector::Container("jackin-chainargos-the-architect-clone-1".to_string())
        );
    }

    #[test]
    fn parses_clone_shorthand_selector() {
        let selector = Selector::parse("agent-smith-clone-1").unwrap();
        assert_eq!(selector, Selector::Container("jackin-agent-smith-clone-1".to_string()));
    }

    #[test]
    fn rejects_malformed_namespaced_selector() {
        assert!(matches!(
            Selector::parse("foo/bar/baz"),
            Err(SelectorError::Invalid(_))
        ));
        assert!(matches!(
            Selector::parse("foo/../bar"),
            Err(SelectorError::Invalid(_))
        ));
        assert!(matches!(
            Selector::parse("Foo/bar"),
            Err(SelectorError::Invalid(_))
        ));
    }
}
```

- [ ] **Step 5: Run tests to verify**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run -E 'test(/selector::tests/)'`
Expected: All selector tests pass

- [ ] **Step 6: Commit**

```bash
git add src/selector.rs
git commit -m "refactor: change container prefix from agent- to jackin- in selector parsing"
```

---

### Task 2: Update instance.rs — Change container naming prefix

**Files:**
- Modify: `src/instance.rs:56-82` (naming functions)
- Modify: `src/instance.rs:84-143` (tests)

- [ ] **Step 1: Update `primary_container_name` to use `jackin-` prefix**

```rust
pub fn primary_container_name(selector: &ClassSelector) -> String {
    match &selector.namespace {
        Some(namespace) => format!("jackin-{namespace}-{}", selector.name),
        None => format!("jackin-{}", selector.name),
    }
}
```

- [ ] **Step 2: Update all tests in instance.rs**

Replace the test module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn picks_next_clone_name() {
        let selector = ClassSelector::new(None, "agent-smith");
        let existing = vec![
            "jackin-agent-smith".to_string(),
            "jackin-agent-smith-clone-1".to_string(),
        ];

        let name = next_container_name(&selector, &existing);

        assert_eq!(name, "jackin-agent-smith-clone-2");
    }

    #[test]
    fn prepares_persisted_claude_state() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();
        std::fs::write(temp.path().join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        let manifest = crate::manifest::AgentManifest::load(temp.path()).unwrap();

        let state = AgentState::prepare(&paths, "jackin-agent-smith", &manifest).unwrap();

        assert!(state.claude_dir.is_dir());
        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
    }

    #[test]
    fn prepares_plugins_json_for_runtime_bootstrap() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\", \"feature-dev@claude-plugins-official\"]\n",
        )
        .unwrap();
        std::fs::write(temp.path().join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();

        let manifest = crate::manifest::AgentManifest::load(temp.path()).unwrap();
        let state = AgentState::prepare(&paths, "jackin-agent-smith", &manifest).unwrap();

        assert!(state.jackin_dir.is_dir());
        assert_eq!(
            std::fs::read_to_string(&state.plugins_json).unwrap(),
            "{\n  \"plugins\": [\n    \"code-review@claude-plugins-official\",\n    \"feature-dev@claude-plugins-official\"\n  ]\n}"
        );
    }
}
```

- [ ] **Step 3: Run tests to verify**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run -E 'test(/instance::tests/)'`
Expected: All instance tests pass

- [ ] **Step 4: Commit**

```bash
git add src/instance.rs
git commit -m "refactor: change container naming prefix from agent- to jackin-"
```

---

### Task 3: Update runtime.rs — Change network naming and update tests

**Files:**
- Modify: `src/runtime.rs:50` (load_agent network naming)
- Modify: `src/runtime.rs:244` (eject_agent network naming)
- Modify: `src/runtime.rs:391-660` (all tests)

- [ ] **Step 1: Update network naming in `load_agent`**

Change line 50 from:
```rust
let network = format!("jackin-{container_name}-net");
```
to:
```rust
let network = format!("{container_name}-net");
```

- [ ] **Step 2: Update network naming in `eject_agent`**

Change line 244 from:
```rust
let network = format!("jackin-{container_name}-net");
```
to:
```rust
let network = format!("{container_name}-net");
```

- [ ] **Step 3: Update `load_owner_repo_registers_source_and_builds_commands` test**

```rust
#[test]
fn load_owner_repo_registers_source_and_builds_commands() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = ClassSelector::new(Some("chainargos"), "the-architect");
    let mut runner = FakeRunner::with_capture_queue([
        String::new(),
        "jackin-chainargos-the-architect".to_string(),
    ]);

    let repo_dir = paths.agents_dir.join("chainargos").join("the-architect");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
    std::fs::write(
        repo_dir.join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
    )
    .unwrap();

    load_agent(&paths, &mut config, &selector, &mut runner).unwrap();

    assert!(std::fs::read_to_string(&paths.config_file)
        .unwrap()
        .contains("chainargos/the-architect"));
    assert!(runner
        .recorded
        .iter()
        .any(|call| call.contains("git -C") || call.contains("git clone")));
    assert!(runner
        .recorded
        .iter()
        .any(|call| call.contains("docker build -t jackin-chainargos-the-architect -f")));
    assert!(runner.recorded.iter().any(|call| {
        call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}"
    }));
    assert!(runner
        .recorded
        .iter()
        .any(|call| call.contains("docker run -it --name jackin-chainargos-the-architect")));
    assert!(runner
        .recorded
        .iter()
        .any(|call| call.contains("/home/claude/.jackin/plugins.json:ro")));
    assert!(!runner
        .recorded
        .iter()
        .any(|call| call.contains("claude plugin install")));
}
```

- [ ] **Step 4: Update `eject_all_targets_only_requested_class_family` test**

```rust
#[test]
fn eject_all_targets_only_requested_class_family() {
    let selector = ClassSelector::new(None, "agent-smith");
    let names = vec![
        "jackin-agent-smith".to_string(),
        "jackin-agent-smith-clone-1".to_string(),
        "jackin-chainargos-the-architect".to_string(),
    ];

    let matched = matching_family(&selector, &names);

    assert_eq!(matched, vec!["jackin-agent-smith", "jackin-agent-smith-clone-1"]);
}
```

- [ ] **Step 5: Update `purge_all_removes_matching_state_directories` test**

```rust
#[test]
fn purge_all_removes_matching_state_directories() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(paths.data_dir.join("jackin-agent-smith")).unwrap();
    std::fs::create_dir_all(paths.data_dir.join("jackin-agent-smith-clone-1")).unwrap();
    std::fs::create_dir_all(paths.data_dir.join("jackin-chainargos-the-architect")).unwrap();
    let selector = ClassSelector::new(None, "agent-smith");

    purge_class_data(&paths, &selector).unwrap();

    assert!(!paths.data_dir.join("jackin-agent-smith").exists());
    assert!(!paths.data_dir.join("jackin-agent-smith-clone-1").exists());
    assert!(paths.data_dir.join("jackin-chainargos-the-architect").exists());
}
```

- [ ] **Step 6: Update `eject_agent_removes_container_dind_and_network` test**

```rust
#[test]
fn eject_agent_removes_container_dind_and_network() {
    let mut runner = FakeRunner::default();

    eject_agent("jackin-agent-smith", &mut runner).unwrap();

    assert_eq!(runner.recorded, vec![
        "docker rm -f jackin-agent-smith",
        "docker rm -f jackin-agent-smith-dind",
        "docker network rm jackin-agent-smith-net",
    ]);
}
```

- [ ] **Step 7: Update `eject_agent_ignores_missing_runtime_resources` test**

```rust
#[test]
fn eject_agent_ignores_missing_runtime_resources() {
    let mut runner = FakeRunner {
        fail_with: vec![
            (
                "docker rm -f jackin-agent-smith".to_string(),
                "Error response from daemon: No such container: jackin-agent-smith".to_string(),
            ),
            (
                "docker rm -f jackin-agent-smith-dind".to_string(),
                "Error response from daemon: No such container: jackin-agent-smith-dind".to_string(),
            ),
            (
                "docker network rm jackin-agent-smith-net".to_string(),
                "Error response from daemon: No such network: jackin-agent-smith-net"
                    .to_string(),
            ),
        ],
        ..Default::default()
    };

    eject_agent("jackin-agent-smith", &mut runner).unwrap();

    assert_eq!(runner.recorded, vec![
        "docker rm -f jackin-agent-smith",
        "docker rm -f jackin-agent-smith-dind",
        "docker network rm jackin-agent-smith-net",
    ]);
}
```

- [ ] **Step 8: Update `exile_all_ejects_all_managed_agents` test**

```rust
#[test]
fn exile_all_ejects_all_managed_agents() {
    let mut runner = FakeRunner::with_capture_queue(["jackin-agent-smith\njackin-agent-smith-clone-1".to_string()]);

    exile_all(&mut runner).unwrap();

    assert_eq!(
        runner.recorded,
        vec![
            "docker ps -a --filter label=jackin.managed=true --format {{.Names}}",
            "docker rm -f jackin-agent-smith",
            "docker rm -f jackin-agent-smith-dind",
            "docker network rm jackin-agent-smith-net",
            "docker rm -f jackin-agent-smith-clone-1",
            "docker rm -f jackin-agent-smith-clone-1-dind",
            "docker network rm jackin-agent-smith-clone-1-net",
        ]
    );
}
```

- [ ] **Step 9: Update `exile_all_continues_when_some_runtime_resources_are_missing` test**

```rust
#[test]
fn exile_all_continues_when_some_runtime_resources_are_missing() {
    let mut runner = FakeRunner {
        fail_with: vec![
            (
                "docker rm -f jackin-agent-smith".to_string(),
                "Error response from daemon: No such container: jackin-agent-smith".to_string(),
            ),
            (
                "docker network rm jackin-agent-smith-net".to_string(),
                "Error response from daemon: No such network: jackin-agent-smith-net"
                    .to_string(),
            ),
        ],
        capture_queue: VecDeque::from(vec!["jackin-agent-smith\njackin-agent-smith-clone-1".to_string()]),
        ..Default::default()
    };

    exile_all(&mut runner).unwrap();

    assert_eq!(
        runner.recorded,
        vec![
            "docker ps -a --filter label=jackin.managed=true --format {{.Names}}",
            "docker rm -f jackin-agent-smith",
            "docker rm -f jackin-agent-smith-dind",
            "docker network rm jackin-agent-smith-net",
            "docker rm -f jackin-agent-smith-clone-1",
            "docker rm -f jackin-agent-smith-clone-1-dind",
            "docker network rm jackin-agent-smith-clone-1-net",
        ]
    );
}
```

- [ ] **Step 10: Update `hardline_uses_docker_attach` test**

```rust
#[test]
fn hardline_uses_docker_attach() {
    let mut runner = FakeRunner::default();

    hardline_agent("jackin-agent-smith", &mut runner).unwrap();

    assert_eq!(runner.recorded.last().unwrap(), "docker attach jackin-agent-smith");
}
```

- [ ] **Step 11: Update `load_agent_runs_attached_with_plugins_mount` test**

```rust
#[test]
fn load_agent_runs_attached_with_plugins_mount() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = ClassSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::with_capture_queue([
        String::new(),
        "jackin-agent-smith".to_string(),
    ]);

    let repo_dir = paths.agents_dir.join("agent-smith");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
    std::fs::write(
        repo_dir.join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
    )
    .unwrap();

    load_agent(&paths, &mut config, &selector, &mut runner).unwrap();

    assert!(runner
        .recorded
        .iter()
        .any(|call| call.contains("docker build -t jackin-agent-smith -f")));
    assert!(runner.recorded.iter().any(|call| {
        call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}"
    }));
    assert!(runner
        .recorded
        .iter()
        .any(|call| call.contains("docker run -it --name jackin-agent-smith")));
    assert!(runner
        .recorded
        .iter()
        .any(|call| call.contains("/home/claude/.jackin/plugins.json:ro")));
    assert!(!runner.recorded.iter().any(|call| call == "docker rm -f jackin-agent-smith"));
    assert!(!runner
        .recorded
        .iter()
        .any(|call| call.contains("claude plugin install")));
}
```

- [ ] **Step 12: Update `load_agent_rolls_back_runtime_on_attached_run_failure` test**

```rust
#[test]
fn load_agent_rolls_back_runtime_on_attached_run_failure() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = ClassSelector::new(None, "agent-smith");
    let mut runner = FakeRunner {
        fail_on: vec!["docker run -it --name jackin-agent-smith".to_string()],
        capture_queue: VecDeque::from(vec![String::new()]),
        ..Default::default()
    };

    let repo_dir = paths.agents_dir.join("agent-smith");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
    std::fs::write(
        repo_dir.join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
    )
    .unwrap();

    let error = load_agent(&paths, &mut config, &selector, &mut runner).unwrap_err();

    assert!(error.to_string().contains("docker run -it --name jackin-agent-smith"));
    assert!(runner.recorded.iter().any(|call| call == "docker rm -f jackin-agent-smith"));
    assert!(runner.recorded.iter().any(|call| call == "docker rm -f jackin-agent-smith-dind"));
    assert!(runner
        .recorded
        .iter()
        .any(|call| call == "docker network rm jackin-agent-smith-net"));
}
```

- [ ] **Step 13: Run tests to verify**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run -E 'test(/runtime::tests/)'`
Expected: All runtime tests pass

- [ ] **Step 14: Commit**

```bash
git add src/runtime.rs
git commit -m "refactor: simplify network naming and update tests for jackin- container prefix"
```

---

### Task 4: Update config.rs — Change default agent and auto-registration URL

**Files:**
- Modify: `src/config.rs:40-51` (resolve_or_register)
- Modify: `src/config.rs:58-67` (default_config)
- Modify: `src/config.rs:70-105` (tests)

- [ ] **Step 1: Update `resolve_or_register` to prepend `jackin-` to auto-derived repo names**

Change line 46 from:
```rust
let source = AgentSource {
    git: format!("git@github.com:{namespace}/{}.git", selector.name),
};
```
to:
```rust
let source = AgentSource {
    git: format!("git@github.com:{namespace}/jackin-{}.git", selector.name),
};
```

- [ ] **Step 2: Update `default_config` to use `agent-smith`**

```rust
fn default_config() -> Self {
    let mut agents = BTreeMap::new();
    agents.insert(
        "agent-smith".to_string(),
        AgentSource {
            git: "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
        },
    );
    Self { agents }
}
```

- [ ] **Step 3: Update all tests in config.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_writes_default_agent_smith_entry() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        let config = AppConfig::load_or_init(&paths).unwrap();

        assert_eq!(
            config.agents.get("agent-smith").unwrap().git,
            "git@github.com:donbeave/jackin-agent-smith.git"
        );
        assert!(paths.config_file.exists());
    }

    #[test]
    fn resolve_or_register_adds_owner_repo_on_first_use() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "the-architect");

        let source = config.resolve_or_register(&selector, &paths).unwrap();

        assert_eq!(source.git, "git@github.com:chainargos/jackin-the-architect.git");
        assert!(std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("[agents.\"chainargos/the-architect\"]"));
    }
}
```

- [ ] **Step 4: Run tests to verify**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run -E 'test(/config::tests/)'`
Expected: All config tests pass

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "refactor: update default agent to agent-smith and add jackin- repo prefix to auto-registration"
```

---

### Task 5: Update manifest.rs — Add Identity support

**Files:**
- Modify: `src/manifest.rs:1-22` (structs)
- Modify: `src/manifest.rs:24-43` (tests)

- [ ] **Step 1: Add `IdentityConfig` struct and update `AgentManifest`**

Replace the struct definitions (lines 1-22) with:

```rust
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentManifest {
    pub dockerfile: String,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    pub claude: ClaudeConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentityConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub plugins: Vec<String>,
}

impl AgentManifest {
    pub fn load(repo_dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = repo_dir.join("jackin.agent.toml");
        let contents = std::fs::read_to_string(&manifest_path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map(|id| id.name.clone())
            .unwrap_or_else(|| fallback.to_string())
    }
}
```

- [ ] **Step 2: Update tests to cover identity parsing**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_manifest_with_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.dockerfile, "Dockerfile");
        assert_eq!(manifest.claude.plugins.len(), 1);
        assert!(manifest.identity.is_none());
    }

    #[test]
    fn loads_manifest_with_identity() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[identity]\nname = \"Agent Smith\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.identity.as_ref().unwrap().name, "Agent Smith");
    }

    #[test]
    fn display_name_uses_identity_when_present() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[identity]\nname = \"Agent Smith\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.display_name("agent-smith"), "Agent Smith");
    }

    #[test]
    fn display_name_falls_back_to_class_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.display_name("agent-smith"), "agent-smith");
    }
}
```

- [ ] **Step 3: Run tests to verify**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run -E 'test(/manifest::tests/)'`
Expected: All manifest tests pass

- [ ] **Step 4: Commit**

```bash
git add src/manifest.rs
git commit -m "feat: add [identity] support to agent manifest for display names"
```

---

### Task 6: Update repo.rs tests — Use new naming

**Files:**
- Modify: `src/repo.rs:71-152` (tests)

- [ ] **Step 1: Update `computes_cached_repo_path_for_namespaced_selector` test**

```rust
#[test]
fn computes_cached_repo_path_for_namespaced_selector() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = ClassSelector::new(Some("chainargos"), "the-architect");

    let repo = CachedRepo::new(&paths, &selector);

    assert_eq!(
        repo.repo_dir,
        paths.agents_dir.join("chainargos").join("the-architect")
    );
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run -E 'test(/repo::tests/)'`
Expected: All repo tests pass

- [ ] **Step 3: Commit**

```bash
git add src/repo.rs
git commit -m "test: update repo tests to use new naming convention"
```

---

### Task 7: Update cli.rs tests — Use new naming

**Files:**
- Modify: `src/cli.rs:29-43` (tests)

- [ ] **Step 1: Update `parses_load_command` test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_load_command() {
        let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Load {
                selector: "agent-smith".to_string(),
            }
        );
    }
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run -E 'test(/cli::tests/)'`
Expected: All CLI tests pass

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs
git commit -m "test: update CLI tests to use agent-smith class name"
```

---

### Task 8: Run full test suite

- [ ] **Step 1: Run all tests**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run`
Expected: All tests pass

- [ ] **Step 2: Fix any failures if needed**

If any tests fail, fix them before proceeding.

---

### Task 9: Update jackin README.md

**Files:**
- Modify: `/Users/donbeave/Projects/donbeave/jackin/README.md`

- [ ] **Step 1: Replace README with updated content**

```markdown
# jackin

`jackin` is a Matrix-inspired CLI for orchestrating AI coding agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

> **Current status:** jackin is built as a proof of concept around [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as its first and only supported agent runtime. Support for additional agent runtimes — [Codex](https://github.com/openai/codex) and [Amp Code](https://ampcode.com) — is planned for future releases.

## Construct

`donbeave/jackin-construct:trixie` is the shared base image for every agent repo. In The Matrix, the construct is the base simulated environment you load before a mission. That maps directly to `jackin`'s shared runtime image: every agent starts from the same construct before layering on its own specialized environment.

## Commands

- `jackin load agent-smith` — send an agent in.
- `jackin hardline jackin-agent-smith` — reattach to a running agent.
- `jackin eject jackin-agent-smith` — pull one agent out.
- `jackin eject agent-smith --all` — pull every Agent Smith out for one class scope.
- `jackin exile` — remove every running agent.
- `jackin purge agent-smith --all` — delete persisted state for one class.

## Naming Convention

Agent repos follow the `jackin-{class-name}` naming convention on GitHub:

- `jackin-agent-smith` — the default agent
- `jackin-neo` — a custom agent named "neo"
- `chainargos/jackin-the-architect` — a namespaced agent

The class name is what you use with `jackin load`. The repo name adds the `jackin-` prefix for discoverability.

## Agent Identity

Agents can declare a display name in `jackin.agent.toml`:

```toml
[identity]
name = "Agent Smith"
```

This name is used for visualization in jackin. When omitted, the class selector name is used instead.

## Storage

- `~/.config/jackin/config.toml` — operator config.
- `~/.jackin/agents/...` — cached agent repositories.
- `~/.jackin/data/<container-name>/` — persisted `.claude`, `.claude.json`, and `plugins.json` for one agent instance.

## Agent Repo Contract

Each agent repo must contain:

- `jackin.agent.toml`
- a Dockerfile at the path declared by `jackin.agent.toml`

The manifest Dockerfile path must be relative and must stay inside the repo checkout.

Derived build-context generation currently rejects symlinks in the agent repo instead of following or preserving them.

The final Dockerfile stage must literally be `FROM donbeave/jackin-construct:trixie`, optionally with an alias such as `FROM donbeave/jackin-construct:trixie AS runtime`. Earlier stages may use any base image.

`agent-smith`-style agent repos only own their agent-specific environment layer. `jackin` owns the runtime wiring around that layer: validating the repo contract, generating the derived Dockerfile, installing Claude into the derived image, injecting the runtime entrypoint, mounting the cached repo checkout at `/workspace`, mounting persisted `.claude`, `.claude.json`, and `plugins.json`, and wiring the per-agent Docker-in-Docker runtime.

## Roadmap

- [x] Claude Code agent runtime
- [ ] Kubernetes platform support
- [ ] [Codex](https://github.com/openai/codex) agent runtime
- [ ] [Amp Code](https://ampcode.com) agent runtime
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README for agent-smith rename and jackin- naming convention"
```

---

### Task 10: Update jackin v1 design doc

**Files:**
- Modify: `/Users/donbeave/Projects/donbeave/jackin/docs/superpowers/specs/2026-04-01-jackin-v1-design.md`

- [ ] **Step 1: Replace all `smith` class references with `agent-smith`**

Apply these replacements throughout the file:
- `"smith"` (as class selector) → `"agent-smith"`
- `agent-smith` (as container name) → `jackin-agent-smith`
- `chainargos` (as namespace) → `chainargos`
- `chainargos/smith` → `chainargos/the-architect`
- `agent-chainargos-smith` → `jackin-chainargos-the-architect`
- `jackin-smith` → `jackin-agent-smith`
- Network names: `jackin-{container}-net` → `{container}-net`
- Storage paths updated to match new naming
- Update the Naming Rules section to reflect new convention
- Add mention of `[identity]` in the Repo Manifest section

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-01-jackin-v1-design.md
git commit -m "docs: update v1 design spec for agent-smith rename"
```

---

### Task 11: Update construct-smith design doc

**Files:**
- Modify: `/Users/donbeave/Projects/donbeave/jackin/docs/superpowers/specs/2026-04-01-jackin-construct-smith-design.md`

- [ ] **Step 1: Update all references**

Apply these replacements:
- `donbeave/smith` → `donbeave/jackin-agent-smith`
- `smith` (as repo/class name) → `agent-smith`
- `chainargos` → `chainargos`

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-04-01-jackin-construct-smith-design.md
git commit -m "docs: update construct-smith design spec for agent-smith rename"
```

---

### Task 12: Rename smith repo locally and update its content

**Files:**
- Rename: `/Users/donbeave/Projects/donbeave/smith` → `/Users/donbeave/Projects/donbeave/jackin-agent-smith`
- Modify: `jackin-agent-smith/jackin.agent.toml`
- Modify: `jackin-agent-smith/README.md`

- [ ] **Step 1: Rename the directory**

```bash
mv /Users/donbeave/Projects/donbeave/smith /Users/donbeave/Projects/donbeave/jackin-agent-smith
```

- [ ] **Step 2: Update `jackin.agent.toml` to add identity**

Write the updated content:

```toml
dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = [
  "code-review@claude-plugins-official",
  "feature-dev@claude-plugins-official",
]
```

- [ ] **Step 3: Update `README.md`**

```markdown
# agent-smith

`agent-smith` is the first public-friendly `jackin` agent repo.

It provides only the agent-specific environment layer for `jackin`, not the final Claude runtime. `jackin` validates this repo's Dockerfile, derives the final image itself, and mounts the cached repo checkout into `/workspace` when you run `jackin load agent-smith`.

## Contract

- final Dockerfile stage must literally be `FROM donbeave/jackin-construct:trixie`
- plugins are declared in `jackin.agent.toml`
- the repo is expected to run cleanly without company-specific secrets, custom CA setup, or private mirrors

## Environment

For v1, `agent-smith` intentionally stays minimal:

- shared shell/runtime tools come from `jackin/construct:trixie`
- this repo preinstalls `node@lts`
- runtime workspace is the repo itself, mounted at `/workspace`
```

- [ ] **Step 4: Commit changes in agent-smith repo**

```bash
cd /Users/donbeave/Projects/donbeave/jackin-agent-smith
git add jackin.agent.toml README.md
git commit -m "refactor: rename to agent-smith and add identity config"
```

---

### Task 13: Rename GitHub repo

- [ ] **Step 1: Rename the repo on GitHub**

```bash
gh repo rename jackin-agent-smith --repo donbeave/smith --yes
```

- [ ] **Step 2: Update the local remote URL**

```bash
cd /Users/donbeave/Projects/donbeave/jackin-agent-smith
git remote set-url origin git@github.com:donbeave/jackin-agent-smith.git
```

- [ ] **Step 3: Push changes**

```bash
cd /Users/donbeave/Projects/donbeave/jackin-agent-smith
git push origin main
```

- [ ] **Step 4: Verify**

```bash
gh repo view donbeave/jackin-agent-smith --json name,url
```

---

### Task 14: Final verification

- [ ] **Step 1: Run full test suite in jackin**

Run: `cd /Users/donbeave/Projects/donbeave/jackin && cargo nextest run`
Expected: All tests pass

- [ ] **Step 2: Verify no remaining old references in jackin source**

```bash
cd /Users/donbeave/Projects/donbeave/jackin
grep -r '"smith"' src/ --include='*.rs'
grep -r 'donbeave/smith' src/ README.md
grep -r 'chainargos' src/ --include='*.rs'
```
Expected: No matches

- [ ] **Step 3: Push jackin changes**

```bash
cd /Users/donbeave/Projects/donbeave/jackin
git push origin main
```

# Jackin Construct And Smith Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `jackin`'s direct agent-image build path with a strict `donbeave/jackin-construct:trixie` derived-image pipeline and create the first public `smith` agent repo that loads through it.

**Architecture:** `jackin` gains three focused units: persisted runtime state preparation, Dockerfile contract validation, and derived-image build context generation. Runtime orchestration then consumes those units to run attached Claude sessions with per-agent networks and DinD sidecars, while a new sibling `smith` repo provides only an agent-specific environment layer on top of `donbeave/jackin-construct:trixie`.

**Tech Stack:** Rust 2024, `anyhow`, `serde`, `serde_json`, `dockerfile-parser`, Docker, shell scripts, GitHub CLI.

---

## File Structure

### `donbeave/jackin`

- Modify: `Cargo.toml` — add runtime dependencies for JSON state and Dockerfile parsing.
- Modify: `src/lib.rs` — export new focused modules.
- Modify: `src/repo.rs` — delegate Dockerfile contract parsing to a dedicated validator and return richer validated repo data.
- Modify: `src/instance.rs` — add `.jackin/plugins.json` persisted state preparation.
- Modify: `src/runtime.rs` — switch from detached `docker exec` orchestration to attached derived-image runtime orchestration.
- Create: `src/repo_contract.rs` — parse Dockerfiles and validate the strict final-stage `FROM donbeave/jackin-construct:trixie` contract.
- Create: `src/derived_image.rs` — create temporary build contexts and render the derived Dockerfile.
- Create: `docker/construct/Dockerfile` — shared construct image source.
- Create: `docker/construct/install-plugins.sh` — generic plugin installer that reads `/home/claude/.jackin/plugins.json` with `jq`.
- Create: `docker/construct/zshrc` — shared shell baseline.
- Create: `docker/runtime/entrypoint.sh` — runtime-owned Claude bootstrap script injected by `jackin`.
- Modify: `README.md` — explain the construct name and the new agent repo contract.

### `donbeave/smith`

- Create: `../smith/README.md` — document `smith` as a public-friendly `jackin` agent repo mounted as `/workspace`.
- Create: `../smith/Dockerfile` — minimal agent-specific layer on `donbeave/jackin-construct:trixie` that preinstalls `node@lts`.
- Create: `../smith/jackin.agent.toml` — plugin declarations and Dockerfile path.
- Create: `../smith/.gitignore` — keep the new repo clean.

### Verification Targets

- Test: `cargo nextest run`
- Verify construct image: `docker build -t donbeave/jackin-construct:trixie docker/construct`
- Verify runtime flow: `cargo run -- load smith`, detach with `Ctrl-P`, `Ctrl-Q`, then `cargo run -- hardline agent-smith`, `cargo run -- eject smith --purge`

### Task 1: Persisted Runtime State And Dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/instance.rs`
- Test: `src/instance.rs`

- [ ] **Step 1: Write the failing state test**

Add this test to `src/instance.rs` below `prepares_persisted_claude_state`:

```rust
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
        let state = AgentState::prepare(&paths, "agent-smith", &manifest).unwrap();

        assert!(state.jackin_dir.is_dir());
        assert_eq!(
            std::fs::read_to_string(&state.plugins_json).unwrap(),
            "{\n  \"plugins\": [\n    \"code-review@claude-plugins-official\",\n    \"feature-dev@claude-plugins-official\"\n  ]\n}"
        );
    }
```

- [ ] **Step 2: Run the focused test and confirm it fails**

Run: `cargo nextest run prepares_plugins_json_for_runtime_bootstrap -E 'test(=prepares_plugins_json_for_runtime_bootstrap)'`

Expected: FAIL with compile errors such as `no field 'jackin_dir' on type 'AgentState'` and `this function takes 2 arguments but 3 arguments were supplied`.

- [ ] **Step 3: Add the runtime dependencies**

Update `Cargo.toml` to this dependency shape:

```toml
[dependencies]
anyhow = "1.0"
clap = { version = "4.5", features = ["derive", "color"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
directories = "6.0"
thiserror = "2.0"
dockerfile-parser = "0.9.0"
tempfile = "3.20"

[dev-dependencies]
assert_cmd = "2.0"
predicates = "3.1"
```

- [ ] **Step 4: Implement persisted `.jackin/plugins.json` state**

Update `src/instance.rs` so `AgentState` owns the new jackin-specific state and writes plugin metadata during `prepare`:

```rust
use crate::manifest::AgentManifest;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentState {
    pub root: PathBuf,
    pub claude_dir: PathBuf,
    pub claude_json: PathBuf,
    pub jackin_dir: PathBuf,
    pub plugins_json: PathBuf,
}

#[derive(Debug, Serialize)]
struct PluginState<'a> {
    plugins: &'a [String],
}

impl AgentState {
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &AgentManifest,
    ) -> anyhow::Result<Self> {
        let root = paths.data_dir.join(container_name);
        let claude_dir = root.join(".claude");
        let claude_json = root.join(".claude.json");
        let jackin_dir = root.join(".jackin");
        let plugins_json = jackin_dir.join("plugins.json");

        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&jackin_dir)?;

        if !claude_json.exists() {
            std::fs::write(&claude_json, "{}")?;
        }

        std::fs::write(
            &plugins_json,
            serde_json::to_string_pretty(&PluginState {
                plugins: &manifest.claude.plugins,
            })?,
        )?;

        Ok(Self {
            root,
            claude_dir,
            claude_json,
            jackin_dir,
            plugins_json,
        })
    }
}
```

- [ ] **Step 5: Re-run the focused test and then the full state module tests**

Run: `cargo nextest run prepares_plugins_json_for_runtime_bootstrap -E 'test(=prepares_plugins_json_for_runtime_bootstrap)'`

Expected: PASS

Run: `cargo nextest run instance::tests --no-capture`

Expected: PASS with the existing naming tests and the new plugins JSON test.

- [ ] **Step 6: Commit the state scaffolding**

```bash
git add Cargo.toml src/instance.rs
git commit -m "feat: persist jackin runtime plugin metadata"
```

### Task 2: Validate The Agent Dockerfile Contract

**Files:**
- Create: `src/repo_contract.rs`
- Modify: `src/lib.rs`
- Modify: `src/repo.rs`
- Test: `src/repo_contract.rs`

- [ ] **Step 1: Write the failing contract tests**

Create `src/repo_contract.rs` with these tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn accepts_final_stage_on_construct_image() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            "FROM rust:1.87 AS builder\nRUN cargo build\n\nFROM donbeave/jackin-construct:trixie AS runtime\nCOPY --from=builder /app /workspace/app\n",
        )
        .unwrap();

        let validated = validate_agent_dockerfile(&dockerfile).unwrap();

        assert_eq!(validated.final_stage_image, CONSTRUCT_IMAGE);
        assert_eq!(validated.final_stage_alias.as_deref(), Some("runtime"));
    }

    #[test]
    fn rejects_final_stage_on_other_image() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(&dockerfile, "FROM debian:trixie\n").unwrap();

        let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

        assert!(error.to_string().contains("donbeave/jackin-construct:trixie"));
    }

    #[test]
    fn rejects_arg_indirection_in_final_from() {
        let temp = tempdir().unwrap();
        let dockerfile = temp.path().join("Dockerfile");
        std::fs::write(
            &dockerfile,
            "ARG BASE=donbeave/jackin-construct:trixie\nFROM ${BASE}\n",
        )
        .unwrap();

        let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

        assert!(error.to_string().contains("literal FROM donbeave/jackin-construct:trixie"));
    }
}
```

- [ ] **Step 2: Run the focused contract tests and confirm they fail**

Run: `cargo nextest run repo_contract::tests --no-capture`

Expected: FAIL with `cannot find function 'validate_agent_dockerfile' in this scope`.

- [ ] **Step 3: Implement the validator module**

Replace `src/repo_contract.rs` with this focused validator:

```rust
use dockerfile_parser::Dockerfile;
use std::path::{Path, PathBuf};

pub const CONSTRUCT_IMAGE: &str = "donbeave/jackin-construct:trixie";

#[derive(Debug, Clone)]
pub struct ValidatedDockerfile {
    pub dockerfile_path: PathBuf,
    pub dockerfile_contents: String,
    pub final_stage_image: String,
    pub final_stage_alias: Option<String>,
}

pub fn validate_agent_dockerfile(dockerfile_path: &Path) -> anyhow::Result<ValidatedDockerfile> {
    let dockerfile_contents = std::fs::read_to_string(dockerfile_path)?;
    let dockerfile = Dockerfile::parse(&dockerfile_contents)
        .map_err(|error| anyhow::anyhow!("invalid agent repo: unable to parse Dockerfile: {error}"))?;

    let final_stage = dockerfile
        .iter_stages()
        .last()
        .ok_or_else(|| anyhow::anyhow!("invalid agent repo: Dockerfile must contain at least one FROM instruction"))?;

    let from = &final_stage.from;
    anyhow::ensure!(
        from.image.as_str() == CONSTRUCT_IMAGE,
        "invalid agent repo: final Dockerfile stage must use literal FROM {}",
        CONSTRUCT_IMAGE
    );

    Ok(ValidatedDockerfile {
        dockerfile_path: dockerfile_path.to_path_buf(),
        dockerfile_contents,
        final_stage_image: from.image.to_string(),
        final_stage_alias: from.alias.clone(),
    })
}
```

- [ ] **Step 4: Wire the validator into repo validation**

Update `src/repo.rs` so `ValidatedAgentRepo` carries the validated Dockerfile metadata:

```rust
use crate::manifest::AgentManifest;
use crate::paths::JackinPaths;
use crate::repo_contract::{validate_agent_dockerfile, ValidatedDockerfile};
use crate::selector::ClassSelector;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ValidatedAgentRepo {
    pub manifest: AgentManifest,
    pub dockerfile: ValidatedDockerfile,
}

pub fn validate_agent_repo(repo_dir: &Path) -> anyhow::Result<ValidatedAgentRepo> {
    let manifest_path = repo_dir.join("jackin.agent.toml");

    if !manifest_path.is_file() {
        anyhow::bail!("invalid agent repo: missing {}", manifest_path.display());
    }

    let manifest = AgentManifest::load(repo_dir)?;
    let dockerfile_path = resolve_manifest_dockerfile_path(repo_dir, &manifest)?;
    let dockerfile = validate_agent_dockerfile(&dockerfile_path)?;

    Ok(ValidatedAgentRepo { manifest, dockerfile })
}
```

Also export the new module from `src/lib.rs`:

```rust
pub mod repo_contract;
```

- [ ] **Step 5: Re-run the contract tests and the repo tests**

Run: `cargo nextest run repo_contract::tests --no-capture`

Expected: PASS

Run: `cargo nextest run repo::tests --no-capture`

Expected: PASS after updating the existing repo assertions from `validated.dockerfile_path` to `validated.dockerfile.dockerfile_path`.

- [ ] **Step 6: Commit the contract validator**

```bash
git add Cargo.toml src/lib.rs src/repo.rs src/repo_contract.rs
git commit -m "feat: validate agent dockerfiles against construct contract"
```

### Task 3: Build The Construct Assets And Derived Build Context

**Files:**
- Create: `docker/construct/Dockerfile`
- Create: `docker/construct/install-plugins.sh`
- Create: `docker/construct/zshrc`
- Create: `docker/runtime/entrypoint.sh`
- Create: `src/derived_image.rs`
- Modify: `src/lib.rs`
- Test: `src/derived_image.rs`

- [ ] **Step 1: Write the failing derived-image tests**

Create `src/derived_image.rs` with these tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn renders_derived_dockerfile_with_workspace_and_entrypoint() {
        let dockerfile = render_derived_dockerfile("FROM donbeave/jackin-construct:trixie\n");

        assert!(dockerfile.contains("RUN curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(dockerfile.contains("WORKDIR /workspace"));
        assert!(dockerfile.contains("COPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh"));
        assert!(dockerfile.contains("ENTRYPOINT [\"/home/claude/entrypoint.sh\"]"));
    }

    #[test]
    fn creates_temp_context_with_repo_copy_and_runtime_assets() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(repo.path().join("jackin.agent.toml"), "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n").unwrap();

        let validated = crate::repo::validate_agent_repo(repo.path()).unwrap();
        let build = create_derived_build_context(repo.path(), &validated).unwrap();

        assert!(build.context_dir.join("Dockerfile").is_file());
        assert!(build.context_dir.join(".jackin-runtime/entrypoint.sh").is_file());
        assert!(build.dockerfile_path.is_file());
    }
}
```

- [ ] **Step 2: Run the focused derived-image tests and confirm they fail**

Run: `cargo nextest run derived_image::tests --no-capture`

Expected: FAIL with `cannot find function 'render_derived_dockerfile' in this scope`.

- [ ] **Step 3: Create the shared construct image assets**

Create `docker/construct/install-plugins.sh`:

```bash
#!/bin/bash
set -euo pipefail

plugins_file="/home/claude/.jackin/plugins.json"

run_maybe_quiet() {
    if [ "${CLAUDE_DEBUG:-0}" = "1" ]; then
        "$@"
    else
        "$@" > /dev/null 2>&1
    fi
}

if [ ! -f "$plugins_file" ]; then
    exit 0
fi

run_maybe_quiet claude plugin marketplace add anthropics/claude-plugins-official || true

jq -r '.plugins[]?' "$plugins_file" | while IFS= read -r plugin; do
    [ -n "$plugin" ] || continue
    run_maybe_quiet claude plugin install "$plugin"
done
```

Create `docker/runtime/entrypoint.sh`:

```bash
#!/bin/bash
set -euo pipefail

run_maybe_quiet() {
    if [ "${CLAUDE_DEBUG:-0}" = "1" ]; then
        "$@"
    else
        "$@" > /dev/null 2>&1
    fi
}

run_maybe_quiet /home/claude/install-plugins.sh

printf '\033[2J\033[H'

exec env CLAUDE_ENV=docker claude --dangerously-skip-permissions --verbose
```

Create `docker/construct/zshrc`:

```bash
export PATH="$HOME/.local/share/mise/shims:$HOME/.local/bin:$PATH"
eval "$(starship init zsh)"
```

Create `docker/construct/Dockerfile`:

```dockerfile
FROM debian:trixie

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG UID=1000

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    bash \
    ca-certificates \
    curl \
    fd-find \
    fzf \
    git \
    git-lfs \
    jq \
    ripgrep \
    sudo \
    tree \
    yq \
    zsh && \
    ln -sf /usr/bin/fdfind /usr/local/bin/fd && \
    git lfs install && \
    rm -rf /var/lib/apt/lists/*

RUN install -m 0755 -d /etc/apt/keyrings && \
    curl -fsSL https://mise.jdx.dev/gpg-key.pub -o /etc/apt/keyrings/mise.asc && \
    chmod a+r /etc/apt/keyrings/mise.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/mise.asc] https://mise.jdx.dev/deb stable main" > /etc/apt/sources.list.d/mise.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends mise && \
    rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc && \
    chmod a+r /etc/apt/keyrings/docker.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" > /etc/apt/sources.list.d/docker.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends docker-ce-cli docker-compose-plugin && \
    rm -rf /var/lib/apt/lists/*

RUN useradd -m -u "$UID" -s /bin/zsh claude && \
    echo "claude ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/claude && \
    install -d -o claude -g claude /home/claude/.claude /home/claude/.jackin

USER claude

ENV PATH="/home/claude/.local/share/mise/shims:/home/claude/.local/bin:${PATH}"

RUN sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended && \
    git clone https://github.com/zsh-users/zsh-autosuggestions ${ZSH_CUSTOM:-/home/claude/.oh-my-zsh/custom}/plugins/zsh-autosuggestions

RUN curl -sS https://starship.rs/install.sh | sh -s -- -y

COPY --chown=claude:claude zshrc /home/claude/.zshrc
COPY --chown=claude:claude install-plugins.sh /home/claude/install-plugins.sh
RUN chmod +x /home/claude/install-plugins.sh
```

- [ ] **Step 4: Implement derived Dockerfile rendering and temp build contexts**

Replace `src/derived_image.rs` with this module and export it from `src/lib.rs` with `pub mod derived_image;`:

```rust
use crate::repo::ValidatedAgentRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ENTRYPOINT_SH: &str = include_str!("../docker/runtime/entrypoint.sh");

pub struct DerivedBuildContext {
    pub temp_dir: TempDir,
    pub context_dir: PathBuf,
    pub dockerfile_path: PathBuf,
}

pub fn render_derived_dockerfile(base_dockerfile: &str) -> String {
    format!(
        "{base_dockerfile}\nUSER root\nRUN curl -fsSL https://claude.ai/install.sh | bash\nRUN claude --version\nCOPY .jackin-runtime/entrypoint.sh /home/claude/entrypoint.sh\nRUN chmod +x /home/claude/entrypoint.sh\nWORKDIR /workspace\nUSER claude\nENTRYPOINT [\"/home/claude/entrypoint.sh\"]\n"
    )
}

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

    let dockerfile_path = context_dir.join(".jackin-runtime/DerivedDockerfile");
    std::fs::write(
        &dockerfile_path,
        render_derived_dockerfile(&validated.dockerfile.dockerfile_contents),
    )?;

    Ok(DerivedBuildContext {
        temp_dir,
        context_dir,
        dockerfile_path,
    })
}

fn copy_dir_all(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = to.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination)?;
        }
    }

    Ok(())
}
```

- [ ] **Step 5: Re-run the derived-image tests and build the construct image locally**

Run: `cargo nextest run derived_image::tests --no-capture`

Expected: PASS

Run: `docker build -t donbeave/jackin-construct:trixie docker/construct`

Expected: PASS with the final image tagged as `donbeave/jackin-construct:trixie`.

- [ ] **Step 6: Commit the construct and derived-image work**

```bash
git add docker/construct docker/runtime src/derived_image.rs src/lib.rs
git commit -m "feat: add construct image and derived build context"
```

### Task 4: Rework Runtime Orchestration For Attached Claude Sessions

**Files:**
- Modify: `src/runtime.rs`
- Modify: `src/repo.rs`
- Modify: `src/instance.rs`
- Test: `src/runtime.rs`

- [ ] **Step 1: Write the failing runtime tests for the new lifecycle**

Add these tests to `src/runtime.rs`:

```rust
    #[test]
    fn hardline_uses_docker_attach() {
        let mut runner = FakeRunner::default();

        hardline_agent("agent-smith", &mut runner).unwrap();

        assert_eq!(runner.recorded.last().unwrap(), "docker attach agent-smith");
    }

    #[test]
    fn load_agent_runs_attached_with_plugins_mount() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "smith");
        let mut runner = FakeRunner::with_capture_queue(["".to_string(), "agent-smith".to_string()]);

        let repo_dir = paths.agents_dir.join("smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM donbeave/jackin-construct:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        load_agent(&paths, &mut config, &selector, &mut runner).unwrap();

        assert!(runner.recorded.iter().any(|call| call.contains("docker build -t jackin-smith -f")));
        assert!(runner.recorded.iter().any(|call| call.contains("docker run -it --name agent-smith")));
        assert!(runner.recorded.iter().any(|call| call.contains("/home/claude/.jackin/plugins.json:ro")));
        assert!(!runner.recorded.iter().any(|call| call == "docker rm -f agent-smith"));
    }
```

- [ ] **Step 2: Run the focused runtime tests and confirm they fail**

Run: `cargo nextest run runtime::tests --no-capture`

Expected: FAIL because runtime still records `docker exec -it agent-smith sh` and still uses detached `docker run -d` plus `docker exec` plugin bootstrapping.

- [ ] **Step 3: Upgrade the fake runner for repeated capture calls**

Update `FakeRunner` in `src/runtime.rs` to support ordered capture output:

```rust
use std::collections::VecDeque;

#[derive(Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub fail_on: Vec<String>,
    pub capture_queue: VecDeque<String>,
}

impl FakeRunner {
    fn with_capture_queue<const N: usize>(outputs: [String; N]) -> Self {
        Self {
            capture_queue: VecDeque::from(outputs.to_vec()),
            ..Default::default()
        }
    }
}

impl CommandRunner for FakeRunner {
    fn capture(
        &mut self,
        program: &str,
        args: &[String],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }
}
```

- [ ] **Step 4: Rewrite `load_agent` and `hardline_agent` around the new pipeline**

Update `src/runtime.rs` to use the derived build context, mounted plugin metadata, attached `docker run`, and `docker attach`:

```rust
use crate::config::AppConfig;
use crate::derived_image::create_derived_build_context;
use crate::docker::CommandRunner;
use crate::instance::{next_container_name, AgentState};
use crate::paths::JackinPaths;
use crate::repo::{validate_agent_repo, CachedRepo};
use crate::selector::ClassSelector;

pub fn load_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &ClassSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let source = config.resolve_or_register(selector, paths)?;
    let cached_repo = CachedRepo::new(paths, selector);
    std::fs::create_dir_all(cached_repo.repo_dir.parent().unwrap())?;

    if cached_repo.repo_dir.exists() {
        runner.run(
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
        runner.run(
            "git",
            &[
                "clone".into(),
                source.git.clone(),
                cached_repo.repo_dir.display().to_string(),
            ],
            None,
        )?;
    }

    let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;
    let existing = list_running_agent_names(runner)?;
    let container_name = next_container_name(selector, &existing);
    let state = AgentState::prepare(paths, &container_name, &validated_repo.manifest)?;
    let build = create_derived_build_context(&cached_repo.repo_dir, &validated_repo)?;

    let image = image_name(selector);
    let network = format!("jackin-{container_name}-net");
    let dind = format!("{container_name}-dind");
    let mut cleanup = LoadCleanup::new(container_name.clone(), dind.clone(), network.clone());

    let load_result = (|| -> anyhow::Result<()> {
        runner.run(
            "docker",
            &["network".into(), "create".into(), network.clone()],
            None,
        )?;

        runner.run(
            "docker",
            &[
                "run".into(),
                "-d".into(),
                "--name".into(),
                dind.clone(),
                "--network".into(),
                network.clone(),
                "--privileged".into(),
                "docker:dind".into(),
            ],
            None,
        )?;

        wait_for_dind(&dind, runner)?;

        runner.run(
            "docker",
            &[
                "build".into(),
                "-t".into(),
                image.clone(),
                "-f".into(),
                build.dockerfile_path.display().to_string(),
                build.context_dir.display().to_string(),
            ],
            None,
        )?;

        runner.run(
            "docker",
            &[
                "run".into(),
                "-it".into(),
                "--name".into(),
                container_name.clone(),
                "--hostname".into(),
                container_name.clone(),
                "--network".into(),
                network.clone(),
                "--label".into(),
                "jackin.managed=true".into(),
                "--label".into(),
                format!("jackin.class={}", selector.key()),
                "-e".into(),
                format!("DOCKER_HOST=tcp://{dind}:2375"),
                "-v".into(),
                format!("{}:/workspace", cached_repo.repo_dir.display()),
                "-v".into(),
                format!("{}:/home/claude/.claude", state.claude_dir.display()),
                "-v".into(),
                format!("{}:/home/claude/.claude.json", state.claude_json.display()),
                "-v".into(),
                format!("{}:/home/claude/.jackin/plugins.json:ro", state.plugins_json.display()),
                image.clone(),
            ],
            None,
        )?;

        Ok(())
    })();

    match load_result {
        Ok(()) => {
            if list_running_agent_names(runner)?.iter().any(|name| name == &container_name) {
                cleanup.disarm();
                Ok(())
            } else {
                cleanup.run(runner);
                Ok(())
            }
        }
        Err(error) => {
            cleanup.run(runner);
            Err(error)
        }
    }
}

pub fn hardline_agent(
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    runner.run("docker", &["attach".into(), container_name.to_string()], None)
}
```

Delete `bootstrap_plugins`, because plugin installation now happens in the container entrypoint through the mounted `plugins.json` file.

- [ ] **Step 5: Run the runtime tests and then the full suite**

Run: `cargo nextest run runtime::tests --no-capture`

Expected: PASS with updated `docker attach`, attached `docker run -it`, and no `docker exec claude plugin install` calls.

Run: `cargo nextest run`

Expected: PASS for the full `jackin` crate.

- [ ] **Step 6: Commit the runtime rewrite**

```bash
git add src/instance.rs src/repo.rs src/runtime.rs src/derived_image.rs
git commit -m "feat: run derived jackin agents as attached claude sessions"
```

### Task 5: Update Jackin Documentation And Verify The Construct Contract

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the README with the construct explanation and new contract**

Replace the current README contract sections with this content:

```markdown
# jackin

`jackin` is a Matrix-inspired CLI for orchestrating Claude Code agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

## Construct

`donbeave/jackin-construct:trixie` is the shared base image for every agent repo. In The Matrix, the construct is the base simulated environment you load before a mission. That maps directly to `jackin`'s shared runtime image: every agent starts from the same construct before layering on its own specialized environment.

## Commands

- `jackin load smith` — send an agent in.
- `jackin hardline agent-smith` — reattach to a running agent.
- `jackin eject agent-smith` — pull one agent out.
- `jackin eject smith --all` — pull every Smith out for one class scope.
- `jackin exile` — remove every running agent.
- `jackin purge smith --all` — delete persisted state for one class.

## Storage

- `~/.config/jackin/config.toml` — operator config.
- `~/.jackin/agents/...` — cached agent repositories.
- `~/.jackin/data/<container-name>/` — persisted `.claude`, `.claude.json`, and `plugins.json` for one agent instance.

## Agent Repo Contract

Each agent repo must contain:

- `jackin.agent.toml`
- a Dockerfile at the path declared by `jackin.agent.toml`

The final Dockerfile stage must literally be `FROM donbeave/jackin-construct:trixie`.

`jackin` validates that Dockerfile, generates the final Claude-ready derived image itself, and mounts the cached repo checkout into `/workspace` at runtime.
```

- [ ] **Step 2: Run the main verification commands after the doc update**

Run: `cargo nextest run`

Expected: PASS

Run: `cargo run -- --help`

Expected: PASS with the existing command list still visible.

- [ ] **Step 3: Commit the `jackin` docs update**

```bash
git add README.md
git commit -m "docs: explain construct image and derived agent contract"
```

### Task 6: Create, Publish, And Verify The First Smith Repo

**Files:**
- Create: `../smith/.gitignore`
- Create: `../smith/README.md`
- Create: `../smith/Dockerfile`
- Create: `../smith/jackin.agent.toml`

- [ ] **Step 1: Create the sibling repo files**

From `/Users/donbeave/Projects/donbeave/jackin`, run `mkdir -p ../smith`, then create `../smith/.gitignore`:

```gitignore
.DS_Store
```

Create `../smith/jackin.agent.toml`:

```toml
dockerfile = "Dockerfile"

[claude]
plugins = [
  "code-review@claude-plugins-official",
  "feature-dev@claude-plugins-official",
]
```

Create `../smith/Dockerfile`:

```dockerfile
FROM donbeave/jackin-construct:trixie

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

USER claude

ENV MISE_TRUSTED_CONFIG_PATHS=/workspace

RUN mise install node@lts && \
    mise use -g --pin node@lts
```

Create `../smith/README.md`:

```markdown
# smith

`smith` is the first public-friendly `jackin` agent repo.

It provides only the agent-specific environment layer for `jackin`, not the final Claude runtime. `jackin` validates this repo's Dockerfile, derives the final image itself, and mounts the cached repo checkout into `/workspace` when you run `jackin load smith`.

## Contract

- final Dockerfile stage must literally be `FROM donbeave/jackin-construct:trixie`
- plugins are declared in `jackin.agent.toml`
- the repo is expected to run cleanly without company-specific secrets, custom CA setup, or private mirrors

## Environment

For v1, `smith` intentionally stays minimal:

- shared shell/runtime tools come from `donbeave/jackin-construct:trixie`
- this repo preinstalls `node@lts`
- runtime workspace is the repo itself, mounted at `/workspace`
```

- [ ] **Step 2: Initialize the new local repo and commit its contents**

Run:

```bash
cd ../smith
git init -b main
git add .gitignore README.md Dockerfile jackin.agent.toml
git commit -m "feat: add initial smith jackin agent repo"
```

Expected: PASS with a new local Git repo at `/Users/donbeave/Projects/donbeave/smith`.

- [ ] **Step 3: Publish `smith` to GitHub**

Run from `../smith`:

```bash
gh repo create donbeave/smith --public --source . --remote origin --push
```

Expected: PASS with `origin` pointing to `git@github.com:donbeave/smith.git`.

- [ ] **Step 4: Verify the new end-to-end flow against the published repo**

Run from `../jackin`:

```bash
docker build -t donbeave/jackin-construct:trixie docker/construct
cargo nextest run
cargo run -- load smith
```

Expected: the repo clones from `git@github.com:donbeave/smith.git`, the derived image builds, and Claude starts inside the attached container.

Then detach with `Ctrl-P`, `Ctrl-Q` and run:

```bash
cargo run -- hardline agent-smith
cargo run -- eject smith --purge
```

Expected: `hardline` reattaches with `docker attach`, and `eject --purge` removes the runtime plus persisted state.

- [ ] **Step 5: Commit the final plan-state docs in `jackin` if verification required any doc touch-ups**

If verification changed only `smith`, commit there:

```bash
cd ../smith
git status --short
git add README.md Dockerfile jackin.agent.toml .gitignore
git commit -m "docs: polish initial smith agent repo"
git push origin main
```

If verification required no code or doc changes, skip this step and leave the repo clean.

# DinD Hostname Env Var Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose `JACKIN_DIND_HOSTNAME` as a jackin-managed runtime env var, reserve it from agent manifests, and update docs/TODO tracking to describe the runtime-owned env vars clearly.

**Architecture:** Keep the behavior local to the existing runtime launch path. `src/runtime.rs` already derives the DinD sidecar name for `DOCKER_HOST`, so reuse that value for a new runtime-owned env var and document both reserved runtime vars in the manifest docs. Manifest validation remains the gate that prevents agent repos from shadowing jackin-managed metadata.

**Tech Stack:** Rust, cargo-nextest, Vocs, MDX

---

### Task 1: Reserve `JACKIN_DIND_HOSTNAME` in manifest validation

**Files:**
- Modify: `src/manifest.rs`

- [ ] **Step 1: Write the failing test**

Add this test near the existing reserved-env validation test in `src/manifest.rs`:

```rust
    #[test]
    fn validate_rejects_reserved_dind_hostname_env_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN_DIND_HOSTNAME]
default = "sidecar"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("JACKIN_DIND_HOSTNAME")
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -E 'test(validate_rejects_reserved_dind_hostname_env_name)'`

Expected: FAIL because `AgentManifest::validate()` does not yet reject `JACKIN_DIND_HOSTNAME`.

- [ ] **Step 3: Write minimal implementation**

Replace the single reserved-name constants with a small reserved-runtime list in `src/manifest.rs`:

```rust
pub const JACKIN_RUNTIME_ENV_NAME: &str = "JACKIN_CLAUDE_ENV";
pub const JACKIN_RUNTIME_ENV_VALUE: &str = "jackin";
pub const JACKIN_DIND_HOSTNAME_ENV_NAME: &str = "JACKIN_DIND_HOSTNAME";

const RESERVED_RUNTIME_ENV_VARS: &[(&str, Option<&str>)] = &[
    (JACKIN_RUNTIME_ENV_NAME, Some(JACKIN_RUNTIME_ENV_VALUE)),
    (JACKIN_DIND_HOSTNAME_ENV_NAME, None),
];
```

Then update validation to reject any reserved runtime env var with an explicit message:

```rust
        for (name, decl) in &self.env {
            if let Some((_, value)) = RESERVED_RUNTIME_ENV_VARS
                .iter()
                .find(|(reserved, _)| name == reserved)
            {
                let detail = match value {
                    Some(value) => format!(" and set automatically to {value}"),
                    None => " and set automatically by jackin at runtime".to_string(),
                };
                anyhow::bail!(
                    "env var {name}: reserved for jackin runtime metadata{detail}"
                );
            }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -E 'test(validate_rejects_reserved_dind_hostname_env_name)'`

Expected: PASS.

- [ ] **Step 5: Run the related existing reserved-env test**

Run: `cargo nextest run -E 'test(validate_rejects_reserved_claude_env_name)'`

Expected: PASS, proving the existing reserved env behavior still works.

- [ ] **Step 6: Commit**

```bash
git add src/manifest.rs
git commit -m "feat: reserve dind hostname runtime env var"
```

### Task 2: Inject `JACKIN_DIND_HOSTNAME` into the runtime container

**Files:**
- Modify: `src/runtime.rs`
- Modify: `src/manifest.rs`

- [ ] **Step 1: Write the failing test**

Update the existing runtime env assertion test in `src/runtime.rs` so it checks both runtime metadata env vars:

```rust
    #[test]
    fn load_agent_sets_runtime_metadata_env_vars_by_default() {
        // existing setup stays the same

        let run_cmd = runner
            .recorded
            .iter()
            .find(|call| call.contains("docker run -it"))
            .unwrap();
        assert!(run_cmd.contains("-e JACKIN_CLAUDE_ENV=jackin"));
        assert!(run_cmd.contains("-e JACKIN_DIND_HOSTNAME=jackin-agent-smith-dind"));
        assert!(!run_cmd.contains("JACKIN_DEBUG"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -E 'test(load_agent_sets_runtime_metadata_env_vars_by_default)'`

Expected: FAIL because the runtime launch command does not yet inject `JACKIN_DIND_HOSTNAME`.

- [ ] **Step 3: Write minimal implementation**

In `src/runtime.rs`, add a sibling env string next to `docker_host` and include a short runtime-owned metadata comment:

```rust
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2375");
    let dind_hostname = format!("{}={dind}", crate::manifest::JACKIN_DIND_HOSTNAME_ENV_NAME);
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
```

Then add it to the `docker run` env flags:

```rust
    let mut run_args: Vec<&str> = vec![
        "run",
        "-it",
        "--name",
        container_name,
        "--hostname",
        container_name,
        "--network",
        network,
        "--label",
        "jackin.managed=true",
        "--label",
        &class_label,
        "--label",
        &display_label,
        "--workdir",
        &workspace.workdir,
        // JACKIN_* runtime metadata is injected by jackin, not declared in agent manifests.
        "-e",
        &docker_host,
        "-e",
        &dind_hostname,
        "-e",
        &git_author_name,
        "-e",
        &git_author_email,
    ];
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -E 'test(load_agent_sets_runtime_metadata_env_vars_by_default)'`

Expected: PASS.

- [ ] **Step 5: Run the adjacent debug test**

Run: `cargo nextest run -E 'test(load_agent_passes_debug_flag_when_enabled)'`

Expected: PASS, proving the runtime env ordering change did not break debug injection.

- [ ] **Step 6: Commit**

```bash
git add src/runtime.rs src/manifest.rs
git commit -m "feat: expose dind hostname to runtime containers"
```

### Task 3: Document reserved runtime env vars and resolve the TODO item

**Files:**
- Modify: `docs/pages/developing/agent-manifest.mdx`
- Modify: `docs/pages/reference/architecture.mdx`
- Modify: `docs/pages/reference/roadmap.mdx`
- Modify: `todo/dind-hostname-env-var.md`
- Modify: `TODO.md`

- [ ] **Step 1: Write the failing docs/tracking expectation**

Define the required doc updates before editing:

```text
- agent-manifest docs list both reserved runtime env vars and explain their meaning
- architecture docs mention the agent reaches DinD-backed services through JACKIN_DIND_HOSTNAME
- roadmap removes the planned DinD hostname bullet from infrastructure improvements
- todo item status becomes Resolved and TODO.md moves it to the Resolved section
```

- [ ] **Step 2: Update the manifest docs**

In `docs/pages/developing/agent-manifest.mdx`, replace the single reserved-var bullet and runtime section with:

```mdx
- `JACKIN_CLAUDE_ENV` is reserved for jackin and cannot be declared in `[env]`
- `JACKIN_DIND_HOSTNAME` is reserved for jackin and cannot be declared in `[env]`
- Circular dependencies are rejected
```

```mdx
### Runtime-managed variables

jackin sets these variables automatically inside the container. Do not declare either of them in `[env]`.

- `JACKIN_CLAUDE_ENV=jackin` marks that the process is running inside a jackin-managed runtime.
- `JACKIN_DIND_HOSTNAME=<container>-dind` is the hostname agents should use to reach services started inside the Docker-in-Docker sidecar.
```

- [ ] **Step 3: Update the architecture and tracking docs**

In `docs/pages/reference/architecture.mdx`, expand the networking bullet list to mention service discovery explicitly:

```mdx
- the agent reaches the daemon through `DOCKER_HOST=tcp://{dind}:2375`
- the agent reaches services launched inside DinD through `JACKIN_DIND_HOSTNAME={dind}`
```

In `todo/dind-hostname-env-var.md`, set `**Status**: Resolved`.

In `TODO.md`, move the DinD hostname item from `## Open Items` to `## Resolved`.

In `docs/pages/reference/roadmap.mdx`, remove this planned bullet:

```mdx
- **DinD hostname env var (`JACKIN_DIND_HOSTNAME`)** — expose the DinD sidecar's hostname so agents can connect to docker-compose services without parsing `DOCKER_HOST`
```

- [ ] **Step 4: Verify docs build**

Run: `bun run build`

Workdir: `docs`

Expected: successful Vocs production build with no MDX errors.

- [ ] **Step 5: Run full project verification**

Run: `cargo fmt -- --check && cargo clippy && cargo nextest run`

Expected: all checks pass with zero warnings and zero failures.

- [ ] **Step 6: Commit**

```bash
git add docs/pages/developing/agent-manifest.mdx docs/pages/reference/architecture.mdx docs/pages/reference/roadmap.mdx todo/dind-hostname-env-var.md TODO.md
git commit -m "docs: document runtime env vars and resolve dind todo"
```

# Security Tools Integration and Environment Variable Prefix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install tirith and shellfirm in the construct image with shell hooks and MCP server registration, and rename all jackin-defined env vars to use the `JACKIN_` prefix.

**Architecture:** Multi-stage Docker build compiles security tools from source in a `rust:trixie` builder stage, copies only the binaries into the construct. Shell hooks in `.zshrc` and MCP registration in `entrypoint.sh` both respect `JACKIN_DISABLE_*` env vars. Env var renames are a straightforward find-and-replace across Rust source and shell scripts.

**Tech Stack:** Rust (existing project), Docker (multi-stage build), shell (zsh/bash), GitHub Actions

---

### Task 1: Rename CLAUDE_ENV to JACKIN_CLAUDE_ENV

**Files:**
- Modify: `src/manifest.rs:5`
- Modify: `src/manifest.rs:741,751`
- Modify: `src/env_resolver.rs:207,214`
- Modify: `src/runtime.rs:1914`
- Modify: `src/derived_image.rs:208`

- [ ] **Step 1: Update the constant in manifest.rs**

In `src/manifest.rs`, change line 5:

```rust
pub const JACKIN_RUNTIME_ENV_NAME: &str = "JACKIN_CLAUDE_ENV";
```

- [ ] **Step 2: Update the manifest validation test**

In `src/manifest.rs`, the `validate_rejects_reserved_claude_env_name` test. Update the TOML env var name and the assertion:

```rust
    #[test]
    fn validate_rejects_reserved_claude_env_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN_CLAUDE_ENV]
default = "docker"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("JACKIN_CLAUDE_ENV"));
    }
```

- [ ] **Step 3: Update the env_resolver test**

In `src/env_resolver.rs`, the `resolves_static_vars_without_prompting` test. Replace both occurrences of `"CLAUDE_ENV"`:

```rust
    #[test]
    fn resolves_static_vars_without_prompting() {
        let mut decls = BTreeMap::new();
        decls.insert("JACKIN_CLAUDE_ENV".to_string(), static_var("docker"));
        let prompter = MockPrompter::new(vec![]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(
            resolved.vars,
            vec![("JACKIN_CLAUDE_ENV".to_string(), "docker".to_string())]
        );
    }
```

- [ ] **Step 4: Update the runtime test**

In `src/runtime.rs`, line 1914. Change the assertion:

```rust
        assert!(run_cmd.contains("-e JACKIN_CLAUDE_ENV=jackin"));
```

- [ ] **Step 5: Update the derived_image test**

In `src/derived_image.rs`, line 208. Change the assertion:

```rust
        assert!(!ENTRYPOINT_SH.contains("JACKIN_CLAUDE_ENV="));
```

- [ ] **Step 6: Run tests to verify**

Run: `cargo fmt -- --check && cargo clippy && cargo nextest run`
Expected: All tests pass with zero warnings.

- [ ] **Step 7: Commit**

```bash
git add src/manifest.rs src/env_resolver.rs src/runtime.rs src/derived_image.rs
git commit -m "refactor: rename CLAUDE_ENV to JACKIN_CLAUDE_ENV"
```

---

### Task 2: Rename CLAUDE_DEBUG to JACKIN_DEBUG

**Files:**
- Modify: `src/runtime.rs:421`
- Modify: `docker/runtime/entrypoint.sh:5,10`
- Modify: `docker/construct/install-plugins.sh:7`

- [ ] **Step 1: Update runtime.rs**

In `src/runtime.rs`, line 421. Change the env var string:

```rust
        run_args.extend_from_slice(&["-e", "JACKIN_DEBUG=1"]);
```

- [ ] **Step 2: Update entrypoint.sh**

In `docker/runtime/entrypoint.sh`, replace all three occurrences of `CLAUDE_DEBUG` with `JACKIN_DEBUG`:

Line 5:
```bash
if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
```

Line 10:
```bash
    if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
```

- [ ] **Step 3: Update install-plugins.sh**

In `docker/construct/install-plugins.sh`, line 7:

```bash
    if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo fmt -- --check && cargo clippy && cargo nextest run`
Expected: All tests pass. The `entrypoint_does_not_override_claude_env` test in `derived_image.rs` still passes because the entrypoint contains `JACKIN_DEBUG`, not `JACKIN_CLAUDE_ENV=`.

- [ ] **Step 5: Commit**

```bash
git add src/runtime.rs docker/runtime/entrypoint.sh docker/construct/install-plugins.sh
git commit -m "refactor: rename CLAUDE_DEBUG to JACKIN_DEBUG"
```

---

### Task 3: Add security tools builder stage to Dockerfile

**Files:**
- Create: `docker/construct/versions.env`
- Modify: `docker/construct/Dockerfile`

- [ ] **Step 1: Create versions.env**

Create `docker/construct/versions.env`:

```
TIRITH_VERSION=0.2.12
SHELLFIRM_VERSION=0.3.9
```

- [ ] **Step 2: Add builder stage to Dockerfile**

Insert the builder stage at the top of `docker/construct/Dockerfile`, before the existing `FROM debian:trixie` line:

```dockerfile
FROM rust:trixie AS security-tools

ARG TIRITH_VERSION
ARG SHELLFIRM_VERSION

RUN cargo install tirith --version "${TIRITH_VERSION}" --locked && \
    cargo install shellfirm --version "${SHELLFIRM_VERSION}" --locked

```

- [ ] **Step 3: Add COPY for binaries**

In `docker/construct/Dockerfile`, immediately after the `FROM debian:trixie` line and before the `SHELL` directive, add:

```dockerfile
COPY --from=security-tools /usr/local/cargo/bin/tirith /usr/local/bin/tirith
COPY --from=security-tools /usr/local/cargo/bin/shellfirm /usr/local/bin/shellfirm
```

- [ ] **Step 4: Commit**

```bash
git add docker/construct/versions.env docker/construct/Dockerfile
git commit -m "feat: add tirith and shellfirm via multi-stage Docker build"
```

---

### Task 4: Wire shell hooks in zshrc

**Files:**
- Modify: `docker/construct/zshrc`

- [ ] **Step 1: Add security tool shell hooks**

Replace the contents of `docker/construct/zshrc` with:

```zsh
export PATH="$HOME/.local/share/mise/shims:$HOME/.local/bin:$PATH"
eval "$(starship init zsh)"

# Security tools (disable with JACKIN_DISABLE_TIRITH=1 / JACKIN_DISABLE_SHELLFIRM=1)
[[ "${JACKIN_DISABLE_TIRITH:-0}" != "1" ]] && eval "$(tirith init --shell zsh)"
[[ "${JACKIN_DISABLE_SHELLFIRM:-0}" != "1" ]] && eval "$(shellfirm init --shell zsh)"
```

- [ ] **Step 2: Commit**

```bash
git add docker/construct/zshrc
git commit -m "feat: add tirith and shellfirm shell hooks with disable mechanism"
```

---

### Task 5: Register MCP servers in entrypoint

**Files:**
- Modify: `docker/runtime/entrypoint.sh`

- [ ] **Step 1: Add MCP registration**

In `docker/runtime/entrypoint.sh`, insert the following block after the `run_maybe_quiet /home/claude/install-plugins.sh` line (line 34) and before the pre-launch hook check:

```bash

# Register security tool MCP servers
if [[ "${JACKIN_DISABLE_TIRITH:-0}" != "1" ]]; then
    run_maybe_quiet claude mcp add tirith -- tirith mcp-server
fi
if [[ "${JACKIN_DISABLE_SHELLFIRM:-0}" != "1" ]]; then
    run_maybe_quiet claude mcp add shellfirm -- shellfirm mcp-server
fi
```

- [ ] **Step 2: Run Rust tests to verify entrypoint embed**

Run: `cargo nextest run -E 'test(entrypoint)'`
Expected: The `entrypoint_does_not_override_claude_env` test passes. The entrypoint contains `JACKIN_DISABLE_TIRITH` and `JACKIN_DISABLE_SHELLFIRM` but not `JACKIN_CLAUDE_ENV=`.

- [ ] **Step 3: Commit**

```bash
git add docker/runtime/entrypoint.sh
git commit -m "feat: register tirith and shellfirm MCP servers in entrypoint"
```

---

### Task 6: Update CI workflow

**Files:**
- Modify: `.github/workflows/construct.yml`

- [ ] **Step 1: Add version loading and build-args**

In `.github/workflows/construct.yml`, add a new step before the "Build and push" step:

```yaml
      - name: Load tool versions
        run: cat docker/construct/versions.env >> "$GITHUB_ENV"
```

Then add `build-args` to the existing "Build and push" step:

```yaml
      - name: Build and push
        uses: docker/build-push-action@v6
        with:
          context: docker/construct
          push: ${{ github.event_name != 'pull_request' }}
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
          build-args: |
            TIRITH_VERSION=${{ env.TIRITH_VERSION }}
            SHELLFIRM_VERSION=${{ env.SHELLFIRM_VERSION }}
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/construct.yml
git commit -m "ci: pass security tool versions as build-args to construct image"
```

---

### Task 7: Update README

**Files:**
- Modify: `docker/construct/README.md`

- [ ] **Step 1: Update the files table and description**

In `docker/construct/README.md`, update the `Dockerfile` description in the table to include security tools:

```markdown
| File | Purpose |
|---|---|
| `Dockerfile` | Builds the construct image on Debian Trixie with core tools (git, Docker CLI, mise, ripgrep, fd, fzf, GitHub CLI, zsh, starship) and security tools (tirith, shellfirm) |
| `zshrc` | Shell configuration — sets up mise shims, starship prompt, and security tool shell hooks |
| `install-plugins.sh` | Runtime script that installs Claude plugins from `~/.jackin/plugins.json` |
| `versions.env` | Pinned versions for security tools (tirith, shellfirm) used as Docker build-args |
```

- [ ] **Step 2: Commit**

```bash
git add docker/construct/README.md
git commit -m "docs: update construct README with security tools"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo fmt -- --check && cargo clippy && cargo nextest run`
Expected: All tests pass, zero warnings, zero formatting issues.

- [ ] **Step 2: Verify no stale references remain**

Run: `grep -r "CLAUDE_DEBUG" src/ docker/` and `grep -r '"CLAUDE_ENV"' src/`
Expected: No matches. All references have been renamed.

- [ ] **Step 3: Review the diff**

Run: `git diff main..HEAD --stat`
Expected: Changes in the expected files only — no unintended modifications.

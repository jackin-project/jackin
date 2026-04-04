# Pre-Launch Hooks and Runtime Environment Variables

**Date:** 2026-04-04
**Status:** Approved
**Motivation:** Unblock ChainArgos migration from custom Docker setup to jackin

## Problem

jackin has no mechanism for:
1. Running custom initialization scripts before Claude Code starts (e.g., ctx7 MCP setup, specstory login)
2. Declaring and resolving runtime environment variables that agents need (e.g., `CLAUDE_ENV`, `POSTGRESQL_DB_HOST`, API keys)

These are the two blocker items from the ChainArgos migration gap analysis.

## Scope

This design covers:
- Pre-launch hook support in agent manifests
- Runtime environment variable declarations with interactive prompting
- Manifest validation (strict parsing + cross-field rules)
- Env resolver module (topological sort, prompting, skip cascading)
- Documentation updates

This design does NOT cover:
- Build-time secrets (`--mount=type=secret`) — deferred
- Process wrappers (wrapping the Claude command) — deferred
- Operator-side env var overrides (workspace/global config) — deferred, captured in `todo/env-var-interpolation.md`
- 1Password / secret manager integration — deferred, existing TODO
- `${VAR}` interpolation in prompt text — deferred, existing TODO

## Design Approach: Split Model with Two-Phase Resolution

The agent manifest (`jackin.agent.toml`) declares *what* the agent needs. A new env resolver module resolves declarations into concrete values at launch time. The resolver is the extension point for future operator overrides, workspace config, and secret managers.

---

## 1. Agent Manifest Schema Changes

### Strict Parsing

All manifest structs use `#[serde(deny_unknown_fields)]` to reject unknown fields. Typos and unsupported fields produce hard errors at parse time, not silent misconfiguration.

### `[hooks]` Section

```toml
[hooks]
pre_launch = "hooks/pre-launch.sh"
```

Single optional field. The value is a relative path to a bash script in the agent repo.

**Validation rules:**
- Path must be relative (no absolute paths)
- Path must stay inside the repo (no `../` escapes)
- No symlinks
- File must exist in the agent repo
- File must not be empty
- Reuses existing `src/repo.rs` path validation logic

### `[env.<NAME>]` Section

Each entry declares one environment variable as a TOML table:

```toml
[env.CLAUDE_ENV]
default = "docker"

[env.PROJECT_TO_CLONE]
interactive = true
options = ["project1", "project2", "project3"]
prompt = "Select a project to clone"

[env.BRANCH_TO_CREATE]
interactive = true
skippable = true
depends_on = ["env.PROJECT_TO_CLONE"]
prompt = "Branch name to create:"
default = "main"
```

**Field reference:**

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `default` | `String` | No | None | Default value (used if no prompt or user accepts default) |
| `interactive` | `bool` | No | `false` | Whether to prompt the user at launch time |
| `skippable` | `bool` | No | `false` | Whether the user can skip this prompt |
| `prompt` | `String` | No | Var name | Text shown to the user when prompting |
| `options` | `Vec<String>` | No | `[]` | Options for select-style prompt |
| `depends_on` | `Vec<String>` | No | `[]` | Env vars that must be resolved first (prefixed with `env.`) |

**Validation rules (post-deserialization):**
- Non-interactive var without `default` is an error (no way to get a value)
- `options` without `interactive = true` is an error
- `skippable` without `interactive = true` is a warning (meaningless)
- `prompt` without `interactive = true` is a warning (ignored at runtime)
- `depends_on` entries must use the `env.` prefix (e.g., `"env.PROJECT_TO_CLONE"`)
- `depends_on` entries must reference vars declared in the same `[env]` section (after stripping the `env.` prefix)
- `depends_on` entries without the `env.` prefix are an error (future prefixes like `secret.` may be added)
- `depends_on` self-references are an error
- `depends_on` cycles are an error (detected via topological sort)

### Rust Structs

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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    pub pre_launch: Option<String>,
}

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

### Manifest Validation Method

```rust
impl AgentManifest {
    pub fn validate(&self, repo_dir: &Path) -> Result<Vec<ManifestWarning>> { ... }
}
```

Called after `AgentManifest::load()` in the existing `validate_agent_repo()` flow. Returns hard errors for invalid state, collects warnings for non-fatal issues.

---

## 2. Env Resolver Module (`src/env_resolver.rs`)

New module responsible for turning env var declarations into resolved values.

### Core API

```rust
pub struct ResolvedEnv {
    pub vars: Vec<(String, String)>,
}

pub fn resolve_env(
    declarations: &BTreeMap<String, EnvVarDecl>,
    prompter: &impl EnvPrompter,
) -> anyhow::Result<ResolvedEnv>
```

### Resolution Flow

1. **Validate dependencies** — check for dangling references, self-references, cycles
2. **Topological sort** — order vars so dependencies are resolved before dependents
3. **Resolve each var in order:**
   - Check if any `depends_on` entry was skipped — if so, skip this var (cascade)
   - If not interactive: use `default` value
   - If interactive: prompt the user via `EnvPrompter` trait
4. **Return** flat list of `(name, value)` pairs for resolved (non-skipped) vars

### Interactive Prompt Modes

| Declaration | Prompt Behavior |
|---|---|
| `interactive = true` | Free-text input, required |
| `interactive = true` + `default` | Free-text input, pre-filled with default |
| `interactive = true` + `options` | Select from list |
| `interactive = true` + `options` + `default` | Select from list, default pre-selected |

Any mode can add `skippable = true` to allow the user to skip.

### Skip Cascade Rules

| Upstream | Downstream | User skips upstream | Behavior |
|---|---|---|---|
| skippable | skippable | yes | downstream skipped |
| skippable | non-skippable | yes | downstream skipped (cascade) |
| skippable | non-skippable | no | downstream prompted, must answer |
| non-skippable | skippable | (can't skip) | downstream prompted, can skip |
| non-skippable | non-skippable | (can't skip) | downstream prompted, must answer |

The `skippable` flag controls whether the user can skip *this specific prompt*. The `depends_on` cascade overrides everything: if an upstream var was skipped, all dependents are skipped regardless of their own `skippable` flag.

### Prompter Trait (for testability)

```rust
pub trait EnvPrompter {
    fn prompt_text(&self, title: &str, default: Option<&str>, skippable: bool) -> PromptResult;
    fn prompt_select(&self, title: &str, options: &[String], default: Option<&str>, skippable: bool) -> PromptResult;
}

pub enum PromptResult {
    Value(String),
    Skipped,
}
```

Production implementation uses a terminal prompter (e.g., `dialoguer` crate). Tests use a mock implementation with canned answers.

---

## 3. Pre-Launch Hook Integration

### Build Time

When the manifest declares `[hooks] pre_launch`, the derived Dockerfile gains:

```dockerfile
USER root
COPY hooks/pre-launch.sh /home/claude/.jackin-runtime/pre-launch.sh
RUN chmod +x /home/claude/.jackin-runtime/pre-launch.sh
USER claude
```

The source path matches the manifest field value. The destination is always `/home/claude/.jackin-runtime/pre-launch.sh` — a stable path for the entrypoint to reference.

Changes in `src/derived_image.rs`:
- `render_derived_dockerfile()` accepts an optional `pre_launch_path` parameter
- When present, adds the `COPY` and `chmod` instructions before the `ENTRYPOINT` line
- The script is already in the build context (part of the agent repo copied by `copy_dir_all`)
- The `COPY` source path must be preserved in `.dockerignore` — `ensure_runtime_assets_are_included()` needs to add a negation rule for the hook path (e.g., `!hooks/pre-launch.sh`) in case the agent's `.dockerignore` excludes it

### Runtime

The entrypoint (`docker/runtime/entrypoint.sh`) gains a new section:

```bash
# Run pre-launch hook if present
if [ -x /home/claude/.jackin-runtime/pre-launch.sh ]; then
    /home/claude/.jackin-runtime/pre-launch.sh
fi
```

This runs between plugin installation and Claude Code launch. The script has access to all resolved env vars (injected via `docker run -e`).

### Entrypoint Execution Order

1. Git identity setup (existing)
2. GitHub auth (existing)
3. Plugin installation (existing)
4. **Pre-launch hook** (new)
5. Clear screen (existing)
6. `exec claude ...` (existing)

If the pre-launch hook exits non-zero, the container stops and the user sees the error output.

---

## 4. Runtime Integration

### Updated Launch Flow

```
clone/pull repo
    -> validate repo (+ strict manifest validation)
    -> resolve env vars (interactive prompts)
    -> build image (includes pre-launch hook in derived layer)
    -> launch container (pass resolved env vars as -e flags)
```

### Env Resolution Timing

Interactive prompts happen **before** the Docker build:
- The user isn't waiting through a multi-minute build before being asked questions
- If the user cancels during prompts, no wasted build
- Resolved env vars are passed at `docker run` time, not build time

### Changes to `src/runtime.rs`

**`load_agent()`:**
- After `validate_agent_repo()`, call `manifest.validate(repo_dir)`
- Call `resolve_env(&manifest.env, &terminal_prompter)` before `build_agent_image()`
- Thread `ResolvedEnv` through to `launch_agent_runtime()`

**`build_agent_image()`:**
- Pass `manifest.hooks.pre_launch` to `create_derived_build_context()` so the derived Dockerfile includes the hook

**`launch_agent_runtime()`:**
- `LaunchContext` gains `resolved_env: &ResolvedEnv`
- Resolved vars added as `-e` flags alongside existing env vars (DOCKER_HOST, GIT_AUTHOR_*, etc.)

### Error Handling

| Scenario | Behavior |
|---|---|
| User cancels during interactive prompt | Abort load, no build |
| Required interactive var gets empty input | Re-prompt |
| Pre-launch hook exits non-zero | Container stops, error shown |
| Dependency cycle in env vars | Hard error at validation, before any prompts |

---

## 5. Documentation Updates

### `developing/agent-manifest.mdx`

- Add `[hooks]` section with field reference and validation rules
- Add `[env.<NAME>]` section with full field table
- Add examples for each interactive mode (static, free-text, select, dependency chain, skippable)
- Update the "Full schema" example to include hooks and env
- Update "Minimal example" to note that hooks and env are optional
- Document strict parsing behavior (`deny_unknown_fields`)

### `reference/architecture.mdx`

- Update "Loading an agent" lifecycle to include env resolution step (between validate and build)
- Update "Image layers" diagram to mention pre-launch hooks in the derived layer
- Note that interactive prompts happen before build

### `commands/load.mdx`

- Document that `jackin load` may prompt for env vars if the agent manifest declares interactive vars

---

## 6. Files to Create or Modify

### New files
- `src/env_resolver.rs` — env var resolution logic, topological sort, prompter trait

### Modified files
- `src/manifest.rs` — new structs (`HooksConfig`, `EnvVarDecl`), `deny_unknown_fields` on all structs, `validate()` method
- `src/derived_image.rs` — conditional pre-launch hook in derived Dockerfile, `render_derived_dockerfile()` signature change
- `src/runtime.rs` — env resolution call, thread `ResolvedEnv` to launch, pass resolved vars as `-e` flags
- `src/repo.rs` — pre-launch hook path validation (reuse existing logic)
- `docker/runtime/entrypoint.sh` — add pre-launch hook execution step
- `Cargo.toml` — add `dialoguer` dependency for interactive prompts
- `src/main.rs` or `src/lib.rs` — register `env_resolver` module
- `docs/src/content/docs/developing/agent-manifest.mdx` — full schema update
- `docs/src/content/docs/reference/architecture.mdx` — lifecycle and layer diagram updates
- `docs/src/content/docs/commands/load.mdx` — interactive prompt notes

---

## Full Manifest Example

```toml
dockerfile = "Dockerfile"

[identity]
name = "The Architect"

[claude]
plugins = [
  "code-review@claude-plugins-official",
  "feature-dev@claude-plugins-official"
]

[hooks]
pre_launch = "hooks/pre-launch.sh"

[env.CLAUDE_ENV]
default = "docker"

[env.CONTEXT7_API_KEY]
interactive = true
prompt = "Context7 API key:"
skippable = true

[env.PROJECT_TO_CLONE]
interactive = true
options = ["project1", "project2", "project3"]
prompt = "Select a project to clone"

[env.BRANCH_TO_CREATE]
interactive = true
depends_on = ["env.PROJECT_TO_CLONE"]
prompt = "Branch name to create:"
default = "main"
```

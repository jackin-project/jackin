# Workspace Env Resolver — Layered Env Vars with Literal / Host / 1Password Sources

**Status:** Proposed
**Date:** 2026-04-23
**Scope:** `jackin` crate only
**PR:** 2 of 3 in the Claude auth strategy series

## Problem

Operators today cannot declare environment variables in jackin config that apply per-workspace, per-agent, or per workspace×agent combination. The only existing env model is manifest-declared (from `jackin.role.toml`) with the agent's own defaults; the operator cannot override it or inject new values without shell-level tricks. There is also no first-class way to resolve secrets at launch: the closest today is manually `export`ing a value before each `jackin launch`.

This is acutely painful for secrets. Operators want to say:

- "My Claude OAuth token lives in 1Password at `op://Personal/Claude Code Token/credential` — use it whenever an agent launches."
- "My `DATABASE_URL` for the `my-project` workspace is a literal; I want it injected automatically."
- "`GITHUB_TOKEN` should pass through from whatever my host shell has."
- "When `chainargos/agent-jones` runs in `my-project`, use a project-specific OpenAI key from 1Password."

This PR introduces the operator-side env system that answers all of those. PR 3 (`auth_forward = "token"`) is a thin consumer: it asserts that `CLAUDE_CODE_OAUTH_TOKEN` is present in the resolved env, nothing more.

## Goals

1. Let operators declare env vars at four precedence layers: global, per-agent-class, per-workspace, per-workspace×agent.
2. Support three value sources per env var, dispatched by a single string schema:
   - Literal value (default)
   - Host env var pass-through (`$NAME` or `${NAME}`)
   - 1Password CLI reference (`op://...`)
3. Resolve all references at launch time and inject the resulting map into the container via `docker run -e`.
4. Fail loudly with actionable messages when resolution cannot succeed.
5. Compose cleanly with the existing manifest-declared env in `src/env_resolver.rs` and `src/env_model.rs` without breaking reserved-name or cycle-detection guarantees.

## Non-Goals

- `jackin config env` CLI helpers (set/show/list). Operators edit TOML in v1. A CLI can come later if the TOML-only path proves awkward.
- Non-1Password secret backends (`pass://`, `keychain://`, `vault://`, AWS Secrets Manager, etc.). The scheme-dispatched design is extensible, but v1 ships exactly two non-literal sources: `$VAR` and `op://`.
- Caching resolved secrets across launches. Each launch re-resolves; 1Password's own biometric session handling decides whether Touch ID prompts fire repeatedly.
- Encrypted at-rest storage in jackin's config. Operators use 1Password (or another manager) for at-rest protection; jackin stores only references.
- Interactive prompts from jackin itself. All prompts (Touch ID, Keychain unlock) come from `op` or the OS, not from jackin.
- Promoting `auth_forward = "token"` to default. PR 3 lands the mode; default-flipping is a future decision.

## Design

### Config schema

Four new `BTreeMap<String, String>` fields, one per precedence layer:

```toml
# Layer 1 — Global: baseline for every agent in every workspace
[env]
CLAUDE_CODE_OAUTH_TOKEN = "op://Personal/Claude Code Token/credential"
GITHUB_TOKEN = "$GITHUB_TOKEN"

# Layer 2 — Per-agent-class: applies whenever this agent is launched, any workspace
[roles.agent-smith.env]
OPENAI_API_KEY = "op://Work/OpenAI/default"

# Agent names with slashes use standard TOML quoted keys
[roles."chainargos/agent-jones".env]
DATABASE_URL = "op://Work/agent-jones/db"

# Layer 3 — Per-workspace: applies to every agent in this workspace
[workspaces.my-project.env]
NODE_ENV = "development"

# Layer 4 — Per-workspace × per-agent: most specific; wins on conflict
[workspaces.my-project.agents."chainargos/agent-jones".env]
OPENAI_API_KEY = "op://Work/my-project/OpenAI"
DATABASE_URL = "op://Work/my-project-jones/db"
```

Schema additions to the Rust types (`src/config/mod.rs`, `src/config/agents.rs`, `src/config/workspaces.rs`):

```rust
pub struct AppConfig {
    // existing fields ...
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

pub struct AgentSource {
    // existing fields ...
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

pub struct WorkspaceConfig {
    // existing fields ...
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub agents: BTreeMap<String, WorkspaceAgentOverride>,
}

pub struct WorkspaceAgentOverride {
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}
```

All four default to empty; operators opting in write only the layers they need.

### Value syntax and scheme dispatch

One string field per env var. The dispatcher inspects the value once at resolve time:

| Value prefix       | Source      | Action                                                               |
| ------------------ | ----------- | -------------------------------------------------------------------- |
| `op://`            | 1Password   | `op read '<value>'` — capture stdout, trim trailing newline          |
| `$NAME` / `${NAME}`| Host env    | `std::env::var("NAME")`                                              |
| *anything else*    | Literal     | Use as-is                                                            |

Dispatch is unambiguous: env var names cannot contain `://`, and a literal starting with `$` is vanishingly rare for real-world env values (if an operator really needs a literal `$foo`, they can escape with `\$foo` or we can reserve `literal:` as a scheme — but v1 does not over-engineer this).

### Resolver

New module `src/env_resolver/workspace_env.rs` (or a function within the existing `src/env_resolver.rs`, depending on which reads cleaner after the split):

```rust
pub struct ResolvedOperatorEnv {
    pub vars: BTreeMap<String, String>,
}

pub fn resolve_operator_env(
    config: &AppConfig,
    workspace: &ResolvedWorkspace,
    agent: &ClassSelector,
) -> Result<ResolvedOperatorEnv>;
```

Resolution order (later layer's key wins on conflict):

1. `config.env`                                  (global)
2. `config.agents[agent].env`                    (role)
3. `config.workspaces[workspace].env`            (workspace)
4. `config.workspaces[workspace].agents[agent].env` (workspace × agent)

Merge pass: walk layer 1→4, inserting into a single `BTreeMap<String, String>` and allowing later layers to overwrite earlier ones. The result is a raw (unresolved) map of `{name → source_string}`.

Resolution pass: iterate the merged map and dispatch each value through the scheme rules above. Collect errors (do not stop on the first failure) so the operator sees all resolution problems in one launch attempt. Return a single aggregate error with all failures if any occurred.

### Interaction with manifest env

`src/env_model.rs` and `src/env_resolver.rs` already resolve the manifest-declared env for an agent. After that resolution completes, overlay the operator-env map on top:

- If an operator env key matches a manifest key: **operator wins**. The agent manifest is sourced from the role repo (potentially third-party); the operator's config is trusted.
- If an operator env key matches a reserved name (e.g. `JACKIN_RUNTIME`, `DOCKER_HOST`, anything in `src/env_model.rs`'s reserved list): **hard error at config load**, not at launch. Configuration is rejected with a message naming the offending key and layer. Reserved-name protection must remain airtight; this is the load-time failure class that keeps operators from accidentally breaking runtime contracts.

Cycle detection: operator env values are strings with at most one scheme prefix. No interpolation, no variable expansion within values, no `${FOO}` substitution that references other operator keys. The resolver is strictly one-pass per value, so there is no cycle to detect. If we later add nested interpolation, the existing cycle-detection infrastructure in `src/env_model.rs` extends naturally.

### Launch-time injection

`src/runtime/launch.rs` currently builds `env_strings` (around lines 447–462). After resolving the manifest env, call `resolve_operator_env(...)`, overlay it on the manifest map, and emit each `KEY=VALUE` via `-e` to `docker run` exactly as today. No new injection path — this feature reuses the existing `docker run -e` loop.

Order of `-e` flags does not matter for Docker; the resolved map is a logical overlay, not a chain.

### Launch-time messaging

When operator env resolves to any keys, jackin prints a single diagnostic line before the container starts:

```
jackin: operator env resolved — 3 vars (2 from op://, 1 from host)
```

Counts only, never values. For debug mode (`--debug`), the message expands to list names only (still no values):

```
jackin: operator env resolved:
  CLAUDE_CODE_OAUTH_TOKEN ← op://Personal/Claude Code Token/credential  (workspace×agent)
  GITHUB_TOKEN            ← $GITHUB_TOKEN                               (global)
  DATABASE_URL            ← literal                                     (workspace)
```

Values are never logged under any mode. This is important for the 1Password case because the reference is config-level and fine to show, but the resolved plaintext is sensitive.

### `op` CLI integration

The resolver invokes `op read '<reference>'` as a subprocess. Rules:

- `op` must be on `$PATH` when any `op://` reference is in scope. Check presence once per launch by shelling `op --version`.
- If `op` is missing → hard error with install link:
  `error: env var 'CLAUDE_CODE_OAUTH_TOKEN' uses op:// source but 1Password CLI ('op') was not found on PATH — install from https://developer.1password.com/docs/cli/`
- If `op read` exits non-zero → hard error with the `op` stderr included verbatim:
  `error: failed to resolve env var 'CLAUDE_CODE_OAUTH_TOKEN' from 'op://Personal/...' — op exited 1: [no item in vault Personal matches ...]`
- If `op` hangs → resolver runs with a configurable timeout (default 30s, long enough for Touch ID + optional system auth). Timeout surfaces as: `error: 'op read' timed out after 30s — ensure the 1Password desktop app is unlocked`.

`op` stdout is captured, the trailing newline trimmed, and that is the resolved value. Nothing is logged to disk; the value lives only in memory long enough to be passed to `docker run`.

### Host env pass-through

`$NAME` or `${NAME}` → `std::env::var("NAME")`.

- Empty string result counts as "set but empty" and is passed through unchanged (same as Unix semantics).
- Unset host var → hard error:
  `error: env var 'GITHUB_TOKEN' references $GITHUB_TOKEN but it is not set in the host environment`
- `$FOO$BAR` or other compound forms are **not supported in v1**. The whole string must be a single reference. If operators need composition, they do it in the literal value or in the upstream env var.

### Reserved names

The existing reserved-name list in `src/env_model.rs` (includes at least `JACKIN_RUNTIME`) is extended (or consulted, depending on its current shape) to reject any operator attempt to declare env vars by those names. Rejection fires at config load, with the error identifying the layer:

```
error: [workspaces.my-project.env] key 'JACKIN_RUNTIME' is reserved and cannot be overridden
```

Some env vars Claude Code documents (e.g. `CLAUDE_CODE_USE_BEDROCK`, `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`) are **not** reserved by jackin — operators are expected to set them. Reserved names are only those that would break jackin's own runtime contract.

## Failure Modes

| Condition                                          | Behavior                                                  |
| -------------------------------------------------- | --------------------------------------------------------- |
| Operator uses reserved name                        | Config load error (not launch)                            |
| `op://` used but `op` not installed                | Launch error, install link                                |
| `op read` exits non-zero                           | Launch error, op's stderr included                        |
| `op read` times out                                | Launch error after 30s default                            |
| `$NAME` used but host env unset                    | Launch error, names the unset var                         |
| Two layers declare conflicting values for one key  | Higher layer wins silently (by design; not a failure)     |
| Multiple resolution errors in one launch           | All reported in one aggregated error                      |
| Config file missing a referenced `[workspaces.foo]`| Existing workspace-resolution error; unchanged            |
| Same agent appears at both layer 2 and layer 4     | By design; layer 4 overrides per key                      |

Resolution failures prevent launch. jackin does not start a container with partial env.

## Security Notes

- Resolved values live only in process memory during `docker run` argument assembly. They are never written to disk (`~/.jackin/data/...` stays untouched by this feature).
- The config file stores references, not secrets. Config is safe to commit to a private repo with 1Password references; committing host-env-var references is also safe since the name is not the secret.
- The debug-mode expanded message intentionally shows references and not values. Never widen this.
- `op read` inherits the current process's stdin/stdout/stderr. If an operator runs jackin in a pipeline, `op`'s Touch ID prompt may surface through stderr — document this in the guide but not as a jackin bug.
- Subprocess invocation uses direct `Command::new("op")` with argument vectors (no shell interpolation). `op://` reference is passed as a single argument, not expanded through a shell. No injection surface.
- Stderr capture from `op` is bounded (first N KB) to prevent a misbehaving `op` from blowing up jackin's memory. Error messages are truncated with an ellipsis if they exceed the bound.
- `op --version` probe uses the same bounded approach.

## Documentation Changes

- `docs/src/content/docs/guides/environment-variables.mdx` (new): canonical explanation of the four layers, the three sources, precedence, failure modes, and worked examples including the 1Password flow.
- `docs/src/content/docs/reference/configuration.mdx`: add the new `[env]`, `[roles.*.env]`, `[workspaces.*.env]`, `[workspaces.*.agents.*.env]` sections to the schema reference.
- `docs/src/content/docs/guides/authentication.mdx`: cross-link to the env guide for the PR 3 token flow.
- `docs/src/content/docs/reference/roadmap/onepassword-integration.mdx`: update status — option #2 (workspace-managed secret references) is delivered by this PR for env vars; files/mounts are still deferred.
- `CHANGELOG.md`: `Added` entry under Unreleased.

## Test Plan

### Unit tests

- Scheme dispatch: literal, `$VAR`, `${VAR}`, `op://...` each route to the right resolver.
- Merge precedence: layer-4 beats layer-3 beats layer-2 beats layer-1 for conflicting keys.
- Non-conflicting keys from all four layers survive the merge.
- Reserved-name override is rejected at config load, with error naming the layer.
- Manifest + operator env overlay: operator wins on conflict.
- Empty-string host env var passes through.
- Unset host env var produces the expected error message.
- Aggregated error reporting: two failing keys produce one error listing both.

### Integration tests

- `op` binary mocked via a `JACKIN_TEST_OP_BIN` env var (or similar test seam) that points to a controllable stub. Stub exits 0 with controlled stdout for happy path, non-zero with controlled stderr for failure paths, sleeps-then-exits for timeout path.
- Full launch test: agent + workspace with env declared at all four layers, launched under a fake-docker runner, verifies the final `-e` flags match expectations.
- Launch-time diagnostic line is printed exactly once and contains only counts in normal mode.
- Debug-mode diagnostic includes references but no values.

### Pre-commit

`cargo fmt -- --check && cargo clippy && cargo nextest run` is clean.

## File-Level Change Map

| File                                                                     | Change                                                                                 |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| `src/config/mod.rs`                                                      | add `env` field to `AppConfig`                                                         |
| `src/config/agents.rs`                                                   | add `env` field to `AgentSource`                                                       |
| `src/config/workspaces.rs`                                               | add `env` and `agents` (of type `WorkspaceAgentOverride`) fields to `WorkspaceConfig`; new `WorkspaceAgentOverride` type |
| `src/env_resolver/mod.rs` or new `src/env_resolver/operator.rs`          | `resolve_operator_env` + scheme dispatch + `op` integration + host-env pass-through    |
| `src/env_model.rs`                                                       | extend reserved-name check to cover the new layers at config load                      |
| `src/runtime/launch.rs`                                                  | call `resolve_operator_env`, overlay on manifest env, emit launch-diagnostic line      |
| `src/tui/output.rs`                                                      | optional: helper for the "operator env resolved" diagnostic line                       |
| `docs/src/content/docs/guides/environment-variables.mdx` (new)           | full guide                                                                             |
| `docs/src/content/docs/reference/configuration.mdx`                      | schema reference update                                                                |
| `docs/src/content/docs/guides/authentication.mdx`                        | cross-link for PR 3                                                                    |
| `docs/src/content/docs/reference/roadmap/onepassword-integration.mdx`    | status update                                                                          |
| `CHANGELOG.md`                                                           | `Added` entry                                                                          |

## Open Questions

None. Scheme dispatch, layer shape, `op` failure handling, precedence, and reserved-name policy were agreed in brainstorming.

## Related

- Roadmap: `docs/src/content/docs/reference/roadmap/onepassword-integration.mdx` — option #2 (workspace-managed secret references) is delivered for env vars by this PR.
- PR 1 (`2026-04-23-auth-sync-default-design.md`) — independent; no dependency either direction.
- PR 3 (`2026-04-23-claude-token-auth-mode-design.md`) — depends on this PR's resolver landing first.

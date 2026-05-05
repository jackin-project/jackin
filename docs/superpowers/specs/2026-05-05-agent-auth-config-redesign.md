# Agent Auth Config — Layered Modes With TUI

**Status:** Proposed
**Date:** 2026-05-05
**Scope:** `jackin` crate; new TUI panel; `AGENTS.md` ground-rule note (already landed alongside this spec).

## Problem

Today the `auth_forward` mode (`sync` / `ignore` / `token`) is Claude-only and lives in two places: a global `[claude]` block and a per-role `[roles.<role>.claude]` override. Codex has no equivalent toggle — its host `auth.json` is always copied when present. API keys are not a first-class concept; they piggyback on the four-layer `env` map, which means there is no enforced relationship between "I want to auth Claude with an API key" and "`ANTHROPIC_API_KEY` is actually set somewhere reachable". `token` mode exists for Claude but is fragile: it depends on `CLAUDE_CODE_OAUTH_TOKEN` being set in some env layer, with no way to surface "did the user actually configure this?" before launch.

The operator wants two changes:

1. Move auth-mode configuration from per-role-global to per-workspace and per-`(workspace × role × agent)`. The per-role-global slot is removed entirely.
2. Treat the credential value as something the TUI manages, with 1Password as a first-class source, and verify at edit time that the credential resolves before allowing the config to be saved.

This spec covers both, generalized to Claude and Codex, with a path open for future agents.

## Goals

1. Move `auth_forward` out of `[roles.<role>.claude]`. That slot is gone — TOML containing it fails to parse.
2. Keep `[claude].auth_forward` as the global default. Add `[codex].auth_forward` as the global default for Codex.
3. Add a workspace layer: `[workspaces.<ws>.<agent>].auth_forward`.
4. Add a most-specific layer: `[workspaces.<ws>.roles.<role>.<agent>].auth_forward`.
5. Modes: `sync` (default), `api_key`, `oauth_token` (Claude only), `ignore`. Codex parser rejects `oauth_token`.
6. Reuse the existing four-layer `env` map for the credential value. The TUI hides the well-known env-var name (`ANTHROPIC_API_KEY` / `CLAUDE_CODE_OAUTH_TOKEN` / `OPENAI_API_KEY`); operators editing TOML by hand still set ordinary env entries.
7. Add an Auth panel to the workspace-manager TUI, peer to the existing Secrets row. Form-level validation prevents saving a mode that requires a credential without a resolved value.
8. At launch, fail-fast with a structured, multi-line error if a mode's required credential is missing after the env merge.

## Non-Goals

- No active provider probe. The form does not call `api.anthropic.com` or `api.openai.com`. The badge "✅ resolves" means "the reference resolves to non-empty content via `op read` or `$VAR` expansion", not "the provider accepted the credential". An on-demand "Verify now" button is a deferred follow-up.
- No backward compatibility. Per `AGENTS.md` ("Project status: pre-release"), no migration code, no compatibility shims, no fallback parsers, no deprecation warnings, no docstrings memorializing old shapes. Stale configs hit the standard `serde` "unknown field" error.
- No bulk operations in the TUI ("set all roles to api_key", "copy from another workspace"). Per-row only.
- No editing of globals from inside a workspace. The Auth panel renders globals read-only with an affordance to jump to the global-config screen.
- No Codex `oauth_token` mode. Codex CLI uses a refresh-token flow that does not match the static-token shape; a future Codex-specific mode would be added separately.
- No cross-layer conflict detection. Most-specific wins, full stop.

## Design

### 1. Architecture and data model

#### TOML surface

```toml
# Global default (lowest precedence)
[claude]
auth_forward = "sync"

[codex]
auth_forward = "sync"

# Workspace-level override
[workspaces.proj.claude]
auth_forward = "api_key"

[workspaces.proj.codex]
auth_forward = "sync"

# Most-specific override: workspace × role × agent
[workspaces.proj.roles.smith.claude]
auth_forward = "api_key"

[workspaces.proj.roles.smith.codex]
auth_forward = "ignore"

# Credentials live in env, unchanged
[workspaces.proj.roles.smith.env]
ANTHROPIC_API_KEY = { op = "op://Work/Claude/api-key", path = "Work/Claude/api-key" }
```

Removed entirely:

- `[roles.<role>.claude]` block (any layer of it).
- `ClaudeRoleConfig` struct.
- `RoleSource.claude` field.

Untouched:

- The existing four-layer `env` merge (`[env]` → `[roles.<r>.env]` → `[workspaces.<ws>.env]` → `[workspaces.<ws>.roles.<r>.env]`).
- The `EnvValue::Plain` / `EnvValue::OpRef { op, path }` shape.
- The `op_picker` widget.

#### Mode resolution

Three layers, most-specific wins:

```
fn resolve_mode(cfg: &AppConfig, agent: Agent, ws: &str, role: &str) -> AuthForwardMode
    1. cfg.workspaces[ws].roles[role].<agent>.auth_forward
    2. cfg.workspaces[ws].<agent>.auth_forward
    3. cfg.<agent>.auth_forward
    default: AuthForwardMode::Sync
```

This replaces the existing `resolve_auth_forward_mode` in `src/config/roles.rs`. The function takes an `agent` argument; the same body resolves Claude or Codex.

#### Credential resolution

The `(agent, mode) → required env-var name` table is the single source of truth:

| Agent  | Mode          | Required env var            |
|--------|---------------|-----------------------------|
| Claude | `sync`        | none                        |
| Claude | `api_key`     | `ANTHROPIC_API_KEY`         |
| Claude | `oauth_token` | `CLAUDE_CODE_OAUTH_TOKEN`   |
| Claude | `ignore`      | none                        |
| Codex  | `sync`        | none                        |
| Codex  | `api_key`     | `OPENAI_API_KEY`            |
| Codex  | `ignore`      | none                        |

The credential value is whatever the existing four-layer env merge produces under that name. No new resolver, no new layering.

#### Per-mode launcher behavior

| Mode          | Container provisioning                                             | Env injection                                          |
|---------------|--------------------------------------------------------------------|--------------------------------------------------------|
| `sync`        | Copy host `~/.claude/.credentials.json` (or macOS Keychain) and `~/.codex/auth.json` into per-container state, as today. | None enforced.                                         |
| `api_key`     | Wipe target host-state files in container state dir.               | `required_env_var(...)` must resolve to non-empty.     |
| `oauth_token` | Wipe Claude state.                                                 | `CLAUDE_CODE_OAUTH_TOKEN` must resolve to non-empty.   |
| `ignore`      | Wipe state.                                                        | None enforced.                                         |

The wipe paths for Claude already exist in `provision_claude_auth` (currently triggered by `Token` and `Ignore`). We generalize the dispatch and add a parallel wipe path for Codex in `provision_codex_auth`.

### 2. Components

#### Modified files

| File | Change |
|------|--------|
| `src/config/mod.rs` | Add `AgentAuthConfig { auth_forward: AuthForwardMode }`. Replace existing `claude: ClaudeConfig` field on `AppConfig` with `claude: Option<AgentAuthConfig>` and add `codex: Option<AgentAuthConfig>`. Delete `ClaudeRoleConfig`. Delete `RoleSource.claude`. |
| `src/workspace/mod.rs` | Add `claude: Option<AgentAuthConfig>` and `codex: Option<AgentAuthConfig>` to `WorkspaceConfig` and to `WorkspaceRoleOverride`. |
| `src/config/roles.rs` | Replace `resolve_auth_forward_mode` with `resolve_mode(cfg, agent, ws, role)` per the algorithm above. |
| `src/config/fixtures/config.round_trip.toml` | Drop `[roles.<r>.claude]`. Add an example for each new layer of each agent. |
| `src/agent/mod.rs` | Add `Agent::required_env_var(self, mode) -> Option<&'static str>`. Add `Agent::supported_modes(self) -> &'static [AuthForwardMode]`. Both methods are exhaustive matches; adding a new agent without extending them is a compile error. |
| `src/instance/auth.rs` | Generalize `provision_claude_auth` to dispatch on the mode resolved for *Claude in this scope*. Generalize `provision_codex_auth` to add wipe paths for `ApiKey` and `Ignore`. |
| `src/runtime/launch.rs` | Generalize `verify_token_env_present` into `verify_credential_env_present(agent, mode, merged_env, layers, scope)`. Update `load_role_with` to call the new resolver per agent. |

#### New files

| File | Purpose |
|------|---------|
| `src/console/widgets/auth_panel/mod.rs` | List of role-agent rows in the current workspace, with effective mode, provenance tag, and credential status badge. |
| `src/console/widgets/auth_panel/form.rs` | Edit form: mode picker, conditional credential block, op-picker invocation, save-disabled invariant. Split from `mod.rs` so the form can be remounted from the launch screen later. |

The workspace-manager TUI mounts the new panel as a peer of the existing Secrets row.

#### Untouched

- `src/operator_env.rs` — env merge, OpRef resolution.
- `src/console/widgets/op_picker/` — picker called as-is by the new form.
- `src/paths.rs`, the data-dir layout, `agent_mounts()` bind-mount construction.

#### Type shapes

```rust
// src/config/mod.rs
pub struct AgentAuthConfig {
    pub auth_forward: AuthForwardMode,
}

pub enum AuthForwardMode {
    Sync,
    ApiKey,
    OAuthToken,  // Claude only; codex parser rejects this
    Ignore,
}

// src/agent/mod.rs
impl Agent {
    pub fn required_env_var(self, mode: AuthForwardMode) -> Option<&'static str> {
        match (self, mode) {
            (Agent::Claude, AuthForwardMode::ApiKey)     => Some("ANTHROPIC_API_KEY"),
            (Agent::Claude, AuthForwardMode::OAuthToken) => Some("CLAUDE_CODE_OAUTH_TOKEN"),
            (Agent::Codex,  AuthForwardMode::ApiKey)     => Some("OPENAI_API_KEY"),
            _ => None,
        }
    }

    pub fn supported_modes(self) -> &'static [AuthForwardMode] {
        match self {
            Agent::Claude => &[
                AuthForwardMode::Sync,
                AuthForwardMode::ApiKey,
                AuthForwardMode::OAuthToken,
                AuthForwardMode::Ignore,
            ],
            Agent::Codex => &[
                AuthForwardMode::Sync,
                AuthForwardMode::ApiKey,
                AuthForwardMode::Ignore,
            ],
        }
    }
}
```

`AgentAuthConfig` is a one-field wrapper today and exists as a wrapper so future fields (refresh-strategy hints, org overrides, anything else) can be added without renaming an `auth_forward`-only TOML key.

### 3. TUI flow

#### Auth panel layout

```
┌─ Auth ────────────────────────────────────────────────────────────┐
│  Global defaults                                                  │
│    Claude: sync                          [open global config]     │
│    Codex:  sync                          [open global config]     │
│  ───────────────────────────────────────────────────────────────  │
│  This workspace                                                   │
│    Claude: api_key (workspace override)  [edit]  [reset]          │
│    Codex:  sync    (inherited)            [edit]                  │
│  ───────────────────────────────────────────────────────────────  │
│  Per role × agent                                                 │
│    smith / Claude: api_key  (workspace)         ✅ resolves       │
│    smith / Codex:  ignore   (most-specific)     —                 │
│    neo   / Claude: oauth_token (most-specific)  ✗ unset           │
│    neo   / Codex:  sync     (global)            —                 │
└───────────────────────────────────────────────────────────────────┘
```

Each row shows the **effective mode**, a **provenance tag** (`global`, `workspace`, `most-specific`, `inherited`), and a **credential status badge** (`✅ resolves`, `✗ unset`, `—` when no credential is needed).

The badge is computed on panel open by running the existing four-layer env merge for the (workspace, role) tuple and checking whether `Agent::required_env_var(agent, mode)` produces a non-empty value. No active probe.

The Global defaults section is read-only inside the workspace-manager scope. `[open global config]` navigates to the global-config screen.

#### Edit form

```
┌─ Edit auth: workspace 'proj' / role 'smith' / Claude ─────────────┐
│  Mode:   [ api_key   ▼ ]                                          │
│  ───────────────────────────────────────────────────────────────  │
│  Credential   (writes ANTHROPIC_API_KEY at this layer)            │
│    ( ) Literal:    [______________________________]               │
│    (•) 1Password:  Work / Claude / api-key   [Pick…]              │
│                    ✅ resolves                                    │
│  ───────────────────────────────────────────────────────────────  │
│  [ Save ]    [ Cancel ]    [ Reset (inherit from above) ]         │
└───────────────────────────────────────────────────────────────────┘
```

- **Mode picker** is sourced from `Agent::supported_modes(agent)`. Codex never shows `oauth_token`. Switching to `sync` or `ignore` collapses the credential block.
- **Credential — Literal** stores the input as `EnvValue::Plain(s)`. Empty string cannot be committed.
- **Credential — 1Password** invokes the existing `op_picker`. The picker drills account → vault → item → field, runs `op read` against the chosen reference at commit time, and refuses to commit if the read fails. The form receives an `OpRef { op, path }` only after a successful read.
- **Save** is disabled until a mode is committed and, if a credential is required, exactly one input method has a committed value. There is no "save with warnings" path.
- **Reset (inherit from above)** removes the entries at this layer (`[workspaces.<ws>.roles.<r>.<agent>]` and the credential env var written *at this layer*); other layers untouched.

#### Persistence

On save:

1. Write `cfg.workspaces[ws].roles[role].<agent>.auth_forward = mode` (or the workspace level, depending on which row is being edited).
2. If the mode requires a credential, write `cfg.workspaces[ws].roles[role].env[<WELL_KNOWN_NAME>] = value`, with the same `EnvValue` shape the secrets row already writes.
3. If the new mode no longer requires a credential and one was previously written *at this exact layer*, leave it in place. The user can delete it from the Secrets panel; we do not auto-prune to avoid clobbering values the user might want to reuse.

The TOML write goes through the existing config-persistence path (atomic write, same serializer).

#### Deliberate non-features

- No active provider probe.
- No editing of globals from inside a workspace.
- No bulk operations.
- No cross-row state — each row reads its three layers and that is the entire computation.

### 4. Validation and errors

Three enforcement points: parse, save, launch. Save-time is the strongest because it prevents bad state from being written to disk.

#### Parse-time

Two parser-level rejections:

**Unknown field — old shapes.** Any TOML containing `[roles.<r>.claude]` produces the standard `serde` error:

```
error: unknown field `claude` at roles.smith, expected one of: `git`, `trusted`, `env`
   in /Users/op/.config/jackin/config.toml line 17
```

No custom message. No memorialization.

**Codex `oauth_token` rejection.** A custom `Deserialize` impl on the Codex slot rejects the unsupported mode:

```
error: auth_forward 'oauth_token' is not supported for codex
   supported modes: sync, api_key, ignore
   in /Users/op/.config/jackin/config.toml line 24 ([codex].auth_forward)
```

#### Save-time

Per the form rules above. Save is disabled until invariants hold; the op-picker's commit-only-on-successful-read rule means any `OpRef` reaching the form has been resolved at pick time. There is no path from form interaction to disk that bypasses these checks.

#### Launch-time

`verify_credential_env_present(agent, mode, merged_env, layers, scope)` runs once per agent that's about to start, after the operator-env merge completes, before `docker run`. If `Agent::required_env_var(agent, mode)` is `None`, return Ok. Otherwise look up the name in `merged_env` and require non-empty.

Failure produces a structured, multi-line error:

```
error: cannot launch claude in workspace 'proj' role 'smith'
       — auth_forward is 'api_key', which requires ANTHROPIC_API_KEY
         to resolve to a non-empty value, but it is unset.

  Effective auth resolution:
    workspace × role × claude    -> api_key       (most-specific)
    workspace × claude            -> (none)
    global  × claude              -> sync

  Env layer resolution for ANTHROPIC_API_KEY (lowest -> highest):
    [env]                                -> unset
    [roles.smith.env]                    -> unset
    [workspaces.proj.env]                -> unset
    [workspaces.proj.roles.smith.env]    -> unset

  Fix one of:
    - Open the Auth panel:  jackin tui workspaces  → 'proj' → Auth → smith / Claude
    - Or by hand:           jackin config env set ANTHROPIC_API_KEY=<value> \
                                --workspace proj --role smith
    - Or change the mode:   set auth_forward = 'sync' at one of the layers above
```

Three things this error does on purpose:

1. Names the mode-resolution layer, so the user sees *why* the mode demands a credential, not just *that* it does.
2. Names every env layer that was checked, with each layer's resolution state (`unset`, `resolved (op://...)`, `resolved (literal)`). Values are never printed — only the kind.
3. Lists three concrete fix paths: TUI, CLI, or config edit.

The error type carries enough structure (`agent`, `mode`, `layers`, `scope`) that the TUI's pre-launch validation can render the same content as a panel with buttons rather than as a wall of text.

#### Render surfaces

| Surface | Render |
|---------|--------|
| CLI launch (`jackin run`, `jackin workspace launch`) | Multi-line ANSI-colored text. Non-zero exit. |
| TUI launch screen | Structured panel with the same content; buttons jump to the Auth panel or env editor. |

Both render from the same `LaunchError::AuthCredentialMissing` variant.

#### Not validated

- Credential format (`sk-ant-…`) — provider prefixes change.
- Credential validity against the provider — no probe.
- Cross-layer conflicts — most-specific wins; overrides are intentional.
- Orphan credentials — env var set but mode does not require it. Harmless; no warning.

### 5. Testing strategy

#### Unit: config parse and round-trip

- `parse_global_only` — `[claude].auth_forward = "sync"` round-trips.
- `parse_workspace_layer` — `[workspaces.X.claude]` round-trips.
- `parse_ws_role_agent_layer` — `[workspaces.X.roles.Y.claude]` round-trips.
- `reject_legacy_role_claude` — TOML with `[roles.Y.claude]` fails to parse; assert error contains `unknown field`.
- `reject_codex_oauth_token` — assert error contains `not supported for codex`.
- Update `config.round_trip.toml` fixture.

#### Unit: mode resolution

- Most-specific wins across all eight layer-set/unset combinations.
- Per-agent isolation: `[claude]` mode never affects Codex.
- Default `Sync` when nothing is set at any layer.

#### Unit: Agent table

- `required_env_var` exhaustively asserted for every `(Agent, AuthForwardMode)` pair.
- `Codex.supported_modes()` does not include `OAuthToken`.
- Compiler enforces exhaustivity — adding a new `Agent` variant fails to compile until the table is extended.

#### Unit: provisioning per mode

In `src/instance/auth.rs`, with temp dirs and mocked host state:

| Test | Setup | Assert |
|------|-------|--------|
| `claude_sync_copies_host` | host creds present | container state contains host content |
| `claude_api_key_wipes` | host creds present | container state file absent |
| `claude_oauth_token_wipes` | host creds present | container state file absent |
| `claude_ignore_wipes` | host creds present | container state file absent |
| `codex_sync_copies` | host `auth.json` present | container `auth.json` matches host |
| `codex_api_key_wipes` | host `auth.json` present | container `auth.json` absent |
| `codex_ignore_wipes` | host `auth.json` present | container `auth.json` absent |

macOS Keychain reads are mocked via the existing trait-injection pattern.

#### Unit: launch validation

In `src/runtime/launch.rs`:

- `sync_mode_no_check` — returns Ok regardless of env.
- `api_key_present` — non-empty `ANTHROPIC_API_KEY` returns Ok.
- `api_key_missing` — empty value returns `LaunchError::AuthCredentialMissing` with structured fields populated.
- `oauth_token_missing_claude` — same shape, `CLAUDE_CODE_OAUTH_TOKEN`.
- `error_renders_with_layer_trace` — `Display` impl produces the multi-line block; snapshot test.

#### Integration: TUI form

Using the existing state-driven TUI test harness:

- `save_disabled_until_mode_set`
- `save_disabled_for_api_key_without_credential`
- `save_enabled_for_sync` (no credential needed)
- `mode_switch_collapses_credential_block`
- `op_picker_commit_persists_OpRef` — mock `OpRunner`; assert `OpRef { op, path }` reaches the form.
- `op_picker_failed_read_blocks_commit` — mock `op read` failure; assert form value remains `None`.
- `reset_clears_layer_only` — only the target layer entries are removed.
- `codex_form_no_oauth_token_option` — picker enumerates `Agent::supported_modes(Codex)`.

#### End-to-end: docker launch

One smoke test per (agent, mode) pair, gated behind `--features e2e`:

- Pre-seed config with the target mode.
- Run `cargo run --bin jackin -- run --debug` against a real container.
- Assert per mode:
  - `sync` — container `~/.claude/.credentials.json` exists and matches host.
  - `api_key` — env var set in container; state files absent.
  - `oauth_token` — env var set in container; state files absent.
  - `ignore` — neither env var nor state files present.

Seven tests total (Claude × 4 + Codex × 3). Skipped on CI without a docker daemon.

#### Not tested

- No active provider probe tests.
- No TUI snapshot/golden tests — assertions on logic state only.
- No backward-compatibility tests — old shape rejected, no migration code, nothing to test there. The single negative test in the parse suite is sufficient.

## Risks and open questions

- **Op-picker commit failure surface.** The form depends on the op-picker refusing to commit on `op read` failure. If the picker today commits the reference before reading it (and treats the read as a separate step), this spec needs the picker to be tightened. Implementation should verify the current picker behavior and tighten if needed.
- **`oauth_token` Codex parity.** Codex's refresh-token flow is genuinely different from Claude's static OAuth token. If a Codex equivalent surfaces later (e.g., via a Codex CLI feature), it lands as a new mode (`codex_refresh_token` or similar), not by repurposing `oauth_token`.
- **Global default editing surface.** This spec treats globals as read-only inside the workspace-manager scope. The spec assumes a "global config" screen exists or is added separately. If no such screen exists today, an inline editor on the Auth panel's Global section (gated behind a confirmation modal) is an acceptable fallback.
- **Orphan credentials.** Per the design, switching mode away from `api_key` does not remove the previously-written credential env var. This is intentional but visible in the Secrets panel as an unused entry. If operators report this as confusing, a follow-up could add a "remove credential too" checkbox on the form.

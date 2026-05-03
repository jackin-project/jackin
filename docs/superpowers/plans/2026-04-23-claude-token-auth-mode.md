# Claude Token Auth Mode — Implementation Plan (PR 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third `AuthForwardMode` variant, `Token`, that relies on the operator-provided `CLAUDE_CODE_OAUTH_TOKEN` environment variable (resolved through the workspace env pipeline) instead of forwarding `/login` session files. In `Token` mode the agent state directory is provisioned with the same empty shape as `Ignore` (`.claude.json` = `{}`, no `.credentials.json`); Claude Code inside the container reads the token from the container env. Launch aborts with an actionable error if `CLAUDE_CODE_OAUTH_TOKEN` is not present in the resolved env. Launch output surfaces the active auth mode on a single diagnostic line.

**Architecture:** Additive change. The enum gains `Token`. `provision_claude_auth` gets a third arm that reuses the `Ignore` filesystem shape (so switching across `sync ↔ token ↔ ignore` always lands in a clean state). A new `AuthProvisionOutcome::TokenMode` lets the launch site tell `Token` apart from `Ignore` for diagnostics. The launch-time presence check runs after `resolve_operator_env` has produced the final merged env map, so `CLAUDE_CODE_OAUTH_TOKEN` resolves through the same pipeline as any other operator-declared env var (including `op://` references). The CLI, docs, and changelog are updated to advertise the new mode.

**Tech Stack:** Rust (edition 2024), `serde` + `toml` 1.x (already in `Cargo.toml`), `owo-colors` for colored output, `anyhow` for errors, `tempfile` + `cargo-nextest` for tests.

**Branch:** `feature/claude-token-auth-mode` (per `BRANCHING.md` — `feature/<short-description>`).
**Commit style:** Conventional Commits with DCO `Signed-off-by` and `Co-authored-by: Claude <noreply@anthropic.com>` per `AGENTS.md`.
**Spec:** `docs/superpowers/specs/2026-04-23-claude-token-auth-mode-design.md`.

### Assumptions

This plan depends on PR 1 and PR 2 having landed:

- **PR 1** is assumed merged. At branch-off, `AuthForwardMode` has only `Ignore` and `Sync` variants, with `Sync` as `#[default]`. `"copy"` is accepted by `FromStr`/`Deserialize` as a deprecated alias that maps to `Sync`. PR 3 adds `Token` as a third variant.
- **PR 2** is assumed merged and provides a workspace-aware env resolver reachable from `src/runtime/launch.rs`. The assumed shape is:
  ```rust
  pub struct ResolvedOperatorEnv {
      pub vars: BTreeMap<String, String>,
      // plus source-tracking fields for diagnostics (e.g. a per-key source enum)
  }
  pub fn resolve_operator_env(
      config: &AppConfig,
      workspace: &ResolvedWorkspace,
      agent: &ClassSelector,
  ) -> anyhow::Result<ResolvedOperatorEnv>;
  ```
  The implementer should **verify the actual PR 2 API** on the first pull of `main` (see Step 0.4 below). If the function name, return type, or source-tracking field shape differs, adjust the one call site in Task 5 to match; everywhere else in the plan only depends on `resolved.vars.get("CLAUDE_CODE_OAUTH_TOKEN")`, which is stable.
- If PR 2 did not ship a source-tracking field, the diagnostic line in Task 6 prints the literal string `CLAUDE_CODE_OAUTH_TOKEN` as the source reference (lossy but correct). If it did ship one (e.g. `sources: BTreeMap<String, EnvSource>`), the diagnostic line should use the source description. Task 6 documents both fallbacks.

---

## File Structure

| File                                                                     | Purpose                                                                           |
| ------------------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| `src/config/mod.rs`                                                      | Add `AuthForwardMode::Token` variant; update `Display`, `FromStr`, unit tests.    |
| `src/instance/mod.rs`                                                    | Add `AuthProvisionOutcome::TokenMode` variant; update doc comment.                |
| `src/instance/auth.rs`                                                   | Add `Token` arm in `provision_claude_auth` (same filesystem shape as `Ignore`).   |
| `src/runtime/launch.rs`                                                  | Launch-time `CLAUDE_CODE_OAUTH_TOKEN` presence check + mode-diagnostic line.      |
| `src/cli/config.rs`                                                      | Help text: add `token` to accepted modes; update example block; update tests.     |
| `docs/src/content/docs/guides/authentication.mdx`                        | New Token mode section; update modes table.                                       |
| `docs/src/content/docs/reference/configuration.mdx`                      | Accepted values for `auth_forward` now include `token`.                           |
| `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`       | Mark token mode as delivered in the "Current State" section.                      |
| `CHANGELOG.md`                                                           | `Added` entry under `## [Unreleased]`.                                            |

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

- [ ] **Step 0.2: Confirm PR 1 and PR 2 have landed**

```bash
grep -n "AuthForwardMode" src/config/mod.rs | head -5
grep -n "resolve_operator_env\|ResolvedOperatorEnv" src/ -r | head -5
```

Expected:
- `AuthForwardMode` enum contains `Ignore` and `Sync` (no `Copy` variant); `Sync` carries `#[default]`.
- `resolve_operator_env` / `ResolvedOperatorEnv` are defined somewhere under `src/` (typically `src/env_resolver*.rs` or `src/env_resolver/` module).

If either precondition is not met, stop and check out the correct base branch.

- [ ] **Step 0.3: Create the feature branch**

```bash
git checkout -b feature/claude-token-auth-mode
```

- [ ] **Step 0.4: Record the actual PR 2 API so Task 5 uses the right call**

```bash
rg -n "pub fn resolve_operator_env|pub struct ResolvedOperatorEnv" src/
```

Write down the exact function signature and struct shape on a scratchpad. In Task 5 you will call this function; the plan uses the assumed shape from the Assumptions section above — adjust to match what PR 2 actually shipped if it differs (argument order, return type wrapping, source-tracking field name).

- [ ] **Step 0.5: Confirm pre-commit gate is currently clean (baseline)**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: all three exit 0. If any fail, do NOT start this work — fix baseline first or you will be chasing unrelated failures.

---

## Task 1: Add `AuthForwardMode::Token` variant with `Display` + `FromStr` support

**Files:**
- Modify: `src/config/mod.rs` (enum definition, `Display`, `FromStr`, unit tests in the bottom `mod tests` block)

- [ ] **Step 1.1: Write failing tests for the new `Token` variant**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/config/mod.rs` (immediately after the existing `existing_config_without_claude_section_deserializes_with_defaults` test):

```rust
    #[test]
    fn auth_forward_mode_from_str_accepts_token() {
        use std::str::FromStr;
        assert_eq!(
            AuthForwardMode::from_str("token").unwrap(),
            AuthForwardMode::Token
        );
    }

    #[test]
    fn auth_forward_mode_display_emits_token() {
        assert_eq!(AuthForwardMode::Token.to_string(), "token");
    }

    #[test]
    fn auth_forward_mode_deserializes_token() {
        let toml_str = r#"
[claude]
auth_forward = "token"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Token);
    }

    #[test]
    fn auth_forward_mode_from_str_error_lists_token() {
        use std::str::FromStr;
        let err = AuthForwardMode::from_str("nope").unwrap_err();
        assert!(
            err.contains("token"),
            "error message should advertise the token mode; got: {err}"
        );
    }
```

- [ ] **Step 1.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin \
  config::tests::auth_forward_mode_from_str_accepts_token \
  config::tests::auth_forward_mode_display_emits_token \
  config::tests::auth_forward_mode_deserializes_token \
  config::tests::auth_forward_mode_from_str_error_lists_token
```

Expected: all four fail (variant does not exist yet; error message still mentions only `sync, ignore`).

- [ ] **Step 1.3: Add the `Token` variant to the enum**

In `src/config/mod.rs`, update the `AuthForwardMode` enum to include `Token`. Replace the existing enum definition (post-PR-1 state — roughly lines 20–30) with:

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
    /// Use a long-lived OAuth token from the operator-resolved env
    /// (`CLAUDE_CODE_OAUTH_TOKEN`). The agent state directory is
    /// provisioned empty (same shape as `Ignore`); Claude Code inside
    /// the container picks up the token from its process environment.
    Token,
}
```

Note: `Deserialize` is still a custom impl carried over from PR 1 (routes through `FromStr`), so no `#[derive(Deserialize)]` is needed here.

- [ ] **Step 1.4: Update the `Display` impl**

Replace the existing `Display` impl (post-PR-1 state — roughly lines 34–42) with:

```rust
impl std::fmt::Display for AuthForwardMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignore => write!(f, "ignore"),
            Self::Sync => write!(f, "sync"),
            Self::Token => write!(f, "token"),
        }
    }
}
```

- [ ] **Step 1.5: Update the `FromStr` impl**

Replace the existing `FromStr` impl (post-PR-1 state — roughly lines 44–60) with:

```rust
impl std::str::FromStr for AuthForwardMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ignore" => Ok(Self::Ignore),
            "sync" => Ok(Self::Sync),
            "token" => Ok(Self::Token),
            // Deprecated alias from PR 1 — accepted to avoid breaking
            // scripts and configs from before the default flipped to
            // `sync`. Callers that want to surface the deprecation
            // should check for the literal `"copy"` themselves before
            // calling `parse()`.
            "copy" => Ok(Self::Sync),
            other => Err(format!(
                "invalid auth_forward mode {other:?}; expected one of: sync, ignore, token"
            )),
        }
    }
}
```

- [ ] **Step 1.6: Run tests — confirm the new ones pass**

```bash
cargo nextest run -p jackin config::tests
```

Expected: all green, including the four new `token` tests. Task 2 will handle the inevitable match-exhaustiveness failures in `src/instance/auth.rs` — if you see them now, that is expected.

- [ ] **Step 1.7: Commit Task 1**

```bash
git add src/config/mod.rs
git commit -s -m "$(cat <<'EOF'
feat(config): add AuthForwardMode::Token variant

Add a third auth_forward variant, Token, for the long-lived
CLAUDE_CODE_OAUTH_TOKEN flow. Display, FromStr, and the custom
Deserialize path all accept the new value. The error message emitted
on an unknown mode now lists "sync, ignore, token".

Provisioning semantics, launch-time presence check, CLI help text, and
docs are added in follow-up commits.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add `AuthProvisionOutcome::TokenMode` and wire the `Token` arm in `provision_claude_auth`

**Files:**
- Modify: `src/instance/mod.rs` (add `TokenMode` variant; update doc comment)
- Modify: `src/instance/auth.rs` (add `AuthForwardMode::Token` match arm + tests)

- [ ] **Step 2.1: Write failing tests for the new provisioning behavior**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/instance/auth.rs`, after the existing `sync_mode_preserves_container_auth_when_host_file_missing` test (around the "Mode transition tests" section):

```rust
    #[test]
    fn token_mode_writes_empty_json_and_no_credentials() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        // Seed host auth — token mode must NOT copy it.
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Token,
            temp.path(),
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
        assert!(
            !state.claude_dir.join(".credentials.json").exists(),
            "token mode must not write .credentials.json"
        );
        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    }

    #[test]
    fn switching_from_sync_to_token_revokes_forwarded_credentials() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: sync mode writes credentials
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();
        assert!(state.claude_dir.join(".credentials.json").exists());

        // Operator switches to token — credentials must be wiped and
        // .claude.json reset to {} so Claude Code inside the container
        // authenticates exclusively via CLAUDE_CODE_OAUTH_TOKEN.
        let (state2, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Token,
            temp.path(),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&state2.claude_json).unwrap(), "{}");
        assert!(!state2.claude_dir.join(".credentials.json").exists());
        assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    }

    #[test]
    fn switching_from_token_to_sync_forwards_fresh_host_creds() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: token mode leaves an empty state
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Token,
            temp.path(),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");

        // Operator switches to sync — host auth must now be forwarded
        let (state2, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(&state2.claude_json)
                .unwrap()
                .contains("test@example.com")
        );
        assert_eq!(
            std::fs::read_to_string(state2.claude_dir.join(".credentials.json")).unwrap(),
            TEST_CREDENTIALS
        );
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
    }

    #[test]
    fn switching_from_token_to_ignore_remains_empty() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // Token mode seeds an empty state
        let (_, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Token,
            temp.path(),
        )
        .unwrap();

        // Switching to ignore must keep the empty shape (no .credentials.json)
        let (state2, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&state2.claude_json).unwrap(), "{}");
        assert!(!state2.claude_dir.join(".credentials.json").exists());
        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    }
```

- [ ] **Step 2.2: Run the new tests — confirm they fail (compile error)**

```bash
cargo nextest run -p jackin instance::auth::tests::token_mode_writes_empty_json_and_no_credentials 2>&1 | tail -20
```

Expected: compile error — `AuthForwardMode::Token` is now known (from Task 1) but `AuthProvisionOutcome::TokenMode` does not exist and the `Token` match arm is missing in `provision_claude_auth`.

- [ ] **Step 2.3: Add `AuthProvisionOutcome::TokenMode` to `src/instance/mod.rs`**

Replace the existing `AuthProvisionOutcome` enum (currently at `src/instance/mod.rs:14-26`) with:

```rust
/// Outcome of the `.claude.json` provisioning step, so callers can surface
/// a one-time notice when host credentials are forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthProvisionOutcome {
    /// No host auth was forwarded (ignore mode).
    Skipped,
    /// Host auth was copied into the container state.
    Copied,
    /// Host auth was synced (overwritten) into the container state.
    Synced,
    /// Mode would have forwarded, but host file was missing — wrote `{}`.
    HostMissing,
    /// Token mode: empty `.claude.json`, no `.credentials.json` —
    /// Claude Code inside the container uses `CLAUDE_CODE_OAUTH_TOKEN`
    /// from the resolved env.
    TokenMode,
}
```

> Note: `Copied` is retained even after PR 1 drops the `Copy` variant, because internal callers still use it as a structural tag (e.g. there was no policy decision in PR 1 to rename it). If PR 1 removed it from the enum, drop the `Copied` line here too — the `Sync` arm in `auth.rs` only emits `Synced`/`HostMissing`, so removing `Copied` is safe. Verify with `rg 'AuthProvisionOutcome::Copied' src/` — if there are zero remaining references outside this file, remove the variant.

- [ ] **Step 2.4: Add the `Token` arm in `provision_claude_auth`**

In `src/instance/auth.rs`, inside the `let outcome = match mode { … }` block (currently at lines 26–73), insert a new arm between `AuthForwardMode::Ignore` and `AuthForwardMode::Sync`. The final match becomes:

```rust
        let outcome = match mode {
            AuthForwardMode::Ignore => {
                // Always ensure a clean slate — if switching from sync/token
                // to ignore, the previously forwarded credentials must be
                // revoked.
                if !claude_json.exists() || std::fs::read_to_string(claude_json)? != "{}" {
                    write_private_file(claude_json, "{}")?;
                }
                if credentials_json.exists() {
                    std::fs::remove_file(&credentials_json)?;
                }
                AuthProvisionOutcome::Skipped
            }
            AuthForwardMode::Token => {
                // Token mode provisions the same empty shape as Ignore —
                // Claude Code inside the container authenticates via
                // CLAUDE_CODE_OAUTH_TOKEN from the resolved env, not via
                // filesystem credentials. Switching from sync → token must
                // still wipe any previously forwarded creds.
                if !claude_json.exists() || std::fs::read_to_string(claude_json)? != "{}" {
                    write_private_file(claude_json, "{}")?;
                }
                if credentials_json.exists() {
                    std::fs::remove_file(&credentials_json)?;
                }
                AuthProvisionOutcome::TokenMode
            }
            AuthForwardMode::Sync => {
                if let Some(creds) = read_host_credentials(host_home) {
                    copy_host_claude_json(&host_claude_json, claude_json)?;
                    write_private_file(&credentials_json, &creds)?;
                    AuthProvisionOutcome::Synced
                } else {
                    // Host has no auth — leave the container's existing
                    // files untouched (it may have credentials from a
                    // previous manual login). Only bootstrap an empty
                    // file if nothing exists yet.
                    if !claude_json.exists() {
                        write_private_file(claude_json, "{}")?;
                    }
                    // Repair permissions on pre-existing auth files that
                    // may have legacy permissive modes (e.g. 0644).
                    repair_permissions(claude_json);
                    repair_permissions(&credentials_json);
                    AuthProvisionOutcome::HostMissing
                }
            }
        };
```

- [ ] **Step 2.5: Run the four new tests — confirm they pass**

```bash
cargo nextest run -p jackin \
  instance::auth::tests::token_mode_writes_empty_json_and_no_credentials \
  instance::auth::tests::switching_from_sync_to_token_revokes_forwarded_credentials \
  instance::auth::tests::switching_from_token_to_sync_forwards_fresh_host_creds \
  instance::auth::tests::switching_from_token_to_ignore_remains_empty
```

Expected: all four green.

- [ ] **Step 2.6: Run full suite for regressions**

```bash
cargo nextest run
```

Expected: all green. If you removed `AuthProvisionOutcome::Copied` in Step 2.3, also confirm `rg 'AuthProvisionOutcome::Copied' src/` returns zero matches.

- [ ] **Step 2.7: Commit Task 2**

```bash
git add src/instance/mod.rs src/instance/auth.rs
git commit -s -m "$(cat <<'EOF'
feat(instance): provision token mode with the ignore filesystem shape

Add AuthProvisionOutcome::TokenMode and a matching arm in
provision_claude_auth. Token mode leaves `.claude.json` as `{}` and
removes any `.credentials.json` — Claude Code inside the container
authenticates via CLAUDE_CODE_OAUTH_TOKEN from the resolved env.

Mode transitions are covered by new tests: sync → token and token →
ignore both leave the state clean; token → sync correctly forwards
fresh host credentials.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Update `src/cli/config.rs` help text — advertise `token` as a mode

**Files:**
- Modify: `src/cli/config.rs` (lines 19–43 for `AuthCommand::Set`; help-text tests at lines 277–302)

- [ ] **Step 3.1: Write failing tests asserting help text mentions `token`**

Edit the existing `config_auth_set_help_shows_examples` test (around line 278). Replace it with:

```rust
    #[test]
    fn config_auth_set_help_shows_examples() {
        let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
        assert!(help.contains("Examples:"));
        assert!(help.contains("jackin config auth set sync"));
        assert!(help.contains("jackin config auth set token"));
        assert!(help.contains("--agent"));
    }
```

Add immediately after it:

```rust
    #[test]
    fn config_auth_set_help_lists_token_as_accepted_mode() {
        let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
        // Modes are listed in the subcommand doc comment. After this
        // PR the accepted modes are sync, ignore, and token.
        assert!(help.contains("sync"));
        assert!(help.contains("ignore"));
        assert!(
            help.contains("token"),
            "help text must advertise the token mode; got:\n{help}"
        );
    }
```

- [ ] **Step 3.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin \
  cli::config::tests::config_auth_set_help_shows_examples \
  cli::config::tests::config_auth_set_help_lists_token_as_accepted_mode
```

Expected: both fail — current help text lists only `sync` and `ignore` (post-PR-1 state).

- [ ] **Step 3.3: Update the `AuthCommand::Set` doc comment and `after_long_help`**

In `src/cli/config.rs`, replace the `Set` variant (post-PR-1 state — approximately lines 19–43) with:

```rust
    /// Set the authentication forwarding mode
    ///
    /// Controls how the host's Claude Code authentication is made available
    /// to agent containers.
    /// Modes: sync (default — overwrite container auth from host on each
    /// launch when host auth exists; preserve container auth when host auth
    /// is absent), ignore (revoke and never forward), token (use a long-lived
    /// CLAUDE_CODE_OAUTH_TOKEN resolved from the operator env — the token
    /// itself is never written to disk; see `jackin` docs on auth forwarding
    /// for setup).
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin config auth set sync
  jackin config auth set ignore
  jackin config auth set token
  jackin config auth set sync --agent agent-smith
  jackin config auth set token --agent chainargos/the-architect"
    )]
    Set {
        /// Authentication forwarding mode: sync, ignore, or token
        mode: String,
        /// Apply to a specific agent instead of globally
        #[arg(long)]
        agent: Option<String>,
    },
```

- [ ] **Step 3.4: Run tests — confirm the cli::config tests are green**

```bash
cargo nextest run -p jackin cli::config::tests
```

Expected: all green.

- [ ] **Step 3.5: Commit Task 3**

```bash
git add src/cli/config.rs
git commit -s -m "$(cat <<'EOF'
docs(cli): advertise "token" as an accepted auth_forward mode

Update the `jackin config auth set` help text and examples to include
`token` as a first-class mode alongside `sync` and `ignore`. Help-text
regression tests cover both the examples block and the accepted-modes
doc comment.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Extract launch-mode diagnostic helper in `src/tui/output.rs`

**Files:**
- Modify: `src/tui/output.rs` (new `auth_mode_notice` helper)
- Modify: `src/tui/mod.rs` (re-export)

- [ ] **Step 4.1: Write failing tests for the new helper**

Add to the bottom of `src/tui/output.rs` (create the `#[cfg(test)]` block if it does not already exist):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Strip ANSI escape sequences so assertions match plain text.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                for inner in chars.by_ref() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    #[test]
    fn auth_mode_notice_token_mentions_source_reference() {
        let line = format_auth_mode_notice_for_test(
            "token",
            Some("CLAUDE_CODE_OAUTH_TOKEN ← op://vault/claude/token"),
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("claude auth:"), "got: {clean}");
        assert!(clean.contains("OAuth token"), "got: {clean}");
        assert!(
            clean.contains("CLAUDE_CODE_OAUTH_TOKEN ← op://vault/claude/token"),
            "got: {clean}"
        );
    }

    #[test]
    fn auth_mode_notice_sync_has_one_liner() {
        let clean = strip_ansi(&format_auth_mode_notice_for_test("sync", None));
        assert!(clean.contains("claude auth:"));
        assert!(clean.contains("host session"));
        assert!(clean.contains("sync"));
    }

    #[test]
    fn auth_mode_notice_ignore_has_one_liner() {
        let clean = strip_ansi(&format_auth_mode_notice_for_test("ignore", None));
        assert!(clean.contains("claude auth:"));
        assert!(clean.contains("none"));
        assert!(clean.contains("ignore"));
    }
}
```

- [ ] **Step 4.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin tui::output::tests
```

Expected: fails — `format_auth_mode_notice_for_test` does not exist.

- [ ] **Step 4.3: Add `auth_mode_notice` + test-visible formatter in `src/tui/output.rs`**

Insert after the existing `hint` function (around line 177 post-PR-1):

```rust
/// Render the one-line launch diagnostic for the active auth mode.
///
/// Shapes:
///   claude auth: host session (sync)
///   claude auth: none (ignore — /login required inside the container)
///   claude auth: OAuth token (CLAUDE_CODE_OAUTH_TOKEN ← <source-reference>)
///
/// `source_reference` is consulted only by the `token` arm; pass the
/// resolver's source description for the `CLAUDE_CODE_OAUTH_TOKEN`
/// entry (e.g. `"op://vault/claude/token"` or
/// `"$CLAUDE_CODE_OAUTH_TOKEN"`). Other modes pass `None`.
pub fn auth_mode_notice(mode: &str, source_reference: Option<&str>) {
    eprintln!("  {}", format_auth_mode_notice_for_test(mode, source_reference));
}

/// Pure formatter extracted for unit-testing the exact output text.
/// Returns the rendered line with ANSI color codes included.
fn format_auth_mode_notice_for_test(mode: &str, source_reference: Option<&str>) -> String {
    let label = "claude auth:".color(rgb(PHOSPHOR_GREEN)).bold().to_string();
    let body = match mode {
        "token" => {
            let src = source_reference.unwrap_or("CLAUDE_CODE_OAUTH_TOKEN");
            format!("OAuth token ({src})")
        }
        "sync" => "host session (sync)".to_string(),
        "ignore" => "none (ignore — /login required inside the container)".to_string(),
        other => format!("{other}"),
    };
    format!(
        "{label} {}",
        body.color(rgb(PHOSPHOR_DIM)),
    )
}
```

- [ ] **Step 4.4: Re-export `auth_mode_notice` from `src/tui/mod.rs`**

In `src/tui/mod.rs:26-29`, add `auth_mode_notice` to the `pub use output::{ ... }` list. Final shape:

```rust
pub use output::{
    auth_mode_notice, clear_screen, fatal, hint, print_config_table, print_deploying, print_logo,
    set_terminal_title, shorten_home, step_fail, step_quiet, step_shimmer,
};
```

> If PR 1 already added `deprecation_warning` to this list, keep it alphabetically ordered.

- [ ] **Step 4.5: Run tests — confirm all green**

```bash
cargo nextest run -p jackin tui::output::tests
cargo nextest run -p jackin
```

Expected: all green.

- [ ] **Step 4.6: Commit Task 4**

```bash
git add src/tui/output.rs src/tui/mod.rs
git commit -s -m "$(cat <<'EOF'
feat(tui): add auth_mode_notice helper for launch-time diagnostics

Add a single-line diagnostic helper that surfaces the active Claude
auth mode at launch time. The token arm includes the source reference
of CLAUDE_CODE_OAUTH_TOKEN (e.g. `op://vault/claude/token` or
`$CLAUDE_CODE_OAUTH_TOKEN`) so operators can tell immediately where the
token came from. Unit-tested via a pure formatter.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Launch-time `CLAUDE_CODE_OAUTH_TOKEN` presence check

**Files:**
- Modify: `src/runtime/launch.rs` (the block around lines 647–685, after `config.resolve_auth_forward_mode`)

- [ ] **Step 5.1: Write a failing unit test for the presence-check helper**

Adding a test at the `load_agent` level requires a full `FakeRunner` plumbing, so extract a small pure helper and unit-test it directly.

In `src/runtime/launch.rs`, add a test at the bottom of the existing `mod tests` block:

```rust
    #[test]
    fn verify_token_env_present_accepts_resolved_token() {
        let mut vars = std::collections::BTreeMap::new();
        vars.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            "sk-ant-oat01-redacted".to_string(),
        );
        assert!(verify_token_env_present(&vars).is_ok());
    }

    #[test]
    fn verify_token_env_missing_returns_actionable_error() {
        let vars = std::collections::BTreeMap::<String, String>::new();
        let err = verify_token_env_present(&vars).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("CLAUDE_CODE_OAUTH_TOKEN"), "got: {msg}");
        // Both remediation paths must be surfaced.
        assert!(
            msg.contains("op://"),
            "error should mention the 1Password remediation path; got: {msg}"
        );
        assert!(
            msg.contains("$CLAUDE_CODE_OAUTH_TOKEN"),
            "error should mention the host env remediation path; got: {msg}"
        );
        assert!(
            msg.contains("[env]"),
            "error should point the operator at the [env] manifest table; got: {msg}"
        );
    }
```

- [ ] **Step 5.2: Run the new tests — confirm they fail**

```bash
cargo nextest run -p jackin \
  runtime::launch::tests::verify_token_env_present_accepts_resolved_token \
  runtime::launch::tests::verify_token_env_missing_returns_actionable_error
```

Expected: compile failure — `verify_token_env_present` does not exist.

- [ ] **Step 5.3: Add the helper and wire it into the launch flow**

At a logical location in `src/runtime/launch.rs` (e.g. immediately after the `claim_container_name` function, before the `LoadCleanup` impl), add:

```rust
/// Verify that `CLAUDE_CODE_OAUTH_TOKEN` is present in the resolved
/// operator env when auth_forward == Token. Returns an actionable
/// error listing both remediation paths when the token is missing.
///
/// Kept as a small pure helper over `BTreeMap<String, String>` so it
/// can be unit-tested without faking the workspace env resolver.
fn verify_token_env_present(
    vars: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    if vars
        .get("CLAUDE_CODE_OAUTH_TOKEN")
        .is_some_and(|v| !v.is_empty())
    {
        return Ok(());
    }
    anyhow::bail!(
        "auth_forward = \"token\" but CLAUDE_CODE_OAUTH_TOKEN is not set in the resolved \
         operator env.\n\
         \n\
         Add it in your workspace config under [env]. Either:\n\
         \n\
         - Reference a 1Password secret:\n\
           [env]\n\
           CLAUDE_CODE_OAUTH_TOKEN = \"op://vault/claude/token\"\n\
         \n\
         - Forward from the host shell:\n\
           [env]\n\
           CLAUDE_CODE_OAUTH_TOKEN = \"$CLAUDE_CODE_OAUTH_TOKEN\"\n\
         \n\
         Generate a token with `claude setup-token`, then either store it in \
         1Password (first form) or export it in your shell (second form)."
    );
}
```

- [ ] **Step 5.4: Call the resolver and hook the presence check into `load_agent_with`**

In `src/runtime/launch.rs`, find the block that currently resolves the auth mode and prints outcome notices (approximately lines 647–685 post-PR-1). Replace it with the following block.

Before the edit (post-PR-1 state), that block looks like:

```rust
        let auth_mode = config.resolve_auth_forward_mode(&selector.key());
        let (state, auth_outcome) = AgentState::prepare(
            paths,
            &container_name,
            &validated_repo.manifest,
            auth_mode,
            &paths.home_dir,
        )?;

        match auth_outcome {
            // ... Copied/Synced/HostMissing/Skipped arms ...
        }
```

Replace with:

```rust
        let auth_mode = config.resolve_auth_forward_mode(&selector.key());

        // Resolve the operator env NOW (pre-launch) so that the token
        // presence check and the launch-time env injection both see the
        // same resolved values. PR 2 owns this resolver; adjust the
        // call signature here if PR 2's final API differs.
        let resolved_operator_env =
            crate::env_resolver::resolve_operator_env(config, workspace, selector)?;

        if matches!(auth_mode, crate::config::AuthForwardMode::Token) {
            verify_token_env_present(&resolved_operator_env.vars)?;
        }

        let (state, auth_outcome) = AgentState::prepare(
            paths,
            &container_name,
            &validated_repo.manifest,
            auth_mode,
            &paths.home_dir,
        )?;

        // Diagnostic line: surface the active auth mode and, for token
        // mode, the source reference of CLAUDE_CODE_OAUTH_TOKEN.
        match auth_mode {
            crate::config::AuthForwardMode::Token => {
                let source_ref = auth_token_source_reference(&resolved_operator_env);
                tui::auth_mode_notice("token", Some(&source_ref));
            }
            crate::config::AuthForwardMode::Sync => {
                tui::auth_mode_notice("sync", None);
            }
            crate::config::AuthForwardMode::Ignore => {
                tui::auth_mode_notice("ignore", None);
            }
        }

        // Verbose outcome notices kept for operator context.
        match auth_outcome {
            crate::instance::AuthProvisionOutcome::Synced => {
                eprintln!(
                    "[jackin] Synced host Claude Code authentication into agent state \
                     (auth_forward=sync)."
                );
            }
            crate::instance::AuthProvisionOutcome::TokenMode => {
                eprintln!(
                    "[jackin] auth_forward=token — agent will use CLAUDE_CODE_OAUTH_TOKEN \
                     from the resolved env."
                );
            }
            crate::instance::AuthProvisionOutcome::HostMissing => match auth_mode {
                crate::config::AuthForwardMode::Sync => {
                    eprintln!(
                        "[jackin] auth_forward=sync but no host credentials found; \
                             preserving existing container auth if present."
                    );
                }
                crate::config::AuthForwardMode::Ignore
                | crate::config::AuthForwardMode::Token => {}
            },
            crate::instance::AuthProvisionOutcome::Copied
            | crate::instance::AuthProvisionOutcome::Skipped => {}
        }
```

Then add this helper function near `verify_token_env_present`:

```rust
/// Return a printable source reference for `CLAUDE_CODE_OAUTH_TOKEN`
/// from the resolved operator env. Falls back to the bare env-var name
/// if PR 2 did not attach a per-key source table, or if the key was
/// resolved from an unknown source.
fn auth_token_source_reference(
    resolved: &crate::env_resolver::ResolvedOperatorEnv,
) -> String {
    // If PR 2 exposed a source table, use it. The field name used here
    // (`sources`) is the assumed shape — adjust if PR 2 chose a
    // different name. If PR 2 did not ship source tracking at all,
    // delete the `if let` arm and fall through to the default string.
    #[allow(clippy::collapsible_match)]
    {
        // Introspection helper: attempt to look up the field by name.
        // If `sources` does not exist on ResolvedOperatorEnv, delete
        // this block and the function returns the default below.
        if let Some(src) = resolved
            .sources
            .get("CLAUDE_CODE_OAUTH_TOKEN")
            .map(ToString::to_string)
        {
            return format!("CLAUDE_CODE_OAUTH_TOKEN \u{2190} {src}");
        }
    }
    "CLAUDE_CODE_OAUTH_TOKEN".to_string()
}
```

> **Implementer note:** the `auth_token_source_reference` body depends on PR 2's exact field shape. If PR 2's `ResolvedOperatorEnv` does NOT expose a `sources` field, delete the inner `if let` block entirely — the function will always return the bare env-var name. The plan tolerates either shape. If PR 2 shipped a richer shape (e.g. per-key enum `EnvSource::OnePasswordRef(String) | EnvSource::HostEnvVar(String) | …`), add a small `match` here and format each variant idiomatically (`"op://…"`, `"$CLAUDE_CODE_OAUTH_TOKEN"`, etc.).

- [ ] **Step 5.5: Wire the resolved env through the existing env-injection site**

The existing code at the start of `launch_agent_runtime` (around line 447–462, `resolved_env` usage) still iterates `ctx.resolved_env.vars` for Docker `-e` flags. That path is the **agent manifest env resolver** (`crate::env_resolver::resolve_env`) — separate from the operator env. Keep that path untouched.

The operator-level env (`resolved_operator_env`) is what contains `CLAUDE_CODE_OAUTH_TOKEN`. Forward its contents into the container using the same `-e KEY=VALUE` flag mechanism.

In `src/runtime/launch.rs`, extend the `LaunchContext<'a>` struct (currently at lines 254–267) by adding one field:

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
    resolved_operator_env: &'a crate::env_resolver::ResolvedOperatorEnv,
    cache_dir: &'a std::path::Path,
}
```

Update the `LaunchContext { … }` literal inside `load_agent_with` (currently around line 693) to include `resolved_operator_env: &resolved_operator_env,`.

Inside `launch_agent_runtime`, after the existing `env_strings` loop (currently around lines 447–462), add a second loop that forwards the operator env. Replace the existing block:

```rust
    let mut env_strings: Vec<String> = Vec::new();
    env_strings.push(format!(
        "{}={}",
        crate::env_model::JACKIN_RUNTIME_ENV_NAME,
        crate::env_model::JACKIN_RUNTIME_ENV_VALUE
    ));
    for (key, value) in &resolved_env.vars {
        if crate::env_model::is_reserved(key) {
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }
```

with:

```rust
    let mut env_strings: Vec<String> = Vec::new();
    env_strings.push(format!(
        "{}={}",
        crate::env_model::JACKIN_RUNTIME_ENV_NAME,
        crate::env_model::JACKIN_RUNTIME_ENV_VALUE
    ));
    for (key, value) in &resolved_env.vars {
        if crate::env_model::is_reserved(key) {
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }
    // Operator-level env (PR 2): forwarded verbatim except for
    // reserved keys. CLAUDE_CODE_OAUTH_TOKEN travels through this path.
    for (key, value) in &resolved_operator_env.vars {
        if crate::env_model::is_reserved(key) {
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }
```

Also update the destructuring at the top of `launch_agent_runtime` (lines 276–289) to include `resolved_operator_env` alongside `resolved_env`.

> **Implementer note:** if the existing test helper `FakeRunner::for_load_agent` does not accommodate the extra env vars, the runner just records the command line — no additional fakes are needed. Existing tests that build `LaunchContext` directly (none do; all go through `load_agent_with`) would need updating, but the current suite reaches `launch_agent_runtime` only via `load_agent`.

- [ ] **Step 5.6: Run tests — confirm all green**

```bash
cargo nextest run -p jackin runtime::launch::tests
cargo nextest run
```

Expected: all green. Existing launch tests should continue to pass because `resolve_operator_env` on a manifest without `[env]` declarations returns an empty `vars` map, and no code path requires the token when `auth_mode != Token`.

> If any existing launch test fails because `resolve_operator_env` requires a populated config, make the minimal test-side change to ensure its config has an empty operator-env section (typically no-op — PR 2 should default to empty).

- [ ] **Step 5.7: Commit Task 5**

```bash
git add src/runtime/launch.rs
git commit -s -m "$(cat <<'EOF'
feat(runtime): enforce CLAUDE_CODE_OAUTH_TOKEN presence for auth_forward=token

Resolve the operator env at launch time via PR 2's resolve_operator_env.
When auth_forward == Token, fail fast with an actionable error if
CLAUDE_CODE_OAUTH_TOKEN is absent from the resolved env — the error
lists both remediation paths (1Password `op://` reference and
`$CLAUDE_CODE_OAUTH_TOKEN` host shell forwarding).

Surface the active auth mode on a single launch-time diagnostic line
("claude auth: OAuth token (CLAUDE_CODE_OAUTH_TOKEN <- <source>)" in
token mode, with matching one-liners for sync and ignore).

Forward the resolved operator env into the container as `-e KEY=VALUE`
flags alongside the existing agent manifest env so Claude Code inside
the container sees CLAUDE_CODE_OAUTH_TOKEN.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Documentation — Token mode section and accepted values

**Files:**
- Modify: `docs/src/content/docs/guides/authentication.mdx`
- Modify: `docs/src/content/docs/reference/configuration.mdx`
- Modify: `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`

- [ ] **Step 6.1: Update `authentication.mdx` — modes table and new Token section**

In `docs/src/content/docs/guides/authentication.mdx`, replace the "Auth forwarding modes" paragraph at line 25 with:

```markdown
jackin' supports three modes, configurable globally or per-agent:

| Mode | Behavior |
|---|---|
| `sync` (default) | When host auth exists, overwrite container auth on each launch. When host auth is absent, preserve existing container auth. |
| `ignore` | Never forward host auth. Revoke any previously forwarded credentials. The agent authenticates itself via `/login`. |
| `token` | Provision the agent with an empty state and inject `CLAUDE_CODE_OAUTH_TOKEN` (from the resolved operator env) into the container. Claude Code uses the token directly — no `/login` session state is bind-mounted. Recommended for long-lived or concurrent sessions. |
```

Immediately after the existing `### ignore` subsection (around line 49, before `## Configuration`), insert a new subsection:

```markdown
### token

The `token` mode is designed for long-lived and concurrent jackin' sessions. Instead of bind-mounting a copy of your host's `/login` session state, jackin' injects a long-lived OAuth token (`CLAUDE_CODE_OAUTH_TOKEN`) into the container's process env. Claude Code inside the container reads the token directly, bypassing the file-based credential store entirely.

Setup:

1. On the host, generate a token:

   ```bash
   claude setup-token
   ```

2. Tell jackin' where to find it by adding a line to the `[env]` section of the workspace manifest. Either reference a 1Password secret:

   ```toml
   [env]
   CLAUDE_CODE_OAUTH_TOKEN = "op://vault/claude/token"
   ```

   or forward from the host shell:

   ```toml
   [env]
   CLAUDE_CODE_OAUTH_TOKEN = "$CLAUDE_CODE_OAUTH_TOKEN"
   ```

3. Enable token mode:

   ```bash
   jackin config auth set token
   ```

On launch, jackin' prints a one-line diagnostic confirming the active mode and where the token came from:

```
claude auth: OAuth token (CLAUDE_CODE_OAUTH_TOKEN ← op://vault/claude/token)
```

If `CLAUDE_CODE_OAUTH_TOKEN` is not present in the resolved env when `token` mode is active, jackin' aborts with an actionable error listing both remediation paths above.

The agent's state directory is provisioned with the same empty shape as `ignore` mode — no `.credentials.json` is written — so switching between `token`, `sync`, and `ignore` is safe.
```

Replace the `config.toml` example at line 80–92 with:

```toml
# Global default
[claude]
auth_forward = "sync"

# Per-agent override
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[roles.agent-smith.claude]
auth_forward = "token"
```

Replace the resolution-order note at line 94 with:

```markdown
Resolution order: **per-agent override > global default > `sync`**.
```

(No change from PR 1 — kept for completeness.)

- [ ] **Step 6.2: Update `configuration.mdx` — accepted values**

In `docs/src/content/docs/reference/configuration.mdx`, replace the Claude Code settings example at lines 20–27 with:

```markdown
```toml
[claude]
auth_forward = "sync"  # "sync" (default), "ignore", or "token"
```

| Field | Description | Default |
|---|---|---|
| `auth_forward` | How the host's Claude Code credentials are made available to agent containers. See [Authentication Forwarding](/guides/authentication). | `sync` |
```

Leave the per-agent override example (lines 31–34) untouched; it still reads `auth_forward = "ignore"`, which remains valid.

- [ ] **Step 6.3: Update the roadmap — mark token mode delivered**

In `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`, replace the "Current State" section (lines 38–53) with:

```markdown
## Current State

Status as of the Token-mode release:

- `sync` — overwrite container auth from host on each launch when host auth exists (default since the `sync`-default release)
- `ignore` — never forward host auth; require in-container `/login`
- `token` — inject `CLAUDE_CODE_OAUTH_TOKEN` from the resolved operator env and leave the agent state directory empty; recommended for long-lived or concurrent sessions (delivered in PR 3 of this series)

Configs that still declare `auth_forward = "copy"` are migrated to `sync` on load with a deprecation notice (delivered in PR 1 of this series).
```

Replace the "Recommendation" section (lines 163–176) with:

```markdown
## Recommendation

Delivered (three-PR series):

1. `sync` is the default; `copy` is deprecated and auto-migrated.
2. A workspace-scoped env resolver populates `CLAUDE_CODE_OAUTH_TOKEN` from `op://` references or host env vars.
3. `token` mode uses that resolved env instead of bind-mounting `/login` session state. Token mode is now the recommended choice for long-lived or concurrent jackin sessions; `sync` remains a good default for ad-hoc single-session usage.
```

- [ ] **Step 6.4: Verify docs site still builds**

```bash
cd docs
bun install --frozen-lockfile
bun run build 2>&1 | tail -20
cd ..
```

Expected: build succeeds. If `bun install` reports an OS-mismatch on `node_modules`, follow the remediation in `docs/AGENTS.md` (`rm -rf node_modules && bun install --frozen-lockfile`).

- [ ] **Step 6.5: Commit Task 6**

```bash
git add docs/src/content/docs/guides/authentication.mdx \
  docs/src/content/docs/reference/configuration.mdx \
  docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx
git commit -s -m "$(cat <<'EOF'
docs(auth): document token mode

Add the Token mode section to the authentication guide, with setup
instructions (op:// and $CLAUDE_CODE_OAUTH_TOKEN forms), the expected
launch-time diagnostic line, and the actionable error shape when the
token is missing.

Mark token mode as delivered in the Current State and Recommendation
sections of the Claude auth strategy roadmap.

List `token` as an accepted value for `auth_forward` in the
configuration reference.

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
head -20 CHANGELOG.md
```

- [ ] **Step 7.2: Add an `Added` entry under `## [Unreleased]`**

Under `## [Unreleased]`, add (following keep-a-changelog style and merging with any existing `Added` heading):

```markdown
### Added

- `auth_forward = "token"` — new Claude Code authentication mode that injects `CLAUDE_CODE_OAUTH_TOKEN` (resolved through the workspace env pipeline) into the container instead of bind-mounting `/login` session state. Recommended for long-lived or concurrent jackin sessions. Launch fails fast with an actionable error if the token is missing from the resolved env; the launch summary includes a one-line diagnostic naming the active mode and, for token mode, the source of the token (`op://…`, `$CLAUDE_CODE_OAUTH_TOKEN`, etc.). See the authentication guide for setup. [#<pr-number>]
```

Leave `<pr-number>` as a literal placeholder for now; it will be filled in once the PR is opened.

- [ ] **Step 7.3: Commit Task 7**

```bash
git add CHANGELOG.md
git commit -s -m "$(cat <<'EOF'
docs(changelog): record token auth_forward mode

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

- [ ] **Step 8.2: Manual smoke — token mode fails fast without the env var**

```bash
# Prepare a workspace that does NOT declare CLAUDE_CODE_OAUTH_TOKEN in [env].
# Then set auth_forward = "token" globally and attempt to launch.
cargo run -- config auth set token
cargo run -- launch 2>&1 | head -30 || true
```

Expected: the launch aborts with the actionable error listing both remediation paths (`op://vault/claude/token` and `$CLAUDE_CODE_OAUTH_TOKEN`).

- [ ] **Step 8.3: Manual smoke — token mode launches cleanly with the env var set**

Add a line to the workspace manifest:

```toml
[env]
CLAUDE_CODE_OAUTH_TOKEN = "$CLAUDE_CODE_OAUTH_TOKEN"
```

Export the token on the host, then launch:

```bash
export CLAUDE_CODE_OAUTH_TOKEN="sk-ant-oat01-your-real-token"
cargo run -- launch 2>&1 | head -30
```

Expected: launch output contains `claude auth: OAuth token (CLAUDE_CODE_OAUTH_TOKEN ← $CLAUDE_CODE_OAUTH_TOKEN)` (or the equivalent source reference format for your PR 2 implementation). Inside the container, `echo $CLAUDE_CODE_OAUTH_TOKEN` returns the token value.

- [ ] **Step 8.4: Manual smoke — config shape**

```bash
cargo run -- config auth show
grep "auth_forward" ~/.config/jackin/config.toml
```

Expected: both report `token`. The on-disk value is written as the canonical `"token"` string.

- [ ] **Step 8.5: Verify commit log is clean and DCO-signed**

```bash
git log main..HEAD --oneline
git log main..HEAD --format="%B" | grep -c "Signed-off-by"
git log main..HEAD --format="%B" | grep -c "Co-authored-by: Claude"
```

Expected: 7 commits (Task 1 through Task 7). `Signed-off-by` count = 7. `Co-authored-by: Claude` count = 7.

- [ ] **Step 8.6: Push and open the PR**

```bash
git push -u origin feature/claude-token-auth-mode
gh pr create --title "feat(auth): add token auth_forward mode (CLAUDE_CODE_OAUTH_TOKEN)" --body "$(cat <<'BODY'
## Summary

Adds a third `auth_forward` mode — `token` — that injects `CLAUDE_CODE_OAUTH_TOKEN` into the container instead of bind-mounting `/login` session state. Recommended for long-lived or concurrent jackin sessions.

- Token is resolved through PR 2's workspace env pipeline (`op://` secrets or host env vars both work).
- The agent state directory is provisioned with the empty `ignore` shape; switching between `token`, `sync`, and `ignore` is safe.
- Launch fails fast with an actionable error when `auth_forward = "token"` but the token is not resolvable, listing both remediation paths.
- Launch output gains a one-line diagnostic: `claude auth: OAuth token (CLAUDE_CODE_OAUTH_TOKEN ← <source>)`, with matching one-liners for `sync` and `ignore`.

Delivers PR 3 of the three-PR Claude auth strategy series, closing the roadmap item in `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`.

Depends on PR 1 (sync-default + copy migration) and PR 2 (workspace env resolver) both being merged.

Spec: `docs/superpowers/specs/2026-04-23-claude-token-auth-mode-design.md`. Plan: `/tmp/plan-pr3.md` (to be moved under `docs/superpowers/plans/` by the implementer on branch creation if the repo convention applies).

## Test plan

- [x] `cargo fmt -- --check && cargo clippy && cargo nextest run` — all green, zero warnings.
- [x] Unit tests for `AuthForwardMode::Token` parse/display/deserialize.
- [x] Unit tests for all four mode transitions across `token`: sync→token, token→sync, token→ignore, ignore→token.
- [x] Unit tests for `verify_token_env_present` covering present and missing cases.
- [x] Unit tests for `format_auth_mode_notice_for_test` across all three modes.
- [x] Manual: token mode aborts with the actionable error when `CLAUDE_CODE_OAUTH_TOKEN` is missing from the resolved env.
- [x] Manual: token mode launches successfully with the env var set; the launch diagnostic surfaces the source.
- [x] Manual: docs site builds (`bun run build`).
- [ ] Reviewer confirms the token source-reference format matches PR 2's `ResolvedOperatorEnv` field shape (see Assumptions section of the plan).
- [ ] Reviewer confirms no token value is ever logged or persisted.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
BODY
)"
```

- [ ] **Step 8.7: STOP — do not merge**

Return the PR URL to the operator. Per `AGENTS.md`, agents must never merge a PR without explicit per-PR operator confirmation. A phrase like "proceed" or "looks good" does NOT authorize merge — only "merge it" or "ship it" does, and even then confirm with the operator before using `--admin`.

---

## Self-Review Checklist (for the implementer)

Before marking this plan complete:

- [ ] No remaining references to the removed `Copy` arm or provisioning outcome (grep: `rg 'AuthForwardMode::Copy|auth_forward = "copy"' src/` — must be zero, with the single exception of the PR 1 deprecation alias in `FromStr`).
- [ ] `AuthProvisionOutcome::TokenMode` is matched in every site that switches on the outcome enum — the compiler will enforce this, but double-check `src/runtime/launch.rs` and any TUI output path.
- [ ] The token value itself never appears in any log line, `eprintln!`, launch summary, or persisted file. Only the **source reference** (e.g. `"op://vault/claude/token"`) is surfaced.
- [ ] `CLAUDE_CODE_OAUTH_TOKEN` is added to the container env via the operator-env forwarding loop, not hardcoded or duplicated.
- [ ] The actionable error message produced by `verify_token_env_present` contains both remediation paths (`op://` and `$CLAUDE_CODE_OAUTH_TOKEN`) exactly as the spec requires.
- [ ] Docs build cleanly (`bun run build`).
- [ ] All seven commits carry both `Signed-off-by:` and `Co-authored-by: Claude <noreply@anthropic.com>`.
- [ ] Pre-commit gate is clean on every commit, not just the final one (run after each task; fix fails before moving on).
- [ ] Manual smokes from Steps 8.2 and 8.3 succeeded on the implementer's machine.
- [ ] `CHANGELOG.md` PR-number placeholder has been replaced with the actual PR number after the PR is opened.
- [ ] The `auth_token_source_reference` helper has been adjusted (or simplified) to match PR 2's actual `ResolvedOperatorEnv` shape.

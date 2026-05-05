# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]

### Changed

- **BREAKING.** Auth-forward configuration moved out of `[roles.<role>.claude]` and into per-agent blocks at three layers (`[claude]` global, `[workspaces.<ws>.claude]`, `[workspaces.<ws>.roles.<role>.claude]`). The same shape now exists for `[codex]`. Stale configs that still set `[roles.<role>.claude]` fail to parse with the standard "unknown field" error. No migration shim — see AGENTS.md "Project status: pre-release".
- New `api_key` mode for both Claude and Codex (`ANTHROPIC_API_KEY` / `OPENAI_API_KEY` from the env layer).
- `Token` mode renamed to `oauth_token`. The deprecated `"copy"` alias is removed.
- The `jackin config auth set --role` flag is removed; the new layered overrides are configured via the TUI Auth panel (or hand-edited TOML).

### Added

- Auth panel in the workspace-manager TUI, peer to the Secrets tab. Lets operators set the mode per (workspace × role × agent) with 1Password integration and form-level validation that prevents saving a mode without a resolved credential.
- Structured `LaunchError::AuthCredentialMissing` with full mode-resolution and env-layer-state arrays for clear diagnostic output when a launch fails because the required credential isn't set.
- 1Password picker integration in the auth form: picking a reference triggers `op read` validation before commit; broken references never reach disk.

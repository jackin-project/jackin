# Auth Forward — `sync` as Default, Deprecate `copy`

**Status:** Proposed
**Date:** 2026-04-23
**Scope:** `jackin` crate only
**PR:** 1 of 3 in the Claude auth strategy series

## Problem

The default `auth_forward = "copy"` mode seeds host Claude OAuth credentials into a container's private state directory on first creation and never refreshes them. When refresh tokens rotate across concurrent Claude Code sessions (host, multiple jackin containers, clones), the forwarded copy silently drifts until the container hits `API Error: 401 Invalid authentication credentials`.

`sync` already exists and is coherent: it overwrites container auth from host on each launch when host auth is present, and preserves container auth when host auth is absent. It does not solve mid-session drift (that is PR 3's scope), but it eliminates the "container carries week-old copied credentials" failure mode that dominates current reports.

The deferred roadmap at `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx` identifies this and recommends `sync` as the better day-to-day default while a token-based path is designed.

## Goals

1. Make `sync` the default for new installs.
2. Migrate existing configs that declare `auth_forward = "copy"` (global or per-agent) to `sync` automatically on first launch after upgrade, with a single-line operator-visible notice.
3. Keep the `"copy"` string accepted as a parser-level deprecated alias so scripts, dotfiles, and docs that still pass it do not fail.
4. Remove the `AuthForwardMode::Copy` enum variant so the provisioning code path collapses to `Sync`, `Ignore`, and the future `Token`. No dead code.
5. Surface the deprecation clearly in CLI, docs, and CHANGELOG.

## Non-Goals

- Solving mid-session OAuth refresh drift. That is explicitly PR 3 (`auth_forward = "token"`).
- Changing `sync` semantics. Everything `sync` does today stays the same.
- Adding new auth modes, storage mechanisms, or secret resolution. Those are PR 2 and PR 3.
- Rewriting operator configs beyond the `auth_forward` fields. The migration touches only the two keys it owns.

## Design

### Config model

`AuthForwardMode` loses its `Copy` variant:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthForwardMode {
    Ignore,
    #[default]
    Sync,
    // Token variant added in PR 3
}
```

`Display` and `FromStr` drop `Copy`. A custom `Deserialize` (or a manual `FromStr` + serde `try_from`) accepts the string `"copy"` and resolves it to `Self::Sync`, carrying a flag so the caller can detect that a migration is needed.

### Migration behavior

At config load (`AppConfig::load` in `src/config/persist.rs`), after successful deserialization, walk both:

- `config.claude.auth_forward` (global)
- `config.agents[*].claude.auth_forward` (per-agent)

Any field whose raw TOML value was the string `"copy"` is both:
1. Normalized in memory to `Sync` — already handled by the serde path, no extra step.
2. Marked as "needs rewrite" so the loader re-serializes the config back to disk after load.

To preserve idempotency: if no `copy` values were seen, do not rewrite the file. The on-disk file only changes when there is actually something to migrate.

Implementation shape: deserialize into an intermediate shape that preserves the raw string alongside the canonical variant, then collapse after detection. Alternatively, run a lightweight pre-pass over the raw TOML document (`toml_edit`) that edits the two known key paths before handing the result to serde — this preserves operator comments and formatting, which is important because jackin already uses `toml_edit` for `AppConfig::save`.

### Operator-visible notice

When a migration occurs, print once per launch to stderr:

```
jackin: migrated auth_forward "copy" → "sync" in ~/.config/jackin/config.toml (copy is deprecated)
```

Use the same `tui::warning` or `tui::hint` helper path used elsewhere (`src/tui/output.rs`) so it respects debug-mode and TTY rules. No color gymnastics — a single diagnostic line.

### CLI behavior

`jackin config auth set copy` accepts the input but:
1. Prints `warning: "copy" is deprecated; saving as "sync"` to stderr.
2. Writes `auth_forward = "sync"` to the config.

`jackin config auth show` reports the effective mode (which will be `sync` after any migration).

Help text for `jackin config auth set` is updated:
- Listed modes become `sync | ignore | token` (once PR 3 lands) with a "*(deprecated)*" annotation on `copy`.
- The `sync` help text gets a new line noting it is now the default.

### Provisioning path

`AgentState::provision_claude_auth` in `src/instance/auth.rs` loses its `Copy` match arm. No other behavior changes. The two existing match arms (`Ignore`, `Sync`) stay as-is.

All tests under `src/instance/auth.rs::tests` that reference `AuthForwardMode::Copy` are rewritten to use `Sync` where the behavior being verified is shared, and deleted where the test was specifically about `Copy`'s "never overwrite" semantics (which is the property we are removing).

### Documentation changes

- `docs/src/content/docs/guides/authentication.mdx`: mode table drops `copy` (or marks it strikethrough with a deprecation note pointing to the changelog), `sync` moves to the top and is labelled the default.
- `docs/src/content/docs/reference/configuration.mdx`: `auth_forward` default updated.
- `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`: "Current State" section updated to note that `copy` is deprecated and `sync` is now the default; roadmap doc itself stays open because PR 3 still has work.
- `CHANGELOG.md`: one entry under `### Changed`, one under `### Deprecated`, called out as a behavior change for existing operators.

## Failure Modes

- **Unknown `auth_forward` value in config**: unchanged — `FromStr` still rejects with the existing error (`invalid auth_forward mode {other:?}; expected one of: sync, ignore`).
- **Config file is read-only when migration tries to rewrite**: emit a warning that the in-memory migration succeeded but the on-disk rewrite failed, with a hint to re-run with writable permissions. Launch continues with the in-memory `Sync` value.
- **Partial config (e.g. per-agent copy but global missing)**: migrate each field independently. No cross-field dependencies.

## Security Notes

No new credential handling. The deprecation moves operators to `sync`, which still bind-mounts `.credentials.json` with `0600` and all existing symlink/permission defenses apply unchanged.

## Test Plan

- Unit: `AuthForwardMode::from_str("copy")` returns `Sync` and sets the deprecation flag.
- Unit: loader writes back a migrated config and leaves a non-copy config untouched.
- Unit: `jackin config auth set copy` writes `sync` and prints the deprecation warning.
- Unit: all existing `sync`/`ignore` behavior tests still pass against the shrunk enum.
- Integration: pre-commit (`cargo fmt --check && cargo clippy && cargo nextest run`) is clean.

## File-Level Change Map

| File                                                                  | Change                                                                   |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `src/config/mod.rs`                                                   | drop `Copy` variant; default becomes `Sync`; custom deserializer for `"copy"` alias |
| `src/config/persist.rs`                                               | detect + rewrite migrated configs via `toml_edit` pass                   |
| `src/instance/auth.rs`                                                | drop `Copy` match arm; update tests                                      |
| `src/cli/config.rs`                                                   | help text + `set copy` deprecation warning                               |
| `src/app/mod.rs`                                                      | config-auth dispatch: warn on `copy` input                               |
| `src/tui/output.rs`                                                   | possibly add a small `deprecation_warning` helper if one does not exist  |
| `docs/src/content/docs/guides/authentication.mdx`                     | update mode table and prose                                              |
| `docs/src/content/docs/reference/configuration.mdx`                   | default updated                                                          |
| `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx`    | status update in "Current State"                                         |
| `CHANGELOG.md`                                                        | `Changed` + `Deprecated` entries                                         |

## Open Questions

None. The migration shape, CLI behavior, and enum surgery were agreed in brainstorming.

## Related

- Roadmap: `docs/src/content/docs/reference/roadmap/claude-auth-strategy.mdx` — originating design.
- PR 2 (`2026-04-23-workspace-env-resolver-design.md`) — independent; no dependency either direction.
- PR 3 (`2026-04-23-claude-token-auth-mode-design.md`) — depends on this PR's slimmed enum and on PR 2's env resolver.

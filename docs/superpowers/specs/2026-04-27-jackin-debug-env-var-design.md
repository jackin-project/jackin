# `JACKIN_DEBUG` Environment Variable for Sticky Debug Output

**Status:** Proposed
**Date:** 2026-04-27
**Scope:** `jackin` crate only

## Problem

Operators who routinely want raw container output for troubleshooting must pass `--debug` on every invocation: `jackin --debug`, `jackin load --debug`, `jackin console --debug`. There is no way to make debug-by-default sticky for a shell session or user account, so the flag gets added and forgotten repeatedly.

A natural unix solution is an environment variable: set it once in `~/.zshrc` / `~/.bashrc` / a CI job, and every `jackin` invocation in that environment defaults to debug-on, with the existing `--debug` flag still available for explicit on/off.

## Goals

1. Add a `JACKIN_DEBUG` environment variable that, when set to a truthy value, makes `--debug` default to `true` for every command that currently accepts `--debug`.
2. Keep the existing `--debug` CLI flag working unchanged. CLI-flag-present continues to mean "debug on".
3. Allow per-invocation override via `JACKIN_DEBUG=0 jackin ...` (or `unset JACKIN_DEBUG`).
4. Surface the env var in `--help` output for the affected commands so it is discoverable through normal CLI exploration.
5. Treat both `LoadArgs.debug` and `ConsoleArgs.debug` consistently — both currently parse `--debug` and both must respect the env var.

## Non-Goals

- A config-file-based debug toggle (`jackin config debug enable`). The brainstorming concluded this is over-engineering for a developer-preference verbosity knob: env vars match the unix idiom (`RUST_LOG`, `NO_COLOR`) and avoid a new TOML field, a new `ConfigCommand` arm, an editor method, dispatch wiring, and migration concerns.
- A `--no-debug` CLI flag. clap's `ArgAction::SetTrue` does not support a CLI-level "force off" without restructuring to `ArgAction::Set` with a `bool` value parser. The standard env-prefix override (`JACKIN_DEBUG=0 jackin load`) is sufficient for the rare case of disabling for one command in a shell that has it sticky.
- Renaming or repurposing the existing `--debug` flag.
- Introducing additional `JACKIN_*` env vars for other flags. This is a single-flag change; broader env-driven config is a separate design.
- Logging-framework changes (level filtering, structured logs, `RUST_LOG` integration). The existing `--debug` behavior is "print raw container output for troubleshooting"; this design only changes its default, not what it does.

## Design

### Where the change lands

Two struct fields, both in `src/cli/agent.rs`:

- `LoadArgs.debug` at `src/cli/agent.rs:48`
- `ConsoleArgs.debug` at `src/cli/agent.rs:85`

Each gets `env = "JACKIN_DEBUG"` added to its `#[arg(...)]` attribute.

```rust
// src/cli/agent.rs (LoadArgs)
/// Print raw container output for troubleshooting
#[arg(long, env = "JACKIN_DEBUG", default_value_t = false)]
pub debug: bool,
```

```rust
// src/cli/agent.rs (ConsoleArgs)
/// Print raw container output for troubleshooting
#[arg(long, env = "JACKIN_DEBUG", default_value_t = false)]
pub debug: bool,
```

`ConsoleArgs` is `#[command(flatten)]`'d into `Cli` in `src/cli/mod.rs:55-56`, so `jackin --debug` (without a subcommand) inherits the env-backed flag automatically — no separate change needed at the top level.

### Precedence

clap's resolution order, given the attribute above, is:

1. CLI flag present (`--debug`) → `true`
2. CLI flag absent, env var set to a truthy value → `true`
3. CLI flag absent, env var set to a falsy value → `false`
4. CLI flag absent, env var unset → `default_value_t` → `false`

Truthy: `1`, `true`, `yes`, `on`, and most other non-empty strings.
Falsy: `0`, `false`, `no`, `off`, `f`, `n`, empty string.

This is the standard `FalseyValueParser` semantics clap applies to `ArgAction::SetTrue` flags backed by an env var — no custom parser needed.

### Help-text surface

clap automatically appends `[env: JACKIN_DEBUG=]` to the rendered help for any flag with an `env =` attribute. The existing per-command help banners (the `BANNER` const + Examples section in `src/cli/agent.rs`) need no edits — the env note threads in beside the flag description on its own.

Existing tests in `src/cli/mod.rs` and `src/cli/agent.rs` that assert help-text contents (e.g. "all subcommand help pages show banner") do not need updating: they check banner / examples presence, not the exact flag-block layout.

### Module dispatch

No changes to `src/cli/dispatch.rs`, `src/app/`, or `src/runtime/`. The `debug: bool` value already flows from the `Args` struct through to the runner; this change only affects how the bool is *populated*, not how it is *consumed*.

### Tests

Add to `src/cli/agent.rs`'s existing `#[cfg(test)] mod tests`:

1. `parses_load_with_jackin_debug_env_truthy` — set `JACKIN_DEBUG=1`, parse `["jackin", "load"]`, assert `debug: true`.
2. `parses_load_with_jackin_debug_env_falsy` — set `JACKIN_DEBUG=0`, parse `["jackin", "load"]`, assert `debug: false`.
3. `cli_debug_flag_overrides_falsy_env` — set `JACKIN_DEBUG=0`, parse `["jackin", "load", "--debug"]`, assert `debug: true`. Locks in clap's "CLI > env" precedence as our intended contract.
4. `bare_jackin_with_jackin_debug_env` — set `JACKIN_DEBUG=1`, parse `["jackin"]`, assert `cli.console_args.debug == true`. Locks in that the flattened top-level form picks up the env var too.
5. `console_subcommand_with_jackin_debug_env` — set `JACKIN_DEBUG=1`, parse `["jackin", "console"]`, assert `debug: true`. Locks in symmetry between `jackin --debug` and `jackin console --debug`.

Tests that mutate process env need `serial_test` or equivalent isolation. Check whether the crate already uses `serial_test` (search for it in `Cargo.toml`); if not, gate env-mutating tests with `#[serial]` from a small new dev-dep, or use `temp-env` for scoped overrides. Prefer `temp-env` if introducing a new dep — it does not require ordering all env-touching tests, only the ones that mutate.

The implementation plan will pick the env-isolation crate after surveying `Cargo.toml`. Either choice is fine; the spec only requires that env-mutating tests do not race with each other or with concurrent unrelated tests.

### Documentation

Help text auto-update (via clap's `[env: JACKIN_DEBUG=]` annotation) is the primary discoverability surface and is sufficient for the spec.

A one-paragraph mention in the operator console docs page (`docs/src/content/docs/commands/console.mdx`) advertising the env var is desirable but is **not required** by this spec — it can ship as a docs-only follow-up. The spec does not block on docs.

### CHANGELOG

A single entry under the next-release section of `CHANGELOG.md`, e.g.:

```
### Added
- `JACKIN_DEBUG=1` environment variable makes `--debug` sticky across invocations
  for commands that accept it (`jackin`, `jackin load`, `jackin console`).
  CLI flag still takes precedence; falsy values (`0`, `false`, `no`) keep debug off.
```

## Risk & rollout

- **Low blast radius.** The change is two attribute additions on existing flags. Default behavior (env unset) is unchanged.
- **No migration.** No config file fields added or removed.
- **Backwards compatible.** Operators who do not set `JACKIN_DEBUG` see no behavior change. Operators who already set the var (none, since it is new) get debug-on default.
- **Test isolation matters.** Concurrent cargo tests share process env. The implementation must scope env mutation to avoid flakiness in the suite — see the Tests section above.

## Out-of-scope follow-ups

- Adding a docs-site page or section for jackin's environment variables (`JACKIN_DEBUG`, plus any future `JACKIN_*` knobs).
- Considering a `JACKIN_LOG` or similar leveled-logging knob if/when jackin grows structured logs beyond the binary `--debug` flag.
- A `--no-debug` flag, if operator demand for fine-grained per-invocation override of a sticky env var ever materializes.

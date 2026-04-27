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

Each gets `env = "JACKIN_DEBUG"` added to its `#[arg(...)]` attribute, plus an explicit `action` and `value_parser`:

```rust
// src/cli/agent.rs (LoadArgs and ConsoleArgs)
/// Print raw container output for troubleshooting
#[arg(
    long,
    env = "JACKIN_DEBUG",
    action = clap::ArgAction::SetTrue,
    value_parser = clap::builder::FalseyValueParser::new(),
)]
pub debug: bool,
```

The two overrides are deliberate. clap derive's default for a `bool` field is `ArgAction::Set` paired with `BoolValueParser` — fine for CLI, but rejects env values like `JACKIN_DEBUG=1` because `BoolValueParser` only accepts the literal strings `"true"` / `"false"`. Forcing `SetTrue` makes `--debug` a presence flag again, and `FalseyValueParser` makes env truthy/falsy strings (`1`, `0`, `yes`, `no`, empty) parse the way an operator would expect.

This also means the original spec's `default_value_t = false` is dropped — `SetTrue` already defaults to `false` when the flag is absent and the env is unset, and `default_value_t` would conflict with the SetTrue action.

clap's `env` attribute requires the `env` feature on the dependency. `Cargo.toml` is updated to enable it (`features = ["derive", "color", "env"]`).

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

Implementation discovered that `unsafe_code = "forbid"` (in `[lints.rust]`) rules out `std::env::set_var` / `remove_var` in unit tests, and adding a dev-dep wrapper crate (`temp-env`, `serial_test`) just to mutate process env is overkill for what's a single env-backed flag. Each subprocess gets its own env, so the integration coverage in <RepoFile path="tests/cli_debug_env.rs" /> uses `assert_cmd::Command` with explicit `.env(...)` / `.env_remove(...)` per test:

1. `help_annotation::load_help_advertises_jackin_debug` — `jackin load --help` output contains the literal substring `[env: JACKIN_DEBUG=`. Proves the binding is attached to `--debug` on `LoadArgs`.
2. `help_annotation::console_help_advertises_jackin_debug` — same for `jackin console --help` (`ConsoleArgs`).
3. `help_annotation::top_level_help_advertises_jackin_debug` — same for `jackin --help` (proves the flattened top-level form inherits the env binding).
4. `env_does_not_break_parsing::jackin_debug_truthy_does_not_break_console_parse` — `JACKIN_DEBUG=1 jackin console` reaches the existing non-TTY error (`CONSOLE_REQUIRES_TTY_ERROR`). Proves clap parses the env value cleanly under the `FalseyValueParser`.
5. `env_does_not_break_parsing::jackin_debug_falsy_does_not_break_console_parse` — same for `JACKIN_DEBUG=0`.
6. `env_does_not_break_parsing::jackin_debug_empty_does_not_break_console_parse` — same for `JACKIN_DEBUG=` (empty string, common in CI).

clap's actual value-resolution semantics for `ArgAction::SetTrue` env vars are clap's contract and not retested here.

#### Existing unit tests adjusted

Existing tests in `src/cli/agent.rs` and `src/cli/dispatch.rs` that destructured `ConsoleArgs { debug: false }` or `LaunchArgs { debug: false }` as an incidental "default" check were updated to match `{ .. }` instead. Reason: with env-backed `--debug`, the field's default depends on the runner's process env (`JACKIN_DEBUG=1` in the operator's shell would otherwise break those unit tests). The tests still assert the routing behavior they were originally testing — they just no longer pin a value that is now environment-dependent. Tests that explicitly assert `debug: true` after passing `--debug` are unchanged: CLI flag wins over env, so the assertion holds regardless of `JACKIN_DEBUG`.

### Documentation

Help text auto-update (via clap's `[env: JACKIN_DEBUG=]` annotation) is the primary discoverability surface and is sufficient for the spec.

A one-paragraph mention in the operator console docs page (`docs/src/content/docs/commands/console.mdx`) advertising the env var is desirable but is **not required** by this spec — it can ship as a docs-only follow-up. The spec does not block on docs.

### CHANGELOG

Not updated by this PR. The operator backfills `CHANGELOG.md` by hand at release time. Roadmap entry under <RepoFile path="docs/src/content/docs/reference/roadmap/jackin-debug-env-var.mdx" /> serves as the user-facing record of the change instead.

## Risk & rollout

- **Low blast radius.** The change is two attribute additions on existing flags. Default behavior (env unset) is unchanged.
- **No migration.** No config file fields added or removed.
- **Backwards compatible.** Operators who do not set `JACKIN_DEBUG` see no behavior change. Operators who already set the var (none, since it is new) get debug-on default.
- **Test isolation matters.** Concurrent cargo tests share process env. The implementation must scope env mutation to avoid flakiness in the suite — see the Tests section above.

## Out-of-scope follow-ups

- Adding a docs-site page or section for jackin's environment variables (`JACKIN_DEBUG`, plus any future `JACKIN_*` knobs).
- Considering a `JACKIN_LOG` or similar leveled-logging knob if/when jackin grows structured logs beyond the binary `--debug` flag.
- A `--no-debug` flag, if operator demand for fine-grained per-invocation override of a sticky env var ever materializes.

# jackin-core

Universal vocabulary types shared across **every** jackin❯ crate. This is the leaf at the bottom of the workspace dependency graph: no jackin❯ dependencies, no `tokio`, no subprocess, no filesystem, no presentation.

## What this crate owns

- Domain nouns every other crate speaks in: agent identity, instance, isolation, manifest fragments, env model, status, launch progress, operator notices.
- Port traits and shared abstractions higher crates implement (e.g. `CommandRunner`), plus the constants, paths, and selector/url/path text helpers reused everywhere.
- Small self-contained widgets/ansi/host-color tokens re-exported by presentation crates.

Because everything depends on `jackin-core`, it must stay dependency-free, side-effect-free, and cheap to compile. Anything that needs `tokio`, the filesystem, a subprocess, or a real adapter belongs in a higher crate.

## Architecture tier and allowed dependencies

**L0 leaf/domain.** Allowed workspace dependencies: **none**. No `tokio`, no I/O, no presentation. This is the floor; nothing in `jackin-core` may depend upward.

## Structure

Grouped by concern (top-level modules under `src/`):

- Identity & model — `agent`, `instance`, `manifest`, `status`, `operator_notice`, `auth`, `account_key`
- Environment — `env_model`, `env_value`, `paths`
- Isolation & git — `isolation`, `isolation_record`, `worktree_dirty`
- Runtime ports & progress — `runner`, `launch_progress`, `prompt_result`, `selector`
- Docker surface — `docker`, `docker_security`
- Observability (stubs re-exported from `jackin-diagnostics`/`jackin-tui`) — `debug_log`, `build_log_sink`
- Presentation tokens (re-exported by `jackin-tui`) — `host_colors`, `ansi_tokens`, `tui_widgets`, `standalone_dialog`, `url_text`, `path_text`
- Shared op/CLI vocabulary — `op_cache`, `op_reference`, `op_types`, `constants`

## Public API

The crate is a vocabulary library: re-export the types/ports/constants you need from `jackin_core::…`. Higher crates implement the port traits defined here (e.g. `CommandRunner`) and pass the domain types through.

## How to verify

```sh
cargo nextest run -p jackin-core
cargo clippy -p jackin-core --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.

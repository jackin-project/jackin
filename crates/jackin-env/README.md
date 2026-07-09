# jackin-env

Operator-environment resolution and 1Password (`op`) CLI integration. Turns declared operator env into resolved values, runs the `op` CLI for secret lookups, and feeds the resolved env layer into runtime launch.

## What this crate owns

- The `operator_env` resolution stack (`env_layer`, `env_resolver`, `resolve`) and the `op` CLI bridge (`op_cli`, `op_runner`, `op_struct`, `token_setup`).
- The interactive 1Password secret picker (`picker`) and host-side Claude env wiring (`host_claude`).
- Output/parse helpers and `test_support` for deterministic env fixtures.

## Architecture tier and allowed dependencies

**L1 application.** Allowed workspace dependencies: `jackin-core`, `jackin-config`, `jackin-protocol`, `jackin-diagnostics`. Stays below presentation so `jackin-launch-tui`/`jackin-console` can consume it.

## Structure

- `src/env_resolver.rs`, `src/env_layer.rs`, `src/resolve.rs` — resolution stack
- `src/op_cli.rs`, `src/op_runner.rs`, `src/op_struct.rs`, `src/token_setup.rs` — 1Password `op` bridge
- `src/picker.rs` — secret picker model (the pure planning half lives in `jackin-console-oppicker`)
- `src/host_claude.rs` — host-side Claude env wiring
- `src/output.rs`, `src/parse_helpers.rs`, `src/test_support.rs` — helpers + fixtures
- subdirs (`env_resolver/`, `op_cli/`, `resolve/`, `token_setup/`, `host_claude/`) — module bodies + tests

## Public API

Resolved env layers and the `op` runner used by `jackin-runtime` during launch. The pure picker *model* is split into `jackin-console-oppicker`; this crate owns the `op` side-effects.

## How to verify

```sh
cargo nextest run -p jackin-env
cargo clippy -p jackin-env --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.

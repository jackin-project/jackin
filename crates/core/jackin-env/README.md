# jackin-env

Operator-environment resolution and 1Password (`op`) CLI integration. Turns declared operator env into resolved values, runs the `op` CLI for secret lookups, and feeds the resolved env layer into runtime launch.

## What this crate owns

- The `operator_env` resolution stack (`env_layer`, `env_resolver`, `resolve`) and the `op` CLI bridge (`op_cli`, `op_runner`, `op_struct`, `token_setup`).
- The interactive 1Password secret picker (`picker`) and host-side Claude env wiring (`host_claude`).
- Output/parse helpers and `test_support` for deterministic env fixtures.

## Architecture tier and allowed dependencies

**L1 application.** Allowed workspace dependencies: `jackin-core`, `jackin-config`, `jackin-protocol`, `jackin-diagnostics`. Stays below presentation so `jackin-launch`/`jackin-console` can consume it.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`env_resolver.rs`](src/env_resolver.rs) · [`env_resolver/`](src/env_resolver) | env resolution | [`tests.rs`](src/env_resolver/tests.rs) |
| [`env_layer.rs`](src/env_layer.rs) *(internal)* | env layer | — |
| [`resolve.rs`](src/resolve.rs) · [`resolve/`](src/resolve) | resolution entry | [`tests.rs`](src/resolve/tests.rs) |
| [`op_cli.rs`](src/op_cli.rs) · [`op_cli/`](src/op_cli) | `op` CLI bridge | [`tests.rs`](src/op_cli/tests.rs) |
| [`op_runner.rs`](src/op_runner.rs) | `op` runner | — |
| [`op_struct.rs`](src/op_struct.rs) | `op` struct types | — |
| [`token_setup.rs`](src/token_setup.rs) · [`token_setup/`](src/token_setup) | `op` token setup | [`tests.rs`](src/token_setup/tests.rs) |
| [`picker.rs`](src/picker.rs) | secret picker model (pure half in `jackin-oppicker`) | — |
| [`host_claude.rs`](src/host_claude.rs) *(internal)* · [`host_claude/`](src/host_claude) | host-side Claude env wiring | [`tests.rs`](src/host_claude/tests.rs) |
| [`output.rs`](src/output.rs) | output helpers | — |
| [`parse_helpers.rs`](src/parse_helpers.rs) | parse helpers | — |
| [`process_telemetry.rs`](src/process_telemetry.rs) · [`process_telemetry/`](src/process_telemetry) | bounded subprocess telemetry ownership | [`tests.rs`](src/process_telemetry/tests.rs) |
| [`test_support.rs`](src/test_support.rs) | test fixtures | — |

## Public API

**All public items are root re-exports; module paths are not API.** Implementation modules are crate-private (`mod` not `pub mod`). Consumers import `jackin_env::{…}` only.

Root surface groups: env resolution (`ResolvedEnv`, `EnvPrompter`, `resolve_env*`, `ResolveEnvError`), operator-env resolve (`resolve_operator_env*`, `OperatorEnvError`, `collect_on_demand_bindings`, …), `op` CLI/runner/struct/picker types, token-setup orchestrator subset, `parse_host_ref`.

Feature `test-support` keeps `jackin_env::test_support` as a public module path.

Typed errors (thiserror): `OperatorEnvError`, `ResolveEnvError`.

## How to verify

```sh
cargo nextest run -p jackin-env
cargo clippy -p jackin-env --all-targets -- -D warnings
```

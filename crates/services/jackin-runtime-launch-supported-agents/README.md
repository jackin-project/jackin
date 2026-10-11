# jackin-runtime-launch-supported-agents

Supported-agent lookup
for a launch role
(console role picker).

## What this crate owns

- Lookup
  (`supported_agents`):
  `resolve_supported_agents_for_console` —
  cached-manifest agent
  list with a
  non-interactive repo
  fallback. The pipeline
  core keeps the real
  launch (S7 split 96).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`supported_agents.rs`](src/supported_agents.rs) | agent lookup | hub `launch` suite (`launch/tests/case_10.rs`) |

## Public API

`supported_agents::resolve_supported_agents_for_console`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-supported-agents
cargo clippy -p jackin-runtime-launch-supported-agents --all-targets -- -D warnings
```

# jackin-runtime-attach-running-gate

Jackin runtime
running-state gate
for attach spawn.

## What this crate owns

- Gating
  (`running_gate`):
  `require_container_running` —
  verify only the
  Docker lifecycle
  state and return
  the prevalidated
  handle, shared by
  the shell-spawn
  path (through
  `require_container_reachable`)
  and the agent-spawn
  path (S7 split
  107).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`running_gate.rs`](src/running_gate.rs) | gating | hub `attach` suite (`attach/tests/case_03.rs`) |

## Public API

`running_gate::require_container_running`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-attach-running-gate
cargo clippy -p jackin-runtime-attach-running-gate --all-targets -- -D warnings
```

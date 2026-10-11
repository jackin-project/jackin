# jackin-instance-agents

Per-agent credential provisioning for role instances: agent provisioners, auth-source snapshot/capture, slot layout, and provisioning telemetry. Depends only on `jackin-instance-credentials`.

## What this crate owns

- Per-agent provisioners (`claude`, `codex`, `amp`, `kimi`, `opencode`, `omp`, `hermes`, `github`, `single_file_agents`) and their slot constructors (`agent_slots`, `single_file_slots`).
- Auth-source snapshot/capture/validation (`snapshot`, `capture`, `capture_unixless`, `validation`, `single_file`) and account sources (`account_sources`).
- Slot layout and bindings (`slots`, `agent_auth`, `ignore`) and bounded provisioning telemetry (`process_telemetry`).

## Architecture tier and allowed dependencies

**L1 application.** Allowed workspace dependencies: `jackin-core`, `jackin-config`, `jackin-process`, `jackin-telemetry`, `jackin-instance-credentials`. Orchestrated by `jackin-instance-roles`; no dependency back toward roles or `jackin-instance`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| per-agent modules | credential provisioners per agent | — |
| [`agent_slots.rs`](src/agent_slots.rs) · [`single_file_slots.rs`](src/single_file_slots.rs) | slot constructors | — |
| [`snapshot.rs`](src/snapshot.rs) · [`capture.rs`](src/capture.rs) · [`validation.rs`](src/validation.rs) | source snapshot/capture | — |
| [`slots.rs`](src/slots.rs) · [`agent_auth.rs`](src/agent_auth.rs) | slot layout/bindings | — |
| [`process_telemetry.rs`](src/process_telemetry.rs) · [`process_telemetry/`](src/process_telemetry) | bounded provisioning telemetry | [`tests.rs`](src/process_telemetry/tests.rs) |

## Public API

Provisioners and slot constructors consumed by `jackin-instance-roles` and re-exported through `jackin-instance`.

## How to verify

```sh
cargo nextest run -p jackin-instance-agents
cargo clippy -p jackin-instance-agents --all-targets -- -D warnings
```

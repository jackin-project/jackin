# jackin-runtime

Container bootstrap pipeline — the orchestrator that turns a resolved workspace + role into a running (or restorable) container and attaches the operator to it. Holds the concrete `DockerApi`/`CommandRunner` implementations, image build, DinD sidecar management, mount materialization, and the launch phases.

This crate is broad by design; the code-health program tracks decomposing it into a thin facade over launch, attach, cleanup, image/build, backend, and instance-lifecycle leaves (see the runtime/launch behavioral spec before any extraction).

## What this crate owns

- The launch pipeline (`runtime`) and its phase contracts: profile validation, workspace/role materialization, trust/source checks, image materialization, env/auth resolution, Docker run, wait-for-state, teardown, foreground attach, cleanup classification.
- Backend clients (`apple_container_client`, `host_daemon`) and host-side exec (`exec_host`).
- Mount isolation integration (`isolation`), the reactive daemon (`reactive_daemon`), and wait-for-state (`spin_wait`).

## Architecture tier and allowed dependencies

**L1 application / orchestration.** Allowed workspace dependencies: `jackin-core`, `jackin-config`, `jackin-env`, `jackin-manifest`, `jackin-docker`, `jackin-image`, `jackin-diagnostics`, `jackin-launch-tui`, `jackin-host`, `jackin-protocol`, `jackin-isolation`, `jackin-instance`, `jackin-tui`, `jackin-build-meta`. It is the integration point — the broadest dependency fan-in.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`runtime.rs`](src/runtime.rs) · [`runtime/`](src/runtime) | launch pipeline + phases | — |
| [`apple_container_client.rs`](src/apple_container_client.rs) · [`apple_container_client/`](src/apple_container_client) | Apple container backend | [`tests.rs`](src/apple_container_client/tests.rs) |
| [`host_daemon.rs`](src/host_daemon.rs) · [`host_daemon/`](src/host_daemon) | host daemon backend | [`tests.rs`](src/host_daemon/tests.rs) |
| [`exec_host.rs`](src/exec_host.rs) · [`exec_host/`](src/exec_host) | host-side command exec | [`tests.rs`](src/exec_host/tests.rs) |
| [`isolation.rs`](src/isolation.rs) · [`isolation/`](src/isolation) | mount isolation integration | [`tests.rs`](src/isolation/tests.rs) |
| [`reactive_daemon.rs`](src/reactive_daemon.rs) · [`reactive_daemon/`](src/reactive_daemon) | reactive daemon | [`tests.rs`](src/reactive_daemon/tests.rs) |
| [`spin_wait.rs`](src/spin_wait.rs) · [`spin_wait/`](src/spin_wait) | wait-for-state | [`tests.rs`](src/spin_wait/tests.rs) |

## Public API

The launch entry points (`launch_role_runtime`, `load_role_with`, `run_launch_core`) consumed by the `jackin` CLI. The [runtime/launch behavioral spec](../../docs/content/docs/reference/developer-reference/specs/runtime-launch.mdx) is the oracle for any extraction.

## How to verify

```sh
cargo nextest run -p jackin-runtime
cargo clippy -p jackin-runtime --all-targets -- -D warnings
```


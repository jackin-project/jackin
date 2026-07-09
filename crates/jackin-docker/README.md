# jackin-docker

Concrete Docker daemon client and subprocess shell runner for jackin❯. The workspace's adapter over Docker — image build/run/inspect, networking, and a captured shell-command runner — implemented behind the ports declared in `jackin-core`.

## What this crate owns

- The Docker client (`docker_client`) — image/container operations against the daemon.
- A captured shell-command runner (`shell_runner`) used across the workspace for `op`, `gh`, and other host CLIs.
- Docker networking helpers (`net`).

## Architecture tier and allowed dependencies

**L2 infrastructure.** Allowed workspace dependencies: `jackin-core`, `jackin-diagnostics`, `jackin-build-meta`. Must NOT depend on presentation (`jackin-launch-tui`, `jackin-console`, `jackin-tui`) — Docker access is infrastructure, consumed by orchestration above.

## Structure

- `src/docker_client.rs` / `src/docker_client/` — Docker daemon client
- `src/shell_runner.rs` / `src/shell_runner/` — captured shell-command runner
- `src/net.rs` / `src/net/` — Docker networking helpers

## Public API

`DockerApi` and `CommandRunner` implementations plus the networking helpers consumed by `jackin-runtime`, `jackin-image`, `jackin-isolation`, and `jackin-host`.

## How to verify

```sh
cargo nextest run -p jackin-docker
cargo clippy -p jackin-docker --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.

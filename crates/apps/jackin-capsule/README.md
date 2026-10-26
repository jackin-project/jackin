# jackin-capsule

In-container control plane for jackin❯ role containers.

In derived role images, `jackin-capsule` runs as PID 1 at `/jackin/runtime/jackin-capsule`. It owns PTYs, sessions, panes, status bar, attach socket, setup, and git trailer hook. The host `jackin` client attaches to that multiplexer, not container logs.

Shared behaviour: [Capsule reference](../../../docs/content/reference/capsule/index.mdx). This README maps crate ownership.

## What this crate owns

- PID 1 + the in-container multiplexer daemon (session/PTY supervision, attach socket, status bar, control protocol).
- Host-side capsule client (stdin/stdout forward, resize, host-affordance bridge).
- In-container runtime setup (git/GitHub init, trailer hooks, agent home seeding, auth handoff, Claude MCP registration).
- Capsule TUI surfaces, clipboard image staging, firewall/sudo-provision helpers, and usage/telemetry re-exports from `jackin-usage`.

Not responsible for: protocol encoding (`jackin-protocol`), host-side launch orchestration (`jackin-runtime`), or config schema migration.

## Architecture tier and allowed dependencies

**L4 entry/glue (binary + lib).** Dependencies include `jackin-brand`, `jackin-core`, `jackin-diagnostics` (OTLP), `jackin-protocol`, `jackin-usage`, `termpane`, `jackin-tui`, TermRock, `jackin-agent-status`, and `jackin-build-meta` (build.rs). Must **not** depend on host runtime (`jackin-runtime`) or other host binaries — the capsule is a different process tree.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, logging/usage re-exports | — |
| [`main.rs`](src/main.rs) | binary entry (PID 1 / client / exec subcommands) | — |
| [`daemon.rs`](src/daemon.rs) · [`daemon/`](src/daemon) | multiplexer shell, PTY/session authority, owned state subsystems, effectful ports | — |
| [`session.rs`](src/session.rs) · [`session/`](src/session) | per-agent PTY sessions | — |
| [`client.rs`](src/client.rs) · [`client/`](src/client) | host-side attach client | — |
| [`client_writer.rs`](src/client_writer.rs) | sole attach-socket writer | — |
| [`attach_context.rs`](src/attach_context.rs) | single host-connection state | — |
| [`attach_protocol.rs`](src/attach_protocol.rs) | attach lifecycle helpers | — |
| [`protocol.rs`](src/protocol.rs) · [`protocol/`](src/protocol) | capsule wire framing helpers | — |
| [`tui.rs`](src/tui.rs) · [`tui/`](src/tui) | composition, chrome/input, ANSI rules, and daemon-facing compositor/input/layout adapters over TermRock and shared operator-info UI | — |
| [`clipboard.rs`](src/clipboard.rs) · [`clipboard/`](src/clipboard) | clipboard image staging + idle expiry | — |
| [`runtime_setup.rs`](src/runtime_setup.rs) · [`runtime_setup/`](src/runtime_setup) | in-container git/auth/MCP setup | — |
| [`usage_relay_proxy.rs`](src/usage_relay_proxy.rs) · [`usage_relay_proxy/`](src/usage_relay_proxy) | container-local scoped usage socket and host stdio tunnel | — |
| [`config.rs`](src/config.rs) | `CapsuleConfig` load/validate | — |
| [`container_context.rs`](src/container_context.rs) | container identity metadata and co-located tests | — |
| [`agent_status.rs`](src/agent_status.rs) · [`agent_status/`](src/agent_status) | capsule-facing status hooks | — |
| [`pid1.rs`](src/pid1.rs) · [`pid1/`](src/pid1) | reaper + signal forward | — |
| [`exec.rs`](src/exec.rs) · [`exec/`](src/exec) | `jackin-exec` / capsule exec | — |
| [`firewall.rs`](src/firewall.rs) · [`firewall/`](src/firewall) | allowlist egress apply | — |
| [`sudo_provision.rs`](src/sudo_provision.rs) · [`sudo_provision/`](src/sudo_provision) | per-profile sudo grant | — |
| [`exit_assess.rs`](src/exit_assess.rs) · [`exit_assess/`](src/exit_assess) | dirty-exit modal assessment | — |
| [`git_context.rs`](src/git_context.rs) | branch/dirty/PR for status bar | — |
| [`pr_context.rs`](src/pr_context.rs) | GitHub PR lookup | — |
| [`pull_request.rs`](src/pull_request.rs) · [`pull_request/`](src/pull_request) | PR snapshots for TUI | — |
| [`socket.rs`](src/socket.rs) · [`socket/`](src/socket) | Unix attach socket helpers | — |
| [`mcp_server.rs`](src/mcp_server.rs) | MCP stdio for `jackin_exec` | — |
| [`output.rs`](src/output.rs) | plain stdout/stderr writers | — |
| [`services.rs`](src/services.rs) · [`services/`](src/services) | side-effect adapters | — |
| [`util.rs`](src/util.rs) · [`util/`](src/util) | shared bounded helpers | — |
| [`wordlist.rs`](src/wordlist.rs) · [`wordlist/`](src/wordlist) | tab codenames | — |
| `alloc_telemetry.rs` | `dhat-heap` live guard; env opt-in uses no-artifact testing mode | — |
| [`debug_panic.rs`](src/debug_panic.rs) · [`debug_panic/`](src/debug_panic) | force-panic debug hook | — |

## Public API

`tui::pane_snapshot` exposes `pane_content_from_damagegrid` and range-scoped `pane_content_range_from_damagegrid` for content-coordinate row materialization (bench + selection/link paths).

Library surface for integration tests and the binary: `daemon`, `client`, `config`, `session`, `tui`, `protocol`, `runtime_setup`, plus `logging`/`telemetry`/`usage` re-exports from `jackin-usage`. Most modules are `pub` so `tests/` and the binary can call them without spawning a PTY; production consumers outside this crate should not depend on capsule internals.

## How to verify

```sh
cargo nextest run -p jackin-capsule
cargo clippy -p jackin-capsule --all-targets -- -D warnings
```

Capsule e2e/smoke is a CI mandate under `.github/` — any PR that touches this crate must note the smoke block.

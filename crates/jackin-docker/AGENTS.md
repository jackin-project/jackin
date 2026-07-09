# AGENTS.md — jackin-docker

Concrete Docker daemon client and subprocess shell runner.

## Hard rules (this crate)

- **Tier & dependencies:** L2 infrastructure. Allowed workspace deps: `jackin-core`, `jackin-diagnostics`, `jackin-build-meta`. Must NOT depend on presentation crates.
- **Keep `README.md` current:** update it when structure, public API, the client, or the runner change (see `crates/AGENTS.md`).
- **Implement ports, don't invent policy.** This crate implements the `DockerApi`/`CommandRunner` ports from `jackin-core`; orchestration policy (when/whether to build or run) lives in `jackin-runtime`. Keep the adapter dumb.
- **Captured, never silent.** Shell-command capture goes through `shell_runner` (capture/timeout/status/redaction); do not call `Command::output` directly (enforced by `clippy::disallowed_methods`).

## What lives here vs elsewhere

- This crate owns: Docker daemon client, captured shell runner, Docker networking.
- *What* to build/run is decided in `jackin-runtime`/`jackin-image`. Redaction/observability of captured output lives in `jackin-diagnostics`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

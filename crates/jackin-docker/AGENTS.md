# AGENTS.md — jackin-docker

Concrete Docker daemon client and subprocess shell runner.

## Rules (this crate)

- Implement ports, don't invent policy: this crate implements the `DockerApi`/`CommandRunner` ports from `jackin-core`; *when/whether* to build or run is decided in `jackin-runtime`.
- Captured, never silent: shell-command capture goes through `shell_runner` (capture/timeout/status/redaction), never bare `Command::output` (also enforced by `clippy::disallowed_methods`).

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

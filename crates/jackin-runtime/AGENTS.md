# AGENTS.md — jackin-runtime

Container bootstrap pipeline — the launch orchestrator.

## Rules (this crate)

- Extract by phase contract, not line count: launch/body extraction proceeds by named phases (validation, materialization, trust checks, image, env/auth, Docker run, wait, teardown, attach, cleanup). The runtime/launch behavioral spec is the oracle.
- Characterization first: keep fast tests around observable launch behavior before extracting a body.
- When a lower crate needs a port/fake currently living here, move it down rather than adding an upward dev-dependency.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

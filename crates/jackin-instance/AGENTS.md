# AGENTS.md — jackin-instance

Role instance lifecycle: instance index, role-state directory, auth provisioning, container naming.

## Rules (this crate)

- Naming is a host↔capsule contract: container/instance naming must match what the capsule side expects — coordinate via `jackin-protocol`, do not invent a parallel scheme.
- The state-directory layout under `~/.jackin/` is operator-internal (contributor/reference surface only); never leak it into operator-facing docs.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

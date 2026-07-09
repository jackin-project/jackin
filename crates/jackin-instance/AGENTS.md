# AGENTS.md — jackin-instance

Role instance lifecycle: instance index, role-state directory, auth provisioning, container naming.

## Hard rules (this crate)

- **Tier & dependencies:** L1 application. Allowed workspace deps: `jackin-core`, `jackin-config`, `jackin-manifest`, `jackin-diagnostics`. No presentation or runtime dependencies — instance lifecycle stays a domain/app concern.
- **Keep `README.md` current:** update it when structure, public API, the instance model, or naming change (see `crates/AGENTS.md`).
- **Naming is a host↔capsule contract.** Container/instance naming must match what the capsule side expects; coordinate via `jackin-protocol`, do not invent a parallel scheme.
- **State directory is operator-internal.** On-disk layout under `~/.jackin/` is internals detail (contributor/reference surface only); never leak it into operator-facing docs.

## What lives here vs elsewhere

- This crate owns: instance index, role-state directory, auth provisioning, naming.
- Manifest *loading/validation* lives in `jackin-manifest`. Mount isolation lives in `jackin-isolation`. Docker run lives in `jackin-runtime`/`jackin-docker`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

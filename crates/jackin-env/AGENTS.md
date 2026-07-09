# AGENTS.md — jackin-env

Operator-environment resolution and 1Password (`op`) CLI integration.

## Hard rules (this crate)

- **Tier & dependencies:** L1 application. Allowed workspace deps: `jackin-core`, `jackin-config`, `jackin-protocol`, `jackin-diagnostics`. Do not depend on presentation crates.
- **Keep `README.md` current:** update it when structure, public API, module layout, or responsibilities change (see `crates/AGENTS.md`).
- **Keep the picker model pure.** Pure model/planning for the 1Password picker lives in `jackin-console-oppicker`; this crate owns only the `op` side-effects and resolution. Do not move planning logic back here.
- **`op` invocations go through the runner, not bare `Command`.** Capture/timeout/redaction belong in the shared runner; secret material is scrubbed via `jackin-diagnostics` redaction helpers.

## What lives here vs elsewhere

- This crate owns: operator-env resolution, the `op` CLI bridge, token setup, host Claude env wiring, the env layer.
- Picker *model* lives in `jackin-console-oppicker`. Redaction/observability substrate lives in `jackin-diagnostics`. Config schema lives in `jackin-config`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

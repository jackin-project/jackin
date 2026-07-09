# AGENTS.md — jackin-env

Operator-environment resolution and 1Password (`op`) CLI integration.

## Rules (this crate)

- Keep the picker pure: pure model/planning for the 1Password picker lives in `jackin-console-oppicker`; this crate owns only the `op` side-effects and resolution. Do not move planning logic back here.
- `op` invocations go through the runner, not bare `Command`; secret material is scrubbed via the `jackin-diagnostics` redaction helpers.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

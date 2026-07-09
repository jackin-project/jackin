# AGENTS.md — jackin-term

Owned terminal model for the `jackin-capsule` re-emitting PTY multiplexer. The detailed engineering record lives in `README.md` — read it before changing the model.

## Hard rules (this crate)

- **Tier & dependencies:** L2 infrastructure. Allowed workspace deps: `jackin-core`, `jackin-diagnostics`. No presentation, no `ratatui`, no host effects — only the terminal model + diff/emit surface.
- **Keep `README.md` current:** update it when structure, public API, the model, or the borrow ledger changes (see `crates/AGENTS.md`).
- **Pure Rust, no foreign bindings.** No `unsafe`, no FFI, no C/Zig. Non-Rust emulators appear as design references only, re-implemented with attribution.
- **Damage recorded at mutation, not recomputed by re-read.** The two-grid drift class (`Defect 44`) is solved structurally by recording dirty spans as `Perform` mutates — do not reintroduce a re-read-and-diff path.
- **No host-side effects.** In-memory only; no filesystem, network, or host mutation.
- **Correctness gates are load-bearing.** The conformance replay harness, fuzz target, and capsule echo-back harness must stay green; do not weaken them to make a change pass.

## What lives here vs elsewhere

- This crate owns: VT/ANSI parsing (over `vte`), the `DamageGrid` cell model, `PassthroughEvents`, snapshot/diff/width helpers.
- The multiplexer/daemon that *drives* this model lives in `jackin-capsule`. Diagnostics substrate lives in `jackin-diagnostics`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

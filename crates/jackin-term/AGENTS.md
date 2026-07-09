# AGENTS.md — jackin-term

Owned terminal model for the `jackin-capsule` re-emitting PTY multiplexer. The full design record (why `vt100` was retired, the borrow/reimplement ledger, correctness guarantees) lives in the [Capsule Terminal Model](../../docs/content/docs/reference/capsule/terminal-model.mdx) doc — read it before changing the model.

## Rules (this crate)

- Damage is recorded at mutation, never recomputed by re-read. The two-grid drift class is solved structurally by recording dirty spans as `Perform` mutates — do not reintroduce a re-read-and-diff path.
- Pure Rust, no foreign bindings: no `unsafe`, no FFI, no C/Zig. Non-Rust emulators are design references only, re-implemented with attribution.
- No host-side effects: in-memory only (no filesystem, network, or host mutation).
- Correctness gates are load-bearing: the conformance replay harness, fuzz target, and capsule echo-back harness must stay green — do not weaken them to make a change pass.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

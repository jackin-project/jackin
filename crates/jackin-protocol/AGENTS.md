# AGENTS.md — jackin-protocol

Shared wire-format contracts between the host CLI and `jackin-capsule`.

## Hard rules (this crate)

- **Tier & dependencies:** L0 domain (wire types). Allowed workspace deps: `jackin-core`. Wire types stay free of presentation and infrastructure concerns; DTOs and their conversions live here, behavior does not.
- **Keep `README.md` current:** update it when structure, public API, module layout, or responsibilities change (see `crates/AGENTS.md`).
- **Protocol changes are a host↔capsule contract.** A change to these types changes both binaries' wire format in lockstep; bump/align both sides in the same PR. Keep the crate dependency-light and serde-focused.
- **No runtime behavior.** Data contracts only. Runtime logic belongs in `jackin` (host) or `jackin-capsule`.

## What lives here vs elsewhere

- This crate owns: serde wire types, socket-path constants, attach/control/runtime/snapshot control messages, agent-status protocol types.
- Encoding/decoding *behavior* and the daemon live in `jackin-capsule`; host-side use lives in `jackin`/`jackin-runtime`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).

# jackin-protocol

Shared wire-format contracts for the host CLI and `jackin-capsule`.

This crate holds serde types, constants, and control messages that cross the host/container boundary. Keeping them here lets the host `jackin` binary and the in-container Capsule binary agree on socket paths, launch config shape, attach protocol messages, and runtime commands without either side depending on the other's full implementation stack.

The crate should stay dependency-light and protocol-focused. Runtime behavior belongs in the host CLI or `jackin-capsule`; shared data contracts belong here.

## Public API

Primary entry: [`ClientFrame`](src/attach.rs) (attach-protocol client frames). Related types:

- `ServerFrame` — capsule→host attach frames
- `ClipboardImageError` — typed clipboard image failure signal (wire payload remains a human-readable message; `from_message` classifies free-form host text)
- `ClientMsg` / `ServerMsg` — control-channel JSON frames

`#![deny(missing_docs)]` is on; public surface is rustdoc-complete.


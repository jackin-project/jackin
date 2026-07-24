# jackin-protocol

Shared wire-format contracts for the host CLI and `jackin-capsule`.

This crate holds serde types, constants, control messages, and shared terminal-handshake adapters that cross the host/container boundary. Keeping them here lets the host `jackin` binary and the in-container Capsule binary agree on socket paths, launch config shape, attach protocol messages, terminal defaults, and runtime commands without either side depending on the other's full implementation stack.

The crate should stay dependency-light and protocol-focused. Runtime behavior belongs in the host CLI or `jackin-capsule`; shared data contracts belong here.

## Public API

Primary entry: [`ClientFrame`](src/attach.rs) (attach-protocol client frames). Related types:

- `ServerFrame` — capsule→host attach frames
- `ClipboardImageError` — typed clipboard image failure signal (wire payload remains a human-readable message; `from_message` classifies free-form host text)
- `ClientMsg` / `ServerMsg` — control-channel JSON frames
- `TelemetryContext` — validated W3C trace context propagated across host/Capsule control frames
- `host_terminal` — the single OSC 10/11 default-color handshake and input-preservation adapter used by both attach clients
- `StatusSlot` — semantic status-bar glance slot a usage quota window fills (`session`, `daily`, `weekly`, `spend`); `daily` carries Amp Free's `N% remaining today` allowance
- `FocusedUsageView::is_refreshing_placeholder()` — one machine predicate for the exact cold `refreshing` placeholder invariant, so host/Swift code never compares display strings

`#![deny(missing_docs)]` is on; public surface is rustdoc-complete.

# jackin-usage-ffi

Synchronous UniFFI facade over `jackin-usage` host runtime for the native macOS
agent-usage menu bar. Swift is display-only — no provider probes, OAuth, HTTP,
or second provider matrix in this crate or in the Swift shell.

## Rules

- Coarse sync API only: open / list / set_enabled / refresh / next_events /
  snapshot / shutdown. No fine-grained probe callbacks into Swift.
- Panic containment at every entry (`catch_entry`); typed `UsageBridgeError`.
- Reuse `jackin_usage::host::HostUsageRuntime` and protocol view fields; do not
  re-shape quotas in Swift.
- `unsafe_code` is allowed only for UniFFI scaffolding (see crate `Cargo.toml`
  lints). Core truth stays in `jackin-usage` (unsafe forbid).

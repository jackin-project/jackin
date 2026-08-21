# jackin-usage-ffi

Synchronous boltffi facade over `jackin-usage` host runtime for the native macOS
agent-usage menu bar. Swift is display-only — no provider probes, OAuth, HTTP,
or second provider matrix in this crate or in the Swift shell.

## Rules

- Coarse sync API only: open / list / set_enabled / refresh / next_events /
  snapshot / list_accounts / set_selected_account / shutdown. No fine-grained
  probe callbacks into Swift.
- Refresh is a non-blocking client operation against the host Rust broker. Waiting
  and provider work stay on Rust worker threads; callers observe one shared
  generation/phase. Never fall back to local probes or filesystem coordination.
- Panic containment at every entry (`catch_entry`); typed `UsageBridgeError`.
- Reuse `jackin_usage::host::HostUsageRuntime` and protocol view fields; do not
  re-shape quotas in Swift.
- `unsafe_code` is allowed only for boltffi scaffolding (see crate `Cargo.toml`
  lints). Core truth stays in `jackin-usage` (unsafe forbid).
- **boltffi only — never UniFFI.** The workspace bans the `uniffi` crate in
  `deny.toml`; do not reintroduce it here or anywhere else.
- No item-level `#[error("...")]` attributes in this crate or in any local path
  dependency it can reach (jackin-usage, jackin-core, ...): the boltffi macro
  scanner reads them as invalid BoltFFI markers and fails the whole build. Use
  variant-level thiserror messages or hand-written `Display`/`Error` impls.
- **Limits only — never token price or historical usage trend.** Export DTOs for
  **usage limits** (remaining/used %, resets, plan, multi-account, provider limit
  windows). **Never** add FFI fields or methods for token unit prices, cost-of-
  session estimates, historical usage/spend series, sparklines, donuts, or
  Today/Yesterday/30-day trends. Match root product limits-only rules and
  `jackin-usage` crate rules.

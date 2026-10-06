# Jackin build and usage boundary naming crosscheck

Status: private static design evidence. No repository source, branch state, build, test, benchmark, or copied target changed. No rootfs binary was executed.

## Evidence identity

Source checkout: `/root/Projects/tailrocks/jackin-project/jackin-refactor-build-split`  
Inspected source HEAD: `f4902db386e029a4a481b13767f7a29b5351af49`  
Checkout instructions read: `AGENTS.md`, `/root/.codex/RTK.md`; no checkout-root `RTK.md` exists.  
The pre-existing worktree has unrelated migration-fixture and plan edits. They were left untouched.

Private design sources read:

- `/root/.velnor-work/jackin-build-split-design/usage-boundary-design-20cb8f7054ef6322c653b5c92bec7ad8f826d810.md` (static at source `20cb8f7054ef6322c653b5c92bec7ad8f826d810`, parent `9f4c32ec4fcb34d228c9cb83a2f49f87db835c93`).
- `/root/.velnor-work/jackin-build-split-design/crate-boundary-design-16c493d5793816c03e6ca8e5b168ee2b7cce86d4-revision1.md` (static at source `16c493d5793816c03e6ca8e5b168ee2b7cce86d4`).
- `/root/.velnor-work/jackin-build-split-design/crate-boundary-design-16c493d5793816c03e6ca8e5b168ee2b7cce86d4.md` is superseded; its packet records SHA-256 `24e9562cfc4d4d49b08cf25dc307f2b97bf7293659159efdbe6e837262405276`.

The old plan/build evidence named in these packets is source `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`; it is not current accepted-main baseline evidence. This packet records names and ownership only and authorizes no extraction or performance claim.

## Selected names and alternatives

Use `jackin-capsule-builder` for the binary-only package; retain executable `build-jackin-capsule`. `jackin-capsule-build` is a reasonable alternative, while a generic `jackin-builder` hides the product boundary. Remove the old Jackin binary target and lifecycle exception when implementation is authorized; do not keep aliases. Current evidence: `crates/jackin/Cargo.toml:31`, `crates/jackin/src/bin/build_jackin_capsule/main.rs:49-62`, and domain APIs from `jackin-core` / `jackin-image`. `jackin-image` remains owner of artifact version, architecture, and file mode contracts.

Prefer the finer capability packages from the newer usage packet over the older broad `jackin-usage-host` umbrella:

- `jackin-session-usage`: Capsule-local token monitoring, cache/refresh behavior, shared pure presentation, and relay-only broker client.
- `jackin-usage-providers`: provider parsers, provider credential adaptation, HTTP fetching, and normalized provider results shared by host and session flows.
- `jackin-usage-store`: Turso chokepoint, SQLite snapshot persistence and OpenCode database reads.
- `jackin-usage-broker`: host account/credential discovery and authorization, provider execution coordination, projection, and broker service.

`jackin-usage-session` and `jackin-usage-service` are plausible alternatives, but the selected names align with the current `UsageBroker` vocabulary and separate the session capability from reusable provider adapters. Reject `jackin-host-usage` as process-oriented and reject new `jackin-usage-types` / `jackin-usage-model` crates because shared data already has owners.

## Existing owners and dependency constraints

- `jackin-core` owns generic `Agent`, `JackinPaths`, account identity hashing, and low-level environment vocabulary (`crates/jackin-core/src/agent.rs:23`, `account_key.rs:14`, `paths.rs:36`).
- `jackin-config` owns persisted `AppConfig`, workspace configuration, account declarations, and credential declarations (`crates/jackin-config/src/app_config.rs:31`, `schema.rs:219`, `accounts.rs:291`). Providers should not own persisted config or depend upward on host config.
- `jackin-protocol` owns cross-process `AccountUsageSnapshotView`, `FocusedUsageView`, broker capabilities, requests, responses, and projection records (`crates/jackin-protocol/src/control.rs:532,565`; `usage_broker.rs:105,1095,1246,1274`). Keep wire DTOs here.
- `jackin-process` is generic subprocess capture/timeout/retry/status (`crates/jackin-process/Cargo.toml:10`, `src/lib.rs:21-52`); usage-specific broker leases, socket lifecycle, and credential authorization remain in the broker domain.
- `jackin-launch` owns launch-progress TUI (`crates/jackin-launch/src/lib.rs:1-13`), not Capsule build or usage service.

Desired one-way capability graph after the planned boundaries exist:

```text
jackin-capsule -> jackin-session-usage -> jackin-protocol -> jackin-core
jackin-usage-providers -> jackin-protocol, jackin-core, jackin-telemetry, HTTP dependencies
jackin-usage-store -> jackin-protocol, jackin-core, jackin-telemetry, turso
jackin-session-usage -> jackin-usage-store (after the separate store move)
jackin-usage-broker -> jackin-session-usage, jackin-usage-providers, jackin-usage-store, jackin-config, jackin-protocol
jackin, jackin-runtime, jackin-usage-ffi -> broker as needed
```

Session must not depend on broker. Provider code must not depend on session, broker, store, or config. Store must not depend on session types or HTTP. Only the broker joins provider and store capabilities. Keep `FocusedUsageView` and other wire records in protocol rather than adding a model crate.

## Current consumer and integration concerns

At source HEAD the monolithic `jackin-usage` manifest joins `reqwest`/`tower` provider code, `turso`, ICU projection, and Unix `nix` broker code (`crates/jackin-usage/Cargo.toml:22-47`). Its modules include coordinator, host, token monitor, usage providers, and snapshot store (`crates/jackin-usage/src/lib.rs:7-18`).

- Capsule depends on the monolith (`crates/jackin-capsule/Cargo.toml:46`), uses `UsageCache`/`UsageRefreshTarget` (`crates/jackin-usage/src/usage.rs:272,286`), `TokenMonitor` (`token_monitor.rs:384`), the relay client (`host/broker.rs:1030`), and formatting. It can shed provider HTTP and host ICU/nix edges; it retains Turso until OpenCode row reading is moved behind the store API (`token_monitor/opencode.rs`). Do not claim Capsule is Turso-free after a session-only move.
- Host discovery calls provider parsers and credential models from `usage.rs` (`host/discovery.rs`, including `ClaudeResolved`/`CodexOAuthCredentials`), so provider adapters cannot be moved into a session-only package.
- `jackin-runtime/src/usage_relay.rs` consumes coordinator and host service APIs; it migrates to broker.
- `jackin-usage-ffi` consumes host APIs and shared presenters, so its post-split dependencies must explicitly cover broker and presenter ownership.
- `jackin` CLI imports host APIs and has a direct Turso-backed cache in `crates/jackin/src/cli/usage/store.rs`; centralizing storage must account for this second database consumer, not just `usage_snapshot_store`.
- The existing `jackin-usage-broker` binary target is currently owned by `jackin` (`crates/jackin/Cargo.toml:27`, source `crates/jackin/src/bin/usage-broker/main.rs`). If a package takes the same name, explicitly give the binary a single owner and remove the old target.
- Command producers for the builder include `jackin-dev`, `jackin-xtask`, Jackin E2E, docs, and `jackin-image` help text. Preserve the MBX-only path, fd/env/exit behavior, and configured target-directory provenance.

The static design does not open implementation: first obtain the second accepted-main baseline and freeze the target thresholds/conditions. Existing historic measurements at `0aa821a...` do not establish current performance or extraction benefit.

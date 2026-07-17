# Plan 011 call-site census and classification

Baseline reconstructed from the last commits before each atomic cutover. Counts exclude tests and xtask fixture tooling. Classifications are updated as each file is migrated under the Plan 011 rulebook; rows that still say `governed` remain open until their generic call sites are removed.

| File | `debug_log` | info | debug | trace | warn | error | Landed classification |
|---|---:|---:|---:|---:|---:|---:|---|
| `crates/jackin-capsule/src/alloc_telemetry.rs` | 0 | 2 | 0 | 0 | 0 | 0 | COMPLETE — developer-only DHAT state stays outside production telemetry and emits no generic lifecycle log |
| `crates/jackin-capsule/src/attach_protocol.rs` | 0 | 17 | 1 | 0 | 2 | 3 | COMPLETE — detached handshake/control failures automatically emit one bodyless typed error from `DetachedCompletion`; persistent attach failures retain one owning `rpc_error`; expected shutdown/channel closure is silent; timeout and join failures use bounded types |
| `crates/jackin-capsule/src/client.rs` | 0 | 1 | 0 | 0 | 0 | 0 | COMPLETE — swallowed hook-report failures emit one bodyless `rpc_error` through `ResultTelemetryExt` |
| `crates/jackin-capsule/src/client_writer.rs` | 0 | 1 | 0 | 2 | 0 | 0 | PARTIAL — expected receiver closure is silent; structural DEBUG firehose still requires classification |
| `crates/jackin-capsule/src/clipboard.rs` | 0 | 2 | 1 | 0 | 0 | 0 | PARTIAL — cleanup failures emit bodyless `io_error`, non-file recovery emits one governed warning, and path narration is removed; stale-transfer DEBUG still requires classification |
| `crates/jackin-capsule/src/container_context.rs` | 0 | 3 | 0 | 0 | 0 | 0 | COMPLETE — identity-source and expected-absence fallback narration deleted; container identity is prohibited telemetry data |
| `crates/jackin-capsule/src/daemon.rs` | 0 | 21 | 4 | 0 | 0 | 0 | COMPLETE — registered agent-state signals own transitions; final daemon errors and handled degradations are typed; raw workdir/terminal/session/PTY/clipboard/error detail and duplicate lifecycle/debug narration are removed |
| `crates/jackin-capsule/src/daemon/compositor.rs` | 0 | 2 | 5 | 2 | 0 | 0 | COMPLETE — Ratatui draw failure emits one bodyless `io_error` at the rendering owner; duplicate failure narration and per-frame pane/session firehose are deleted; registered render metrics own duration and volume |
| `crates/jackin-capsule/src/daemon/context_mgmt.rs` | 0 | 6 | 8 | 0 | 0 | 0 | COMPLETE — branch and PR workers emit validated bounded `background.cycle` operations; delivered lookup failures and closed result channels classify independently and automatically emit typed errors; raw branch, HEAD, request, cache, and error narration is deleted |
| `crates/jackin-capsule/src/daemon/control.rs` | 0 | 6 | 2 | 2 | 0 | 0 | COMPLETE — invalid correlation, repeated handshake, unknown control messages, absent capture sessions, capture I/O, and host clipboard failures emit bodyless typed errors; registered RPC operations own response outcomes; raw paths, session/request IDs, geometry, input sizes, and peer payloads are deleted |
| `crates/jackin-capsule/src/daemon/file_export.rs` | 0 | 2 | 2 | 0 | 0 | 0 | COMPLETE — export validation/open/read failures emit one bodyless `io_error` through `ResultTelemetryExt`; operator notices retain useful detail; path, basename, digest, transfer identity, size, and policy narration are deleted from telemetry |
| `crates/jackin-capsule/src/daemon/input_dispatch.rs` | 0 | 4 | 8 | 0 | 0 | 0 | COMPLETE — unknown provider fallback emits one recovered-degradation warning and split failures use typed `launch_failed`; per-action, key, session, agent, mouse, scrollback, terminal-mode, and encoded-byte narration is deleted; PTY writer metrics own delivered bytes |
| `crates/jackin-capsule/src/daemon/mouse_input.rs` | 0 | 2 | 25 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/multiplexer_utils.rs` | 0 | 3 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/pane_layout.rs` | 0 | 3 | 1 | 0 | 0 | 0 | COMPLETE — reaped split races emit one recovered-degradation warning; registered PTY/session/UI signals own successful split, close, focus, and resize behavior; pane/session/agent/provider labels and geometry are deleted |
| `crates/jackin-capsule/src/daemon/ports.rs` | 0 | 0 | 1 | 0 | 0 | 0 | COMPLETE — stale reporter events retain the explicit always-ACK contract and remain silent; registered agent-state signals own applied transitions, and raw session identity is deleted |
| `crates/jackin-capsule/src/daemon/resource_metrics.rs` | 0 | 0 | 0 | 0 | 0 | 0 | migrated to standard governed process metrics |
| `crates/jackin-capsule/src/daemon/session_lifecycle.rs` | 0 | 8 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/debug_panic.rs` | 0 | 1 | 0 | 0 | 0 | 0 | COMPLETE — the shared panic hook owns one registered `app.crash`; pre-panic narration is deleted |
| `crates/jackin-capsule/src/firewall.rs` | 0 | 4 | 0 | 0 | 0 | 0 | COMPLETE — selected policy is owned by the isolation event; invalid/unresolvable allowlist members emit typed recovered-degradation WARN; raw hosts and counts are omitted |
| `crates/jackin-capsule/src/git_context.rs` | 0 | 12 | 5 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/lib.rs` | 0 | 1 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/pid1.rs` | 0 | 8 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/pr_context.rs` | 0 | 1 | 3 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/runtime_setup.rs` | 0 | 7 | 3 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/services/input_bindings.rs` | 0 | 2 | 0 | 0 | 0 | 0 | COMPLETE — parser return values own invalid-input fallback; raw environment values and key material are prohibited telemetry data, so generic narration is deleted |
| `crates/jackin-capsule/src/session.rs` | 0 | 20 | 6 | 2 | 0 | 0 | COMPLETE — PTY/lock/resize failures use bounded `ResultTelemetryExt`; PTY exit owns wait classification; expected closure/EOF and raw byte/identity/detail chatter deleted |
| `crates/jackin-capsule/src/socket.rs` | 0 | 7 | 2 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/tui/run.rs` | 0 | 0 | 6 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-capsule/src/util.rs` | 0 | 5 | 5 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-config/src/app_config/persist.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-config/src/migrations.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console-oppicker/src/state.rs` | 1 | 0 | 0 | 0 | 0 | 0 | DELETE — returned reference owns fallback behavior; vault/item names and field identifiers prohibited |
| `crates/jackin-console/src/services/role_source.rs` | 5 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console/src/tui/input/auth.rs` | 3 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console/src/tui/input/editor.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console/src/tui/input/editor/agents.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console/src/tui/input/editor/modal.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console/src/tui/input/global_mounts/auth.rs` | 4 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console/src/tui/state.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console/src/tui/state/manager.rs` | 4 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-core/src/debug_log.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-core/src/lib.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-core/src/worktree_dirty.rs` | 1 | 0 | 1 | 0 | 0 | 0 | governed DEBUG detail; governed DEBUG detail |
| `crates/jackin-diagnostics/src/lib.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-docker/src/docker_client.rs` | 23 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-docker/src/net.rs` | 4 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-host/src/caffeinate.rs` | 1 | 0 | 0 | 0 | 0 | 0 | DELETE — command result and typed error own teardown semantics; process identifiers prohibited |
| `crates/jackin-host/src/host_clipboard.rs` | 2 | 0 | 0 | 0 | 0 | 0 | DELETE — return value owns implicit-paste resolution; host paths prohibited |
| `crates/jackin-image/src/agent_binary.rs` | 8 | 0 | 0 | 0 | 0 | 0 | DELETE generic fallback — registered cache/download boundaries and active-run compact records own semantics; agent/version/URL/path/error detail prohibited |
| `crates/jackin-image/src/capsule_binary.rs` | 11 | 0 | 0 | 0 | 0 | 0 | DELETE generic fallback — registered cache boundary and active-run compact records own semantics; version/architecture/path detail prohibited |
| `crates/jackin-image/src/image_build.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-image/src/image_decision.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-instance/src/auth.rs` | 7 | 0 | 0 | 0 | 0 | 0 | DELETE — typed auth outcome owns semantics; credential paths, CLI stderr, and credential-file parse details prohibited |
| `crates/jackin-instance/src/manifest.rs` | 5 | 0 | 0 | 0 | 0 | 0 | DELETE — typed persistence result owns semantics; state paths and container identifiers prohibited |
| `crates/jackin-isolation/src/cleanup.rs` | 9 | 0 | 0 | 0 | 0 | 0 | DELETE — cleanup result and typed errors own semantics; paths, mount/container names, branches, and command material prohibited |
| `crates/jackin-isolation/src/finalize.rs` | 6 | 0 | 0 | 0 | 0 | 0 | DELETE — finalizer decision owns semantics; container/mount names and shared diagnostic prose prohibited |
| `crates/jackin-isolation/src/materialize.rs` | 27 | 0 | 0 | 0 | 0 | 0 | DELETE — incidental materialization chatter; launch/isolation boundary owns semantics; paths, workspace/container names, branches, commits, and URLs prohibited |
| `crates/jackin-isolation/src/state.rs` | 4 | 0 | 0 | 0 | 0 | 0 | DELETE — persistence return value owns semantics; state paths and mount names prohibited |
| `crates/jackin-launch-tui/src/progress.rs` | 1 | 0 | 0 | 0 | 0 | 0 | DELETE — recovered lock poison has no operator action; returned acknowledgement state owns behavior |
| `crates/jackin-runtime/src/apple_container_client.rs` | 7 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/exec_host.rs` | 8 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/host_daemon.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/reactive_daemon.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/apple_container.rs` | 14 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/attach.rs` | 2 | 0 | 0 | 0 | 0 | 0 | REPLACE deterministic socket-path fallback with one bodyless recovered-degradation warning; remaining DEBUG sites require classification |
| `crates/jackin-runtime/src/runtime/cleanup.rs` | 1 | 0 | 0 | 0 | 0 | 0 | REPLACE corrupt-manifest fallback with one bodyless recovered-degradation warning; remaining DEBUG site requires classification |
| `crates/jackin-runtime/src/runtime/docker_profile.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/host_attach.rs` | 24 | 0 | 0 | 0 | 0 | 0 | REPLACE terminal-reset teardown failures with bodyless typed `io_error`; remaining DEBUG sites require classification |
| `crates/jackin-runtime/src/runtime/image.rs` | 13 | 0 | 0 | 0 | 0 | 0 | REPLACE handled lookup/inspect/version failures with one bodyless recovered-degradation warning per decision; remaining DEBUG sites require classification |
| `crates/jackin-runtime/src/runtime/image/build.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/image/version.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/launch/exit_diagnosis.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/launch/launch_dind.rs` | 8 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs` | 12 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core/orchestrate.rs` | 4 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` | 16 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/launch/launch_slot.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/launch/trust.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/repo_cache.rs` | 4 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/universe.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-term/src/grid/perform.rs` | 0 | 0 | 1 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-usage/src/logging.rs` | 0 | 0 | 1 | 1 | 0 | 0 | REPLACE — registered `app.crash` panic boundary owns the error and flush; raw panic payload prohibited |
| `crates/jackin-usage/src/telemetry.rs` | 0 | 3 | 1 | 0 | 0 | 0 | REPLACE remaining init failure — typed health plus best-effort `config_error`; DELETE duplicate active lifecycle containing session identity |
| `crates/jackin-usage/src/token_monitor.rs` | 0 | 0 | 1 | 0 | 0 | 0 | REPLACE — `ResultTelemetryExt` emits typed `io_error`; collector absence owns behavior; provider labels, host paths, and raw errors prohibited |
| `crates/jackin-usage/src/token_monitor/opencode.rs` | 0 | 0 | 3 | 0 | 0 | 0 | REPLACE — `ResultTelemetryExt` emits typed `db_error`; collector false outcome owns behavior; database path and raw errors prohibited |
| `crates/jackin-usage/src/usage.rs` | 0 | 4 | 1 | 0 | 0 | 0 | REPLACE remaining credential failures — typed `io_error`/`config_error`; expected absence stays silent; credential paths and raw errors prohibited |
| `crates/jackin-usage/src/usage/codex.rs` | 0 | 2 | 2 | 0 | 0 | 0 | REPLACE remaining provider failures — typed `http_error`/`rpc_error`/`io_error`; config paths and raw dependency errors prohibited |
| `crates/jackin-usage/src/usage/refresh.rs` | 0 | 9 | 2 | 0 | 0 | 0 | REPLACE remaining refresh/persistence failures — owning spans plus typed `timeout`/`panic`/provider/`io_error`/`config_error` events and one governed WARN on recovery; cache keys, paths, reasons, and raw errors prohibited |
| `crates/jackin-usage/src/usage_snapshot_store.rs` | 0 | 0 | 1 | 0 | 0 | 0 | REPLACE — `ResultTelemetryExt` emits typed `db_error`; original upsert error remains the returned owner; raw rollback error prohibited |
| `crates/jackin/src/console/effects.rs` | 14 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin/src/console/services.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin/src/console/tui/run.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |

Baseline totals: 283 legacy host debug sites, 169 capsule INFO sites, 107 capsule DEBUG sites, 9 payload-trace sites, 2 WARN sites, and 3 ERROR sites.

Current production invocation census after the isolation, instance, host, image-fallback, launch-TUI, usage-collector, oppicker, PTY-session, Capsule daemon/attach/client/clipboard/context/firewall/compositor/context-management/control/file-export/input-dispatch/pane-layout/ports/resource-metrics, input-binding, and recovered-degradation migration passes: 50 `telemetry_info!` and 205 `telemetry_debug!` sites. The generic macro machinery and these 255 sites remain open; `telemetry_trace!`, `telemetry_warn!`, and `telemetry_error!` invocations are zero, and macro names in definitions or documentation are excluded.

# Plan 011 call-site census and classification

Baseline reconstructed from the last commits before each atomic cutover. Counts exclude tests and xtask fixture tooling. Classifications are updated as each file is migrated under the Plan 011 rulebook; rows that still say `governed` remain open until their generic call sites are removed.

| File | `debug_log` | info | debug | trace | warn | error | Landed classification |
|---|---:|---:|---:|---:|---:|---:|---|
| `crates/jackin-capsule/src/alloc_telemetry.rs` | 0 | 2 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/attach_protocol.rs` | 0 | 17 | 1 | 0 | 2 | 3 | governed INFO lifecycle/state; governed DEBUG detail; governed WARN degradation; typed ERROR owner |
| `crates/jackin-capsule/src/client.rs` | 0 | 1 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/client_writer.rs` | 0 | 1 | 0 | 2 | 0 | 0 | governed INFO lifecycle/state; structural counts only; raw payload removed |
| `crates/jackin-capsule/src/clipboard.rs` | 0 | 2 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/container_context.rs` | 0 | 3 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/daemon.rs` | 0 | 21 | 4 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/compositor.rs` | 0 | 2 | 5 | 2 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail; structural counts only; raw payload removed |
| `crates/jackin-capsule/src/daemon/context_mgmt.rs` | 0 | 6 | 8 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/control.rs` | 0 | 6 | 2 | 2 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail; structural counts only; raw payload removed |
| `crates/jackin-capsule/src/daemon/file_export.rs` | 0 | 2 | 2 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/input_dispatch.rs` | 0 | 4 | 8 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/mouse_input.rs` | 0 | 2 | 25 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/multiplexer_utils.rs` | 0 | 3 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/pane_layout.rs` | 0 | 3 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/ports.rs` | 0 | 0 | 1 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/resource_metrics.rs` | 0 | 0 | 3 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-capsule/src/daemon/session_lifecycle.rs` | 0 | 8 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/debug_panic.rs` | 0 | 1 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/firewall.rs` | 0 | 4 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/git_context.rs` | 0 | 12 | 5 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/lib.rs` | 0 | 1 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/pid1.rs` | 0 | 8 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/pr_context.rs` | 0 | 1 | 3 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/runtime_setup.rs` | 0 | 7 | 3 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/services/input_bindings.rs` | 0 | 2 | 0 | 0 | 0 | 0 | governed INFO lifecycle/state |
| `crates/jackin-capsule/src/session.rs` | 0 | 20 | 6 | 2 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail; structural counts only; raw payload removed |
| `crates/jackin-capsule/src/socket.rs` | 0 | 7 | 2 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-capsule/src/tui/run.rs` | 0 | 0 | 6 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-capsule/src/util.rs` | 0 | 5 | 5 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-config/src/app_config/persist.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-config/src/migrations.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-console-oppicker/src/state.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
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
| `crates/jackin-launch-tui/src/progress.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/apple_container_client.rs` | 7 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/exec_host.rs` | 8 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/host_daemon.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/reactive_daemon.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/apple_container.rs` | 14 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/attach.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/cleanup.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/docker_profile.rs` | 1 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/host_attach.rs` | 24 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-runtime/src/runtime/image.rs` | 13 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
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
| `crates/jackin-usage/src/logging.rs` | 0 | 0 | 1 | 1 | 0 | 0 | governed DEBUG detail; structural counts only; raw payload removed |
| `crates/jackin-usage/src/telemetry.rs` | 0 | 3 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-usage/src/token_monitor.rs` | 0 | 0 | 1 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-usage/src/token_monitor/opencode.rs` | 0 | 0 | 3 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin-usage/src/usage.rs` | 0 | 4 | 1 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-usage/src/usage/codex.rs` | 0 | 2 | 2 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-usage/src/usage/refresh.rs` | 0 | 9 | 2 | 0 | 0 | 0 | governed INFO lifecycle/state; governed DEBUG detail |
| `crates/jackin-usage/src/usage_snapshot_store.rs` | 0 | 0 | 1 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin/src/console/effects.rs` | 14 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin/src/console/services.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |
| `crates/jackin/src/console/tui/run.rs` | 2 | 0 | 0 | 0 | 0 | 0 | governed DEBUG detail |

Baseline totals: 283 legacy host debug sites, 169 capsule INFO sites, 107 capsule DEBUG sites, 9 payload-trace sites, 2 WARN sites, and 3 ERROR sites.

Current production census after the isolation, instance, host, and image-fallback deletion passes: 158 `telemetry_info!`, 267 `telemetry_debug!`, 10 `telemetry_warn!`, and 4 `telemetry_error!` sites. The generic macro machinery and these 439 sites remain open.

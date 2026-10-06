//! jackin-capsule: in-container capsule daemon, sessions, and TUI.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`daemon`] — capsule daemon module the binary runs.

use anyhow::Result;
use jackin_capsule::{config, daemon, exec, runtime_setup};
use jackin_telemetry::ResultTelemetryExt as _;

use argv::{
    exec_invocation, forced_daemon_mode, invoked_as_prepare_commit_msg_hook, resolve_initial_agent,
};
use client_dispatch::run_client_mode;

mod argv;
mod client_dispatch;
mod usage_relay_proxy;

#[cfg(test)]
pub(crate) use argv::is_daemon_entrypoint_args;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// CLI for `jackin-capsule`.
///
/// Mode is determined by:
/// - PID == 1 → daemon mode (supervisor + multiplexer + socket control plane)
/// - PID != 1 → client mode (connect to daemon, run interactive UI)
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "__isolated-exec") {
        return jackin_capsule::process_isolation::run_isolated_command(&args[2..]);
    }
    if invoked_as_prepare_commit_msg_hook(&args) {
        return runtime_setup::run_prepare_commit_msg_hook(&args[1..]);
    }

    // `jackin-exec` (argv0 symlink) and `jackin-capsule exec …` are always
    // client-side credential execs, never the daemon. Check before the daemon
    // gate below so an inherited `JACKIN_CAPSULE_FORCE_DAEMON` in the
    // apple-container VM env cannot capture this invocation into daemon mode.
    if let Some(exec_args) = exec_invocation(&args) {
        return exec::run(exec_args).await;
    }

    // Daemon mode when PID 1 (Docker backend, capsule is the entrypoint) or when
    // the apple-container `JACKIN_CAPSULE_FORCE_DAEMON` marker applies to *this*
    // invocation (see `forced_daemon_mode` — the env is inherited by
    // `container exec` children, so it cannot mark the entrypoint on its own).
    let is_pid1 = std::process::id() == 1 || forced_daemon_mode(&args);

    if is_pid1 {
        let mut telemetry = jackin_capsule::telemetry::init()
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::ConfigError)?;
        let launch_config = config::load()
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::ConfigError)?;
        let agent = resolve_initial_agent(&args, &launch_config)
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::ConfigError)?;
        let result = daemon::run_daemon(agent, launch_config, &mut telemetry).await;
        if result.is_err() {
            telemetry.daemon_failed();
        }
        result
    } else {
        run_client_mode(&args).await
    }
}

#[cfg(test)]
mod tests;

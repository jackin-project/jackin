//! Per-session process and filesystem isolation.
//!
//! The capsule supervisor is trusted. Agent sessions are not: they receive a
//! unique numeric identity, retain only the two DAC capabilities needed to
//! work in host bind mounts, and are confined with Landlock before `exec`.
//! Landlock is required rather than best-effort because DAC override would
//! otherwise let one slot walk into another slot's bind mount.
//!
//! The session boundary is intentionally layered. On Landlock ABI 9 and
//! newer, pathname Unix-socket resolution is denied outside explicitly
//! writable session roots. Older kernels only get the ABI-3 filesystem rules;
//! the daemon's kernel peer-UID plus per-session bearer capability then gates
//! the control socket. Mutable setup, temporary files, and git metadata live
//! below one root allocated for the exact PTY session. There is no agent-child
//! grant for capsule-wide state, `/tmp`, or shared GitHub CLI credentials.

// Landlock/capability syscalls are the fail-closed isolation boundary; unsafe
// is confined to per-site `#[expect]`s inside the Linux-only code below and
// reviewed as a security API. No unsafe code remains on other targets.

#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::{Result, bail};
#[cfg(target_os = "linux")]
use jackin_protocol::SessionIdentity;

/// Dispatch the internal session wrapper.
///
/// Arguments are: `<instance-or-> <uid> <gid> <program> [args...]`.
/// The wrapper is intentionally not a public user-facing command.
///
/// # Errors
///
/// Returns an error when the session identity is not admitted, the required
/// Linux isolation boundary cannot be installed, or the target cannot be
/// executed.
pub fn run_isolated_command(args: &[String]) -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        bail!("isolated agent sessions require a Linux Landlock boundary");
    }

    #[cfg(target_os = "linux")]
    {
        if args.len() < 4 {
            bail!("isolated session wrapper requires instance, uid, gid, and program");
        }
        let instance = (args[0] != "-").then_some(args[0].as_str());
        let uid = args[1]
            .parse::<u32>()
            .context("invalid isolated session uid")?;
        let gid = args[2]
            .parse::<u32>()
            .context("invalid isolated session gid")?;
        let identity = SessionIdentity { uid, gid };
        let program = &args[3];
        let program_args = &args[4..];

        let config = crate::config::load()?;
        let expected = admitted_identity(&config, instance);
        anyhow::ensure!(
            expected == Some(identity),
            "isolated session identity is not admitted for this target"
        );

        linux::run(&config, instance, identity, program, program_args)
    }
}

#[cfg(target_os = "linux")]
fn admitted_identity(
    config: &jackin_protocol::CapsuleConfig,
    instance: Option<&str>,
) -> Option<SessionIdentity> {
    match instance {
        Some(id) => config.identity_for_instance(id),
        None => config.shell_identity,
    }
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(test)]
mod tests;

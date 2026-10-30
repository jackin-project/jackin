#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::{Result, bail};
#[cfg(target_os = "linux")]
use jackin_protocol::SessionIdentity;

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

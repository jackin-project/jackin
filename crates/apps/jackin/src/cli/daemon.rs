use clap::Subcommand;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DaemonCommand {
    Serve,

    Install,

    Uninstall,

    Start,

    Stop,

    Restart,

    Status,
}

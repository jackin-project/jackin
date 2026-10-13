use clap::Subcommand;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DaemonCommand {
    /// Run the host daemon in the foreground
    Serve,
    /// Install a per-user launchd/systemd service file
    Install,
    /// Remove the per-user launchd/systemd service file
    Uninstall,
    /// Start the daemon for this user
    Start,
    /// Ask the running daemon to stop
    Stop,
    /// Stop then start the daemon
    Restart,
    /// Show daemon protocol and lifecycle status
    Status,
}

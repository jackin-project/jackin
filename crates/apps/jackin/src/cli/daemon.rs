use clap::Subcommand;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DaemonCommand {
    #[command(
        about = "Run the host daemon in the foreground",
        long_about = "Run the host daemon in the foreground"
    )]
    Serve,
    #[command(
        about = "Install a per-user launchd/systemd service file",
        long_about = "Install a per-user launchd/systemd service file"
    )]
    Install,
    #[command(
        about = "Remove the per-user launchd/systemd service file",
        long_about = "Remove the per-user launchd/systemd service file"
    )]
    Uninstall,
    #[command(
        about = "Start the daemon for this user",
        long_about = "Start the daemon for this user"
    )]
    Start,
    #[command(
        about = "Ask the running daemon to stop",
        long_about = "Ask the running daemon to stop"
    )]
    Stop,
    #[command(
        about = "Stop then start the daemon",
        long_about = "Stop then start the daemon"
    )]
    Restart,
    #[command(
        about = "Show daemon protocol and lifecycle status",
        long_about = "Show daemon protocol and lifecycle status"
    )]
    Status,
}

use clap::{Args, Subcommand};

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DaemonCommand {
    /// Install and start the per-user daemon service
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Install,
    /// Stop and remove the per-user daemon service
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Uninstall,
    /// Start the daemon in the background
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Start,
    /// Stop the running daemon
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Stop,
    /// Restart the daemon
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Restart,
    /// Print daemon status
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Status,
    /// Tail daemon logs
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Logs(LogsArgs),
    /// Run the daemon in the foreground
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Serve,
    /// Ask the daemon to warm host-side caches
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Warm,
    /// Send a host macOS notification through the daemon
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Notify(NotifyArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct LogsArgs {
    /// Number of recent lines to print
    #[arg(short = 'n', long, default_value_t = 80)]
    pub lines: usize,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct NotifyArgs {
    /// Notification title
    #[arg(long)]
    pub title: String,
    /// Notification body
    #[arg(long)]
    pub body: String,
    /// Notification urgency: low, normal, or high; high requests a native sound
    #[arg(long, default_value = "normal")]
    pub urgency: String,
}

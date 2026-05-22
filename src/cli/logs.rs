use clap::Args;

use super::{BANNER, HELP_STYLES};

/// Show or follow the in-container multiplexer log
///
/// The multiplexer writes one line per operator action, PTY event, and
/// PID-1 reap to a host-readable file under each container's state
/// directory. This subcommand resolves that path and either prints it
/// (default), tails the file, or copies the last N lines into a
/// shareable bundle file for bug reports.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin logs                          # list active containers + log paths
  jackin logs jk-k7p9m2xq-agentsmith   # print last 200 lines of one container's log
  jackin logs the-architect --follow   # tail -f the log
  jackin logs the-architect --tail 500
  jackin logs the-architect --bundle /tmp/jackin-bug.txt"
)]
pub struct LogsArgs {
    /// Container base name, instance ID, or role selector. Omitted: list
    /// every container with a log file. Required for --print, --follow,
    /// or --bundle.
    pub selector: Option<String>,

    /// Print the resolved log path instead of its content. Implied when
    /// no other action flag is set and a selector is given.
    #[arg(long, conflicts_with_all = ["follow", "bundle"])]
    pub path: bool,

    /// Last N lines to print (default 200). Ignored when --follow is set.
    #[arg(long, value_name = "N", default_value_t = 200)]
    pub tail: usize,

    /// Follow the log as it grows (like `tail -f`). Ctrl+C to stop.
    #[arg(long, conflicts_with_all = ["path", "bundle"])]
    pub follow: bool,

    /// Copy the last `--tail` lines into a fresh file at the given path
    /// (for pasting into bug reports). Existing file is overwritten.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["path", "follow"])]
    pub bundle: Option<std::path::PathBuf>,
}

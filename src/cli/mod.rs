use clap::builder::styling::{AnsiColor, Effects, Styles};

pub(super) const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::White.on_default())
    .valid(AnsiColor::BrightGreen.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

pub(super) const BANNER: &str = concat!(
    "\n\n\n",
    "\x1b[94m",
    "    │ │╷│ │╷│ ╷  │╷│ │╷│ │╷│\n",
    "    │ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│\n",
    "    ╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵\n",
    "               ╵\n",
    "\x1b[0m",
    "\x1b[1;97m",
    "          j a c k i n\n",
    "\x1b[0m",
    "\x1b[38;5;67m",
    "       operator terminal\n",
    "\x1b[0m",
);

pub mod config;
pub mod root;
pub mod workspace;

pub use config::{AuthCommand, ConfigCommand, MountCommand, TrustCommand};
pub use root::{Cli, Command};
pub use workspace::WorkspaceCommand;

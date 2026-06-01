//! Non-TUI 1Password CLI availability services.

use crate::operator_env::{OpCli, OpRunner as _};

/// Probe whether the 1Password CLI is available for this console session.
pub fn cli_available() -> bool {
    OpCli::new_probe().probe().is_ok()
}

//! Non-TUI 1Password CLI availability services.

use crate::operator_env::{OpCli, OpRunner as _};
use jackin_tui::runtime::BlockingSubscription;

/// Probe whether the 1Password CLI is available for this console session.
pub fn cli_available() -> bool {
    OpCli::new_probe().probe().is_ok()
}

/// Validate that a picked 1Password reference can be read without blocking the TUI.
pub fn start_ref_validation(
    op_ref: crate::operator_env::OpRef,
) -> BlockingSubscription<anyhow::Result<()>> {
    let runner = OpCli::new().with_account(op_ref.account.clone());
    let op = op_ref.op;
    jackin_tui::runtime::spawn_blocking_subscription(move || runner.read(&op).map(|_| ()))
}

/// Validate that a picked 1Password reference can be read.
pub fn validate_ref(op_ref: &crate::operator_env::OpRef) -> anyhow::Result<()> {
    let runner = OpCli::new().with_account(op_ref.account.clone());
    runner.read(&op_ref.op).map(|_| ())
}

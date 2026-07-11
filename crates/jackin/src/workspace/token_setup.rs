//! Workspace Claude token setup orchestrator.
//!
//! Moved to `crates/jackin-env/src/token_setup.rs`.

pub use jackin_env::{
    DEFAULT_FIELD_LABEL, DEFAULT_ITEM_TEMPLATE, DoctorReport, EditExistingTarget, JACKIN_TAG,
    RevokeReport, TokenSetupArgs, TokenSetupReport, TokenSetupScope, expiry_days_for_launch,
    mint_token_value, prior_token_slot, run_doctor, run_revoke, run_setup,
    tags_indicate_jackin_owned, vault_for_rotate,
};

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace Claude token setup orchestrator.
//!
//! Moved to `crates/jackin-env/src/token_setup.rs`.

pub use jackin_env::{
    DEFAULT_FIELD_LABEL, DEFAULT_ITEM_CATEGORY, DEFAULT_ITEM_TEMPLATE, DoctorReport,
    EditExistingTarget, JACKIN_TAG, RevokeReport, TokenSetupArgs, TokenSetupReport,
    TokenSetupScope, WORKSPACE_TAG_PREFIX, clear_expiry_stamp, days_until_expiry,
    expiry_cache_path, expiry_days_for_launch, mint_token_value, prior_token_slot, run_doctor,
    run_doctor_with_runner, run_revoke, run_revoke_with_runner, run_setup, run_setup_with_runner,
    tags_indicate_jackin_owned, vault_for_rotate, write_expiry_stamp,
};

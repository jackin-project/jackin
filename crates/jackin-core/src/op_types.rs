// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! 1Password picker data types shared between `jackin-env` and `jackin-console`.
//!
//! These plain-data structs are the transfer objects for `op` CLI results.
//! Defining them here breaks the `jackin-env → jackin-console` layering
//! inversion: both crates now import from `jackin-core` rather than
//! `jackin-env` importing from the TUI-layer `jackin-console`.

/// 1Password account metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpAccount {
    pub id: String,
    pub email: String,
    pub url: String,
}

/// 1Password vault metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpVault {
    pub id: String,
    pub name: String,
}

/// 1Password item metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpItem {
    pub id: String,
    pub name: String,
    pub subtitle: String,
}

/// 1Password field metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpField {
    pub id: String,
    pub label: String,
    pub field_type: String,
    pub concealed: bool,
    pub reference: String,
}

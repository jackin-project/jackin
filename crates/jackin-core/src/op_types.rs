//! 1Password picker data types shared between `jackin-env` and `jackin-console`.
//!
//! These plain-data structs are the transfer objects for `op` CLI results.
//! Defining them here breaks the `jackin-env → jackin-console` layering
//! inversion: both crates now import from `jackin-core` rather than
//! `jackin-env` importing from the TUI-layer `jackin-console`.

/// 1Password account metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpAccount {
    /// Account UUID.
    pub id: String,
    /// Sign-in email for display.
    pub email: String,
    /// Account URL (1Password domain).
    pub url: String,
}

/// 1Password vault metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpVault {
    /// Vault UUID.
    pub id: String,
    /// Human-readable vault name.
    pub name: String,
}

/// 1Password item metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpItem {
    /// Item UUID.
    pub id: String,
    /// Item title.
    pub name: String,
    /// Secondary subtitle line from `op` list output.
    pub subtitle: String,
}

/// 1Password field metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpField {
    /// Field UUID or stable id.
    pub id: String,
    /// Field label shown to the operator.
    pub label: String,
    /// Field type string from `op` (e.g. `"STRING"`, `"CONCEALED"`).
    pub field_type: String,
    /// Whether the field value is concealed (password-like).
    pub concealed: bool,
    /// Full `op://…` secret reference for this field.
    pub reference: String,
}

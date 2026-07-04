// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `OpStructRunner` and `OpWriteRunner` traits for structured 1Password access.

use jackin_core::op_types::{OpAccount, OpField, OpItem, OpVault};
use jackin_core::{FieldTarget, OpRef};

/// Structural `op` queries used by the picker — metadata browser.
///
/// Distinct from [`super::OpRunner`] (single-value resolution): the picker is
/// a metadata browser and must never deserialize a secret value.
pub trait OpStructRunner {
    /// Doubles as the sign-in probe before any other call.
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>>;
    /// `account = None` lets `op` use its default-account context.
    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>>;
    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>>;
    fn item_get(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<OpField>>;
}

/// Mutating 1Password operations used by the workspace-token setup orchestrator.
///
/// Held in a separate trait from [`OpStructRunner`] so the read-only safety
/// contract on the picker's `OpCache` cannot be accidentally widened.
/// All write paths take secret material on **stdin**, never on argv.
pub trait OpWriteRunner {
    /// Create an item and return the canonical `op://...` reference.
    fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef>;

    /// Overwrite or add a single field in an existing 1Password item.
    fn item_field_set(
        &self,
        item_id: &str,
        vault_id: &str,
        target: &FieldTarget,
        value: &str,
        section: Option<&str>,
    ) -> anyhow::Result<OpRef>;

    /// Delete an item entirely.
    fn item_delete(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Read an item's `tags` array.
    fn item_tags(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<String>>;
}

/// Parameters for [`OpWriteRunner::item_create`]. Borrowed-form to avoid
/// cloning every string at the call site.
#[derive(Debug, Clone, Copy)]
pub struct OpItemCreateParams<'a> {
    pub vault_id: &'a str,
    pub title: &'a str,
    pub category: &'a str,
    pub field_label: &'a str,
    pub value: &'a str,
    pub notes_plain: Option<&'a str>,
    pub tags: &'a [&'a str],
    pub section: Option<&'a str>,
}

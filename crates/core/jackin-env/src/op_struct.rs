// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{FieldTarget, OpRef};
use jackin_core::{OpAccount, OpField, OpItem, OpItemDetail, OpVault};

pub trait OpStructRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>>;

    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>>;

    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>>;

    fn item_get(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<OpItemDetail<OpField>>;
}

pub trait OpWriteRunner {
    fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef>;

    fn item_field_set(
        &self,
        item_id: &str,
        vault_id: &str,
        target: &FieldTarget,
        value: &str,
        section: Option<&jackin_core::OpSectionTarget>,
    ) -> anyhow::Result<OpRef>;

    fn item_delete(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<()>;

    fn item_tags(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<String>>;
}

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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) struct StubOp {
    vaults: Vec<jackin_core::OpVault>,
    items: Vec<jackin_core::OpItem>,
    fields: Vec<jackin_core::OpField>,
}

impl jackin_env::OpStructRunner for StubOp {
    fn account_list(&self) -> anyhow::Result<Vec<jackin_core::OpAccount>> {
        Ok(Vec::new())
    }

    fn vault_list(&self, _account: Option<&str>) -> anyhow::Result<Vec<jackin_core::OpVault>> {
        Ok(self.vaults.clone())
    }

    fn item_list(
        &self,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<Vec<jackin_core::OpItem>> {
        Ok(self.items.clone())
    }

    fn item_get(
        &self,
        _item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<jackin_core::OpField>> {
        Ok(jackin_core::OpItemDetail {
            fields: self.fields.clone(),
            sections: Vec::new(),
        })
    }
}

pub(super) fn stub_op() -> StubOp {
    StubOp {
        vaults: vec![jackin_core::OpVault {
            id: "vault-uuid".to_owned(),
            name: "Vault".to_owned(),
        }],
        items: vec![jackin_core::OpItem {
            id: "item-uuid".to_owned(),
            name: "Item".to_owned(),
            subtitle: String::new(),
        }],
        fields: vec![jackin_core::OpField {
            id: "field-id".to_owned(),
            section_id: None,
            label: "key".to_owned(),
            field_type: "CONCEALED".to_owned(),
            concealed: true,
            reference: "op://vault-uuid/item-uuid/key".to_owned(),
        }],
    }
}

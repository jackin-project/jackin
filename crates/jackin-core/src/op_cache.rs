// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Session-scoped cache for `op` structural-metadata calls.
//!
// SAFETY: callers must store only structural metadata (UUIDs, names,
// labels, types) in the generic `Account`/`Vault`/`Item`/`Field` slots —
// never field values. Credentials must never enter this cache.

use std::collections::BTreeMap;

/// Sentinel for the "no `--account` flag" case so map keys can stay
/// `String` instead of `Option<String>`.
pub const DEFAULT_ACCOUNT_KEY: &str = "";

// SAFETY: each `Vec` holds only the structural-metadata projection of an
// `op` entity; the caller-supplied types deliberately omit any field value.
#[derive(Debug, Clone)]
pub struct OpCache<Account, Vault, Item, Field> {
    accounts: Option<Vec<Account>>,
    vaults: BTreeMap<String, Vec<Vault>>,
    items: BTreeMap<(String, String), Vec<Item>>,
    fields: BTreeMap<(String, String, String), Vec<Field>>,
}

fn account_key(account: Option<&str>) -> String {
    account.unwrap_or(DEFAULT_ACCOUNT_KEY).to_owned()
}

impl<Account, Vault, Item, Field> Default for OpCache<Account, Vault, Item, Field> {
    fn default() -> Self {
        Self {
            accounts: None,
            vaults: BTreeMap::new(),
            items: BTreeMap::new(),
            fields: BTreeMap::new(),
        }
    }
}

impl<Account, Vault, Item, Field> OpCache<Account, Vault, Item, Field>
where
    Account: Clone,
    Vault: Clone,
    Item: Clone,
    Field: Clone,
{
    #[must_use]
    pub fn get_accounts(&self) -> Option<Vec<Account>> {
        self.accounts.clone()
    }

    pub fn put_accounts(&mut self, accounts: Vec<Account>) {
        self.accounts = Some(accounts);
    }

    pub fn invalidate_accounts(&mut self) {
        self.accounts = None;
    }

    #[must_use]
    pub fn get_vaults(&self, account: Option<&str>) -> Option<Vec<Vault>> {
        self.vaults.get(&account_key(account)).cloned()
    }

    pub fn put_vaults(&mut self, account: Option<&str>, vaults: Vec<Vault>) {
        self.vaults.insert(account_key(account), vaults);
    }

    pub fn invalidate_vaults(&mut self, account: Option<&str>) {
        self.vaults.remove(&account_key(account));
    }

    #[must_use]
    pub fn get_items(&self, account: Option<&str>, vault_id: &str) -> Option<Vec<Item>> {
        self.items
            .get(&(account_key(account), vault_id.to_owned()))
            .cloned()
    }

    pub fn put_items(&mut self, account: Option<&str>, vault_id: &str, items: Vec<Item>) {
        self.items
            .insert((account_key(account), vault_id.to_owned()), items);
    }

    pub fn invalidate_items(&mut self, account: Option<&str>, vault_id: &str) {
        self.items
            .remove(&(account_key(account), vault_id.to_owned()));
    }

    #[must_use]
    pub fn get_fields(
        &self,
        account: Option<&str>,
        vault_id: &str,
        item_id: &str,
    ) -> Option<Vec<Field>> {
        self.fields
            .get(&(
                account_key(account),
                vault_id.to_owned(),
                item_id.to_owned(),
            ))
            .cloned()
    }

    pub fn put_fields(
        &mut self,
        account: Option<&str>,
        vault_id: &str,
        item_id: &str,
        fields: Vec<Field>,
    ) {
        self.fields.insert(
            (
                account_key(account),
                vault_id.to_owned(),
                item_id.to_owned(),
            ),
            fields,
        );
    }

    pub fn invalidate_fields(&mut self, account: Option<&str>, vault_id: &str, item_id: &str) {
        self.fields.remove(&(
            account_key(account),
            vault_id.to_owned(),
            item_id.to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests;

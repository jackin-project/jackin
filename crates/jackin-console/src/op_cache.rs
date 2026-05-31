//! Session-scoped cache for `op` structural-metadata calls.
//!
// SAFETY: stores only structural metadata (UUIDs, names, labels,
// types). Field values are never read from `op item get` JSON — see
// RawOpField in operator_env.rs. Credentials never enter this cache.

use std::collections::BTreeMap;

/// Sentinel for the "no `--account` flag" case so map keys can stay
/// `String` instead of `Option<String>`.
pub const DEFAULT_ACCOUNT_KEY: &str = "";

// SAFETY: every value here is a typed `Vec<Op*>` from `OpStructRunner`,
// which deliberately omits the `value` field — see `RawOpField` in
// `operator_env.rs`.
#[derive(Debug, Clone)]
pub struct OpCache<Account, Vault, Item, Field> {
    accounts: Option<Vec<Account>>,
    vaults: BTreeMap<String, Vec<Vault>>,
    items: BTreeMap<(String, String), Vec<Item>>,
    fields: BTreeMap<(String, String, String), Vec<Field>>,
}

fn account_key(account: Option<&str>) -> String {
    account.unwrap_or(DEFAULT_ACCOUNT_KEY).to_string()
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
            .get(&(account_key(account), vault_id.to_string()))
            .cloned()
    }

    pub fn put_items(&mut self, account: Option<&str>, vault_id: &str, items: Vec<Item>) {
        self.items
            .insert((account_key(account), vault_id.to_string()), items);
    }

    pub fn invalidate_items(&mut self, account: Option<&str>, vault_id: &str) {
        self.items
            .remove(&(account_key(account), vault_id.to_string()));
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
                vault_id.to_string(),
                item_id.to_string(),
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
                vault_id.to_string(),
                item_id.to_string(),
            ),
            fields,
        );
    }

    pub fn invalidate_fields(&mut self, account: Option<&str>, vault_id: &str, item_id: &str) {
        self.fields.remove(&(
            account_key(account),
            vault_id.to_string(),
            item_id.to_string(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    #[derive(Clone)]
    struct Account {
        id: String,
        email: String,
        url: String,
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    struct Vault {
        id: String,
        name: String,
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    struct Item {
        id: String,
        name: String,
        subtitle: String,
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    struct Field {
        id: String,
        label: String,
        field_type: String,
        concealed: bool,
        reference: String,
    }

    type TestCache = OpCache<Account, Vault, Item, Field>;

    fn account(id: &str) -> Account {
        Account {
            id: id.to_string(),
            email: format!("{id}@example.com"),
            url: format!("{id}.1password.com"),
        }
    }

    fn vault(name: &str) -> Vault {
        Vault {
            id: format!("v-{name}"),
            name: name.to_string(),
        }
    }

    fn item(name: &str) -> Item {
        Item {
            id: format!("i-{name}"),
            name: name.to_string(),
            subtitle: String::new(),
        }
    }

    fn field(label: &str) -> Field {
        Field {
            id: label.to_string(),
            label: label.to_string(),
            field_type: "STRING".to_string(),
            concealed: false,
            reference: String::new(),
        }
    }

    #[test]
    fn empty_cache_misses_everything() {
        let cache = TestCache::default();
        assert!(cache.get_accounts().is_none());
        assert!(cache.get_vaults(None).is_none());
        assert!(cache.get_vaults(Some("acct1")).is_none());
        assert!(cache.get_items(None, "v1").is_none());
        assert!(cache.get_fields(None, "v1", "i1").is_none());
    }

    #[test]
    fn put_then_get_round_trips() {
        let mut cache = TestCache::default();

        cache.put_accounts(vec![account("a1"), account("a2")]);
        let got = cache.get_accounts().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, "a1");

        cache.put_vaults(Some("a1"), vec![vault("Personal")]);
        cache.put_vaults(None, vec![vault("Default")]);
        assert_eq!(
            cache.get_vaults(Some("a1")).unwrap()[0].name,
            "Personal".to_string()
        );
        assert_eq!(
            cache.get_vaults(None).unwrap()[0].name,
            "Default".to_string(),
            "default-account uses the empty-string key but reads back via None"
        );
        cache.put_items(Some("a1"), "v-Personal", vec![item("API Keys")]);
        assert_eq!(
            cache.get_items(Some("a1"), "v-Personal").unwrap()[0].name,
            "API Keys"
        );

        cache.put_fields(
            Some("a1"),
            "v-Personal",
            "i-API Keys",
            vec![field("password")],
        );
        assert_eq!(
            cache
                .get_fields(Some("a1"), "v-Personal", "i-API Keys")
                .unwrap()[0]
                .label,
            "password"
        );
    }

    #[test]
    fn invalidate_removes_entry() {
        let mut cache = TestCache::default();
        cache.put_vaults(Some("a1"), vec![vault("Personal")]);
        cache.put_items(Some("a1"), "v1", vec![item("API Keys")]);
        cache.put_fields(Some("a1"), "v1", "i1", vec![field("password")]);

        cache.invalidate_vaults(Some("a1"));
        assert!(cache.get_vaults(Some("a1")).is_none());

        cache.invalidate_items(Some("a1"), "v1");
        assert!(cache.get_items(Some("a1"), "v1").is_none());

        cache.invalidate_fields(Some("a1"), "v1", "i1");
        assert!(cache.get_fields(Some("a1"), "v1", "i1").is_none());
    }

    #[test]
    fn account_keys_are_distinct() {
        let mut cache = TestCache::default();
        cache.put_vaults(Some("a1"), vec![vault("a1-Personal")]);
        cache.put_vaults(Some("a2"), vec![vault("a2-Personal")]);
        cache.put_vaults(None, vec![vault("default-Personal")]);

        assert_eq!(cache.get_vaults(Some("a1")).unwrap()[0].name, "a1-Personal");
        assert_eq!(cache.get_vaults(Some("a2")).unwrap()[0].name, "a2-Personal");
        assert_eq!(cache.get_vaults(None).unwrap()[0].name, "default-Personal");
    }

    /// Compile-time guard for the structural field shape cached by this
    /// generic helper. Root `OpField` carries the same field set.
    #[test]
    fn op_cache_does_not_store_field_values() {
        let mut cache = TestCache::default();
        cache.put_fields(None, "v1", "i1", vec![field("password")]);
        let stored = cache.get_fields(None, "v1", "i1").unwrap();
        for f in stored {
            let Field {
                id: _,
                label: _,
                field_type: _,
                concealed: _,
                reference: _,
            } = f;
        }
    }
}

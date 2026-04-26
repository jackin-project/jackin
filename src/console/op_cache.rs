//! Session-scoped cache for `op` structural-metadata calls used by the
//! 1Password picker.
//!
//! The cache is allocated once per `jackin console` invocation (on
//! [`crate::console::state::ConsoleState`]) and consulted by the picker
//! before every `OpStructRunner` call. Subsequent drilling within the
//! same session reuses the parsed `Vec<OpAccount>` / `Vec<OpVault>` /
//! `Vec<OpItem>` / `Vec<OpField>` rather than re-spawning `op`, which
//! makes navigation feel instant after the first call.
//!
// SAFETY: the cache stores only structural metadata (account UUIDs, vault
// names, item titles, field labels/types). Field values are never read
// from `op item get` JSON — see RawOpField in operator_env.rs which omits
// the `value` key. Credentials never enter this cache by construction.
//!
//! Account-scoped keys use the `account_id` UUID as reported by `op
//! account list`. Single-account / default-account setups use the empty
//! string `""` as the account key — matching the "no `--account` flag"
//! invocation the runner makes when handed `None`.
//!
//! Cache hits are exact: a query for `(account_id, vault_id, item_id)`
//! either returns a clone of the stored vector or a miss. There is no
//! partial / fuzzy invalidation. The picker's `r` keypress invalidates a
//! single pane's entry so the next call refetches.

use std::collections::BTreeMap;

use crate::operator_env::{OpAccount, OpField, OpItem, OpVault};

/// Empty-string sentinel for the "no `--account` flag" case.
///
/// `OpStructRunner::vault_list(None)` and friends invoke `op` without a
/// `--account` flag, falling back to `op`'s default-account context.
/// We store those results under the empty-string account key so the
/// cache map can use a single concrete `String` type without an `Option`
/// indirection at every call site.
pub const DEFAULT_ACCOUNT_KEY: &str = "";

/// Session-scoped cache of structural `op` metadata.
///
// SAFETY: every value in this struct is the typed `Vec<Op*>` produced by
// `OpStructRunner` — these types deliberately omit the `value` field of
// `op item get` (see `RawOpField` in `operator_env.rs`). Credentials
// never enter the cache by construction.
#[derive(Debug, Default, Clone)]
pub struct OpCache {
    /// Result of the most recent `op account list` call. `None` until
    /// the first call.
    accounts: Option<Vec<OpAccount>>,
    /// Result of `op vault list` keyed by `account_id` (or
    /// [`DEFAULT_ACCOUNT_KEY`] for the default-account case).
    vaults: BTreeMap<String, Vec<OpVault>>,
    /// Result of `op item list` keyed by `(account_id, vault_id)`.
    items: BTreeMap<(String, String), Vec<OpItem>>,
    /// Result of `op item get` keyed by `(account_id, vault_id, item_id)`.
    fields: BTreeMap<(String, String, String), Vec<OpField>>,
}

/// Map an `Option<&str>` account argument to the cache's
/// owned-`String` key. Threading `None` through as the empty string
/// keeps the map keys homogeneous.
fn account_key(account: Option<&str>) -> String {
    account.unwrap_or(DEFAULT_ACCOUNT_KEY).to_string()
}

impl OpCache {
    /// Return a clone of the cached accounts list, or `None` on a miss.
    #[must_use]
    pub fn get_accounts(&self) -> Option<Vec<OpAccount>> {
        self.accounts.clone()
    }

    /// Store the result of an `op account list` call.
    pub fn put_accounts(&mut self, accounts: Vec<OpAccount>) {
        self.accounts = Some(accounts);
    }

    /// Forget the cached accounts list. Next call will refetch.
    pub fn invalidate_accounts(&mut self) {
        self.accounts = None;
    }

    /// Return a clone of the cached vault list for `account` (use `None`
    /// for the default-account case), or `None` on a miss.
    #[must_use]
    pub fn get_vaults(&self, account: Option<&str>) -> Option<Vec<OpVault>> {
        self.vaults.get(&account_key(account)).cloned()
    }

    pub fn put_vaults(&mut self, account: Option<&str>, vaults: Vec<OpVault>) {
        self.vaults.insert(account_key(account), vaults);
    }

    pub fn invalidate_vaults(&mut self, account: Option<&str>) {
        self.vaults.remove(&account_key(account));
    }

    #[must_use]
    pub fn get_items(&self, account: Option<&str>, vault_id: &str) -> Option<Vec<OpItem>> {
        self.items
            .get(&(account_key(account), vault_id.to_string()))
            .cloned()
    }

    pub fn put_items(&mut self, account: Option<&str>, vault_id: &str, items: Vec<OpItem>) {
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
    ) -> Option<Vec<OpField>> {
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
        fields: Vec<OpField>,
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
    use crate::operator_env::{OpAccount, OpField, OpItem, OpVault};

    fn account(id: &str) -> OpAccount {
        OpAccount {
            id: id.to_string(),
            email: format!("{id}@example.com"),
            url: format!("{id}.1password.com"),
        }
    }

    fn vault(name: &str) -> OpVault {
        OpVault {
            id: format!("v-{name}"),
            name: name.to_string(),
        }
    }

    fn item(name: &str) -> OpItem {
        OpItem {
            id: format!("i-{name}"),
            name: name.to_string(),
            subtitle: String::new(),
        }
    }

    fn field(label: &str) -> OpField {
        OpField {
            id: label.to_string(),
            label: label.to_string(),
            field_type: "STRING".to_string(),
            concealed: false,
            reference: String::new(),
        }
    }

    #[test]
    fn empty_cache_misses_everything() {
        let cache = OpCache::default();
        assert!(cache.get_accounts().is_none());
        assert!(cache.get_vaults(None).is_none());
        assert!(cache.get_vaults(Some("acct1")).is_none());
        assert!(cache.get_items(None, "v1").is_none());
        assert!(cache.get_fields(None, "v1", "i1").is_none());
    }

    #[test]
    fn put_then_get_round_trips() {
        let mut cache = OpCache::default();

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
        let mut cache = OpCache::default();
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
        // Vaults stored against `Some("a1")` and `Some("a2")` must not
        // collide, and neither must collide with the `None`/default key.
        let mut cache = OpCache::default();
        cache.put_vaults(Some("a1"), vec![vault("a1-Personal")]);
        cache.put_vaults(Some("a2"), vec![vault("a2-Personal")]);
        cache.put_vaults(None, vec![vault("default-Personal")]);

        assert_eq!(cache.get_vaults(Some("a1")).unwrap()[0].name, "a1-Personal");
        assert_eq!(cache.get_vaults(Some("a2")).unwrap()[0].name, "a2-Personal");
        assert_eq!(cache.get_vaults(None).unwrap()[0].name, "default-Personal");
    }

    /// Compile-time guarantee — mirrors the `op_struct_runner_item_get_parses_fields_no_value`
    /// test in `operator_env.rs`. If `OpField` ever grows a `value`
    /// field, this struct-pattern destructure will fail to compile, and
    /// callers will be forced to re-review the cache's trust model.
    #[test]
    fn op_cache_does_not_store_field_values() {
        let mut cache = OpCache::default();
        cache.put_fields(None, "v1", "i1", vec![field("password")]);
        let stored = cache.get_fields(None, "v1", "i1").unwrap();
        for f in stored {
            // Exhaustive destructure: any new field added to `OpField`
            // (in particular `value`) breaks compilation here.
            let OpField {
                id: _,
                label: _,
                field_type: _,
                concealed: _,
                reference: _,
            } = f;
        }
    }
}

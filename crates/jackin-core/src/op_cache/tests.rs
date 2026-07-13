// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `op_cache`.
use super::*;

#[expect(dead_code, reason = "test DTO mirrors op account payload fields")]
#[derive(Clone)]
struct Account {
    id: String,
    email: String,
    url: String,
}

#[expect(dead_code, reason = "test DTO mirrors op vault payload fields")]
#[derive(Clone)]
struct Vault {
    id: String,
    name: String,
}

#[expect(dead_code, reason = "test DTO mirrors op item payload fields")]
#[derive(Clone)]
struct Item {
    id: String,
    name: String,
    subtitle: String,
}

#[expect(dead_code, reason = "test DTO mirrors op field payload fields")]
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
        id: id.to_owned(),
        email: format!("{id}@example.com"),
        url: format!("{id}.1password.com"),
    }
}

fn vault(name: &str) -> Vault {
    Vault {
        id: format!("v-{name}"),
        name: name.to_owned(),
    }
}

fn item(name: &str) -> Item {
    Item {
        id: format!("i-{name}"),
        name: name.to_owned(),
        subtitle: String::new(),
    }
}

fn field(label: &str) -> Field {
    Field {
        id: label.to_owned(),
        label: label.to_owned(),
        field_type: "STRING".to_owned(),
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
        "Personal".to_owned()
    );
    assert_eq!(
        cache.get_vaults(None).unwrap()[0].name,
        "Default".to_owned(),
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

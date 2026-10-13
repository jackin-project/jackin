// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[expect(dead_code, reason = "test DTO mirrors op account payload fields")]
#[derive(Clone)]
pub(super) struct Account {
    pub(super) id: String,
    email: String,
    url: String,
}

#[expect(dead_code, reason = "test DTO mirrors op vault payload fields")]
#[derive(Clone)]
pub(super) struct Vault {
    id: String,
    pub(super) name: String,
}

#[expect(dead_code, reason = "test DTO mirrors op item payload fields")]
#[derive(Clone)]
pub(super) struct Item {
    id: String,
    pub(super) name: String,
    subtitle: String,
}

#[expect(dead_code, reason = "test DTO mirrors op field payload fields")]
#[derive(Clone)]
pub(super) struct Field {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) field_type: String,
    pub(super) concealed: bool,
    pub(super) reference: String,
}

pub(super) type TestCache = OpCache<Account, Vault, Item, Field>;

pub(super) fn account(id: &str) -> Account {
    Account {
        id: id.to_owned(),
        email: format!("{id}@example.com"),
        url: format!("{id}.1password.com"),
    }
}

pub(super) fn vault(name: &str) -> Vault {
    Vault {
        id: format!("v-{name}"),
        name: name.to_owned(),
    }
}

pub(super) fn item(name: &str) -> Item {
    Item {
        id: format!("i-{name}"),
        name: name.to_owned(),
        subtitle: String::new(),
    }
}

pub(super) fn field(label: &str) -> Field {
    Field {
        id: label.to_owned(),
        label: label.to_owned(),
        field_type: "STRING".to_owned(),
        concealed: false,
        reference: String::new(),
    }
}

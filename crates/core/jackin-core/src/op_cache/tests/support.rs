// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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


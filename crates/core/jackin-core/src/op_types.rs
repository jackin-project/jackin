// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpAccount {
    pub id: String,
    pub email: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpVault {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpItem {
    pub id: String,
    pub name: String,
    pub subtitle: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpSection {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpItemDetail<Field> {
    pub fields: Vec<Field>,
    pub sections: Vec<OpSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpField {
    pub id: String,
    pub section_id: Option<String>,
    pub label: String,
    pub field_type: String,
    pub concealed: bool,
    pub reference: String,
}

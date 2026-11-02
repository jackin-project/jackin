// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

use crate::display::plain;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compatibility {
    pub openshell_version: String,
    pub openshell_tag: String,
    pub openshell_commit: String,
    pub codex_version: String,
}

#[derive(Debug, Error)]
pub enum CompatibilityError {
    #[error("syntax error in compatibility.lock.toml: {}", plain(.message))]
    Syntax { message: String },
    #[error("missing key {} in compatibility.lock.toml", plain(.key))]
    MissingField { key: String },
}

const LOCK_TEXT: &str = include_str!("../../../compatibility.lock.toml");

fn field(table: &toml::Table, section: &str, key: &str) -> Result<String, CompatibilityError> {
    table
        .get(section)
        .and_then(toml::Value::as_table)
        .and_then(|section| section.get(key))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CompatibilityError::MissingField {
            key: format!("{section}.{key}"),
        })
}

impl Compatibility {
    pub fn pinned() -> Result<Compatibility, CompatibilityError> {
        let table: toml::Table =
            LOCK_TEXT
                .parse()
                .map_err(|error: toml::de::Error| CompatibilityError::Syntax {
                    message: error.message().to_owned(),
                })?;
        Ok(Compatibility {
            openshell_version: field(&table, "openshell", "version")?,
            openshell_tag: field(&table, "openshell", "tag")?,
            openshell_commit: field(&table, "openshell", "commit")?,
            codex_version: field(&table, "codex", "version")?,
        })
    }
}

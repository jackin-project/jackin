// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Parse and format `op://vault/item/field` references used by the 1Password
//! secret-resolution paths.
//!
//! Vocabulary shared by `jackin-env` (token resolution) and `jackin-console`
//! (the Auth-tab picker); it lives here so neither has to reach into the other
//! for the grammar. Not responsible for calling the `op` CLI or rendering any
//! widget.

/// Structured parts of an `op://...` reference.
///
/// Syntax: `op://<vault>/<item>/[<section>/]<field>`. Account scope is
/// not encoded in the path; multi-account picks live separately on the
/// selected account. See
/// <https://developer.1password.com/docs/cli/secret-reference-syntax/>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpReferenceParts {
    pub vault: String,
    pub item: String,
    pub section: Option<String>,
    pub field: String,
}

impl OpReferenceParts {
    /// Operator-facing copy-pasteable `op item delete` invocation.
    pub fn manual_delete_hint(&self) -> impl std::fmt::Display + '_ {
        struct Hint<'a> {
            item: &'a str,
            vault: &'a str,
        }
        impl std::fmt::Display for Hint<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "op item delete {} --vault {}", self.item, self.vault)
            }
        }
        Hint {
            item: &self.item,
            vault: &self.vault,
        }
    }
}

#[must_use]
pub fn parse_op_reference(value: &str) -> Option<OpReferenceParts> {
    let path = value.strip_prefix("op://")?;
    let path = path.split('?').next().unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|s| s.is_empty()) {
        return None;
    }
    match parts.as_slice() {
        [vault, item, field] => Some(OpReferenceParts {
            vault: (*vault).to_owned(),
            item: (*item).to_owned(),
            section: None,
            field: (*field).to_owned(),
        }),
        [vault, item, section, field] => Some(OpReferenceParts {
            vault: (*vault).to_owned(),
            item: (*item).to_owned(),
            section: Some((*section).to_owned()),
            field: (*field).to_owned(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

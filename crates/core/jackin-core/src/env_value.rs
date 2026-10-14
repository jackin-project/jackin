// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::OpSection;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    OpRef(OpRef),

    Extended(Extended),

    Plain(String),
}

impl EnvValue {
    pub const fn as_persisted_str(&self) -> &str {
        match self {
            Self::Plain(s) => s.as_str(),
            Self::OpRef(r) => r.op.as_str(),
            Self::Extended(e) => e.value.as_str(),
        }
    }

    #[must_use]
    pub fn as_display_str(&self) -> String {
        match self {
            Self::Plain(s) => s.clone(),
            Self::OpRef(r) => crate::display_op_breadcrumb_path(&r.path),
            Self::Extended(e) => e.value.clone(),
        }
    }

    pub const fn is_on_demand(&self) -> bool {
        match self {
            Self::OpRef(r) => r.on_demand,
            Self::Extended(e) => e.on_demand,
            Self::Plain(_) => false,
        }
    }
}

impl From<String> for EnvValue {
    fn from(s: String) -> Self {
        Self::Plain(s)
    }
}

impl From<&str> for EnvValue {
    fn from(s: &str) -> Self {
        Self::Plain(s.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpRef {
    #[serde(deserialize_with = "deserialize_op_uri")]
    pub op: String,

    #[serde(rename = "breadcrumb", with = "versioned_op_breadcrumb")]
    pub path: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_demand: bool,
}

mod versioned_op_breadcrumb {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    const VERSION: u8 = 1;

    #[derive(Serialize)]
    struct Encoded<'a> {
        version: u8,
        value: &'a str,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Decoded {
        version: u8,
        value: String,
    }

    pub(super) fn serialize<S>(path: &String, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if crate::parse_op_breadcrumb_path(path).is_none() {
            return Err(serde::ser::Error::custom(
                "1Password breadcrumb is not a valid v1 escaped path",
            ));
        }
        Encoded {
            version: VERSION,
            value: path,
        }
        .serialize(serializer)
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = Decoded::deserialize(deserializer)?;
        if encoded.version != VERSION {
            return Err(serde::de::Error::custom(format!(
                "unsupported 1Password breadcrumb version {}; expected {VERSION}",
                encoded.version
            )));
        }
        if crate::parse_op_breadcrumb_path(&encoded.value).is_none() {
            return Err(serde::de::Error::custom(
                "1Password breadcrumb value is not a valid v1 escaped path",
            ));
        }
        Ok(encoded.value)
    }
}

fn deserialize_op_uri<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    if value.starts_with("op://") {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(format!(
            "op reference must start with op://: {value:?}"
        )))
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extended {
    pub value: String,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_demand: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTarget {
    Existing { id: String, label: String },

    New { label: String },
}

impl FieldTarget {
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Existing { id, .. } => Some(id),
            Self::New { .. } => None,
        }
    }

    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Existing { label, .. } | Self::New { label } => label,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpSectionTarget {
    Existing(OpSection),

    NewLabel(String),
}

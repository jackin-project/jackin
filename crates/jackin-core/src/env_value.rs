// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `EnvValue` and `OpRef`: the universal env-reference vocabulary.
//!
//! `EnvValue` is the canonical representation of an operator-env value —
//! either a 1Password `op://` reference or a literal/`$VAR` string. Both
//! `jackin-config` (workspace env maps) and `jackin-env` (resolution logic)
//! depend on these types, so they live in the leaf crate.

/// A resolved or unresolved operator env value.
///
/// - `OpRef`: a 1Password `op://...` reference to be resolved via `op read`
/// - `Plain`: a literal string or `$VAR` / `${VAR}` expansion reference
///
/// Untagged serde: serde picks the variant by structural shape — inline TOML
/// table with an `op` key → `OpRef`, inline table with a `value` key →
/// `Extended`, scalar string → `Plain`. Legacy bare `op://...` strings
/// deserialize as `Plain` and are passed through to the container as literals
/// (no resolution attempt).
///
/// Variant order is load-bearing for untagged discrimination: `OpRef`
/// (`deny_unknown_fields`) rejects a `{ value = … }` table and falls through to
/// `Extended` (`deny_unknown_fields`), which rejects anything without a `value`
/// field and falls through to the `Plain` scalar fallback. `OpRef` must stay
/// first and `Plain` must stay last.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    /// 1Password `op://` reference table form.
    OpRef(OpRef),
    /// Literal/`$VAR` table form with optional `on_demand`.
    Extended(Extended),
    /// Scalar string literal or `$VAR` / `${VAR}` expansion.
    Plain(String),
}

impl EnvValue {
    /// The raw string used as the persisted representation.
    ///
    /// For `Plain`, the value as stored; for `OpRef`, the UUID-form `op://`
    /// URI (what `op read` resolves). Use this for internal merging and
    /// comparison, not operator-facing display.
    pub const fn as_persisted_str(&self) -> &str {
        match self {
            Self::Plain(s) => s.as_str(),
            Self::OpRef(r) => r.op.as_str(),
            Self::Extended(e) => e.value.as_str(),
        }
    }

    /// Human-readable display form. For `Plain`, same as `as_persisted_str`;
    /// for `OpRef`, the snapshot breadcrumb (`path`) — stale if the 1Password
    /// item was renamed since pick time.
    pub const fn as_display_str(&self) -> &str {
        match self {
            Self::Plain(s) => s.as_str(),
            Self::OpRef(r) => r.path.as_str(),
            Self::Extended(e) => e.value.as_str(),
        }
    }

    /// Whether this value is injected on demand (at `jackin-exec` time) rather
    /// than at container launch. On-demand values are filtered out of the
    /// launch env and resolved later through the operator credential picker.
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

/// Pinned 1Password reference. `op` is the canonical UUID-form URI we pass
/// to `op read`; `path` is a snapshot breadcrumb for human-readable display,
/// captured at pick time.
///
/// # Snapshot semantics
///
/// `op` is the source of truth for resolution; `path` is purely display. If
/// the underlying 1Password item is renamed after the pick, `op` continues to
/// resolve to the same secret while `path` shows the stale name. Drift is
/// operator-visible (the editor breadcrumb shows the stale name) but
/// resolver-invisible (resolution uses `op` only).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpRef {
    /// Canonical `op://` URI.
    /// `op://<vault_id>/<item_id>/[<section_id>/]<field_id>[?attribute=<name>]`
    #[serde(deserialize_with = "deserialize_op_uri")]
    pub op: String,

    /// Snapshot breadcrumb: `<Vault>/<Item>/[<Section>/]<Field>`.
    pub path: String,

    /// 1Password account (id/email) the ref resolves against. `None` = op's
    /// default/only account. Reads pin to this so multi-account vaults
    /// resolve correctly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,

    /// `true` = inject on demand at `jackin-exec` time via the operator
    /// credential picker, never at container launch. `false` (default) =
    /// always-available, injected at launch (current behavior). Omitted from
    /// serialized TOML when `false` so existing refs round-trip unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_demand: bool,
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

/// A literal/`$VAR` env value carrying optional on-demand metadata.
///
/// This is the table form of a `Plain` value: `{ value = "…" }`. It exists so
/// a literal or host-env-expansion value can opt into on-demand injection
/// (`{ value = "$GH_TOKEN", on_demand = true }`) without becoming a 1Password
/// reference. A table without `on_demand` (or with `on_demand = false`) is
/// semantically identical to the equivalent `Plain` scalar; the config editor
/// collapses it back to the scalar form when on-demand is toggled off.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extended {
    /// The literal value or `$VAR` / `${VAR}` expansion reference.
    pub value: String,

    /// `true` = inject on demand at `jackin-exec` time; `false` (default) =
    /// inject at launch. Omitted from serialized TOML when `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_demand: bool,
}

/// Which field an `item_field_set` write targets in an existing item.
///
/// Fusing the field id and label into one type makes the two states the
/// write distinguishes explicit and mutually exclusive: an overwrite always
/// carries the exact op id, while an append carries only the label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTarget {
    /// Overwrite the field with this exact op id.
    Existing {
        /// Exact `op` field id.
        id: String,
        /// Display label for the field.
        label: String,
    },
    /// Append (or overwrite if same label exists) with this label.
    New {
        /// Label for the new (or same-label) field.
        label: String,
    },
}

impl FieldTarget {
    /// The exact op field id to match on, or `None` for the append path.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Existing { id, .. } => Some(id),
            Self::New { .. } => None,
        }
    }

    /// The field label — for an append it names the new field; for an
    /// overwrite it is display/read-back context only.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Existing { label, .. } | Self::New { label } => label,
        }
    }
}

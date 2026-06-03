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
/// table → `OpRef`, scalar string → `Plain`. Legacy bare `op://...` strings
/// deserialize as `Plain` and are passed through to the container as literals
/// (no resolution attempt).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    OpRef(OpRef),
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
        }
    }

    /// Human-readable display form. For `Plain`, same as `as_persisted_str`;
    /// for `OpRef`, the snapshot breadcrumb (`path`) — stale if the 1Password
    /// item was renamed since pick time.
    pub const fn as_display_str(&self) -> &str {
        match self {
            Self::Plain(s) => s.as_str(),
            Self::OpRef(r) => r.path.as_str(),
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
        Self::Plain(s.to_string())
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
    pub op: String,

    /// Snapshot breadcrumb: `<Vault>/<Item>/[<Section>/]<Field>`.
    pub path: String,

    /// 1Password account (id/email) the ref resolves against. `None` = op's
    /// default/only account. Reads pin to this so multi-account vaults
    /// resolve correctly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// Which field an `item_field_set` write targets in an existing item.
///
/// Fusing the field id and label into one type makes the two states the
/// write distinguishes explicit and mutually exclusive: an overwrite always
/// carries the exact op id, while an append carries only the label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTarget {
    /// Overwrite the field with this exact op id.
    Existing { id: String, label: String },
    /// Append (or overwrite if same label exists) with this label.
    New { label: String },
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

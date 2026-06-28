use crate::op_cli::OpCli;
use jackin_core::FieldTarget;
use jackin_core::OpRef;

/// Re-exported from `jackin-env` — canonical definitions live there.
pub use crate::op_struct::{OpItemCreateParams, OpStructRunner, OpWriteRunner};

pub fn default_op_struct_runner() -> std::sync::Arc<dyn OpStructRunner + Send + Sync> {
    std::sync::Arc::new(OpCli::new())
}

/// Re-exported from `jackin-core` — canonical definitions live there so
/// `jackin-env` no longer depends on `jackin-console` for data types.
pub use jackin_core::op_types::{OpAccount, OpField, OpItem, OpVault};

pub type OpCache = jackin_core::op_cache::OpCache<OpAccount, OpVault, OpItem, OpField>;

// Accept either `id` or `account_uuid` so the probe works against
// current and older op CLI shapes. `email` / `url` default to empty
// because older `op` versions may omit them.
#[derive(serde::Deserialize)]
pub(crate) struct RawOpAccount {
    #[serde(alias = "account_uuid")]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) email: String,
    #[serde(default)]
    pub(crate) url: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct RawOpVault {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct RawOpItem {
    pub(crate) id: String,
    pub(crate) title: String,
    // Missing on secure notes and other non-login item types.
    #[serde(default)]
    pub(crate) additional_information: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct RawOpItemDetail {
    #[serde(default)]
    pub(crate) fields: Vec<RawOpField>,
}

// SAFETY: 'value' is intentionally absent from this struct. The picker is a
// metadata browser; serde must not deserialize secret values into memory.
// Any change adding a `value` field here breaks the picker's trust model.
//
// `reference` IS deserialized: the string `op://...` that 1Password's
// CLI emits per field is metadata, not a credential, and the picker
// commits it verbatim instead of synthesizing a path from display
// names (which mishandled section nesting and `/`/whitespace in
// names).
#[derive(serde::Deserialize)]
pub(crate) struct RawOpField {
    pub(super) id: String,
    #[serde(default)]
    pub(super) label: String,
    #[serde(rename = "type", default)]
    pub(super) field_type: String,
    #[serde(default)]
    pub(super) purpose: String,
    #[serde(default)]
    pub(super) reference: String,
}

impl From<RawOpAccount> for OpAccount {
    fn from(raw: RawOpAccount) -> Self {
        Self {
            id: raw.id,
            email: raw.email,
            url: raw.url,
        }
    }
}

impl From<RawOpVault> for OpVault {
    fn from(raw: RawOpVault) -> Self {
        Self {
            id: raw.id,
            name: raw.name,
        }
    }
}

impl From<RawOpItem> for OpItem {
    fn from(raw: RawOpItem) -> Self {
        Self {
            id: raw.id,
            name: raw.title,
            subtitle: raw.additional_information,
        }
    }
}

impl From<RawOpField> for OpField {
    fn from(raw: RawOpField) -> Self {
        let concealed = raw.field_type == "CONCEALED" || raw.purpose == "PASSWORD";
        Self {
            id: raw.id,
            label: raw.label,
            field_type: raw.field_type,
            concealed,
            reference: raw.reference,
        }
    }
}

/// Slug a 1Password section label into a deterministic section id:
/// lowercase, collapse each run of non-alphanumeric characters into a
/// single `_`, and trim leading/trailing `_`. Empty results fall back
/// to `"section"` so the id is always a valid non-empty identifier.
pub(crate) fn op_section_id(label: &str) -> String {
    let mut id = String::with_capacity(label.len());
    let mut pending_underscore = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_underscore && !id.is_empty() {
                id.push('_');
            }
            pending_underscore = false;
            id.push(ch.to_ascii_lowercase());
        } else {
            pending_underscore = true;
        }
    }
    if id.is_empty() {
        "section".to_owned()
    } else {
        id
    }
}

/// Apply a single concealed-field edit to a parsed `op item get` JSON
/// value in place, ready to pipe back to `op item edit`.
///
/// [`FieldTarget::Existing`] is located by its exact op id, so a same-
/// labeled field in another section is never clobbered, and the field's
/// existing `section` is left untouched — overwriting a value must not
/// re-parent the field (GUI-created section ids are opaque, not the
/// `label` slug). A stale id (gone since it was picked) bails loudly
/// rather than appending a stray field. [`FieldTarget::New`] places a new
/// `CONCEALED` field (overwriting a same-label field if one exists),
/// in `section` when one is supplied, registering that section if missing.
pub(crate) fn apply_field_edit(
    item: &mut serde_json::Value,
    target: &FieldTarget,
    value: &str,
    section: Option<&str>,
) -> anyhow::Result<()> {
    let fields = item["fields"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("item has no `fields` array"))?;

    let label = target.label();
    let found = match target {
        FieldTarget::Existing { id, .. } => {
            fields.iter_mut().find(|f| f["id"].as_str() == Some(id))
        }
        FieldTarget::New { label } => fields.iter_mut().find(|f| {
            f["label"].as_str() == Some(label.as_str()) || f["id"].as_str() == Some(label.as_str())
        }),
    };

    let section_id = section.map(op_section_id);
    let mut appended_in_section = false;
    match (found, target) {
        (Some(field), _) => {
            field["value"] = serde_json::Value::String(value.to_owned());
            field["type"] = serde_json::Value::String("CONCEALED".to_owned());
        }
        // A specific field id was requested but is gone (renamed/deleted in
        // 1Password since it was picked, or read from a stale cache). Fail
        // loudly instead of appending a stray label-named field — the
        // read-back would then miss the id and error anyway, but only after
        // mutating the operator's item.
        (None, FieldTarget::Existing { id, .. }) => anyhow::bail!(
            "field id {id:?} not found in the item — it may have been renamed or deleted in \
             1Password since it was picked; re-open the picker to refresh and retry"
        ),
        (None, FieldTarget::New { .. }) => {
            let mut field = serde_json::json!({
                "id": label,
                "label": label,
                "type": "CONCEALED",
                "value": value,
            });
            if let Some(id) = section_id.as_deref() {
                field["section"] = serde_json::json!({ "id": id });
                appended_in_section = true;
            }
            fields.push(field);
        }
    }

    // Register the section only when a new field was actually placed in
    // it; an overwrite never creates or moves sections.
    if appended_in_section && let (Some(id), Some(label)) = (section_id.as_deref(), section) {
        if !item["sections"].is_array() {
            item["sections"] = serde_json::Value::Array(Vec::new());
        }
        let Some(sections) = item["sections"].as_array_mut() else {
            return Ok(());
        };
        if !sections.iter().any(|s| s["id"].as_str() == Some(id)) {
            sections.push(serde_json::json!({ "id": id, "label": label }));
        }
    }
    Ok(())
}

/// Locate the edited field in the JSON `op item edit` returns and build the
/// UUID-form `OpRef`. [`FieldTarget::Existing`] matches by the exact id
/// (stable across the edit); [`FieldTarget::New`] matches by label (case-
/// insensitive), since `op` assigns the new field's id. The `op://` ref is
/// built from UUIDs (vault/item/field ids) so it survives renames; `path`
/// carries the human-readable names for display, same three-segment shape.
pub(crate) fn resolve_edited_field_ref(
    updated: &serde_json::Value,
    target: &FieldTarget,
    vault_id: &str,
    item_id: &str,
    account: Option<String>,
) -> anyhow::Result<OpRef> {
    let label = target.label();
    let updated_fields = updated["fields"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("updated item has no `fields` array"))?;

    let field = updated_fields
        .iter()
        .find(|f| match target {
            FieldTarget::Existing { id, .. } => f["id"].as_str() == Some(id),
            FieldTarget::New { label } => {
                f["label"]
                    .as_str()
                    .is_some_and(|l| l.eq_ignore_ascii_case(label))
                    || f["id"].as_str() == Some(label)
            }
        })
        .ok_or_else(|| {
            let labels: Vec<&str> = updated_fields
                .iter()
                .filter_map(|f| f["label"].as_str())
                .collect();
            anyhow::anyhow!(
                "`op item edit` returned no field matching {target:?}; \
                 observed labels: {labels:?}"
            )
        })?;

    let vid = updated["vault"]["id"].as_str().unwrap_or(vault_id);
    let iid = updated["id"].as_str().unwrap_or(item_id);
    let fid = field["id"].as_str().unwrap_or(label);
    let op_uri = format!("op://{vid}/{iid}/{fid}");

    let vault_name = updated["vault"]["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(vault_id);
    let item_title = updated["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(item_id);
    let field_label_display = field["label"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(label);
    let path = format!("{vault_name}/{item_title}/{field_label_display}");

    Ok(OpRef {
        op: op_uri,
        path,
        account,
        on_demand: false,
    })
}

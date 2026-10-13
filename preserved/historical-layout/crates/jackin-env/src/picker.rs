// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::op_cli::OpCli;
use jackin_core::FieldTarget;
use jackin_core::OpRef;

/// Re-exported from `jackin-env` — canonical definitions live there.
use crate::op_struct::OpStructRunner;

/// Production [`OpStructRunner`] backed by the default [`OpCli`].
pub fn default_op_struct_runner() -> std::sync::Arc<dyn OpStructRunner + Send + Sync> {
    std::sync::Arc::new(OpCli::new())
}

/// Re-exported from `jackin-core` — canonical definitions live there so
/// `jackin-env` no longer depends on `jackin-console` for data types.
pub use jackin_core::{OpAccount, OpField, OpItem, OpVault};

/// Picker cache over 1Password account/vault/item/field metadata.
pub type OpCache = jackin_core::OpCache<OpAccount, OpVault, OpItem, OpField>;

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
    #[serde(default)]
    pub(crate) sections: Vec<RawOpSection>,
}

#[derive(serde::Deserialize)]
pub(crate) struct RawOpSection {
    pub(super) id: String,
    #[serde(default)]
    pub(super) label: String,
}

// SAFETY: 'value' is intentionally absent from this struct. The picker is a
// metadata browser; serde must not deserialize secret values into memory.
// Any change adding a `value` field here breaks the picker's trust model.
//
// `reference` IS deserialized: the string `op://...` that 1Password's
// CLI emits per field is metadata, not a credential. The picker uses it as
// fallback section identity when section metadata is absent; known IDs and
// labels are otherwise kept separate when building the final reference.
#[derive(serde::Deserialize)]
pub(crate) struct RawOpField {
    pub(super) id: String,
    #[serde(default)]
    pub(super) section: Option<RawOpFieldSection>,
    #[serde(default)]
    pub(super) label: String,
    #[serde(rename = "type", default)]
    pub(super) field_type: String,
    #[serde(default)]
    pub(super) purpose: String,
    #[serde(default)]
    pub(super) reference: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct RawOpFieldSection {
    pub(super) id: String,
}

impl From<RawOpSection> for jackin_core::OpSection {
    fn from(raw: RawOpSection) -> Self {
        Self {
            id: raw.id,
            label: raw.label,
        }
    }
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
            section_id: raw.section.map(|section| section.id),
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

/// Keep selected IDs exact; allocate a fresh ID for a newly typed label.
fn target_section_id(
    item: &serde_json::Value,
    section: Option<&jackin_core::OpSectionTarget>,
) -> anyhow::Result<Option<String>> {
    use jackin_core::OpSectionTarget;
    let Some(section) = section else {
        return Ok(None);
    };
    let sections = item["sections"].as_array();
    match section {
        OpSectionTarget::Existing(target_section) => {
            anyhow::ensure!(
                jackin_core::is_valid_op_reference_path_component(&target_section.id),
                "section id {:?} cannot be represented as one `op://` path component; re-open the picker to refresh and retry",
                target_section.id
            );
            let matching_sections: Vec<&serde_json::Value> = sections
                .into_iter()
                .flatten()
                .filter(|section| section["id"].as_str() == Some(target_section.id.as_str()))
                .collect();
            anyhow::ensure!(
                matching_sections.len() == 1,
                "section id {:?} matched {} section records; re-open the picker to refresh and retry",
                target_section.id,
                matching_sections.len()
            );
            Ok(Some(target_section.id.clone()))
        }
        OpSectionTarget::NewLabel(label) => {
            anyhow::ensure!(
                !label.is_empty(),
                "section label must not be empty; cannot create a valid 1Password reference"
            );
            let base = op_section_id(label);
            anyhow::ensure!(
                jackin_core::is_valid_op_reference_path_component(&base),
                "generated section id {base:?} cannot be represented as one `op://` path component"
            );
            let mut id = base.clone();
            let mut suffix = 2_u64;
            while sections.is_some_and(|sections| {
                sections
                    .iter()
                    .any(|section| section["id"].as_str() == Some(&id))
            }) {
                id = format!("{base}_{suffix}");
                suffix = suffix
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("section ID space exhausted"))?;
            }
            Ok(Some(id))
        }
    }
}

/// Preserve and validate selected IDs before piping the edited item to `op`.
fn existing_field_section_id(
    item: &serde_json::Value,
    target: &FieldTarget,
) -> anyhow::Result<Option<String>> {
    let FieldTarget::Existing { id, .. } = target else {
        return Ok(None);
    };
    anyhow::ensure!(
        jackin_core::is_valid_op_reference_path_component(id),
        "field id {id:?} cannot be represented as one `op://` path component; re-open the picker to refresh and retry"
    );
    let Some(field) = item["fields"]
        .as_array()
        .and_then(|fields| fields.iter().find(|field| field["id"].as_str() == Some(id)))
    else {
        // The usual stale-id error is raised by `apply_field_edit` below.
        return Ok(None);
    };
    let field_id = field["id"].as_str().unwrap_or_default();
    anyhow::ensure!(
        jackin_core::is_valid_op_reference_path_component(field_id),
        "field id {field_id:?} cannot be represented as one `op://` path component; re-open the picker to refresh and retry"
    );
    let reference_section = field
        .get("reference")
        .and_then(serde_json::Value::as_str)
        .and_then(jackin_core::parse_op_reference)
        .and_then(|parts| parts.section);
    let section_id = if let Some(section_id) = field
        .pointer("/section/id")
        .and_then(serde_json::Value::as_str)
    {
        section_id
    } else if let Some(section_segment) = reference_section.as_deref() {
        // A reference may contain either an opaque section ID or a label.
        // Resolve it against item metadata before reusing it in an edit.
        let sections = item["sections"].as_array().ok_or_else(|| {
            anyhow::anyhow!(
                "field id {id:?} has section segment {section_segment:?}, but item section metadata is unavailable; re-open the picker to refresh and retry"
            )
        })?;
        let exact_ids: Vec<&str> = sections
            .iter()
            .filter(|section| section["id"].as_str() == Some(section_segment))
            .filter_map(|section| section["id"].as_str())
            .collect();
        if let [section_id] = exact_ids.as_slice() {
            *section_id
        } else if exact_ids.len() > 1 {
            anyhow::bail!(
                "field id {id:?} has section segment {section_segment:?} matching {} duplicate section records; re-open the picker to refresh and retry",
                exact_ids.len()
            )
        } else {
            let mut matching_ids: Vec<&str> = Vec::new();
            for section in sections.iter().filter(|section| {
                section["label"]
                    .as_str()
                    .is_some_and(|label| label.eq_ignore_ascii_case(section_segment))
            }) {
                if let Some(section_id) = section["id"].as_str()
                    && !matching_ids.contains(&section_id)
                {
                    matching_ids.push(section_id);
                }
            }
            match matching_ids.as_slice() {
                [section_id] => *section_id,
                [] => anyhow::bail!(
                    "field id {id:?} has section segment {section_segment:?} that does not match a section ID or label; re-open the picker to refresh and retry"
                ),
                matches => anyhow::bail!(
                    "field id {id:?} has ambiguous section segment {section_segment:?} matching {} section IDs; re-open the picker to refresh and retry",
                    matches.len()
                ),
            }
        }
    } else {
        return Ok(None);
    };
    anyhow::ensure!(
        jackin_core::is_valid_op_reference_path_component(section_id),
        "field id {id:?} belongs to section id {section_id:?}, which cannot be represented as one `op://` path component; re-open the picker to refresh and retry"
    );
    Ok(Some(section_id.to_owned()))
}

/// Creation identity is the pair (section, label). Exact field IDs remain
/// global identities for an existing selection.
pub(crate) fn matches_field_target(
    field: &serde_json::Value,
    target: &FieldTarget,
    section_id: Option<&str>,
) -> bool {
    match target {
        FieldTarget::Existing { id, .. } => field["id"].as_str() == Some(id),
        FieldTarget::New { label } => {
            field["section"]["id"].as_str() == section_id && field["label"].as_str() == Some(label)
        }
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
/// `CONCEALED` field (overwriting a same-label field in the selected section),
/// in `section` when one is supplied, registering that section if missing.
pub(crate) fn apply_field_edit(
    item: &mut serde_json::Value,
    target: &FieldTarget,
    value: &str,
    section: Option<&jackin_core::OpSectionTarget>,
) -> anyhow::Result<AppliedFieldEdit> {
    let fields = item["fields"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("item has no `fields` array"))?;
    if let FieldTarget::Existing { id, .. } = target {
        let matches = fields
            .iter()
            .filter(|field| field["id"].as_str() == Some(id))
            .count();
        anyhow::ensure!(
            matches <= 1,
            "field id {id:?} is ambiguous: {matches} fields have this ID; re-open the picker to refresh and retry"
        );
    }

    let section_id = if matches!(target, FieldTarget::New { .. }) {
        target_section_id(item, section)?
    } else {
        if matches!(section, Some(jackin_core::OpSectionTarget::Existing(_))) {
            // A caller-supplied existing section ID is still input to the
            // reference pipeline even when an existing field keeps its own
            // section. Validate it before any remote mutation.
            target_section_id(item, section)?;
        }
        existing_field_section_id(item, target)?
    };
    let label = target.label();
    let matching_indices: Vec<usize> = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            matches_field_target(field, target, section_id.as_deref()).then_some(index)
        })
        .collect();
    anyhow::ensure!(
        matching_indices.len() <= 1,
        "field target {target:?} is ambiguous in section {section_id:?}: {} fields match; re-open the picker to refresh and retry",
        matching_indices.len()
    );
    let found_index = matching_indices.first().copied();
    let existing_field_id = found_index
        .and_then(|index| fields.get(index))
        .and_then(|field| field["id"].as_str())
        .map(str::to_owned);
    if let Some(index) = found_index {
        let field_id = fields
            .get(index)
            .and_then(|field| field["id"].as_str())
            .unwrap_or_default();
        anyhow::ensure!(
            jackin_core::is_valid_op_reference_path_component(field_id),
            "existing field id {field_id:?} cannot be represented as one `op://` path component; re-open the picker to refresh and retry"
        );
    }
    let fields = item["fields"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("item has no `fields` array"))?;
    let found = found_index.and_then(|index| fields.get_mut(index));

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
                // Per the 1Password JSON-template contract, an empty id asks
                // the CLI to generate a unique field ID. Labels are not
                // globally unique because separate sections may reuse them.
                "id": "",
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
    if appended_in_section
        && let (Some(id), Some(jackin_core::OpSectionTarget::NewLabel(label))) =
            (section_id.as_deref(), section)
    {
        if !item["sections"].is_array() {
            item["sections"] = serde_json::Value::Array(Vec::new());
        }
        let Some(sections) = item["sections"].as_array_mut() else {
            return Ok(AppliedFieldEdit {
                section_id,
                existing_field_id,
            });
        };
        if !sections.iter().any(|s| s["id"].as_str() == Some(id)) {
            sections.push(serde_json::json!({ "id": id, "label": label }));
        }
    }
    Ok(AppliedFieldEdit {
        section_id,
        existing_field_id,
    })
}

pub(crate) struct AppliedFieldEdit {
    pub(crate) section_id: Option<String>,
    pub(crate) existing_field_id: Option<String>,
}

/// Locate the edited field in the JSON `op item edit` returns and build its
/// `OpRef`. [`FieldTarget::Existing`] matches by the exact id (stable across
/// the edit); a same-label existing [`FieldTarget::New`] keeps its preflighted
/// ID, while a new field is located by section id and label after `op` assigns
/// its ID. The URI always uses IDs; breadcrumb labels use local path escaping.
pub(crate) fn resolve_edited_field_ref(
    updated: &serde_json::Value,
    target: &FieldTarget,
    vault_id: &str,
    item_id: &str,
    account: Option<String>,
    edit: &AppliedFieldEdit,
    section_target: Option<&jackin_core::OpSectionTarget>,
) -> anyhow::Result<OpRef> {
    let label = target.label();
    let section_id = edit.section_id.as_deref();
    let existing_field_id = edit.existing_field_id.as_deref();

    let updated_fields = updated["fields"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("updated item has no `fields` array"))?;

    let matching_fields: Vec<&serde_json::Value> = updated_fields
        .iter()
        .filter(|field| {
            existing_field_id.map_or_else(
                || matches_field_target(field, target, section_id),
                |id| field["id"].as_str() == Some(id),
            )
        })
        .collect();
    anyhow::ensure!(
        matching_fields.len() == 1,
        "`op item edit` returned {} fields matching {target:?}; expected exactly one",
        matching_fields.len()
    );
    let field = matching_fields.first().copied().ok_or_else(|| {
        let labels: Vec<&str> = updated_fields
            .iter()
            .filter_map(|f| f["label"].as_str())
            .collect();
        anyhow::anyhow!(
            "`op item edit` returned no field matching {target:?}; \
                 observed labels: {labels:?}"
        )
    })?;

    // The edit target IDs were validated before the mutating CLI call. Keep
    // those stable identities for existing entities; only a newly generated
    // field ID must be learned from the CLI response.
    let vid = vault_id;
    let iid = item_id;
    let fid = existing_field_id
        .or_else(|| field["id"].as_str())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`op item edit` returned no field ID for {target:?}"))?;
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
    let returned_section_id = field
        .pointer("/section/id")
        .and_then(serde_json::Value::as_str);
    let section_id = section_id.or(returned_section_id);
    if let Some(id) = section_id {
        anyhow::ensure!(
            jackin_core::is_valid_op_reference_path_component(id),
            "section id {id:?} cannot be represented as one `op://` path component"
        );
    }
    let section_label = section_id
        .and_then(|section_id| {
            updated
                .pointer("/sections")
                .and_then(serde_json::Value::as_array)
                .and_then(|sections| {
                    sections.iter().find(|section| {
                        section.pointer("/id").and_then(serde_json::Value::as_str)
                            == Some(section_id)
                    })
                })
                .and_then(|section| section.pointer("/label"))
                .and_then(serde_json::Value::as_str)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| {
            section_target.and_then(|target| match target {
                jackin_core::OpSectionTarget::Existing(section)
                    if Some(section.id.as_str()) == section_id =>
                {
                    Some(section.label.clone())
                }
                jackin_core::OpSectionTarget::NewLabel(label) if section_id.is_some() => {
                    Some(label.clone())
                }
                _ => None,
            })
        });
    let op_uri = jackin_core::build_op_reference(vid, iid, section_id, fid).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot build a valid `op://` reference from the existing 1Password IDs; re-open the picker to refresh and retry"
        )
    })?;
    let section_display = section_label.as_deref().or(section_id);
    let section_path_part = section_display
        .map(|name| format!("{}/", jackin_core::encode_op_breadcrumb_segment(name)))
        .unwrap_or_default();
    let path = format!(
        "{}/{}/{}{}",
        jackin_core::encode_op_breadcrumb_segment(vault_name),
        jackin_core::encode_op_breadcrumb_segment(item_title),
        section_path_part,
        jackin_core::encode_op_breadcrumb_segment(field_label_display)
    );

    Ok(OpRef {
        op: op_uri,
        path,
        account,
        on_demand: false,
    })
}

#[cfg(test)]
mod tests;

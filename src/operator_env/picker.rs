use super::cli::OpCli;
pub(super) use jackin_core::FieldTarget;
use jackin_core::OpRef;

/// Structural `op` queries used by the picker.
///
/// Distinct from [`super::OpRunner`] (single-value resolution): the picker is
/// a metadata browser and must never deserialize a secret value — see
/// [`RawOpField`].
pub trait OpStructRunner {
    /// Doubles as the sign-in probe before any other call.
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>>;
    /// `account = None` lets `op` use its default-account context.
    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>>;
    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>>;
    fn item_get(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<OpField>>;
}

pub fn default_op_struct_runner() -> std::sync::Arc<dyn OpStructRunner + Send + Sync> {
    std::sync::Arc::new(OpCli::new())
}

/// `id` is the `account_uuid` accepted by `op --account <id>`. `email`
/// and `url` feed the picker's Account pane.
pub type OpAccount = jackin_console::tui::components::op_picker::OpPickerAccount;

pub type OpVault = jackin_console::tui::components::op_picker::OpPickerVault;

/// `name` comes from JSON `title`; `subtitle` from
/// `additional_information` (login username/email, empty on secure
/// notes) — used to disambiguate items sharing a title.
pub type OpItem = jackin_console::tui::components::op_picker::OpPickerItem;

/// Field metadata only — the value is intentionally absent.
///
/// `reference` is the verbatim `op://...` 1Password emits per field;
/// the picker commits this rather than synthesizing a path from
/// display names (synthesis was wrong for sections, names containing
/// `/`, or whitespace).
pub type OpField = jackin_console::tui::components::op_picker::OpPickerField;

pub type OpCache = jackin_console::tui::components::op_picker::OpPickerCache;

// Accept either `id` or `account_uuid` so the probe works against
// current and older op CLI shapes. `email` / `url` default to empty
// because older `op` versions may omit them.
#[derive(serde::Deserialize)]
pub(super) struct RawOpAccount {
    #[serde(alias = "account_uuid")]
    pub(super) id: String,
    #[serde(default)]
    pub(super) email: String,
    #[serde(default)]
    pub(super) url: String,
}

#[derive(serde::Deserialize)]
pub(super) struct RawOpVault {
    pub(super) id: String,
    pub(super) name: String,
}

#[derive(serde::Deserialize)]
pub(super) struct RawOpItem {
    pub(super) id: String,
    pub(super) title: String,
    // Missing on secure notes and other non-login item types.
    #[serde(default)]
    pub(super) additional_information: String,
}

#[derive(serde::Deserialize)]
pub(super) struct RawOpItemDetail {
    #[serde(default)]
    pub(super) fields: Vec<RawOpField>,
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

/// Mutating 1Password operations used by the workspace-token setup
/// orchestrator.
///
/// Held in a separate trait from [`OpStructRunner`] so the read-only
/// SAFETY contract on the picker's `OpCache` cannot be accidentally
/// widened by a future `item_create` impl that decides to memoise
/// its return value.
///
/// All write paths take secret material on **stdin**, never on argv —
/// `op item create login.password=value` is forbidden because that
/// places the secret in `/proc/<pid>/cmdline` where any process on the
/// host with the right uid can read it. Implementations must use
/// `op item create login.password[password]=-` (or the equivalent
/// `--field`) and pipe the value through stdin.
///
/// See `docs/src/content/docs/reference/roadmap/workspace-claude-token-setup.mdx`
/// for the operator-facing flow this trait powers.
pub trait OpWriteRunner {
    /// Create an item and return the canonical `op://...` reference
    /// pointing at the named field. `value` lands on the child's
    /// stdin — never on argv.
    ///
    /// `category` is an `op` item category in the underscore form the
    /// CLI accepts (`"API_CREDENTIAL"`, `"PASSWORD"`, `"SECURE_NOTE"`;
    /// see `op item template list`). `notes_plain` populates the
    /// item's free-form notes block (used by the orchestrator to
    /// stamp `{workspace, host, created, expires, token_sha256_prefix}`).
    fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef>;

    /// Overwrite (or add) a single field in an existing 1Password item.
    ///
    /// For [`FieldTarget::Existing`] the field is located by its exact op
    /// id, its `value` is overwritten, its type is set to `CONCEALED`, and
    /// its existing section placement is left untouched — overwriting a
    /// value must never re-parent the field. For [`FieldTarget::New`] the
    /// field is located by label (overwrite if present); if no such field
    /// exists a new `CONCEALED` field is appended, placed in `section` when
    /// one is given. All other fields and item metadata are preserved.
    ///
    /// The secret value reaches `op` via stdin (GET → modify in-process
    /// → EDIT via stdin), following the same never-on-argv contract as
    /// `item_create`. The implementation issues two `op` invocations:
    /// 1. `op item get <id> --vault <vault> --format json` — fetch the
    ///    full item template.
    /// 2. `op item edit <id> --vault <vault> --format json` — pipe the
    ///    modified template back on stdin.
    fn item_field_set(
        &self,
        item_id: &str,
        vault_id: &str,
        target: &FieldTarget,
        value: &str,
        section: Option<&str>,
    ) -> anyhow::Result<OpRef>;

    /// Delete an item entirely. Used by
    /// `jackin workspace claude-token revoke --delete-op-item` and
    /// by the rotate flow to remove the prior 1P item once the new
    /// one is wired and validated.
    fn item_delete(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Read an item's `tags` array. Used by the rotate flow to decide
    /// whether the prior item is jackin-owned (and therefore safe to
    /// delete) versus an item the operator adopted via `--reuse` /
    /// interactive edit-in-place (which jackin must not delete, since it
    /// may hold the operator's other fields).
    fn item_tags(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<String>>;
}

/// Parameters for [`OpWriteRunner::item_create`]. Borrowed-form to
/// match the existing `OpStructRunner` style and avoid cloning every
/// string at the call site.
///
/// The `op` account is pinned on the [`OpCli`] instance via
/// [`OpCli::with_account`] before the call — there is no per-call
/// override, mirroring how [`super::OpRunner::read`] consumes
/// [`OpCli::account`].
#[derive(Debug, Clone, Copy)]
pub struct OpItemCreateParams<'a> {
    pub vault_id: &'a str,
    pub title: &'a str,
    /// `op` item category in the underscore form (e.g.
    /// [`crate::workspace::token_setup::DEFAULT_ITEM_CATEGORY`]).
    pub category: &'a str,
    /// Field label (`"token"`, `"password"`, etc.).
    pub field_label: &'a str,
    /// Field value — lands on stdin, never on argv.
    pub value: &'a str,
    /// Optional `notesPlain` block (provenance metadata stamp).
    pub notes_plain: Option<&'a str>,
    /// `op` item tags applied at create time so list/search filters
    /// can find every jackin-managed item.
    pub tags: &'a [&'a str],
    /// Optional 1Password section label. When set, the field is placed
    /// in a section with this label; when `None`, the field is unsectioned.
    pub section: Option<&'a str>,
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
        "section".to_string()
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
            field["value"] = serde_json::Value::String(value.to_string());
            field["type"] = serde_json::Value::String("CONCEALED".to_string());
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
        let sections = item["sections"]
            .as_array_mut()
            .expect("sections coerced to array above");
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
    })
}

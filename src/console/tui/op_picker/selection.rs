//! Selection commit helpers for the 1Password picker.

use super::{OpPickerFieldRef, OpPickerItemRef, OpPickerState, OpPickerVaultRef};
use jackin_console::tui::components::op_picker::build_op_picker_ref;

/// Build an `OpRef` from the picker's currently-selected vault/item/field.
///
/// The `op` field uses UUID-form identifiers from the picker's pane
/// selections. The `path` field uses human-readable names, with an
/// inline `Item[subtitle]` annotation when the item shares its name
/// with another item in the same vault (ambiguity-aware).
///
/// Bracket-bearing item names suppress the subtitle embed. Empty subtitles
/// also suppress the embed.
///
/// Section info is recovered by parsing `field.reference`, which `op item get`
/// emits in canonical form. If it is empty or unparseable, this falls back to
/// a 3-segment URI; production `op item get` should populate `reference`.
///
/// # Panics
///
/// Panics if vault or item are not selected.
pub(crate) fn build_op_ref_on_commit(
    state: &OpPickerState,
    field: &super::OpPickerField,
) -> crate::operator_env::OpRef {
    let vault = state
        .selected_vault
        .as_ref()
        .expect("vault must be selected before commit");
    let item = state
        .selected_item
        .as_ref()
        .expect("item must be selected before commit");

    let built = build_op_picker_ref(
        OpPickerVaultRef {
            id: &vault.id,
            name: &vault.name,
        },
        OpPickerItemRef {
            id: &item.id,
            name: &item.name,
            subtitle: &item.subtitle,
        },
        state.items.iter().map(|item| OpPickerItemRef {
            id: &item.id,
            name: &item.name,
            subtitle: &item.subtitle,
        }),
        OpPickerFieldRef {
            id: &field.id,
            label: &field.label,
            reference: &field.reference,
        },
        state.fields.iter().map(|field| OpPickerFieldRef {
            id: &field.id,
            label: &field.label,
            reference: &field.reference,
        }),
    );

    if built.empty_reference_with_sibling_refs {
        crate::debug_log!(
            "op_picker",
            "empty field.reference for {}/{} (id {}); sibling fields have references — falling back to 3-segment URI",
            vault.name,
            item.name,
            field.id
        );
    }

    crate::operator_env::OpRef {
        op: built.op,
        path: built.path,
        account: state.selected_account_id(),
    }
}

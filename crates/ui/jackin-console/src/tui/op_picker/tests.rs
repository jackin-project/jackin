// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{LoadResult, OpPickerState, start_load};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jackin_oppicker::{
    ModalOutcome, OpLoadState, OpPickerCache, OpPickerError, OpPickerField, OpPickerItem,
    OpPickerItemRef, OpPickerLoadRequest, OpPickerStage, OpPickerVault, OpSection,
    build_op_picker_ref,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

fn picker_cache() -> Rc<RefCell<OpPickerCache>> {
    Rc::new(RefCell::new(OpPickerCache::default()))
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

#[test]
fn picker_commit_uses_canonical_ids_and_display_labels() {
    let built = build_op_picker_ref(
        jackin_oppicker::OpPickerVaultRef {
            id: "vault1",
            name: "Engineering",
        },
        OpPickerItemRef {
            id: "item9",
            name: "Deploy",
            subtitle: "production",
        },
        [OpPickerItemRef {
            id: "item9",
            name: "Deploy",
            subtitle: "production",
        }],
        jackin_oppicker::OpPickerFieldRef {
            id: "field7",
            label: "password",
            reference: "op://vault1/item9/section3/field7",
            section_id: Some("section3"),
            section_label: None,
        },
        [jackin_oppicker::OpPickerFieldRef {
            id: "field7",
            label: "password",
            reference: "op://vault1/item9/section3/field7",
            section_id: Some("section3"),
            section_label: None,
        }],
        &[OpSection {
            id: "section3".to_owned(),
            label: "Owner".to_owned(),
        }],
    )
    .expect("canonical ref builds");
    assert_eq!(built.op, "op://vault1/item9/section3/field7");
    assert_eq!(built.path, "Engineering/Deploy/Owner/password");
    assert!(!built.empty_reference_with_sibling_refs);
    let colliding = build_op_picker_ref(
        jackin_oppicker::OpPickerVaultRef {
            id: "vault1",
            name: "Engineering",
        },
        OpPickerItemRef {
            id: "item9",
            name: "Deploy",
            subtitle: "production",
        },
        [
            OpPickerItemRef {
                id: "item9",
                name: "Deploy",
                subtitle: "production",
            },
            OpPickerItemRef {
                id: "item4",
                name: "Deploy",
                subtitle: "staging",
            },
        ],
        jackin_oppicker::OpPickerFieldRef {
            id: "field7",
            label: "password",
            reference: "op://vault1/item9/section3/field7",
            section_id: Some("section3"),
            section_label: None,
        },
        [jackin_oppicker::OpPickerFieldRef {
            id: "field7",
            label: "password",
            reference: "op://vault1/item9/section3/field7",
            section_id: Some("section3"),
            section_label: None,
        }],
        &[OpSection {
            id: "section3".to_owned(),
            label: "Owner".to_owned(),
        }],
    )
    .expect("colliding ref builds");
    assert_eq!(colliding.op, "op://vault1/item9/section3/field7");
    assert_eq!(
        colliding.path,
        "Engineering/Deploy[production]/Owner/password"
    );
}

#[test]
fn create_mode_field_refresh_stays_on_field_and_keeps_section() {
    let mut picker = OpPickerState::new_create_with_cache(picker_cache(), "new item", "new field");
    picker.stage = OpPickerStage::Field;
    picker.load_state = OpLoadState::Ready;
    picker.field_refresh_in_place = true;
    let section = OpSection {
        id: "section1".to_owned(),
        label: "Owner".to_owned(),
    };
    picker.selected_section = Some(section.clone());
    picker.attach_load_receiver(jackin_oppicker::ready_load_subscription(
        LoadResult::Fields(Ok(jackin_core::OpItemDetail {
            fields: vec![
                OpPickerField {
                    id: "field1".to_owned(),
                    section_id: Some("section1".to_owned()),
                    label: "password".to_owned(),
                    field_type: "CONCEALED".to_owned(),
                    concealed: true,
                    reference: "op://vault1/item1/section1/field1".to_owned(),
                },
                OpPickerField {
                    id: "field2".to_owned(),
                    section_id: Some("section1".to_owned()),
                    label: "username".to_owned(),
                    field_type: "STRING".to_owned(),
                    concealed: false,
                    reference: "op://vault1/item1/section1/field2".to_owned(),
                },
            ],
            sections: vec![section.clone()],
        })),
    ));
    assert!(picker.poll_load());
    assert_eq!(picker.stage, OpPickerStage::Field);
    assert_eq!(picker.selected_section, Some(section));
    assert!(!picker.field_refresh_in_place);
    assert_eq!(picker.fields.len(), 2);
}

#[test]
fn op_cache_picker_does_not_store_field_values() {
    let field = OpPickerField {
        id: "field1".to_owned(),
        section_id: Some("section1".to_owned()),
        label: "password".to_owned(),
        field_type: "CONCEALED".to_owned(),
        concealed: true,
        reference: "op://vault1/item1/section1/field1".to_owned(),
    };
    let OpPickerField {
        id,
        section_id,
        label,
        field_type,
        concealed,
        reference,
    } = &field;
    assert_eq!(id, "field1");
    assert_eq!(section_id.as_deref(), Some("section1"));
    assert_eq!(label, "password");
    assert_eq!(field_type, "CONCEALED");
    assert!(concealed);
    assert_eq!(reference, "op://vault1/item1/section1/field1");
    assert_eq!(
        format!("{field:?}"),
        "OpField { id: \"field1\", section_id: Some(\"section1\"), label: \"password\", field_type: \"CONCEALED\", concealed: true, reference: \"op://vault1/item1/section1/field1\" }"
    );
    let mut cache = OpPickerCache::default();
    cache.put_fields(None, "vault1", "item1", vec![field.clone()]);
    assert_eq!(cache.get_fields(None, "vault1", "item1"), Some(vec![field]));
    assert!(cache.get_fields(None, "vault1", "item2").is_none());
}

struct FakeRunner {
    vaults: Vec<OpPickerVault>,
    items: Vec<OpPickerItem>,
    fail_items: bool,
    seen: Mutex<Vec<String>>,
}

impl jackin_env::OpStructRunner for FakeRunner {
    fn account_list(&self) -> anyhow::Result<Vec<jackin_core::OpAccount>> {
        Ok(Vec::new())
    }

    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpPickerVault>> {
        self.seen
            .lock()
            .expect("seen lock")
            .push(format!("vaults:{}", account.unwrap_or_default()));
        Ok(self.vaults.clone())
    }

    fn item_list(
        &self,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<OpPickerItem>> {
        self.seen
            .lock()
            .expect("seen lock")
            .push(format!("items:{vault_id}:{}", account.unwrap_or_default()));
        if self.fail_items {
            return Err(anyhow::anyhow!("listing failed"));
        }
        Ok(self.items.clone())
    }

    fn item_get(
        &self,
        _item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpPickerField>> {
        Ok(jackin_core::OpItemDetail {
            fields: Vec::new(),
            sections: Vec::new(),
        })
    }
}

fn drain_load(picker: &mut OpPickerState, attempts: usize) {
    for _ in 0..attempts {
        if picker.poll_load() {
            return;
        }
        std::thread::yield_now();
    }
}

#[test]
fn vault_list_uses_injected_runner_in_async_worker() {
    let vault = OpPickerVault {
        id: "vault1".to_owned(),
        name: "Engineering".to_owned(),
    };
    let fake = Arc::new(FakeRunner {
        vaults: vec![vault.clone()],
        items: Vec::new(),
        fail_items: false,
        seen: Mutex::new(Vec::new()),
    });
    let observed = Arc::clone(&fake);
    let runner: Arc<dyn jackin_env::OpStructRunner + Send + Sync> = fake;
    let cached = start_load(
        Some(LoadResult::Vaults(Ok(vec![vault.clone()]))),
        OpPickerLoadRequest::Vaults { account_id: None },
        Arc::clone(&runner),
    );
    assert!(matches!(
        cached,
        jackin_oppicker::LoadSubscription::Ready(_)
    ));
    let worker = start_load(
        None,
        OpPickerLoadRequest::Vaults {
            account_id: Some("account1".to_owned()),
        },
        Arc::clone(&runner),
    );
    assert!(matches!(
        worker,
        jackin_oppicker::LoadSubscription::Worker(_)
    ));
    let mut picker = OpPickerState::new_with_cache(picker_cache());
    picker.attach_load_receiver(worker);
    drain_load(&mut picker, 5000);
    assert_eq!(picker.vaults, vec![vault]);
    assert!(matches!(picker.load_state, OpLoadState::Ready));
    assert!(
        observed
            .seen
            .lock()
            .expect("seen lock")
            .contains(&"vaults:account1".to_owned())
    );
}

#[test]
fn item_list_uses_injected_runner_in_async_worker() {
    let item = OpPickerItem {
        id: "item9".to_owned(),
        name: "Deploy".to_owned(),
        subtitle: "production".to_owned(),
    };
    let fake = Arc::new(FakeRunner {
        vaults: Vec::new(),
        items: vec![item.clone()],
        fail_items: false,
        seen: Mutex::new(Vec::new()),
    });
    let observed = Arc::clone(&fake);
    let runner: Arc<dyn jackin_env::OpStructRunner + Send + Sync> = fake;
    let mut picker = OpPickerState::new_with_cache(picker_cache());
    picker.load_state = OpLoadState::Ready;
    assert!(matches!(
        picker.handle_key(key(KeyCode::Down)),
        ModalOutcome::Continue
    ));
    assert!(matches!(
        picker.handle_key(key(KeyCode::Up)),
        ModalOutcome::Continue
    ));
    assert!(observed.seen.lock().expect("seen lock").is_empty());
    picker.attach_load_receiver(start_load(
        None,
        OpPickerLoadRequest::Items {
            account_id: Some("account1".to_owned()),
            vault_id: "vault1".to_owned(),
        },
        Arc::clone(&runner),
    ));
    drain_load(&mut picker, 5000);
    assert_eq!(picker.items, vec![item.clone()]);
    assert!(matches!(picker.load_state, OpLoadState::Ready));
    assert!(
        observed
            .seen
            .lock()
            .expect("seen lock")
            .contains(&"items:vault1:account1".to_owned())
    );
    let failing: Arc<dyn jackin_env::OpStructRunner + Send + Sync> = Arc::new(FakeRunner {
        vaults: Vec::new(),
        items: Vec::new(),
        fail_items: true,
        seen: Mutex::new(Vec::new()),
    });
    picker.attach_load_receiver(start_load(
        None,
        OpPickerLoadRequest::Items {
            account_id: None,
            vault_id: "vault1".to_owned(),
        },
        failing,
    ));
    drain_load(&mut picker, 5000);
    assert!(matches!(
        picker.load_state,
        OpLoadState::Error(OpPickerError::Recoverable { .. })
    ));
    assert_eq!(picker.items, vec![item.clone()]);
}

fn field_fixture(id: &str, label: &str) -> OpPickerField {
    OpPickerField {
        id: id.to_owned(),
        section_id: Some("section1".to_owned()),
        label: label.to_owned(),
        field_type: "STRING".to_owned(),
        concealed: false,
        reference: format!("op://vault1/item1/section1/{id}"),
    }
}

#[test]
fn left_collapse_via_header_keeps_selection_in_range() {
    let mut picker = OpPickerState::new_with_cache(picker_cache());
    picker.stage = OpPickerStage::Field;
    picker.load_state = OpLoadState::Ready;
    picker.fields = vec![
        field_fixture("field1", "first"),
        field_fixture("field2", "second"),
        OpPickerField {
            id: "field3".to_owned(),
            section_id: Some("section2".to_owned()),
            label: "third".to_owned(),
            field_type: "STRING".to_owned(),
            concealed: false,
            reference: "op://vault1/item1/section2/field3".to_owned(),
        },
    ];
    picker.sections = vec![
        OpSection {
            id: "section1".to_owned(),
            label: "First".to_owned(),
        },
        OpSection {
            id: "section2".to_owned(),
            label: "Second".to_owned(),
        },
    ];
    let rows = picker.build_field_display_rows();
    assert_eq!(rows.len(), 5);
    picker.field_list_state.set_active(Some(4));
    assert!(matches!(
        picker.handle_key(key(KeyCode::Left)),
        ModalOutcome::Continue
    ));
    assert_eq!(picker.field_list_state.active(), Some(&4));
    assert!(picker.collapsed_sections.is_empty());
    picker.field_list_state.set_active(Some(0));
    assert!(matches!(
        picker.handle_key(key(KeyCode::Left)),
        ModalOutcome::Continue
    ));
    assert!(picker.collapsed_sections.contains("section1"));
    let rows = picker.build_field_display_rows();
    assert_eq!(rows.len(), 3);
    assert!(
        picker
            .field_list_state
            .active()
            .is_some_and(|index| *index < rows.len())
    );
    assert!(matches!(
        picker.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert!(!picker.collapsed_sections.contains("section1"));
    assert_eq!(picker.build_field_display_rows().len(), 5);
}

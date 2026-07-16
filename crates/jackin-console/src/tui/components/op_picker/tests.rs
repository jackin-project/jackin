// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `op_picker` component.
use std::collections::HashSet;

use super::*;

#[test]
fn background_worker_disconnected_message_is_component_owned() {
    assert_eq!(
        background_worker_disconnected_error_message(),
        "background worker disconnected",
    );
}

#[test]
fn probe_load_error_state_classifies_operator_states() {
    assert!(matches!(
        probe_load_error_state("failed to spawn op"),
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotInstalled))
    ));
    assert!(matches!(
        probe_load_error_state("not currently signed in"),
        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn))
    ));
}

#[test]
fn recoverable_load_error_state_preserves_message() {
    assert!(matches!(
        recoverable_load_error_state("field read failed"),
        OpLoadState::Error(OpPickerError::Recoverable { message }) if message == "field read failed"
    ));
}

#[test]
fn disconnected_worker_error_state_uses_standard_message() {
    assert!(matches!(
        disconnected_worker_error_state(),
        OpLoadState::Error(OpPickerError::Recoverable { message })
            if message == background_worker_disconnected_error_message()
    ));
}

#[test]
fn blocked_load_key_plan_cancels_loading_or_fatal_on_escape() {
    assert_eq!(
        blocked_load_key_plan(&OpLoadState::Loading { spinner_tick: 0 }, true),
        Some(OpPickerBlockedLoadKeyPlan::Cancel)
    );
    assert_eq!(
        blocked_load_key_plan(
            &OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults)),
            true,
        ),
        Some(OpPickerBlockedLoadKeyPlan::Cancel)
    );
}

#[test]
fn blocked_load_key_plan_continues_loading_or_fatal_on_other_keys() {
    assert_eq!(
        blocked_load_key_plan(&OpLoadState::Loading { spinner_tick: 0 }, false),
        Some(OpPickerBlockedLoadKeyPlan::Continue)
    );
    assert_eq!(
        blocked_load_key_plan(
            &OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults)),
            false,
        ),
        Some(OpPickerBlockedLoadKeyPlan::Continue)
    );
}

#[test]
fn blocked_load_key_plan_ignores_ready_and_recoverable_states() {
    assert_eq!(blocked_load_key_plan(&OpLoadState::Ready, true), None);
    assert_eq!(
        blocked_load_key_plan(&recoverable_load_error_state("temporary failure"), true),
        None
    );
}

#[test]
fn breadcrumb_omits_pane_type_suffix_multi_account() {
    let title = breadcrumb_title(
        OpPickerStage::Vault,
        true,
        "alice@example.com",
        "ignored",
        "ignored",
    );
    assert_eq!(title, "alice@example.com");
    assert!(!title.contains("Vaults"), "no `Vaults` suffix: {title}");

    let title = breadcrumb_title(
        OpPickerStage::Item,
        true,
        "alice@example.com",
        "Personal",
        "",
    );
    assert_eq!(title, "alice@example.com \u{2192} Personal");
    assert!(!title.contains("Items"));

    let title = breadcrumb_title(
        OpPickerStage::Field,
        true,
        "alice@example.com",
        "Personal",
        "API Keys",
    );
    assert_eq!(
        title,
        "alice@example.com \u{2192} Personal \u{2192} API Keys"
    );
    assert!(!title.contains("Fields"));
}

#[test]
fn breadcrumb_single_account_uses_brand_or_bare_context() {
    let v = breadcrumb_title(OpPickerStage::Vault, false, "", "Personal", "");
    assert_eq!(v, "1Password");

    let i = breadcrumb_title(OpPickerStage::Item, false, "", "Personal", "API Keys");
    assert_eq!(i, "Personal");

    let f = breadcrumb_title(OpPickerStage::Field, false, "", "Personal", "API Keys");
    assert_eq!(f, "Personal \u{2192} API Keys");
}

#[test]
fn breadcrumb_account_pane_is_bare_brand() {
    let title = breadcrumb_title(OpPickerStage::Account, true, "ignored", "", "");
    assert_eq!(title, "1Password");
}

#[test]
fn probe_error_message_classifies_operator_states() {
    assert!(matches!(
        classify_probe_error_message("failed to spawn op"),
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    ));
    assert!(matches!(
        classify_probe_error_message("not currently signed in"),
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    ));
    assert!(matches!(
        classify_probe_error_message("boom"),
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { .. })
    ));
}

#[test]
fn probe_error_downcast_classifies_without_substring() {
    // Decoy messages prove the typed source wins over the fallback classifier.
    let not_installed = anyhow::Error::new(jackin_core::OpProbeError::NotInstalled {
        detail: "xyzzy".into(),
    })
    .context("xyzzy decoy — not a spawn phrase");
    assert!(matches!(
        classify_probe_error(&not_installed),
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    ));

    let not_signed = anyhow::Error::new(jackin_core::OpProbeError::NotSignedIn {
        detail: "xyzzy".into(),
    })
    .context("xyzzy decoy — not a signin phrase");
    assert!(matches!(
        classify_probe_error(&not_signed),
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    ));

    let timeout = anyhow::Error::new(jackin_core::OpProbeError::Timeout { seconds: 9 })
        .context("xyzzy decoy timeout");
    assert!(matches!(
        classify_probe_error(&timeout),
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { .. })
    ));

    let other = anyhow::Error::new(jackin_core::OpProbeError::Other {
        message: "xyzzy".into(),
    });
    assert!(matches!(
        classify_probe_error(&other),
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { message }) if message.contains("xyzzy")
    ));
}

#[test]
fn section_choices_deduplicate_in_first_seen_order() {
    let choices = section_choices_from_references([
        "op://Vault/Item/token",
        "op://Vault/Item/Auth/password",
        "op://Vault/Item/Deploy/key",
        "op://Vault/Item/Auth/otp",
    ]);
    assert_eq!(
        choices,
        vec![None, Some("Auth".to_owned()), Some("Deploy".to_owned())]
    );
}

#[test]
fn browse_field_rows_group_sections_and_respect_collapse() {
    let mut collapsed = HashSet::new();
    collapsed.insert("Auth".to_owned());
    let rows = browse_field_display_rows(
        [
            "op://Vault/Item/root",
            "op://Vault/Item/Auth/password",
            "op://Vault/Item/Auth/otp",
            "op://Vault/Item/Deploy/key",
        ],
        &collapsed,
    );
    assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 0 }));
    assert!(matches!(
        rows[1],
        FieldDisplayRow::SectionHeader {
            ref name,
            field_count: 2
        } if name == "Auth"
    ));
    assert!(matches!(
        rows[2],
        FieldDisplayRow::SectionHeader {
            ref name,
            field_count: 1
        } if name == "Deploy"
    ));
    assert!(matches!(rows[3], FieldDisplayRow::Field { field_idx: 3 }));
}

#[test]
fn create_field_rows_scope_to_section_and_add_sentinel() {
    let rows = create_field_display_rows(
        [
            "op://Vault/Item/root",
            "op://Vault/Item/Auth/password",
            "op://Vault/Item/Auth/otp",
        ],
        Some("Auth"),
    );
    assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 1 }));
    assert!(matches!(rows[1], FieldDisplayRow::Field { field_idx: 2 }));
    assert!(matches!(rows[2], FieldDisplayRow::NewFieldSentinel));
}

#[test]
fn selected_index_routes_by_visible_stage() {
    assert_eq!(
        selected_index_for_stage(
            OpPickerStage::Account,
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
        ),
        Some(1)
    );
    assert_eq!(
        selected_index_for_stage(
            OpPickerStage::Field,
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
        ),
        Some(5)
    );
    assert_eq!(
        selected_index_for_stage(
            OpPickerStage::FieldLabel,
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
        ),
        None
    );
}

#[test]
fn naming_stage_input_routes_by_naming_stage() {
    let item = item_name_input_state("");
    let field = field_label_input_state("");
    let section = section_name_input_state("");

    assert_eq!(
        naming_stage_input_for_stage(OpPickerStage::NewItemName, &item, &field, &section)
            .map(TextInputState::label),
        Some("Item name")
    );
    assert_eq!(
        naming_stage_input_for_stage(OpPickerStage::FieldLabel, &item, &field, &section)
            .map(TextInputState::label),
        Some("Field label")
    );
    assert_eq!(
        naming_stage_input_for_stage(OpPickerStage::NewSectionName, &item, &field, &section)
            .map(TextInputState::label),
        Some("Section name")
    );
    assert!(naming_stage_input_for_stage(OpPickerStage::Field, &item, &field, &section).is_none());
}

#[test]
fn filter_reset_selection_routes_only_filterable_stages() {
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::Account, 2, 3, 4, 5),
        Some(Some(0))
    );
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::Item, 2, 3, 0, 5),
        Some(None)
    );
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::Section, 2, 3, 4, 5),
        None
    );
    assert_eq!(
        filter_reset_selection_for_stage(OpPickerStage::FieldLabel, 2, 3, 4, 5),
        None
    );
}

#[test]
fn field_stage_back_plan_preserves_create_mode_sections() {
    assert_eq!(
        field_stage_back_plan(&OpPickerMode::Create {
            item_name_default: String::new(),
            field_label_default: String::new(),
        }),
        FieldStageBackPlan {
            stage: OpPickerStage::Section,
            reset_selected_section: true,
            clear_fields: false,
            clear_collapsed_sections: false,
            clear_selected_item: false,
            reset_section_list: true,
        }
    );
    assert_eq!(
        field_stage_back_plan(&OpPickerMode::Browse),
        FieldStageBackPlan {
            stage: OpPickerStage::Item,
            reset_selected_section: false,
            clear_fields: true,
            clear_collapsed_sections: true,
            clear_selected_item: true,
            reset_section_list: false,
        }
    );
}

#[test]
fn field_stage_refresh_plan_tracks_create_mode_in_place_reload() {
    assert_eq!(
        field_stage_refresh_plan(&OpPickerMode::Browse),
        FieldStageRefreshPlan {
            clear_fields: true,
            reset_field_list: true,
            clear_collapsed_sections: true,
            refresh_in_place: false,
        }
    );
    assert_eq!(
        field_stage_refresh_plan(&OpPickerMode::Create {
            item_name_default: String::new(),
            field_label_default: String::new(),
        }),
        FieldStageRefreshPlan {
            clear_fields: true,
            reset_field_list: true,
            clear_collapsed_sections: true,
            refresh_in_place: true,
        }
    );
}

#[test]
fn section_stage_back_plan_returns_to_item() {
    assert_eq!(
        section_stage_back_plan(),
        SectionStageBackPlan {
            stage: OpPickerStage::Item,
            clear_fields: true,
            clear_collapsed_sections: true,
            clear_selected_section: true,
            clear_selected_item: true,
        }
    );
}

#[test]
fn section_stage_commit_plan_resolves_sentinel_and_choices() {
    let choices = vec![None, Some("api".to_owned())];

    assert_eq!(
        section_stage_commit_plan(Some(0), &choices),
        SectionStageCommitPlan::ExistingSection {
            selected_section: None
        }
    );
    assert_eq!(
        section_stage_commit_plan(Some(1), &choices),
        SectionStageCommitPlan::ExistingSection {
            selected_section: Some("api".to_owned())
        }
    );
    assert_eq!(
        section_stage_commit_plan(Some(2), &choices),
        SectionStageCommitPlan::NewSectionName
    );
    assert_eq!(
        section_stage_commit_plan(Some(3), &choices),
        SectionStageCommitPlan::NoSelection
    );
}

#[test]
fn item_stage_back_plan_returns_to_vault() {
    assert_eq!(
        item_stage_back_plan(),
        ItemStageBackPlan {
            stage: OpPickerStage::Vault,
            clear_items: true,
            clear_selected_item: true,
        }
    );
}

#[test]
fn item_stage_commit_plan_routes_existing_new_and_empty() {
    assert_eq!(
        item_stage_commit_plan(Some(Some("item"))),
        ItemStageCommitPlan::ExistingItem("item")
    );
    assert_eq!(
        item_stage_commit_plan::<&str>(Some(None)),
        ItemStageCommitPlan::NewItemName
    );
    assert_eq!(
        item_stage_commit_plan::<&str>(None),
        ItemStageCommitPlan::NoSelection
    );
}

#[test]
fn item_stage_refresh_plan_clears_loaded_state() {
    assert_eq!(
        item_stage_refresh_plan(),
        ItemStageRefreshPlan {
            clear_items: true,
            reset_item_list: true,
        }
    );
}

#[test]
fn vault_stage_back_plan_handles_single_and_multi_account() {
    assert_eq!(vault_stage_back_plan(1), VaultStageBackPlan::Cancel);
    assert_eq!(
        vault_stage_back_plan(2),
        VaultStageBackPlan::BackToAccount {
            stage: OpPickerStage::Account,
            clear_selected_vault: true,
            clear_vaults: true,
            reset_vault_list: true,
            ready_load_state: true,
        }
    );
}

#[test]
fn vault_stage_commit_plan_routes_existing_and_empty() {
    assert_eq!(
        vault_stage_commit_plan(Some("vault")),
        VaultStageCommitPlan::ExistingVault("vault")
    );
    assert_eq!(
        vault_stage_commit_plan::<&str>(None),
        VaultStageCommitPlan::NoSelection
    );
}

#[test]
fn vault_stage_refresh_plan_clears_loaded_state() {
    assert_eq!(
        vault_stage_refresh_plan(),
        VaultStageRefreshPlan {
            clear_vaults: true,
            reset_vault_list: true,
            clear_selected_vault: true,
        }
    );
}

#[test]
fn account_stage_refresh_plan_clears_loaded_state() {
    assert_eq!(
        account_stage_refresh_plan(),
        AccountStageRefreshPlan {
            clear_accounts: true,
            reset_account_list: true,
            clear_selected_account: true,
        }
    );
}

#[test]
fn account_stage_commit_plan_routes_existing_and_empty() {
    assert_eq!(
        account_stage_commit_plan(Some("account")),
        AccountStageCommitPlan::ExistingAccount("account")
    );
    assert_eq!(
        account_stage_commit_plan::<&str>(None),
        AccountStageCommitPlan::NoSelection
    );
}

#[test]
fn section_header_collapse_target_routes_only_headers() {
    let row = FieldDisplayRow::SectionHeader {
        name: "Auth".to_owned(),
        field_count: 2,
    };
    let mut collapsed = HashSet::new();

    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Collapse),
        Some(("Auth".to_owned(), true))
    );
    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Expand),
        Some(("Auth".to_owned(), false))
    );
    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Toggle),
        Some(("Auth".to_owned(), true))
    );

    collapsed.insert("Auth".to_owned());
    assert_eq!(
        section_header_collapse_target(Some(&row), &collapsed, SectionCollapseIntent::Toggle),
        Some(("Auth".to_owned(), false))
    );
    assert_eq!(
        section_header_collapse_target(
            Some(&FieldDisplayRow::NewFieldSentinel),
            &collapsed,
            SectionCollapseIntent::Toggle,
        ),
        None
    );
}

#[test]
fn field_stage_commit_plan_routes_row_kinds() {
    let row = FieldDisplayRow::SectionHeader {
        name: "Auth".to_owned(),
        field_count: 2,
    };
    let collapsed = HashSet::new();
    assert_eq!(
        field_stage_commit_plan(Some(&row), &collapsed, Some("Auth")),
        FieldStageCommitPlan::ToggleSection {
            name: "Auth".to_owned(),
            collapsed: true,
        }
    );

    assert_eq!(
        field_stage_commit_plan(
            Some(&FieldDisplayRow::Field { field_idx: 3 }),
            &collapsed,
            Some("Auth"),
        ),
        FieldStageCommitPlan::ExistingField { field_idx: 3 }
    );
    assert_eq!(
        field_stage_commit_plan(Some(&FieldDisplayRow::NewFieldSentinel), &collapsed, None),
        FieldStageCommitPlan::NewField {
            pending_section: None,
            field_label_origin: FieldLabelOrigin::NewField,
            stage: OpPickerStage::FieldLabel,
        }
    );
    assert_eq!(
        field_stage_commit_plan(Some(&FieldDisplayRow::NewSectionSentinel), &collapsed, None),
        FieldStageCommitPlan::NoSelection
    );
}

#[test]
fn naming_stage_plans_name_next_stage_and_pending_section() {
    assert_eq!(
        new_item_name_commit_plan(),
        NamingStagePlan {
            stage: OpPickerStage::FieldLabel,
            field_label_origin: Some(FieldLabelOrigin::NewItem),
            pending_section: None,
            clear_pending_section: false,
        }
    );
    assert_eq!(
        new_section_name_commit_plan("  Deploy  "),
        NamingStagePlan {
            stage: OpPickerStage::FieldLabel,
            field_label_origin: Some(FieldLabelOrigin::NewSection),
            pending_section: Some("Deploy".to_owned()),
            clear_pending_section: false,
        }
    );
    assert_eq!(
        field_label_cancel_plan(FieldLabelOrigin::NewField),
        NamingStagePlan {
            stage: OpPickerStage::Field,
            field_label_origin: None,
            pending_section: None,
            clear_pending_section: true,
        }
    );
}

#[test]
fn matches_filter_accepts_empty_or_any_matching_value() {
    assert!(matches_filter("", ["anything"]));
    assert!(matches_filter("api", ["Stripe", "API token"]));
    assert!(matches_filter(
        "example",
        ["alice@example.com", "https://example.test"]
    ));
    assert!(!matches_filter("missing", ["one", "two"]));
}

#[test]
fn build_op_picker_ref_uses_uuid_op_and_clean_path_for_unique_item() {
    let built = build_op_picker_ref(
        OpPickerVaultRef {
            id: "v_uuid",
            name: "Private",
        },
        OpPickerItemRef {
            id: "i_uuid",
            name: "Stripe",
            subtitle: "",
        },
        [OpPickerItemRef {
            id: "i_uuid",
            name: "Stripe",
            subtitle: "",
        }],
        OpPickerFieldRef {
            id: "f_uuid",
            label: "api key",
            reference: "op://Private/Stripe/api key",
        },
        [OpPickerFieldRef {
            id: "f_uuid",
            label: "api key",
            reference: "op://Private/Stripe/api key",
        }],
    );
    assert_eq!(built.op, "op://v_uuid/i_uuid/f_uuid");
    assert_eq!(built.path, "Private/Stripe/api key");
    assert!(!built.empty_reference_with_sibling_refs);
}

#[test]
fn build_op_picker_ref_preserves_sections_and_ambiguous_subtitles() {
    let built = build_op_picker_ref(
        OpPickerVaultRef {
            id: "v_uuid",
            name: "Private",
        },
        OpPickerItemRef {
            id: "i_a",
            name: "Claude",
            subtitle: "alice@example.com",
        },
        [
            OpPickerItemRef {
                id: "i_a",
                name: "Claude",
                subtitle: "alice@example.com",
            },
            OpPickerItemRef {
                id: "i_b",
                name: "Claude",
                subtitle: "bob@example.com",
            },
        ],
        OpPickerFieldRef {
            id: "f_uuid",
            label: "token",
            reference: "op://Private/Claude/Auth/token",
        },
        [OpPickerFieldRef {
            id: "f_uuid",
            label: "token",
            reference: "op://Private/Claude/Auth/token",
        }],
    );
    assert_eq!(built.op, "op://v_uuid/i_a/Auth/f_uuid");
    assert_eq!(built.path, "Private/Claude[alice@example.com]/Auth/token");
}

#[test]
fn build_op_picker_ref_flags_empty_reference_with_sibling_refs() {
    let built = build_op_picker_ref(
        OpPickerVaultRef {
            id: "v_uuid",
            name: "Private",
        },
        OpPickerItemRef {
            id: "i_uuid",
            name: "MyItem",
            subtitle: "",
        },
        [OpPickerItemRef {
            id: "i_uuid",
            name: "MyItem",
            subtitle: "",
        }],
        OpPickerFieldRef {
            id: "f_noref",
            label: "notes",
            reference: "",
        },
        [
            OpPickerFieldRef {
                id: "f_noref",
                label: "notes",
                reference: "",
            },
            OpPickerFieldRef {
                id: "f_sectioned",
                label: "password",
                reference: "op://Private/MyItem/Auth/password",
            },
        ],
    );
    assert_eq!(built.op, "op://v_uuid/i_uuid/f_noref");
    assert_eq!(built.path, "Private/MyItem/notes");
    assert!(built.empty_reference_with_sibling_refs);
}

#[test]
fn section_lines_append_new_section_sentinel() {
    let lines = section_lines([None, Some("Auth".to_owned())], Some(2));
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[0].spans[0].content.as_ref(),
        "(root)",
        "root choice renders first"
    );
    assert_eq!(
        lines[1].spans[0].content.as_ref(),
        "Auth",
        "named section renders second"
    );
    assert_eq!(
        lines[2].spans[0].content.as_ref(),
        "+ New section",
        "sentinel renders last without embedding selection chrome"
    );
}

#[test]
fn account_vault_and_item_lines_leave_selection_to_shared_renderer() {
    let account = account_lines(
        [OpPickerAccountRef {
            email: "alice@example.com",
            url: "alice.1password.com",
        }],
        Some(0),
    );
    assert_eq!(account[0].spans[0].content.as_ref(), "alice@example.com");
    assert_eq!(
        account[0].spans[2].content.as_ref(),
        "(alice.1password.com)"
    );

    let vault = vault_lines(
        [OpPickerVaultRef {
            id: "v1",
            name: "Private",
        }],
        None,
    );
    assert_eq!(vault[0].spans[0].content.as_ref(), "Private");

    let items = item_choice_lines(
        [
            Some(OpPickerItemRef {
                id: "i1",
                name: "Claude",
                subtitle: "alice@example.com",
            }),
            None,
        ],
        Some(1),
    );
    assert_eq!(items[0].spans[0].content.as_ref(), "Claude");
    assert_eq!(items[0].spans[2].content.as_ref(), "alice@example.com");
    assert_eq!(items[1].spans[0].content.as_ref(), "+ New item");
}

#[test]
fn field_lines_render_headers_fields_and_sentinels() {
    let mut collapsed = HashSet::new();
    collapsed.insert("Auth".to_owned());
    let lines = field_lines(
        [
            FieldDisplayRow::SectionHeader {
                name: "Auth".to_owned(),
                field_count: 1,
            },
            FieldDisplayRow::Field { field_idx: 0 },
            FieldDisplayRow::NewFieldSentinel,
        ],
        [OpPickerFieldDisplayRef {
            id: "f1",
            label: "token",
            field_type: "CONCEALED",
            concealed: true,
        }],
        &collapsed,
        Some(1),
    );

    assert_eq!(lines[0].spans[0].content.as_ref(), "\u{25b6}");
    assert_eq!(lines[1].spans[0].content.as_ref(), "token");
    assert_eq!(lines[1].spans[2].content.as_ref(), "(concealed)");
    assert_eq!(lines[2].spans[0].content.as_ref(), "+ New field");
}

struct RenderStateFixture {
    stage: OpPickerStage,
    selected: Option<usize>,
    load_state: OpLoadState,
}

impl RenderStateFixture {
    const fn new(stage: OpPickerStage, selected: Option<usize>) -> Self {
        Self {
            stage,
            selected,
            load_state: OpLoadState::Ready,
        }
    }
}

impl OpPickerRenderState for RenderStateFixture {
    fn stage(&self) -> OpPickerStage {
        self.stage
    }

    fn load_state(&self) -> &OpLoadState {
        &self.load_state
    }

    fn filter_buffer(&self) -> &'static str {
        ""
    }

    fn account_count(&self) -> usize {
        2
    }

    fn selected_account_email(&self) -> &'static str {
        "alice@example.com"
    }

    fn selected_vault_name(&self) -> &'static str {
        "Private"
    }

    fn selected_item_name(&self) -> &'static str {
        "Cloudflare"
    }

    fn selected_item_subtitle(&self) -> &'static str {
        "alice@example.com"
    }

    fn naming_stage_input(&self) -> Option<&TextInputState<'static>> {
        None
    }

    fn account_lines(&self) -> Vec<Line<'static>> {
        account_lines(
            [
                OpPickerAccountRef {
                    email: "alice@example.com",
                    url: "alice.1password.com",
                },
                OpPickerAccountRef {
                    email: "bob@example.com",
                    url: "bob.1password.com",
                },
            ],
            self.selected,
        )
    }

    fn vault_lines(&self) -> Vec<Line<'static>> {
        vault_lines(
            [OpPickerVaultRef {
                id: "v1",
                name: "Private",
            }],
            self.selected,
        )
    }

    fn item_lines(&self) -> Vec<Line<'static>> {
        item_choice_lines(
            [
                Some(OpPickerItemRef {
                    id: "i1",
                    name: "Cloudflare",
                    subtitle: "alice@example.com",
                }),
                Some(OpPickerItemRef {
                    id: "i2",
                    name: "GitHub",
                    subtitle: "bob@example.com",
                }),
            ],
            self.selected,
        )
    }

    fn section_lines(&self) -> Vec<Line<'static>> {
        section_lines([None, Some("Auth".to_owned())], self.selected)
    }

    fn field_lines(&self) -> Vec<Line<'static>> {
        field_lines(
            [FieldDisplayRow::Field { field_idx: 0 }],
            [OpPickerFieldDisplayRef {
                id: "f1",
                label: "password",
                field_type: "CONCEALED",
                concealed: true,
            }],
            &HashSet::new(),
            self.selected,
        )
    }

    fn selected_index(&self) -> Option<usize> {
        self.selected
    }
}

fn render_picker_buffer(state: &RenderStateFixture, w: u16, h: u16) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_picker(f, Rect::new(0, 0, w, h), state))
        .unwrap();
    term.backend().buffer().clone()
}

#[test]
fn loading_descriptor_names_current_load_target() {
    assert_eq!(
        loading_descriptor(OpPickerStage::Account, false, "", "", "", ""),
        "loading accounts\u{2026}"
    );
    assert_eq!(
        loading_descriptor(OpPickerStage::Vault, true, "alice@example.com", "", "", ""),
        "loading vaults from alice@example.com\u{2026}"
    );
    assert_eq!(
        loading_descriptor(
            OpPickerStage::Field,
            false,
            "",
            "",
            "Claude",
            "alice@example.com"
        ),
        "loading Claude (alice@example.com)\u{2026}"
    );
    assert_eq!(
        loading_title_stage(OpPickerStage::Field),
        OpPickerStage::Item
    );
}

#[test]
fn fatal_body_lines_truncate_generic_errors() {
    let long = "x".repeat(140);
    let lines = fatal_body_lines(&OpPickerFatalState::GenericFatal { message: long });
    assert_eq!(lines[0].spans[0].content.as_ref(), "1Password CLI error.");
    assert_eq!(lines[2].spans[0].content.chars().count(), 120);

    let missing = fatal_body_lines(&OpPickerFatalState::NotInstalled);
    assert!(missing.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("brew install"))
    }));
}

#[test]
fn field_label_origin_maps_to_cancel_stage() {
    assert_eq!(
        FieldLabelOrigin::NewItem.cancel_stage(),
        OpPickerStage::NewItemName
    );
    assert_eq!(
        FieldLabelOrigin::NewField.cancel_stage(),
        OpPickerStage::Field
    );
    assert_eq!(
        FieldLabelOrigin::NewSection.cancel_stage(),
        OpPickerStage::NewSectionName
    );
}

#[test]
fn field_label_commit_plan_trims_and_routes_item_presence() {
    assert_eq!(
        field_label_commit_plan(
            Some("account"),
            "vault",
            Some("item"),
            Some("section".to_owned()),
            "ignored".to_owned(),
            "  token  ",
        ),
        FieldLabelCommitPlan::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some("section".to_owned()),
            field_label: "token".to_owned(),
        }
    );
    assert_eq!(
        field_label_commit_plan::<&str, &str, &str>(
            None,
            "vault",
            None,
            None,
            "login".to_owned(),
            "  password  ",
        ),
        FieldLabelCommitPlan::NewItem {
            account: None,
            vault: "vault",
            item_name: "login".to_owned(),
            section: None,
            field_label: "password".to_owned(),
        }
    );
}

#[test]
fn field_label_commit_selection_builds_component_owned_selection_shape() {
    let selection = field_label_commit_selection::<&str, &str, &str, &str, (&str, String)>(
        FieldLabelCommitPlan::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some("api".to_owned()),
            field_label: "token".to_owned(),
        },
        |label| ("new", label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some("api".to_owned()),
            field: ("new", "token".to_owned()),
        }
    );

    let selection = field_label_commit_selection::<&str, &str, &str, &str, (&str, String)>(
        FieldLabelCommitPlan::NewItem {
            account: None,
            vault: "vault",
            item_name: "Login".to_owned(),
            section: None,
            field_label: "password".to_owned(),
        },
        |label| ("new", label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::NewItem {
            account: None,
            vault: "vault",
            item_name: "Login".to_owned(),
            section: None,
            field_label: "password".to_owned(),
        }
    );
}

#[test]
fn existing_field_commit_plan_routes_create_mode_to_field_target_data() {
    assert_eq!(
        existing_field_commit_plan(
            &OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            },
            "field-id",
            "token",
            Some("api".to_owned()),
        ),
        ExistingFieldCommitPlan::EditItemField {
            section: Some("api".to_owned()),
            field_id: "field-id".to_owned(),
            field_label: "token".to_owned(),
        }
    );
    assert_eq!(
        existing_field_commit_plan(&OpPickerMode::Browse, "field-id", "token", None),
        ExistingFieldCommitPlan::ExistingReference,
    );
}

#[test]
fn existing_field_commit_selection_builds_component_owned_selection_shape() {
    let selection = existing_field_commit_selection(
        ExistingFieldCommitPlan::EditItemField {
            section: Some("api".to_owned()),
            field_id: "field-id".to_owned(),
            field_label: "token".to_owned(),
        },
        ExistingFieldCommitSelectionInput {
            account: Some("account"),
            vault: "vault",
            item: "item",
        },
        || "op://unused",
        |id, label| (id, label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::EditItemField {
            account: Some("account"),
            vault: "vault",
            item: "item",
            section: Some("api".to_owned()),
            field: ("field-id".to_owned(), "token".to_owned()),
        }
    );

    let selection = existing_field_commit_selection::<&str, &str, &str, &str, (String, String)>(
        ExistingFieldCommitPlan::ExistingReference,
        ExistingFieldCommitSelectionInput {
            account: None,
            vault: "vault",
            item: "item",
        },
        || "op://vault/item/field",
        |id, label| (id, label),
    );
    assert_eq!(
        selection,
        OpPickerSelection::Existing("op://vault/item/field")
    );
}

#[test]
fn stage_classification_separates_naming_and_filterable_lists() {
    assert!(OpPickerStage::FieldLabel.is_naming());
    assert!(OpPickerStage::NewSectionName.is_naming());
    assert!(!OpPickerStage::Field.is_naming());

    assert!(OpPickerStage::Account.is_filterable());
    assert!(OpPickerStage::Field.is_filterable());
    assert!(!OpPickerStage::Section.is_filterable());
    assert!(!OpPickerStage::FieldLabel.is_filterable());
}

#[test]
fn account_load_completion_plan_keeps_root_adapter_out_of_transition_policy() {
    assert_eq!(accounts_loaded_plan(0), AccountsLoadedPlan::NotSignedIn);
    assert_eq!(
        accounts_loaded_plan(1),
        AccountsLoadedPlan::SelectSingleAccount
    );
    assert_eq!(accounts_loaded_plan(2), AccountsLoadedPlan::ShowAccountPane);
}

#[test]
fn vault_item_and_field_load_completion_plans_keep_root_adapter_out_of_transition_policy() {
    assert_eq!(vaults_loaded_plan(0), VaultsLoadedPlan::NoVaults);
    assert_eq!(
        vaults_loaded_plan(2),
        VaultsLoadedPlan::ShowVaultPane { selected: Some(0) }
    );

    assert_eq!(items_loaded_plan(0).selected, None);
    assert_eq!(items_loaded_plan(3).selected, Some(0));

    assert_eq!(
        fields_loaded_plan(&OpPickerMode::Browse, false, 2, 4),
        FieldsLoadedPlan::ShowFieldPane {
            field_selected: Some(0),
            clear_selected_section: true,
        }
    );
    assert_eq!(
        fields_loaded_plan(
            &OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            },
            false,
            2,
            4,
        ),
        FieldsLoadedPlan::ShowSectionPane {
            stage: OpPickerStage::Section,
            section_selected: Some(0),
            clear_selected_section: true,
        }
    );
    assert_eq!(
        fields_loaded_plan(
            &OpPickerMode::Create {
                item_name_default: String::new(),
                field_label_default: String::new(),
            },
            true,
            2,
            4,
        ),
        FieldsLoadedPlan::RefreshFieldPane {
            field_selected: Some(0),
            clear_refresh_in_place: true,
        }
    );
}

#[test]
fn refresh_completion_resets_selection_when_rows_change() {
    assert_eq!(items_loaded_plan(1).selected, Some(0));
    assert_eq!(items_loaded_plan(0).selected, None);

    assert_eq!(
        fields_loaded_plan(&OpPickerMode::Browse, true, 1, 1),
        FieldsLoadedPlan::RefreshFieldPane {
            field_selected: Some(0),
            clear_refresh_in_place: true,
        }
    );
    assert_eq!(
        fields_loaded_plan(&OpPickerMode::Browse, true, 1, 0),
        FieldsLoadedPlan::RefreshFieldPane {
            field_selected: None,
            clear_refresh_in_place: true,
        }
    );
}

#[test]
fn recoverable_banner_preserves_selected_list_geometry() {
    let mut state = RenderStateFixture::new(OpPickerStage::Account, Some(1));
    state.load_state = recoverable_load_error_state("temporary op failure");
    let buffer = render_picker_buffer(&state, 60, 9);
    let selected_y = (0..9)
        .find(|y| {
            (0..60)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains("bob@example.com")
        })
        .expect("selected account should remain visible below the banner");

    assert!(
        selected_y > 4,
        "selected row should render in the list area below banner/filter rows"
    );
}

#[test]
fn field_load_sort_policy_puts_concealed_fields_first() {
    #[derive(Debug, PartialEq, Eq)]
    struct Field {
        label: &'static str,
        concealed: bool,
    }

    let mut fields = vec![
        Field {
            label: "plain",
            concealed: false,
        },
        Field {
            label: "secret",
            concealed: true,
        },
    ];

    sort_fields_by_concealed_first(&mut fields, |field| field.concealed);

    assert_eq!(fields[0].label, "secret");
    assert_eq!(fields[1].label, "plain");
}

#[test]
fn selected_account_helpers_derive_cache_key_from_selected_account() {
    struct Account {
        id: &'static str,
    }

    let account = Account { id: "acct_1" };

    assert_eq!(
        selected_account_id(Some(&account), |account| account.id),
        Some("acct_1".to_owned())
    );
    assert_eq!(
        selected_account_id_ref(Some(&account), |account| account.id),
        Some("acct_1")
    );
    assert_eq!(
        selected_account_id::<Account>(None, |account| account.id),
        None
    );
    assert_eq!(
        selected_account_id_ref::<Account>(None, |account| account.id),
        None
    );
}

#[test]
fn selected_entity_id_or_default_derives_or_falls_back_empty() {
    struct Vault {
        id: &'static str,
    }

    let vault = Vault { id: "vault_1" };

    assert_eq!(
        selected_entity_id_or_default(Some(&vault), |vault| vault.id),
        "vault_1"
    );
    assert_eq!(
        selected_entity_id_or_default::<Vault>(None, |vault| vault.id),
        ""
    );
}

#[test]
fn selected_entity_label_or_empty_derives_or_falls_back_empty() {
    struct Item {
        name: &'static str,
    }

    let item = Item { name: "login" };

    assert_eq!(
        selected_entity_label_or_empty(Some(&item), |item| item.name),
        "login"
    );
    assert_eq!(
        selected_entity_label_or_empty::<Item>(None, |item| item.name),
        ""
    );
}

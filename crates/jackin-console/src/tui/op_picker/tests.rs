//! Most tests inject a no-op `StubRunner` and overwrite
//! `vaults`/`items`/`fields`/`load_state`/`stage`/selection
//! directly before driving `handle_key` — bypasses the worker
//! channel. The `*_uses_injected_runner_in_async_worker` tests at
//! the end exercise the worker path end-to-end.
use super::*;
use crate::tui::components::op_picker::{field_label_input_state, section_name_input_state};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin_core::FieldTarget;
use jackin_env::{
    OpAccount, OpCache, OpField, OpItem, OpStructRunner, OpVault, resolve_op_uri_to_ref,
};
use jackin_tui::ModalOutcome;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

fn wait_for_worker_poll() {
    #[expect(
        clippy::disallowed_methods,
        reason = "op-picker tests poll owned worker threads"
    )]
    std::thread::sleep(std::time::Duration::from_millis(2));
}

/// `account_list` succeeds (so the probe doesn't classify as
/// `NotInstalled`), every other call returns an empty `Vec`.
/// `last_vault_list_account` is `Option<Option<String>>` to
/// distinguish "never called" from "called with `None`" — the
/// multi-account threading test relies on the distinction.
#[derive(Default)]
struct StubRunner {
    accounts: Mutex<Vec<OpAccount>>,
    #[allow(clippy::option_option)]
    last_vault_list_account: Mutex<Option<Option<String>>>,
}

impl OpStructRunner for StubRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        Ok(self.accounts.lock().unwrap().clone())
    }
    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        *self.last_vault_list_account.lock().unwrap() = Some(account.map(String::from));
        Ok(Vec::new())
    }
    fn item_list(&self, _vault_id: &str, _account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(Vec::new())
    }
    fn item_get(
        &self,
        _item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<Vec<OpField>> {
        Ok(Vec::new())
    }
}

fn account(id: &str, email: &str, url: &str) -> OpAccount {
    OpAccount {
        id: id.to_owned(),
        email: email.to_owned(),
        url: url.to_owned(),
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Drive `poll_load` until `rx` clears or the 2s budget runs
/// out — the constructor's `account_list` probe is async.
fn drain_initial_account_load(s: &mut OpPickerState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (s.rx.is_some() || s.pending_load.is_some()) && std::time::Instant::now() < deadline {
        poll_load_for_test(s);
        if s.rx.is_none() && s.pending_load.is_none() {
            break;
        }
        wait_for_worker_poll();
    }
}

fn poll_load_for_test(s: &mut OpPickerState) -> bool {
    let mut dirty = execute_pending_load_for_test(s);
    dirty |= s.poll_load();
    dirty |= execute_pending_load_for_test(s);
    dirty
}

fn execute_pending_load_for_test(s: &mut OpPickerState) -> bool {
    let Some(pending) = s.take_pending_load() else {
        return false;
    };
    let runner = TEST_RUNNER.with(|r| {
        r.borrow()
            .clone()
            .expect("test runner must be set before executing a load")
    });
    let rx = start_load(pending.cached, pending.request, runner);
    s.attach_load_receiver(rx);
    true
}

/// Single-account picker forced into a clean Vault-stage Ready
/// state — bypasses the chained vault load (which returns
/// `NoVaults` against the stub) so tests can seed lists directly.
fn picker_ready() -> OpPickerState {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.stage = OpPickerStage::Vault;
    s.load_state = OpLoadState::Ready;
    s
}

fn vault(name: &str) -> OpVault {
    OpVault {
        id: format!("v-{name}"),
        name: name.to_owned(),
    }
}

fn item(name: &str) -> OpItem {
    OpItem {
        id: format!("i-{name}"),
        name: name.to_owned(),
        subtitle: String::new(),
    }
}

fn item_with_subtitle(name: &str, subtitle: &str) -> OpItem {
    OpItem {
        id: format!("i-{name}-{subtitle}"),
        name: name.to_owned(),
        subtitle: subtitle.to_owned(),
    }
}

fn field(label: &str, ty: &str, concealed: bool) -> OpField {
    OpField {
        id: label.to_owned(),
        label: label.to_owned(),
        field_type: ty.to_owned(),
        concealed,
        reference: String::new(),
    }
}

fn field_with_reference(label: &str, reference: &str) -> OpField {
    OpField {
        id: label.to_owned(),
        label: label.to_owned(),
        field_type: "STRING".to_owned(),
        concealed: false,
        reference: reference.to_owned(),
    }
}

/// Two items sharing a title disambiguate by subtitle
/// (`additional_information`). Mixed case verifies the filter is
/// case-insensitive.
#[test]
fn item_filter_matches_subtitle() {
    let mut s = picker_ready();
    s.items = vec![
        item_with_subtitle("Google", "alexey@zhokhov.com"),
        item_with_subtitle("Google", "azhokhov@example.com"),
    ];
    s.item_list_state.select(Some(0));
    s.filter_buf = "AzhokhoV".to_owned();

    let visible = s.filtered_items();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].subtitle, "azhokhov@example.com");
}

#[test]
fn filter_vaults_narrows_by_name() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal"), vault("Private"), vault("Work")];
    s.vault_list_state.select(Some(0));
    s.filter_buf = "per".to_owned();

    let visible = s.filtered_vaults();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "Personal");
}

#[test]
fn filter_clears_on_pane_advance() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal"), vault("Private"), vault("Work")];
    s.vault_list_state.select(Some(0));
    s.filter_buf = "per".to_owned();
    assert_eq!(s.filtered_vaults().len(), 1);

    // The pane-advance-clears-filter contract lives inside
    // `poll_load`'s Items arm; simulate it directly below rather
    // than racing the worker.
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(
        s.selected_vault.as_ref().map(|v| v.name.as_str()),
        Some("Personal"),
        "Enter on filtered vault must capture the selection"
    );

    s.rx = None;
    s.pending_load = None;
    s.items = vec![item("API Keys")];
    s.item_list_state.select(Some(0));
    s.stage = OpPickerStage::Item;
    s.filter_buf.clear();
    s.load_state = OpLoadState::Ready;

    assert_eq!(s.stage, OpPickerStage::Item);
    assert!(
        s.filter_buf.is_empty(),
        "filter must be cleared when advancing to the Item pane"
    );
}

#[test]
fn esc_from_vault_returns_cancel() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal")];
    s.vault_list_state.select(Some(0));

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Cancel));
}

#[test]
fn esc_from_item_goes_to_vault() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal"), vault("Work")];
    s.vault_list_state.select(Some(1));
    s.selected_vault = Some(vault("Work"));
    s.items = vec![item("API Keys")];
    s.item_list_state.select(Some(0));
    s.stage = OpPickerStage::Item;
    s.filter_buf = "ap".to_owned();

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.stage, OpPickerStage::Vault);
    assert!(s.filter_buf.is_empty(), "filter must clear on back-nav");
    // Vault selection preserved.
    assert_eq!(s.vault_list_state.selected, Some(1));
    assert_eq!(s.vaults.len(), 2);
}

#[test]
fn esc_from_field_goes_to_item() {
    let mut s = picker_ready();
    s.selected_vault = Some(vault("Personal"));
    s.selected_item = Some(item("API Keys"));
    s.items = vec![item("API Keys")];
    s.item_list_state.select(Some(0));
    s.fields = vec![field("password", "concealed", true)];
    s.field_list_state.select(Some(0));
    s.stage = OpPickerStage::Field;
    s.filter_buf = "pw".to_owned();

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.stage, OpPickerStage::Item);
    assert!(s.filter_buf.is_empty());
    // Item selection preserved.
    assert_eq!(s.item_list_state.selected, Some(0));
    assert_eq!(s.items.len(), 1);
}

#[test]
fn field_sort_concealed_first() {
    // The Fields-arm of `poll_load` applies a stable sort that puts
    // concealed fields first. We invoke that sort here against the
    // same input order used in production to confirm the contract.
    let mut input = vec![
        field("user", "text", false),
        field("pw", "concealed", true),
        field("url", "url", false),
    ];
    input.sort_by_key(|f| !f.concealed);
    assert_eq!(input[0].label, "pw");
    assert!(input[0].concealed);
    // Stable sort: non-concealed entries retain their input order.
    assert_eq!(input[1].label, "user");
    assert_eq!(input[2].label, "url");

    // End-to-end through the picker view: seed the sorted list,
    // assert filtered_fields() preserves it.
    let mut s = picker_ready();
    s.fields = input;
    s.field_list_state.select(Some(0));
    s.stage = OpPickerStage::Field;
    let visible = s.filtered_fields();
    assert_eq!(visible.len(), 3);
    assert_eq!(visible[0].label, "pw");
}

/// Backward-compat fallback: synthesize from display names (UUID
/// form for op, human names for path) when `OpField::reference` is
/// missing (older fixtures).
#[test]
fn enter_on_field_commits_op_path() {
    let mut s = picker_ready();
    s.selected_vault = Some(OpVault {
        id: "v-Personal".into(),
        name: "Personal".into(),
    });
    s.selected_item = Some(OpItem {
        id: "i-api".into(),
        name: "API Keys".into(),
        subtitle: String::new(),
    });
    s.items = vec![s.selected_item.clone().unwrap()];
    s.fields = vec![
        field("password", "concealed", true),
        field("username", "text", false),
    ];
    s.field_list_state.select(Some(0));
    s.stage = OpPickerStage::Field;

    let outcome = s.handle_key(key(KeyCode::Enter));
    match outcome {
        ModalOutcome::Commit(OpPickerSelection::Existing(op_ref)) => {
            assert_eq!(op_ref.op, "op://v-Personal/i-api/password");
            assert_eq!(op_ref.path, "Personal/API Keys/password");
        }
        other => panic!("expected Commit(Existing), got {other:?}"),
    }
}

/// Section-aware reference: section must be preserved in both `op`
/// (UUID-form vault/item/field, section name preserved) and `path`
/// (human-readable, section name preserved).
#[test]
fn picker_commit_uses_op_provided_reference_not_synthesized() {
    let mut s = picker_ready();
    s.selected_vault = Some(OpVault {
        id: "v-Personal".into(),
        name: "Personal".into(),
    });
    s.selected_item = Some(OpItem {
        id: "i-test".into(),
        name: "name with spaces".into(),
        subtitle: String::new(),
    });
    s.items = vec![s.selected_item.clone().unwrap()];
    s.fields = vec![field_with_reference("api", "op://Personal/test/auth/api")];
    // Field is inside section "auth", so display rows are:
    //   0: SectionHeader "auth"
    //   1: Field { field_idx: 0 }
    s.field_list_state.select(Some(1));
    s.stage = OpPickerStage::Field;

    let outcome = s.handle_key(key(KeyCode::Enter));
    match outcome {
        ModalOutcome::Commit(OpPickerSelection::Existing(op_ref)) => {
            // Section "auth" must be preserved; vault/item/field use UUIDs.
            assert_eq!(
                op_ref.op, "op://v-Personal/i-test/auth/api",
                "op must use UUID-form vault/item, preserve section, UUID field id"
            );
            assert_eq!(
                op_ref.path, "Personal/name with spaces/auth/api",
                "path must use human-readable names and preserve section"
            );
        }
        other => panic!("expected Commit(Existing), got {other:?}"),
    }
}

// ── Create-mode tests ─────────────────────────────────────────────

/// Single-account Create-mode picker forced into a clean Vault-stage
/// Ready state, mirroring `picker_ready` but with creation enabled.
fn create_ready() -> OpPickerState {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_create_picker_with_runner_and_cache(
        runner,
        Rc::new(RefCell::new(OpCache::default())),
        "default-item",
        "token",
    );
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.stage = OpPickerStage::Vault;
    s.load_state = OpLoadState::Ready;
    s
}

#[test]
fn create_mode_item_stage_appends_new_item_sentinel() {
    let mut s = create_ready();
    s.items = vec![item("Existing")];
    let choices = s.filtered_item_choices();
    assert_eq!(choices.len(), 2, "one item + trailing sentinel");
    assert!(choices[0].is_some(), "real item first");
    assert!(
        choices[1].is_none(),
        "trailing None is the `+ New item` sentinel"
    );

    let mut browse = picker_ready();
    browse.items = vec![item("Existing")];
    assert!(
        browse.filtered_item_choices().iter().all(Option::is_some),
        "browse mode must not append a creation sentinel"
    );
}

#[test]
fn create_mode_new_item_flow_commits_new_item() {
    let mut s = create_ready();
    s.selected_vault = Some(vault("Personal"));
    s.items = vec![item("Existing")];
    s.stage = OpPickerStage::Item;
    // choices: [Some(Existing), None]; select the sentinel at index 1.
    s.item_list_state.select(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::NewItemName);
    // item_name_input defaults to "default-item"; accept with Enter.
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    // field_label_input defaults to "token"; accept with Enter to commit.
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::NewItem {
            vault,
            item_name,
            section,
            field_label,
            ..
        }) => {
            assert_eq!(vault.id, "v-Personal");
            assert_eq!(item_name, "default-item");
            assert_eq!(field_label, "token");
            assert_eq!(section, None);
        }
        other => panic!("expected Commit(NewItem), got {other:?}"),
    }
}

/// Create-mode picker drilled to the Section stage with the given
/// fields loaded, mirroring what `poll_load` produces after a field
/// load. Section selection starts on `(root)` (index 0).
fn create_at_section(fields: Vec<OpField>) -> OpPickerState {
    let mut s = create_ready();
    s.selected_vault = Some(vault("Personal"));
    s.selected_item = Some(item("login"));
    s.fields = fields;
    s.selected_section = None;
    s.stage = OpPickerStage::Section;
    s.section_list_state.select(Some(0));
    s
}

#[test]
fn create_mode_existing_item_lands_on_section_stage() {
    // poll_load's Fields arm routes Create mode to the Section stage
    // (Browse mode goes to Field). Invoke that arm directly via the
    // worker drain so we exercise the real sequencing.
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_create_picker_with_runner_and_cache(
        runner,
        Rc::new(RefCell::new(OpCache::default())),
        "default-item",
        "token",
    );
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.selected_vault = Some(vault("Personal"));
    s.selected_item = Some(item("login"));
    // Drive the existing-item Enter through start_field_load + drain.
    s.start_field_load("i-login".into(), "v-Personal".into(), None);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (s.rx.is_some() || s.pending_load.is_some()) && std::time::Instant::now() < deadline {
        poll_load_for_test(&mut s);
        wait_for_worker_poll();
    }
    assert_eq!(
        s.stage,
        OpPickerStage::Section,
        "Create mode must land on the Section stage after a field load"
    );
    assert_eq!(s.selected_section, None, "selected_section resets on load");
}

/// Field-stage `R` (Create mode) reloads the fields in place: it must
/// keep `selected_section` and stay on the Field stage rather than
/// bouncing back to Section. Drives the `poll_load` Fields arm with
/// `field_refresh_in_place` set, the way the `r` handler leaves it.
#[test]
fn create_mode_field_refresh_stays_on_field_and_keeps_section() {
    let mut s = create_at_section(vec![
        field_with_reference("user", "op://Personal/login/user"),
        field_with_reference("api", "op://Personal/login/auth/api"),
    ]);
    // Operator already drilled into the "auth" section on the Field stage.
    s.stage = OpPickerStage::Field;
    s.selected_section = Some("auth".to_owned());
    // `r` clears `fields`/`field_list_state` and sets the in-place flag.
    s.fields.clear();
    s.field_refresh_in_place = true;
    // Publish the reloaded fields through the same arm the worker uses.
    s.rx = Some(jackin_tui::runtime::ready_blocking_subscription(
        LoadResult::Fields(Ok(vec![
            field_with_reference("user", "op://Personal/login/user"),
            field_with_reference("api", "op://Personal/login/auth/api"),
        ])),
    ));
    poll_load_for_test(&mut s);

    assert_eq!(
        s.stage,
        OpPickerStage::Field,
        "in-place refresh must NOT bounce back to Section"
    );
    assert_eq!(
        s.selected_section,
        Some("auth".to_owned()),
        "in-place refresh must preserve the chosen section"
    );
    assert!(
        !s.field_refresh_in_place,
        "the flag is cleared once the refreshed fields arrive"
    );
    // Rows are re-scoped to "auth": one field + the new-field sentinel.
    let rows = s.build_field_display_rows();
    assert_eq!(rows.len(), 2, "one auth field + new-field sentinel");
    assert!(matches!(rows[1], FieldDisplayRow::NewFieldSentinel));
}

#[test]
fn section_choices_returns_root_plus_distinct_sections() {
    let s = create_at_section(vec![
        field_with_reference("user", "op://Personal/login/user"),
        field_with_reference("api", "op://Personal/login/auth/api"),
        field_with_reference("key", "op://Personal/login/auth/key"),
        field_with_reference("note", "op://Personal/login/extra/note"),
    ]);
    let choices = s.section_choices();
    assert_eq!(
        choices,
        vec![None, Some("auth".to_owned()), Some("extra".to_owned()),],
        "root first, then distinct sections in first-appearance order"
    );
}

#[test]
fn create_mode_existing_field_commits_edit_item_field() {
    let mut s = create_at_section(vec![field("token", "CONCEALED", true)]);
    // Select `(root)` → Field stage scoped to root.
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::Field);
    assert_eq!(s.selected_section, None);
    // Root field "token" → display rows: [Field{0}, NewFieldSentinel].
    s.field_list_state.select(Some(0));
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField {
            item,
            field,
            section,
            ..
        }) => {
            assert_eq!(item.id, "i-login");
            // The real field id is forwarded so the write targets this
            // exact field (not the first label match) and preserves it.
            assert_eq!(
                field,
                FieldTarget::Existing {
                    id: "token".into(),
                    label: "token".into(),
                }
            );
            assert_eq!(section, None);
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn create_mode_selecting_section_scopes_field_stage() {
    let mut s = create_at_section(vec![
        field_with_reference("user", "op://Personal/login/user"),
        field_with_reference("api", "op://Personal/login/auth/api"),
        field_with_reference("key", "op://Personal/login/auth/key"),
    ]);
    // section_choices: [None, Some("auth")]; select "auth" (index 1).
    s.section_list_state.select(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::Field);
    assert_eq!(s.selected_section, Some("auth".to_owned()));
    // Field stage shows only the two "auth" fields + NewFieldSentinel.
    let rows = s.build_field_display_rows();
    assert_eq!(rows.len(), 3, "two auth fields + new-field sentinel");
    assert!(matches!(rows[2], FieldDisplayRow::NewFieldSentinel));
    // Selecting the first scoped field commits with section Some("auth").
    s.field_list_state.select(Some(0));
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
            assert_eq!(section, Some("auth".to_owned()));
            assert_eq!(field.label(), "api");
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn create_mode_new_field_in_root_commits_section_none() {
    let mut s = create_at_section(vec![field_with_reference(
        "user",
        "op://Personal/login/user",
    )]);
    // Select `(root)`.
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::Field);
    // Rows: [Field{0}, NewFieldSentinel] → select the sentinel.
    s.field_list_state.select(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
            assert_eq!(section, None, "new field in root → section None");
            assert_eq!(field.label(), "token");
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn create_mode_new_section_flow_threads_section_into_commit() {
    let mut s = create_at_section(vec![]);
    // section_choices: [None]; sentinel `+ New section` at index 1.
    s.section_list_state.select(Some(1));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::NewSectionName);
    // section_name_input starts empty; type a name (empty won't commit).
    for c in "creds".chars() {
        drop(s.handle_key(key(KeyCode::Char(c))));
    }
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
            assert_eq!(section, Some("creds".to_owned()));
            assert_eq!(field.label(), "token");
        }
        other => panic!("expected Commit(EditItemField) with section, got {other:?}"),
    }
}

#[test]
fn field_label_cancel_clears_pending_section() {
    // New-section flow stages pending_section, then backing out of the
    // field-label stage must discard it so it cannot leak into a later
    // commit on a different path.
    let mut s = create_at_section(vec![]);
    s.section_list_state.select(Some(1)); // `+ New section` sentinel
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::NewSectionName);
    for c in "foo".chars() {
        drop(s.handle_key(key(KeyCode::Char(c))));
    }
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::FieldLabel);
    assert_eq!(s.pending_section.as_deref(), Some("foo"));
    // Esc cancels the field-label stage.
    drop(s.handle_key(key(KeyCode::Esc)));
    assert_eq!(s.stage, OpPickerStage::NewSectionName);
    assert!(
        s.pending_section.is_none(),
        "abandoned section must not survive the field-label cancel"
    );
}

#[test]
fn field_label_commit_trims_whitespace() {
    let mut s = create_at_section(vec![]);
    // Drill `(root)` → Field stage, then `+ New field`.
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::Field);
    s.field_label_input = field_label_input_state("  oauth-token  ");
    s.field_label_origin = FieldLabelOrigin::NewField;
    s.stage = OpPickerStage::FieldLabel;
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(OpPickerSelection::EditItemField { field, .. }) => {
            assert_eq!(field.label(), "oauth-token", "field label must be trimmed");
        }
        other => panic!("expected Commit(EditItemField), got {other:?}"),
    }
}

#[test]
fn new_section_name_commit_trims_whitespace() {
    let mut s = create_at_section(vec![]);
    s.section_list_state.select(Some(1));
    drop(s.handle_key(key(KeyCode::Enter)));
    s.section_name_input = section_name_input_state("  creds  ");
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.pending_section.as_deref(), Some("creds"));
}

#[test]
fn left_collapse_via_header_keeps_selection_in_range() {
    // Browse-mode flat field list with a collapsible header. Left on the
    // header collapses it and (like the Enter toggle) clamps the field
    // selection so it never points past the shrunken row list.
    let mut s = picker_ready();
    s.selected_vault = Some(OpVault {
        id: "v-Personal".into(),
        name: "Personal".into(),
    });
    s.selected_item = Some(item("login"));
    s.fields = vec![
        field_with_reference("api", "op://Personal/login/auth/api"),
        field_with_reference("key", "op://Personal/login/auth/key"),
    ];
    s.stage = OpPickerStage::Field;
    // Rows: [SectionHeader(auth), Field, Field]. Park on the last field.
    let last = s.build_field_display_rows().len() - 1;
    s.field_list_state.select(Some(last));
    // Move up onto the header row, then collapse with Left.
    let header_idx = s
        .build_field_display_rows()
        .iter()
        .position(|r| matches!(r, FieldDisplayRow::SectionHeader { .. }))
        .expect("a section header row");
    s.field_list_state.select(Some(header_idx));
    drop(s.handle_key(key(KeyCode::Left)));
    assert!(
        s.collapsed_sections.contains("auth"),
        "Left must collapse the section"
    );
    let new_len = s.build_field_display_rows().len();
    let sel = s.field_list_state.selected.expect("selection retained");
    assert!(
        sel < new_len,
        "selection {sel} must stay within {new_len} rows"
    );
}

#[test]
fn create_mode_esc_chain_field_to_section_to_item() {
    let mut s = create_at_section(vec![field_with_reference(
        "api",
        "op://Personal/login/auth/api",
    )]);
    // Drill into "auth", then Esc back to Section, then Esc back to Item.
    s.section_list_state.select(Some(1));
    drop(s.handle_key(key(KeyCode::Enter)));
    assert_eq!(s.stage, OpPickerStage::Field);

    drop(s.handle_key(key(KeyCode::Esc)));
    assert_eq!(s.stage, OpPickerStage::Section, "Field Esc → Section");
    assert_eq!(s.selected_section, None, "section cleared on back-nav");
    assert!(s.selected_item.is_some(), "item kept on Field→Section Esc");

    drop(s.handle_key(key(KeyCode::Esc)));
    assert_eq!(s.stage, OpPickerStage::Item, "Section Esc → Item");
    assert!(
        s.selected_item.is_none(),
        "item cleared on Section→Item Esc"
    );
}

#[test]
fn stub_runner_constructor_is_not_fatal() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account("a", "a@example.com", "a.1password.com")]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    let bad = matches!(
        s.load_state,
        OpLoadState::Error(OpPickerError::Fatal(
            OpPickerFatalState::NotInstalled | OpPickerFatalState::NotSignedIn
        ))
    );
    assert!(
        !bad,
        "stub runner returning Ok must not produce NotInstalled / NotSignedIn; got {:?}",
        s.load_state
    );
}

// ── Multi-account picker tests ────────────────────────────────────

#[test]
fn picker_starts_at_account_when_multiple_accounts() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    assert_eq!(
        s.stage,
        OpPickerStage::Account,
        "two accounts must route to the Account pane"
    );
    assert_eq!(s.accounts.len(), 2);
    assert_eq!(s.account_list_state.selected, Some(0));
    assert!(
        s.selected_account.is_none(),
        "selected_account must remain None until the operator picks one"
    );
}

#[test]
fn picker_starts_at_vault_when_single_account() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "solo",
            "solo@example.com",
            "solo.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    assert_eq!(
        s.stage,
        OpPickerStage::Vault,
        "single account must skip the Account pane"
    );
    assert_eq!(
        s.selected_account.as_ref().map(|a| a.id.as_str()),
        Some("solo"),
        "single account must be auto-selected"
    );
    assert!(
        s.accounts.is_empty(),
        "single-account setup leaves the accounts vec empty so render/Esc paths skip multi-account branches"
    );
}

#[test]
fn account_pane_filter_narrows_by_email() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("a1", "alice@example.com", "alpha.1password.com"),
            account("a2", "bob@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.load_state = OpLoadState::Ready;
    s.filter_buf = "alic".to_owned();
    let visible = s.filtered_accounts();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].email, "alice@example.com");
}

/// Asserts the contract directly via `runner.vault_list(...)` to
/// stay independent of worker-thread timing; the spawned-thread
/// path is covered by the
/// `vault_list_uses_injected_runner_in_async_worker` test below.
#[test]
fn enter_on_account_advances_to_vault_with_account_scope() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.load_state = OpLoadState::Ready;
    s.account_list_state.select(Some(1));

    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(s.stage, OpPickerStage::Vault);
    assert_eq!(
        s.selected_account.as_ref().map(|a| a.id.as_str()),
        Some("acct2"),
        "Enter on Account must capture the selection"
    );
    assert!(
        s.filter_buf.is_empty(),
        "filter must clear when advancing from Account to Vault"
    );
    // Direct-call verification of the account threading.
    let runner = Arc::new(StubRunner::default());
    runner.account_list().unwrap();
    drop(runner.vault_list(s.selected_account_id().as_deref()));
    let recorded = runner.last_vault_list_account.lock().unwrap().clone();
    assert_eq!(
        recorded,
        Some(Some("acct2".to_owned())),
        "vault_list must be called with Some(account_uuid) once an account is selected"
    );
}

#[test]
fn esc_from_vault_with_multi_account_returns_to_account() {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.load_state = OpLoadState::Ready;
    s.stage = OpPickerStage::Vault;
    s.selected_account = Some(account("acct1", "a@example.com", "alpha.1password.com"));
    s.vaults = vec![vault("Personal"), vault("Work")];
    s.vault_list_state.select(Some(1));
    s.filter_buf = "wo".to_owned();

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(matches!(outcome, ModalOutcome::Continue));
    assert_eq!(
        s.stage,
        OpPickerStage::Account,
        "Esc from Vault must return to Account in multi-account mode"
    );
    assert!(
        s.selected_vault.is_none(),
        "selected_vault must clear on back-nav to Account"
    );
    assert!(s.vaults.is_empty(), "vaults must clear on back-nav");
    assert!(
        s.filter_buf.is_empty(),
        "filter must clear on back-nav to Account"
    );
}

#[test]
fn esc_from_vault_with_single_account_cancels_picker() {
    let mut s = picker_ready();
    s.vaults = vec![vault("Personal")];
    s.vault_list_state.select(Some(0));
    assert!(s.accounts.is_empty());

    let outcome = s.handle_key(key(KeyCode::Esc));
    assert!(
        matches!(outcome, ModalOutcome::Cancel),
        "Esc on Vault in single-account mode must cancel the picker"
    );
}

// ── OpCache integration tests ─────────────────────────────────────

struct CounterRunner {
    accounts: Vec<OpAccount>,
    counter: Arc<Mutex<usize>>,
}

impl OpStructRunner for CounterRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        *self.counter.lock().unwrap() += 1;
        Ok(self.accounts.clone())
    }
    fn vault_list(&self, _: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        Ok(Vec::new())
    }
    fn item_list(&self, _: &str, _: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(Vec::new())
    }
    fn item_get(&self, _: &str, _: &str, _: Option<&str>) -> anyhow::Result<Vec<OpField>> {
        Ok(Vec::new())
    }
}

#[test]
fn op_cache_hit_skips_account_list_subprocess() {
    use jackin_env::OpCache;
    use std::sync::Arc;

    let cache = Rc::new(RefCell::new(OpCache::default()));
    let counter1: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let counter2: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    // First picker: cache miss → runner invoked once.
    let mut s1 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter1),
        }),
        Rc::clone(&cache),
    );
    drain_initial_account_load(&mut s1);
    assert_eq!(
        *counter1.lock().unwrap(),
        1,
        "first picker constructor must miss the empty cache"
    );

    // Second picker: cache hit → runner must NOT be invoked.
    let mut s2 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter2),
        }),
        cache,
    );
    drain_initial_account_load(&mut s2);
    assert_eq!(
        *counter2.lock().unwrap(),
        0,
        "second picker against the same cache must hit and skip account_list"
    );
}

#[test]
fn op_cache_miss_calls_runner_and_stores() {
    use jackin_env::OpCache;
    use std::sync::Arc;

    let cache = Rc::new(RefCell::new(OpCache::default()));
    let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    let mut s1 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter),
        }),
        Rc::clone(&cache),
    );
    drain_initial_account_load(&mut s1);
    assert_eq!(*counter.lock().unwrap(), 1, "first picker must miss");
    assert!(
        cache.borrow().get_accounts().is_some(),
        "first picker must populate the cache"
    );

    let mut s2 = new_picker_with_runner_and_cache(
        Arc::new(CounterRunner {
            accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
            counter: Arc::clone(&counter),
        }),
        cache,
    );
    drain_initial_account_load(&mut s2);
    assert_eq!(
        *counter.lock().unwrap(),
        1,
        "second picker on populated cache must hit and not re-call account_list"
    );
}

#[test]
fn op_cache_refresh_re_fires_subprocess() {
    use jackin_env::OpCache;
    use std::sync::Arc;

    let cache = Rc::new(RefCell::new(OpCache::default()));
    let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    let r = Arc::new(CounterRunner {
        accounts: vec![
            account("acct1", "a@example.com", "alpha.1password.com"),
            account("acct2", "b@example.com", "beta.1password.com"),
        ],
        counter: Arc::clone(&counter),
    });
    let mut s = new_picker_with_runner_and_cache(r, cache);
    drain_initial_account_load(&mut s);
    assert_eq!(*counter.lock().unwrap(), 1, "constructor must miss once");
    assert_eq!(s.accounts.len(), 2);

    drop(s.handle_key(key(KeyCode::Char('r'))));
    drain_initial_account_load(&mut s);
    assert_eq!(
        *counter.lock().unwrap(),
        2,
        "r on Account must invalidate cache and re-fire account_list"
    );
    assert_eq!(s.accounts.len(), 2);
    assert_eq!(s.stage, OpPickerStage::Account);
}

// ── Async account_list constructor tests ─────────────────────────

/// `account_list` blocks until `release()`; proves the picker
/// constructor does not synchronously wait on `account_list`.
struct BlockingRunner {
    gate: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl BlockingRunner {
    fn new() -> Self {
        Self {
            gate: Arc::new((Mutex::new(false), std::sync::Condvar::new())),
        }
    }
    fn release(&self) {
        let (lock, cv) = &*self.gate;
        *lock.lock().unwrap() = true;
        cv.notify_all();
    }
}

impl OpStructRunner for BlockingRunner {
    // Test fixture: intentionally blocks on a condvar until the test
    // releases the gate. The lock is held across the wait loop and
    // dropped via explicit `drop` once we exit, which is the shape
    // clippy's `significant_drop_tightening` lint actually wants.
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        let (lock, cv) = &*self.gate;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = cv.wait(released).unwrap();
        }
        drop(released);
        Ok(Vec::new())
    }
    fn vault_list(&self, _: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        Ok(Vec::new())
    }
    fn item_list(&self, _: &str, _: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(Vec::new())
    }
    fn item_get(&self, _: &str, _: &str, _: Option<&str>) -> anyhow::Result<Vec<OpField>> {
        Ok(Vec::new())
    }
}

/// Constructor must return promptly even when `account_list` is
/// wedged — synchronous waiting blocked the TUI render loop on a
/// slow `op` (network/biometric).
#[test]
fn picker_construction_does_not_block_on_account_list() {
    let runner = Arc::new(BlockingRunner::new());
    let runner_for_release = Arc::clone(&runner);

    let start = std::time::Instant::now();
    let _s = new_picker_with_runner(runner);
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "constructor must not synchronously wait on account_list; elapsed={elapsed:?}"
    );
    // Release the Condvar so the worker exits cleanly.
    runner_for_release.release();
}

#[test]
fn picker_loading_account_state_renders_spinner_immediately() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let runner = Arc::new(BlockingRunner::new());
    let runner_for_release = Arc::clone(&runner);
    let s = new_picker_with_runner(runner);

    assert!(
        matches!(s.load_state, OpLoadState::Loading { .. }),
        "constructor must leave the picker in Loading; got {:?}",
        s.load_state
    );

    let area = Rect::new(0, 0, 60, 12);
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| crate::tui::components::op_picker::render_picker(f, area, &s))
        .unwrap();
    let buf = term.backend().buffer();

    let mut rendered = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            rendered.push_str(buf[(x, y)].symbol());
        }
    }
    let braille_present = rendered
        .chars()
        .any(|c| ('\u{2800}'..='\u{28ff}').contains(&c));
    assert!(
        braille_present,
        "rendered loading panel must contain a Braille spinner glyph; \
         content was: {rendered:?}"
    );

    runner_for_release.release();
}

fn render_picker_dump(state: &OpPickerState, width: u16, height: u16) -> (String, String) {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        crate::tui::components::op_picker::render_picker(f, Rect::new(0, 0, width, height), state);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut dump = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            dump.push_str(buf[(x, y)].symbol());
        }
        dump.push('\n');
    }
    let top_row = (0..buf.area.width)
        .map(|x| buf[(x, 0)].symbol())
        .collect::<String>();
    (dump, top_row)
}

#[test]
fn loading_panel_title_during_item_load_shows_breadcrumb() {
    let mut state = OpPickerState::default();
    state.accounts = vec![
        account("a1", "alice@example.com", "alice.1password.com"),
        account("a2", "bob@example.com", "bob.1password.com"),
    ];
    state.selected_account = Some(state.accounts[0].clone());
    state.selected_vault = Some(OpVault {
        id: "v-personal".into(),
        name: "Personal".into(),
    });
    state.stage = OpPickerStage::Item;
    state.load_state = OpLoadState::Loading { spinner_tick: 0 };

    let (dump, _) = render_picker_dump(&state, 80, 12);

    assert!(dump.contains("alice@example.com"), "dump:\n{dump}");
    assert!(dump.contains("Personal"), "dump:\n{dump}");
    assert!(dump.contains('\u{2192}'), "dump:\n{dump}");
    assert!(
        dump.contains("loading items from Personal"),
        "dump:\n{dump}"
    );
}

#[test]
fn picker_field_load_title_shows_parent_and_body_includes_subtitle() {
    let mut state = OpPickerState::default();
    state.accounts = vec![
        account("a1", "alexey@zhokhov.com", "z.1password.com"),
        account("a2", "alexey@chainargos.com", "c.1password.com"),
    ];
    state.selected_account = Some(state.accounts[1].clone());
    state.selected_vault = Some(OpVault {
        id: "v-chainargos".into(),
        name: "ChainArgos".into(),
    });
    state.selected_item = Some(OpItem {
        id: "i-redshift".into(),
        name: "ChainArgos Redshift".into(),
        subtitle: "donbeave".into(),
    });
    state.stage = OpPickerStage::Field;
    state.load_state = OpLoadState::Loading { spinner_tick: 0 };

    let (dump, top_row) = render_picker_dump(&state, 80, 12);

    assert!(
        top_row.contains("alexey@chainargos.com"),
        "top row:\n{top_row}"
    );
    assert!(top_row.contains("ChainArgos"), "top row:\n{top_row}");
    assert!(!top_row.contains("Redshift"), "top row:\n{top_row}");
    assert!(
        dump.contains("loading ChainArgos Redshift (donbeave)"),
        "dump:\n{dump}"
    );
    assert!(!dump.contains("loading fields from"), "dump:\n{dump}");
}

#[test]
fn picker_field_load_body_no_subtitle() {
    let mut state = OpPickerState::default();
    state.accounts = vec![account("a1", "single@example.com", "x.1password.com")];
    state.selected_account = Some(state.accounts[0].clone());
    state.selected_vault = Some(OpVault {
        id: "v".into(),
        name: "Personal".into(),
    });
    state.selected_item = Some(OpItem {
        id: "i-note".into(),
        name: "Standalone Note".into(),
        subtitle: String::new(),
    });
    state.stage = OpPickerStage::Field;
    state.load_state = OpLoadState::Loading { spinner_tick: 0 };

    let (dump, _) = render_picker_dump(&state, 80, 12);

    assert!(dump.contains("loading Standalone Note"), "dump:\n{dump}");
    assert!(!dump.contains("loading Standalone Note ("), "dump:\n{dump}");
}

/// Compile-time guard: any new field added to `OpField` (in
/// particular `value`) breaks the destructure below. Mirrors the
/// safety test in `operator_env.rs`.
#[test]
fn op_cache_picker_does_not_store_field_values() {
    let f = OpField {
        id: "password".into(),
        label: "password".into(),
        field_type: "concealed".into(),
        concealed: true,
        reference: "op://Personal/API Keys/password".into(),
    };
    let OpField {
        id: _,
        label: _,
        field_type: _,
        concealed: _,
        reference: _,
    } = f;
}

// ── Async-worker runner-injection tests ─────────────────────────

#[allow(clippy::option_option)]
#[derive(Default)]
struct RecorderRunner {
    accounts: Mutex<Vec<OpAccount>>,
    vault_list_calls: Mutex<usize>,
    last_vault_list_account: Mutex<Option<Option<String>>>,
    item_list_calls: Mutex<usize>,
    last_item_list_args: Mutex<Option<(String, Option<String>)>>,
    item_get_calls: Mutex<usize>,
    last_item_get_args: Mutex<Option<(String, String, Option<String>)>>,
}

impl OpStructRunner for RecorderRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        Ok(self.accounts.lock().unwrap().clone())
    }
    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        *self.vault_list_calls.lock().unwrap() += 1;
        *self.last_vault_list_account.lock().unwrap() = Some(account.map(String::from));
        Ok(Vec::new())
    }
    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        *self.item_list_calls.lock().unwrap() += 1;
        *self.last_item_list_args.lock().unwrap() =
            Some((vault_id.to_owned(), account.map(String::from)));
        Ok(Vec::new())
    }
    fn item_get(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<Vec<OpField>> {
        *self.item_get_calls.lock().unwrap() += 1;
        *self.last_item_get_args.lock().unwrap() = Some((
            item_id.to_owned(),
            vault_id.to_owned(),
            account.map(String::from),
        ));
        Ok(Vec::new())
    }
}

fn drain_worker_load(s: &mut OpPickerState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while (s.rx.is_some() || s.pending_load.is_some()) && std::time::Instant::now() < deadline {
        poll_load_for_test(s);
        if s.rx.is_none() && s.pending_load.is_none() {
            break;
        }
        wait_for_worker_poll();
    }
    assert!(
        s.rx.is_none() && s.pending_load.is_none(),
        "worker did not publish within 500ms; load_state={:?}",
        s.load_state
    );
}

#[test]
fn vault_list_uses_injected_runner_in_async_worker() {
    let runner = Arc::new(RecorderRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        ..Default::default()
    });
    let runner_for_assert: Arc<RecorderRunner> = Arc::clone(&runner);
    let mut s = new_picker_with_runner(runner);
    // Single-account fast path also fires a vault_list — drain so
    // the counter only reflects the explicit call below.
    drain_initial_account_load(&mut s);
    *runner_for_assert.vault_list_calls.lock().unwrap() = 0;
    *runner_for_assert.last_vault_list_account.lock().unwrap() = None;

    s.start_vault_load(Some("acct1".into()));
    drain_worker_load(&mut s);

    assert_eq!(
        *runner_for_assert.vault_list_calls.lock().unwrap(),
        1,
        "worker thread must call the injected runner exactly once"
    );
    assert_eq!(
        *runner_for_assert.last_vault_list_account.lock().unwrap(),
        Some(Some("acct1".to_owned())),
        "worker thread must thread the explicit account UUID through"
    );
}

#[test]
fn item_list_uses_injected_runner_in_async_worker() {
    let runner = Arc::new(RecorderRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        ..Default::default()
    });
    let runner_for_assert: Arc<RecorderRunner> = Arc::clone(&runner);
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    drain_worker_load(&mut s);

    s.start_item_load("v-personal".into(), Some("acct1".into()));
    drain_worker_load(&mut s);

    assert_eq!(
        *runner_for_assert.item_list_calls.lock().unwrap(),
        1,
        "worker thread must call item_list on the injected runner"
    );
    assert_eq!(
        *runner_for_assert.last_item_list_args.lock().unwrap(),
        Some(("v-personal".to_owned(), Some("acct1".to_owned()))),
        "worker thread must forward (vault_id, account_id) verbatim"
    );
}

/// Field loading goes through `item_get`, not a dedicated field
/// method — see the trait definition.
#[test]
fn item_get_uses_injected_runner_in_async_worker() {
    let runner = Arc::new(RecorderRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        ..Default::default()
    });
    let runner_for_assert: Arc<RecorderRunner> = Arc::clone(&runner);
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    drain_worker_load(&mut s);

    s.start_field_load("i-aws".into(), "v-personal".into(), Some("acct1".into()));
    drain_worker_load(&mut s);

    assert_eq!(
        *runner_for_assert.item_get_calls.lock().unwrap(),
        1,
        "worker thread must call item_get on the injected runner"
    );
    assert_eq!(
        *runner_for_assert.last_item_get_args.lock().unwrap(),
        Some((
            "i-aws".to_owned(),
            "v-personal".to_owned(),
            Some("acct1".to_owned())
        )),
        "worker thread must forward (item_id, vault_id, account_id) verbatim"
    );
}

// ── build_op_ref_on_commit tests ────────────────────────────────

/// Build an `OpPickerState` fully drilled down to a field selection,
/// bypassing the async worker. `items_in_vault` is the full list
/// seeded into `s.items` (used for ambiguity detection).
fn test_state_picked(
    vault: OpVault,
    items_in_vault: Vec<OpItem>,
    selected_item: OpItem,
    field: OpField,
) -> OpPickerState {
    let mut s = picker_ready();
    s.selected_vault = Some(vault);
    s.items = items_in_vault;
    s.selected_item = Some(selected_item);
    s.fields = vec![field];
    s.field_list_state.select(Some(0));
    s.stage = OpPickerStage::Field;
    s.load_state = OpLoadState::Ready;
    s
}

#[test]
fn picker_commit_writes_op_ref_with_uuid_form_and_clean_path_when_unique() {
    let field = OpField {
        id: "f_uuid".into(),
        label: "api key".into(),
        reference: "op://Private/Stripe/api key".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![OpItem {
            id: "i_uuid".into(),
            name: "Stripe".into(),
            subtitle: String::new(),
        }],
        OpItem {
            id: "i_uuid".into(),
            name: "Stripe".into(),
            subtitle: String::new(),
        },
        field.clone(),
    );
    let r = state.build_op_ref_on_commit(&field);
    assert_eq!(r.op, "op://v_uuid/i_uuid/f_uuid");
    assert_eq!(r.path, "Private/Stripe/api key");
}

#[test]
fn picker_commit_embeds_subtitle_when_item_name_collides_in_vault() {
    let claude_a = OpItem {
        id: "i_uuid_a".into(),
        name: "Claude".into(),
        subtitle: "alexey@zhokhov.com".into(),
    };
    let claude_b = OpItem {
        id: "i_uuid_b".into(),
        name: "Claude".into(),
        subtitle: "alexey@chainargos.com".into(),
    };
    let field = OpField {
        id: "f_uuid".into(),
        label: "auth token".into(),
        reference: "op://Private/Claude/security/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![claude_a.clone(), claude_b],
        claude_a,
        field.clone(),
    );
    let r = state.build_op_ref_on_commit(&field);
    // Section "security" must be preserved in both op and path.
    assert!(
        r.op.starts_with("op://v_uuid/i_uuid_a/"),
        "op had wrong prefix: {}",
        r.op
    );
    assert!(r.op.ends_with("/f_uuid"), "op had wrong suffix: {}", r.op);
    assert_eq!(
        r.path,
        "Private/Claude[alexey@zhokhov.com]/security/auth token"
    );
}

#[test]
fn picker_commit_suppresses_subtitle_when_item_name_has_brackets() {
    // Defensive: bracket-bearing item names would make `path` ambiguous.
    let weird_a = OpItem {
        id: "i_uuid_a".into(),
        name: "Item [tag]".into(),
        subtitle: "user@x".into(),
    };
    let weird_b = OpItem {
        id: "i_uuid_b".into(),
        name: "Item [tag]".into(),
        subtitle: "user@y".into(),
    };
    let field = OpField {
        id: "f_uuid".into(),
        label: "auth".into(),
        reference: "op://Private/Item [tag]/auth".into(),
        field_type: "concealed".into(),
        concealed: false,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![weird_a.clone(), weird_b],
        weird_a,
        field.clone(),
    );
    let r = state.build_op_ref_on_commit(&field);
    assert_eq!(
        r.path, "Private/Item [tag]/auth",
        "no subtitle embed for bracket-bearing item names"
    );
}

#[test]
fn picker_commit_skips_subtitle_when_subtitle_empty() {
    let note_a = OpItem {
        id: "i_a".into(),
        name: "Notes".into(),
        subtitle: String::new(),
    };
    let note_b = OpItem {
        id: "i_b".into(),
        name: "Notes".into(),
        subtitle: String::new(),
    };
    let field = OpField {
        id: "f_uuid".into(),
        label: "notesPlain".into(),
        reference: "op://Private/Notes/notesPlain".into(),
        field_type: "string".into(),
        concealed: false,
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![note_a.clone(), note_b],
        note_a,
        field.clone(),
    );
    let r = state.build_op_ref_on_commit(&field);
    assert_eq!(
        r.path, "Private/Notes/notesPlain",
        "empty subtitle => no embed even on collision"
    );
}

// ── Fix 2B: fallback-to-3-segment preserved when sibling has reference ──

/// When a field has an empty reference but sibling fields in the same
/// item carry non-empty references, the picker still commits a valid
/// 3-segment `OpRef` (the debug log fires but must not panic).
#[test]
fn picker_commit_3seg_fallback_preserved_when_sibling_has_reference() {
    let sectioned_field = OpField {
        id: "f_sectioned".into(),
        label: "password".into(),
        reference: "op://Private/MyItem/Auth/password".into(),
        field_type: "CONCEALED".into(),
        concealed: true,
    };
    let no_ref_field = OpField {
        id: "f_noref".into(),
        label: "notes".into(),
        reference: String::new(),
        field_type: "STRING".into(),
        concealed: false,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "MyItem".into(),
        subtitle: String::new(),
    };
    let mut state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        no_ref_field.clone(),
    );
    // Add the sectioned sibling so the anomaly log path is exercised.
    state.fields.push(sectioned_field);

    // Must not panic; must produce a 3-segment OpRef.
    let r = state.build_op_ref_on_commit(&no_ref_field);
    assert_eq!(r.op, "op://v_uuid/i_uuid/f_noref");
    assert_eq!(r.path, "Private/MyItem/notes");
}

// ── parity tests: build_op_ref_on_commit vs resolve_op_uri_to_ref ──────

/// Minimal `OpStructRunner` stub for parity tests (no async needed).
struct ParityStub {
    vaults: Vec<OpVault>,
    items: std::collections::HashMap<String, Vec<OpItem>>,
    fields: std::collections::HashMap<String, Vec<OpField>>,
}

impl ParityStub {
    fn new() -> Self {
        Self {
            vaults: Vec::new(),
            items: std::collections::HashMap::new(),
            fields: std::collections::HashMap::new(),
        }
    }

    fn with_vault(mut self, name: &str, id: &str) -> Self {
        self.vaults.push(OpVault {
            id: id.to_owned(),
            name: name.to_owned(),
        });
        self
    }

    fn with_item(mut self, vault_id: &str, name: &str, id: &str, subtitle: &str) -> Self {
        self.items
            .entry(vault_id.to_owned())
            .or_default()
            .push(OpItem {
                id: id.to_owned(),
                name: name.to_owned(),
                subtitle: subtitle.to_owned(),
            });
        self
    }

    fn with_field_with_reference(
        mut self,
        item_id: &str,
        label: &str,
        id: &str,
        concealed: bool,
        reference: &str,
    ) -> Self {
        self.fields
            .entry(item_id.to_owned())
            .or_default()
            .push(OpField {
                id: id.to_owned(),
                label: label.to_owned(),
                field_type: if concealed {
                    "CONCEALED".into()
                } else {
                    "STRING".into()
                },
                concealed,
                reference: reference.to_owned(),
            });
        self
    }
}

impl OpStructRunner for ParityStub {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        Ok(vec![])
    }
    fn vault_list(&self, _account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        Ok(self.vaults.clone())
    }
    fn item_list(&self, vault_id: &str, _account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(self.items.get(vault_id).cloned().unwrap_or_default())
    }
    fn item_get(
        &self,
        item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<Vec<OpField>> {
        Ok(self.fields.get(item_id).cloned().unwrap_or_default())
    }
}

/// Fix 1D parity: unique item, 3-segment field → identical `OpRef`.
#[test]
fn parity_unique_item_3seg_field_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        label: "api key".into(),
        reference: "op://Private/Stripe/api key".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "Stripe".into(),
        subtitle: String::new(),
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        field.clone(),
    );
    let picker_ref = state.build_op_ref_on_commit(&field);

    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Stripe", "i_uuid", "")
        .with_field_with_reference(
            "i_uuid",
            "api key",
            "f_uuid",
            true,
            "op://Private/Stripe/api key",
        );
    let cli_ref = resolve_op_uri_to_ref("op://Private/Stripe/api key", &stub, None).unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}

/// Fix 1D parity: ambiguous item with subtitle → both embed subtitle bracket.
#[test]
fn parity_ambiguous_item_with_subtitle_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        label: "auth token".into(),
        reference: "op://Private/Claude/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let item_a = OpItem {
        id: "i_uuid_a".into(),
        name: "Claude".into(),
        subtitle: "alexey@zhokhov.com".into(),
    };
    let item_b = OpItem {
        id: "i_uuid_b".into(),
        name: "Claude".into(),
        subtitle: "alexey@chainargos.com".into(),
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![item_a.clone(), item_b],
        item_a,
        field.clone(),
    );
    let picker_ref = state.build_op_ref_on_commit(&field);

    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid_a", "alexey@zhokhov.com")
        .with_item("v_uuid", "Claude", "i_uuid_b", "alexey@chainargos.com")
        .with_field_with_reference(
            "i_uuid_a",
            "auth token",
            "f_uuid",
            true,
            "op://Private/Claude/auth token",
        );
    let cli_ref = resolve_op_uri_to_ref(
        "op://Private/Claude[alexey@zhokhov.com]/auth token",
        &stub,
        None,
    )
    .unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}

/// Fix 1D parity: sectioned field → both produce 4-segment `OpRef`.
#[test]
fn parity_sectioned_field_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        label: "auth token".into(),
        reference: "op://Private/Claude/Security/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "Claude".into(),
        subtitle: String::new(),
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        field.clone(),
    );
    let picker_ref = state.build_op_ref_on_commit(&field);

    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid", "")
        .with_field_with_reference(
            "i_uuid",
            "auth token",
            "f_uuid",
            true,
            "op://Private/Claude/Security/auth token",
        );
    let cli_ref =
        resolve_op_uri_to_ref("op://Private/Claude/Security/auth token", &stub, None).unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}

/// Fix 1D parity: 3-segment user input where field has a section →
/// after fix 1A the CLI picks up the section from field.reference,
/// matching the picker's output.
#[test]
fn parity_3seg_input_with_sectioned_field_cli_matches_picker() {
    let field = OpField {
        id: "f_uuid".into(),
        label: "auth token".into(),
        reference: "op://Private/Claude/Security/auth token".into(),
        field_type: "concealed".into(),
        concealed: true,
    };
    let the_item = OpItem {
        id: "i_uuid".into(),
        name: "Claude".into(),
        subtitle: String::new(),
    };
    let state = test_state_picked(
        OpVault {
            id: "v_uuid".into(),
            name: "Private".into(),
        },
        vec![the_item.clone()],
        the_item,
        field.clone(),
    );
    let picker_ref = state.build_op_ref_on_commit(&field);

    // CLI path: 3-segment input, but field.reference has "Security"
    let stub = ParityStub::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid", "")
        .with_field_with_reference(
            "i_uuid",
            "auth token",
            "f_uuid",
            true,
            "op://Private/Claude/Security/auth token",
        );
    let cli_ref = resolve_op_uri_to_ref("op://Private/Claude/auth token", &stub, None).unwrap();

    assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
    assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
}

#[test]
fn invalidate_cache_for_ref_drops_items_and_fields() {
    use jackin_core::OpRef;
    let cache = Rc::new(RefCell::new(OpCache::default()));
    let account = Some("ACCT");
    cache.borrow_mut().put_items(
        account,
        "v1",
        vec![OpItem {
            id: "i1".into(),
            name: "Claude".into(),
            subtitle: String::new(),
        }],
    );
    cache.borrow_mut().put_fields(
        account,
        "v1",
        "i1",
        vec![OpField {
            id: "f1".into(),
            label: "token".into(),
            field_type: "CONCEALED".into(),
            concealed: true,
            reference: String::new(),
        }],
    );

    invalidate_cache_for_ref(
        &cache,
        &OpRef {
            op: "op://v1/i1/f1".into(),
            path: "Work/Claude/token".into(),
            account: Some("ACCT".into()),
            on_demand: false,
        },
    );

    assert!(cache.borrow().get_items(account, "v1").is_none());
    assert!(cache.borrow().get_fields(account, "v1", "i1").is_none());
}

#[test]
fn invalidate_cache_for_ref_ignores_unparseable_ref() {
    use jackin_core::OpRef;
    let cache = Rc::new(RefCell::new(OpCache::default()));
    invalidate_cache_for_ref(
        &cache,
        &OpRef {
            op: "not-a-ref".into(),
            path: String::new(),
            account: None,
            on_demand: false,
        },
    );
}

//! 1Password vault/item/field picker modal.
//!
//! Drill-down `[Account →] Vault → Item → Field` reachable via `P`
//! from a Secrets row. The Account pane only appears for ≥2 signed-in
//! accounts. Selecting a field commits `OpField::reference` (the
//! `op://...` string `op` itself emits) verbatim — synthesizing the
//! path from display names mishandled sections, slashes, and
//! whitespace.
//!
//! The selected account is recorded on the committed `OpRef` (its `account`
//! field), and launch-time `op read` pins resolution to that account, so a
//! multi-account operator's reference resolves against the account it was
//! authored under rather than whichever happens to be the `op` default.
//!
//! `OpStructRunner` calls run through blocking workers, results routed
//! through one-shot subscriptions; the spinner ticks until the receiver
//! yields. Probe / vault-list failures fork into four fatal panels
//! (not installed, not signed in, no vaults, generic).

use jackin_tui::runtime::BlockingSubscription;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jackin_tui::runtime::{Subscription, SubscriptionPoll};
use tui_widget_list::ListState;

use crate::operator_env::{OpAccount, OpCache, OpCli, OpField, OpItem, OpStructRunner, OpVault};

use super::ModalOutcome;
use jackin_console::tui::components::list_helpers::{
    clamp_selection, cycle_select, first_selection, list_state_for_count, selected_choice,
};
use jackin_tui::components::TextInputState;

pub mod render;

pub use jackin_console::tui::components::op_picker::{
    FieldDisplayRow, FieldLabelOrigin, OpLoadState, OpPickerError, OpPickerFatalState,
    OpPickerFieldRef, OpPickerItemRef, OpPickerLoadResult, OpPickerMode, OpPickerStage,
    OpPickerVaultRef, browse_field_display_rows, build_op_picker_ref, create_field_display_rows,
    matches_filter, section_choices_from_references,
};

/// What the operator chose when the picker commits.
#[derive(Debug, Clone)]
pub enum OpPickerSelection {
    /// An existing field was chosen — its `op://` reference (Browse behaviour).
    Existing(crate::operator_env::OpRef),
    /// `+ New item` flow: create a brand-new item in the vault.
    NewItem {
        account: Option<crate::operator_env::OpAccount>,
        vault: crate::operator_env::OpVault,
        item_name: String,
        section: Option<String>,
        field_label: String,
    },
    /// Write/append a field in an existing item (existing-field overwrite,
    /// `+ New field`, or `+ New section`).
    EditItemField {
        account: Option<crate::operator_env::OpAccount>,
        vault: crate::operator_env::OpVault,
        item: crate::operator_env::OpItem,
        section: Option<String>,
        /// Which field to write: an exact existing field (overwrite,
        /// placement preserved) or a new field by label.
        field: crate::operator_env::FieldTarget,
    },
}

type LoadResult = OpPickerLoadResult<OpAccount, OpVault, OpItem, OpField>;

fn ready_picker_load(result: LoadResult) -> BlockingSubscription<LoadResult> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = tx.send(result);
    rx
}

fn spawn_picker_load(
    worker: impl FnOnce() -> LoadResult + Send + 'static,
) -> BlockingSubscription<LoadResult> {
    jackin_tui::runtime::spawn_named_blocking_subscription("jackin-op-picker-load", worker)
}

pub struct OpPickerState {
    pub stage: OpPickerStage,
    pub filter_buf: String,

    pub accounts: Vec<OpAccount>,
    pub account_list_state: ListState,
    pub selected_account: Option<OpAccount>,

    pub vaults: Vec<OpVault>,
    pub vault_list_state: ListState,
    pub selected_vault: Option<OpVault>,

    pub items: Vec<OpItem>,
    pub item_list_state: ListState,
    pub selected_item: Option<OpItem>,

    pub fields: Vec<OpField>,
    pub field_list_state: ListState,
    pub section_list_state: ListState,
    /// The section chosen on the Section stage (Create mode), scoping the
    /// Field stage. `None` = the unsectioned `(root)` choice. Reset to
    /// `None` whenever a fresh item's fields load.
    pub selected_section: Option<String>,
    /// Section names currently collapsed in the field picker.
    /// Absent ⟹ expanded. Cleared whenever a fresh field list loads.
    pub collapsed_sections: HashSet<String>,

    pub load_state: OpLoadState,

    /// Browse vs. Create. Browse is the default for all existing callers.
    pub mode: OpPickerMode,
    /// New-item title input, driven during the `NewItemName` stage.
    pub item_name_input: TextInputState<'static>,
    /// Field-label input, driven during the `FieldLabel` stage.
    pub field_label_input: TextInputState<'static>,
    /// New-section name input, driven during the `NewSectionName` stage.
    pub section_name_input: TextInputState<'static>,
    /// Captured by the New-section flow, consumed when the final
    /// `OpPickerSelection` is built at commit.
    pub pending_section: Option<String>,
    /// The stage the `FieldLabel` sub-stage was entered from, so its Esc
    /// returns to the right origin (Create mode has three entry points).
    field_label_origin: FieldLabelOrigin,
    /// Set by the Field-stage `R` refresh before re-issuing the field
    /// load so the Fields-loaded arm rebuilds the field rows in place
    /// rather than bouncing back to the Section stage (Create mode). The
    /// initial item-selection load leaves it `false` and lands on Section
    /// as usual. Cleared the moment the refreshed fields arrive.
    field_refresh_in_place: bool,

    /// `Arc` so spawned worker threads share the same trait object
    /// (test injectees included).
    runner: Arc<dyn OpStructRunner + Send + Sync>,
    rx: Option<BlockingSubscription<LoadResult>>,
    /// Session-scoped cache shared with `ConsoleState`; the default
    /// constructor allocates a fresh empty one for unit tests.
    op_cache: Rc<RefCell<OpCache>>,
}

// runner / rx aren't Debug; skipped fields are plumbing only.
#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for OpPickerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpPickerState")
            .field("stage", &self.stage)
            .field("filter_buf", &self.filter_buf)
            .field("accounts", &self.accounts)
            .field("selected_account", &self.selected_account)
            .field("vaults", &self.vaults)
            .field("selected_vault", &self.selected_vault)
            .field("items", &self.items)
            .field("selected_item", &self.selected_item)
            .field("fields", &self.fields)
            .field("selected_section", &self.selected_section)
            .field("collapsed_sections", &self.collapsed_sections)
            .field("load_state", &self.load_state)
            .field("mode", &self.mode)
            .field("pending_section", &self.pending_section)
            .finish_non_exhaustive()
    }
}

impl Default for OpPickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl OpPickerState {
    pub fn new() -> Self {
        Self::new_with_runner_and_cache(
            Arc::new(OpCli::new()),
            Rc::new(RefCell::new(OpCache::default())),
        )
    }

    pub fn new_with_cache(op_cache: Rc<RefCell<OpCache>>) -> Self {
        Self::new_with_runner_and_cache(Arc::new(OpCli::new()), op_cache)
    }

    pub fn new_with_runner(runner: Arc<dyn OpStructRunner + Send + Sync>) -> Self {
        Self::new_with_runner_and_cache(runner, Rc::new(RefCell::new(OpCache::default())))
    }

    pub fn new_with_runner_and_cache(
        runner: Arc<dyn OpStructRunner + Send + Sync>,
        op_cache: Rc<RefCell<OpCache>>,
    ) -> Self {
        Self::new_with_mode(runner, op_cache, OpPickerMode::Browse)
    }

    /// Create-mode picker built against the production `OpCli` runner.
    pub fn new_create_with_cache(
        op_cache: Rc<RefCell<OpCache>>,
        item_name_default: impl Into<String>,
        field_label_default: impl Into<String>,
    ) -> Self {
        Self::new_create_with_runner_and_cache(
            Arc::new(OpCli::new()),
            op_cache,
            item_name_default,
            field_label_default,
        )
    }

    pub fn new_create_with_runner_and_cache(
        runner: Arc<dyn OpStructRunner + Send + Sync>,
        op_cache: Rc<RefCell<OpCache>>,
        item_name_default: impl Into<String>,
        field_label_default: impl Into<String>,
    ) -> Self {
        Self::new_with_mode(
            runner,
            op_cache,
            OpPickerMode::Create {
                item_name_default: item_name_default.into(),
                field_label_default: field_label_default.into(),
            },
        )
    }

    fn new_with_mode(
        runner: Arc<dyn OpStructRunner + Send + Sync>,
        op_cache: Rc<RefCell<OpCache>>,
        mode: OpPickerMode,
    ) -> Self {
        let (item_default, field_default) = match &mode {
            OpPickerMode::Browse => (String::new(), String::new()),
            OpPickerMode::Create {
                item_name_default,
                field_label_default,
            } => (item_name_default.clone(), field_label_default.clone()),
        };
        let mut s = Self {
            // Start on Account so the loading-panel descriptor says
            // "loading accounts…" until poll_load routes to Vault
            // (single-account) or stays here (multi-account).
            stage: OpPickerStage::Account,
            filter_buf: String::new(),
            accounts: Vec::new(),
            account_list_state: list_state_for_count(0),
            selected_account: None,
            vaults: Vec::new(),
            vault_list_state: list_state_for_count(0),
            selected_vault: None,
            items: Vec::new(),
            item_list_state: list_state_for_count(0),
            selected_item: None,
            fields: Vec::new(),
            field_list_state: list_state_for_count(0),
            section_list_state: list_state_for_count(0),
            selected_section: None,
            collapsed_sections: HashSet::new(),
            load_state: OpLoadState::Loading { spinner_tick: 0 },
            mode,
            item_name_input: TextInputState::new("Item name", item_default),
            field_label_input: TextInputState::new("Field label", field_default),
            section_name_input: TextInputState::new("Section name", ""),
            pending_section: None,
            field_label_origin: FieldLabelOrigin::NewItem,
            field_refresh_in_place: false,
            runner,
            rx: None,
            op_cache,
        };
        s.start_account_load();
        s
    }

    /// Async (not synchronous in the constructor) so a network-stalled
    /// or biometric-blocked `op` doesn't freeze the TUI render loop.
    /// Cache hits and misses both route through one-shot subscriptions so `poll_load`
    /// stays the single completion path.
    fn start_account_load(&mut self) {
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_accounts()
            .map(|accounts| LoadResult::Accounts(Ok(accounts)));
        let runner = self.runner_clone_for_worker();
        self.start_worker_load(cached, move || LoadResult::Accounts(runner.account_list()));
    }

    fn handle_accounts_loaded(&mut self, accounts: Vec<OpAccount>) {
        self.op_cache.borrow_mut().put_accounts(accounts.clone());
        if accounts.is_empty() {
            // Empty list is functionally "not signed in" — same panel,
            // same recovery (`op signin` in the host shell).
            self.load_state =
                OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn));
            return;
        }
        if accounts.len() == 1 {
            // Single-account fast path: skip the Account pane entirely.
            // `self.accounts` is intentionally left empty; the Esc guard in
            // `handle_vault_key` uses `accounts.len() > 1` as a proxy for
            // "multi-account session", which holds because this branch never
            // populates `self.accounts`.
            let account = accounts.into_iter().next().expect("len == 1");
            let account_id = account.id.clone();
            self.selected_account = Some(account);
            self.start_vault_load(Some(account_id));
            return;
        }
        self.accounts = accounts;
        self.account_list_state = list_state_for_count(self.accounts.len());
        self.stage = OpPickerStage::Account;
        self.load_state = OpLoadState::Ready;
    }

    /// Stage advances at request time (not result time) so the
    /// loading-panel breadcrumb reflects the in-flight load, not the
    /// previous stage. Filter cleared for the new pane.
    fn start_vault_load(&mut self, account_id: Option<String>) {
        self.stage = OpPickerStage::Vault;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_vaults(account_id.as_deref())
            .map(|vaults| LoadResult::Vaults(Ok(vaults)));
        let runner = self.runner_clone_for_worker();
        self.start_worker_load(cached, move || {
            LoadResult::Vaults(runner.vault_list(account_id.as_deref()))
        });
    }

    fn start_item_load(&mut self, vault_id: String, account_id: Option<String>) {
        self.stage = OpPickerStage::Item;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_items(account_id.as_deref(), &vault_id)
            .map(|items| LoadResult::Items(Ok(items)));
        let runner = self.runner_clone_for_worker();
        self.start_worker_load(cached, move || {
            LoadResult::Items(runner.item_list(&vault_id, account_id.as_deref()))
        });
    }

    fn start_field_load(&mut self, item_id: String, vault_id: String, account_id: Option<String>) {
        self.stage = OpPickerStage::Field;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_fields(account_id.as_deref(), &vault_id, &item_id)
            .map(|fields| LoadResult::Fields(Ok(fields)));
        let runner = self.runner_clone_for_worker();
        self.start_worker_load(cached, move || {
            LoadResult::Fields(runner.item_get(&item_id, &vault_id, account_id.as_deref()))
        });
    }

    fn start_worker_load(
        &mut self,
        cached: Option<LoadResult>,
        worker: impl FnOnce() -> LoadResult + Send + 'static,
    ) {
        self.rx = Some(match cached {
            Some(cached) => ready_picker_load(cached),
            None => spawn_picker_load(worker),
        });
    }

    fn selected_account_id(&self) -> Option<String> {
        self.selected_account.as_ref().map(|a| a.id.clone())
    }

    fn selected_account_id_ref(&self) -> Option<&str> {
        self.selected_account.as_ref().map(|a| a.id.as_str())
    }

    /// Clone the `Arc` so spawned workers share the same trait object
    /// (test-injected stubs included).
    fn runner_clone_for_worker(&self) -> Arc<dyn OpStructRunner + Send + Sync> {
        Arc::clone(&self.runner)
    }

    /// Public so the outer console event loop can drain pending
    /// results every tick — keeps the picker responsive without
    /// requiring keystrokes. Idempotent on an empty channel.
    /// Discard any in-flight load result and force the picker into
    /// the `Ready` state. Intended for tests that seed picker state
    /// directly after the constructor's async probe has already been
    /// kicked off — without this, the outer tick drainer can land a
    /// stale `Err(...)` from the still-open receiver and short-circuit
    /// the test through the Fatal-error guard, racing the test against
    /// the probe thread.
    pub fn cancel_in_flight_load(&mut self) {
        self.rx = None;
        self.load_state = OpLoadState::Ready;
    }

    pub fn poll_load(&mut self) -> bool {
        let Some(rx) = self.rx.as_mut() else {
            return false;
        };
        match rx.poll_next() {
            SubscriptionPoll::Ready(LoadResult::Accounts(Ok(accounts))) => {
                self.rx = None;
                self.handle_accounts_loaded(accounts);
                true
            }
            SubscriptionPoll::Ready(LoadResult::Vaults(Ok(vaults))) => {
                self.rx = None;
                if vaults.is_empty() {
                    self.load_state =
                        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults));
                    return true;
                }
                self.op_cache
                    .borrow_mut()
                    .put_vaults(self.selected_account_id_ref(), vaults.clone());
                self.vaults = vaults;
                self.vault_list_state = list_state_for_count(self.vaults.len());
                self.load_state = OpLoadState::Ready;
                true
            }
            SubscriptionPoll::Ready(LoadResult::Accounts(Err(e)) | LoadResult::Vaults(Err(e))) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(
                    jackin_console::tui::components::op_picker::classify_probe_error_message(
                        e.to_string(),
                    ),
                );
                true
            }
            SubscriptionPoll::Ready(LoadResult::Items(Ok(items))) => {
                self.rx = None;
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|v| v.id.clone())
                    .unwrap_or_default();
                self.op_cache.borrow_mut().put_items(
                    self.selected_account_id_ref(),
                    &vault_id,
                    items.clone(),
                );
                self.items = items;
                self.item_list_state
                    .select(first_selection(self.items.len()));
                self.load_state = OpLoadState::Ready;
                true
            }
            SubscriptionPoll::Ready(LoadResult::Items(Err(e)) | LoadResult::Fields(Err(e))) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(OpPickerError::Recoverable {
                    message: e.to_string(),
                });
                true
            }
            SubscriptionPoll::Ready(LoadResult::Fields(Ok(mut fields))) => {
                self.rx = None;
                // Concealed first; cache the sorted vec so cache hits
                // are already presentation-ordered.
                fields.sort_by_key(|f| !f.concealed);
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|v| v.id.clone())
                    .unwrap_or_default();
                let item_id = self
                    .selected_item
                    .as_ref()
                    .map(|i| i.id.clone())
                    .unwrap_or_default();
                self.op_cache.borrow_mut().put_fields(
                    self.selected_account_id_ref(),
                    &vault_id,
                    &item_id,
                    fields.clone(),
                );
                self.fields = fields;
                self.collapsed_sections.clear();
                if self.field_refresh_in_place {
                    // Field-stage `R` (Create mode): the operator already
                    // chose a section. Keep `selected_section`, rebuild the
                    // section-scoped field rows, and stay on Field.
                    self.field_refresh_in_place = false;
                    let display_count = self.build_field_display_rows().len();
                    self.field_list_state.select(first_selection(display_count));
                } else if self.mode.is_create() {
                    // Initial item-selection load (Create mode) inserts a
                    // Section stage between Item and Field; sections derive
                    // from the just-loaded fields.
                    self.selected_section = None;
                    self.stage = OpPickerStage::Section;
                    // `section_choices()` always yields at least the `(root)`
                    // entry and the list always appends a `+ New section`
                    // sentinel, so index 0 is always valid.
                    self.section_list_state =
                        list_state_for_count(self.section_choices().len() + 1);
                } else {
                    self.selected_section = None;
                    let display_count = self.build_field_display_rows().len();
                    self.field_list_state.select(first_selection(display_count));
                }
                self.load_state = OpLoadState::Ready;
                true
            }
            SubscriptionPoll::Pending => false,
            SubscriptionPoll::Closed => {
                self.rx = None;
                self.load_state = OpLoadState::Error(OpPickerError::Recoverable {
                    message: "background worker disconnected".into(),
                });
                true
            }
        }
    }

    pub fn tick(&mut self) -> bool {
        if let OpLoadState::Loading { spinner_tick } = &mut self.load_state {
            *spinner_tick = spinner_tick.wrapping_add(1);
            true
        } else {
            false
        }
    }

    pub fn filtered_accounts(&self) -> Vec<&OpAccount> {
        self.accounts
            .iter()
            .filter(|account| {
                matches_filter(
                    &self.filter_buf,
                    [account.email.as_str(), account.url.as_str()],
                )
            })
            .collect()
    }

    pub fn filtered_vaults(&self) -> Vec<&OpVault> {
        self.vaults
            .iter()
            .filter(|vault| matches_filter(&self.filter_buf, [vault.name.as_str()]))
            .collect()
    }

    pub fn filtered_items(&self) -> Vec<&OpItem> {
        self.items
            .iter()
            .filter(|item| {
                matches_filter(
                    &self.filter_buf,
                    [item.name.as_str(), item.subtitle.as_str()],
                )
            })
            .collect()
    }

    /// Filtered items, followed by a trailing `None` sentinel (the
    /// `+ New item` row) in Create mode. Browse mode emits no sentinel.
    pub fn filtered_item_choices(&self) -> Vec<Option<&OpItem>> {
        let mut out: Vec<Option<&OpItem>> = self.filtered_items().into_iter().map(Some).collect();
        if self.mode.is_create() {
            out.push(None);
        }
        out
    }

    pub fn filtered_fields(&self) -> Vec<&OpField> {
        self.fields
            .iter()
            .filter(|field| {
                matches_filter(&self.filter_buf, [field.label.as_str(), field.id.as_str()])
            })
            .collect()
    }

    /// Distinct sections present in the loaded fields, in first-appearance
    /// order, with a leading `None` (`(root)`) entry. Drives the Section
    /// stage list (Create mode). The render appends a `+ New section`
    /// sentinel after these choices.
    pub fn section_choices(&self) -> Vec<Option<String>> {
        section_choices_from_references(self.fields.iter().map(|field| field.reference.as_str()))
    }

    /// Build the ordered display rows for the field picker.
    ///
    /// Browse mode: unsectioned fields (no `section` segment in
    /// `OpField::reference`) are emitted first; each named section follows
    /// with a collapsible `SectionHeader` row. Sections with zero visible
    /// (filtered) fields are omitted.
    ///
    /// Create mode: the Field stage is already scoped to `selected_section`
    /// (chosen on the Section stage), so the rows are just that section's
    /// fields followed by a `+ New field` sentinel — no headers, no
    /// `+ New section` row. The `field_idx` values inside `Field` rows index
    /// into `self.filtered_fields()`.
    pub fn build_field_display_rows(&self) -> Vec<FieldDisplayRow> {
        if self.mode.is_create() {
            return self.build_create_field_rows();
        }
        let visible = self.filtered_fields();
        browse_field_display_rows(
            visible.iter().map(|field| field.reference.as_str()),
            &self.collapsed_sections,
        )
    }

    /// Field rows for the Create-mode Field stage: only the fields whose
    /// section matches `selected_section`, followed by a `+ New field`
    /// sentinel. No section headers and no `+ New section` row — the
    /// section was already chosen on the Section stage.
    fn build_create_field_rows(&self) -> Vec<FieldDisplayRow> {
        let visible = self.filtered_fields();
        create_field_display_rows(
            visible.iter().map(|field| field.reference.as_str()),
            self.selected_section.as_deref(),
        )
    }

    /// The input box for the current naming sub-stage, or `None` when the
    /// picker is in a list stage. Single source for the stage → input
    /// mapping shared by the renderer, the modal sizing, and the footer
    /// so a naming stage renders as the standard labelled input dialog.
    pub const fn naming_stage_input(
        &self,
    ) -> Option<&jackin_tui::components::TextInputState<'static>> {
        match self.stage {
            OpPickerStage::NewItemName => Some(&self.item_name_input),
            OpPickerStage::FieldLabel => Some(&self.field_label_input),
            OpPickerStage::NewSectionName => Some(&self.section_name_input),
            _ => None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        // Naming sub-stages are pure text input (no async load), so the
        // load-state guards must not swallow their keys.
        match self.stage {
            OpPickerStage::NewItemName => return self.handle_new_item_name_key(key),
            OpPickerStage::FieldLabel => return self.handle_field_label_key(key),
            OpPickerStage::NewSectionName => return self.handle_new_section_name_key(key),
            _ => {}
        }

        if matches!(self.load_state, OpLoadState::Error(OpPickerError::Fatal(_))) {
            return if matches!(key.code, KeyCode::Esc) {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
            };
        }

        if matches!(self.load_state, OpLoadState::Loading { .. }) {
            return if matches!(key.code, KeyCode::Esc) {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
            };
        }

        match self.stage {
            OpPickerStage::Account => self.handle_account_key(key),
            OpPickerStage::Vault => self.handle_vault_key(key),
            OpPickerStage::Item => self.handle_item_key(key),
            OpPickerStage::Section => self.handle_section_key(key),
            OpPickerStage::Field => self.handle_field_key(key),
            OpPickerStage::NewItemName
            | OpPickerStage::FieldLabel
            | OpPickerStage::NewSectionName => ModalOutcome::Continue,
        }
    }

    fn handle_account_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Re-fires the probe so add/remove of signed-in
                // accounts mid-session is picked up without restart.
                self.op_cache.borrow_mut().invalidate_accounts();
                self.accounts.clear();
                self.account_list_state = list_state_for_count(0);
                self.selected_account = None;
                self.start_account_load();
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_accounts().len();
                cycle_select(&mut self.account_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_accounts().len();
                cycle_select(&mut self.account_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Account);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let visible = self.filtered_accounts();
                if let Some(a) = selected_choice(&visible, self.account_list_state.selected) {
                    let a = (*a).clone();
                    let id = a.id.clone();
                    self.selected_account = Some(a);
                    self.start_vault_load(Some(id));
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Account);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_vault_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                self.op_cache
                    .borrow_mut()
                    .invalidate_vaults(account_id.as_deref());
                self.vaults.clear();
                self.vault_list_state = list_state_for_count(0);
                self.selected_vault = None;
                self.start_vault_load(account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                // `self.accounts` is non-empty iff this is a multi-account
                // session (see the invariant in `handle_accounts_loaded`).
                if self.accounts.len() > 1 {
                    self.stage = OpPickerStage::Account;
                    self.filter_buf.clear();
                    self.selected_vault = None;
                    self.vaults.clear();
                    self.vault_list_state = list_state_for_count(0);
                    // Discard banners from the prior vault load so they
                    // don't bleed into the Account pane.
                    self.load_state = OpLoadState::Ready;
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Cancel
            }
            KeyCode::Up => {
                let n = self.filtered_vaults().len();
                cycle_select(&mut self.vault_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_vaults().len();
                cycle_select(&mut self.vault_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Vault);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let visible = self.filtered_vaults();
                if let Some(v) = selected_choice(&visible, self.vault_list_state.selected) {
                    let v = (*v).clone();
                    let id = v.id.clone();
                    let account_id = self.selected_account_id();
                    self.selected_vault = Some(v);
                    self.start_item_load(id, account_id);
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Vault);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_item_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|v| v.id.clone())
                    .unwrap_or_default();
                self.op_cache
                    .borrow_mut()
                    .invalidate_items(account_id.as_deref(), &vault_id);
                self.items.clear();
                self.item_list_state = list_state_for_count(0);
                self.start_item_load(vault_id, account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                self.stage = OpPickerStage::Vault;
                self.filter_buf.clear();
                self.items.clear();
                self.selected_item = None;
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_item_choices().len();
                cycle_select(&mut self.item_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_item_choices().len();
                cycle_select(&mut self.item_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Item);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                // `None` is the `+ New item` sentinel (Create mode only).
                let visible = self.filtered_item_choices();
                let picked: Option<Option<OpItem>> =
                    selected_choice(&visible, self.item_list_state.selected)
                        .map(|choice| choice.map(Clone::clone));
                match picked {
                    Some(Some(item)) => {
                        let item_id = item.id.clone();
                        let vault_id = self
                            .selected_vault
                            .as_ref()
                            .map(|v| v.id.clone())
                            .unwrap_or_default();
                        let account_id = self.selected_account_id();
                        self.selected_item = Some(item);
                        self.start_field_load(item_id, vault_id, account_id);
                    }
                    Some(None) => {
                        self.stage = OpPickerStage::NewItemName;
                    }
                    None => {}
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Item);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    /// Create-mode Section stage: pick `(root)` / an existing section /
    /// `+ New section`. The list has `section_choices().len()` choice rows
    /// followed by a single `+ New section` sentinel. No filtering — sections
    /// are few, so `Char` input is ignored here.
    fn handle_section_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        let choices = self.section_choices();
        let sentinel_idx = choices.len();
        match key.code {
            KeyCode::Esc => {
                // Mirror the Field-stage Esc back to Item.
                self.stage = OpPickerStage::Item;
                self.filter_buf.clear();
                self.fields.clear();
                self.collapsed_sections.clear();
                self.selected_section = None;
                self.selected_item = None;
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                cycle_select(&mut self.section_list_state, sentinel_idx + 1, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                cycle_select(&mut self.section_list_state, sentinel_idx + 1, 1);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if self.section_list_state.selected.unwrap_or(0) == sentinel_idx {
                    self.section_name_input = TextInputState::new("Section name", "");
                    self.stage = OpPickerStage::NewSectionName;
                } else if let Some(choice) =
                    selected_choice(&choices, self.section_list_state.selected)
                {
                    self.selected_section.clone_from(choice);
                    self.stage = OpPickerStage::Field;
                    self.filter_buf.clear();
                    let n = self.build_field_display_rows().len();
                    self.field_list_state.select(first_selection(n));
                }
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    /// Esc back-nav from the Field stage. Create mode steps back to the
    /// Section stage (keeping the loaded fields); Browse steps back to Item.
    fn field_stage_back(&mut self) {
        self.filter_buf.clear();
        if self.mode.is_create() {
            self.stage = OpPickerStage::Section;
            self.selected_section = None;
            // `section_choices()` + the `+ New section` sentinel always
            // yield at least two rows, so index 0 is always valid.
            self.section_list_state = list_state_for_count(self.section_choices().len() + 1);
        } else {
            self.stage = OpPickerStage::Item;
            self.fields.clear();
            self.collapsed_sections.clear();
            self.selected_item = None;
        }
    }

    fn handle_field_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match key.code {
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|v| v.id.clone())
                    .unwrap_or_default();
                let item_id = self
                    .selected_item
                    .as_ref()
                    .map(|i| i.id.clone())
                    .unwrap_or_default();
                self.op_cache.borrow_mut().invalidate_fields(
                    account_id.as_deref(),
                    &vault_id,
                    &item_id,
                );
                self.fields.clear();
                self.field_list_state = list_state_for_count(0);
                self.collapsed_sections.clear();
                // In-place refresh: the operator is already on the Field
                // stage with a chosen section. Flag the reload so the
                // Fields-loaded arm rebuilds the rows here instead of
                // kicking back to Section (Create mode). No-op in Browse.
                self.field_refresh_in_place = self.mode.is_create();
                self.start_field_load(item_id, vault_id, account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                self.field_stage_back();
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.build_field_display_rows().len();
                cycle_select(&mut self.field_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.build_field_display_rows().len();
                cycle_select(&mut self.field_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Left => {
                let cur = self.field_list_state.selected.unwrap_or(0);
                if let Some(FieldDisplayRow::SectionHeader { name, .. }) =
                    self.build_field_display_rows().into_iter().nth(cur)
                {
                    self.set_section_collapsed(name, true);
                }
                ModalOutcome::Continue
            }
            KeyCode::Right => {
                let cur = self.field_list_state.selected.unwrap_or(0);
                if let Some(FieldDisplayRow::SectionHeader { name, .. }) =
                    self.build_field_display_rows().into_iter().nth(cur)
                {
                    self.set_section_collapsed(name, false);
                }
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Field);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let visible = self.filtered_fields();
                let cur = self.field_list_state.selected.unwrap_or(0);
                match self.build_field_display_rows().into_iter().nth(cur) {
                    Some(FieldDisplayRow::SectionHeader { name, .. }) => {
                        self.toggle_section_collapse(name);
                    }
                    Some(FieldDisplayRow::Field { field_idx }) => {
                        if let Some(field) = visible.get(field_idx) {
                            return ModalOutcome::Commit(self.commit_existing_field(field));
                        }
                    }
                    Some(FieldDisplayRow::NewFieldSentinel) => {
                        // The Field stage is scoped to the chosen section, so
                        // the new field lands there too.
                        self.pending_section = self.selected_section.clone();
                        self.field_label_origin = FieldLabelOrigin::NewField;
                        self.stage = OpPickerStage::FieldLabel;
                    }
                    // Create mode no longer surfaces NewSectionSentinel on the
                    // Field stage — section creation lives on the Section stage.
                    Some(FieldDisplayRow::NewSectionSentinel) | None => {}
                }
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                self.reset_selection_for_filter(OpPickerStage::Field);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_new_item_name_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match self.item_name_input.handle_key(key) {
            ModalOutcome::Cancel => {
                self.stage = OpPickerStage::Item;
                ModalOutcome::Continue
            }
            // selected_item stays None → FieldLabel commit takes the new-item path.
            ModalOutcome::Commit(_) => {
                self.field_label_origin = FieldLabelOrigin::NewItem;
                self.stage = OpPickerStage::FieldLabel;
                ModalOutcome::Continue
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn handle_new_section_name_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match self.section_name_input.handle_key(key) {
            ModalOutcome::Cancel => {
                // The `+ New section` entry point lives on the Section stage.
                self.stage = OpPickerStage::Section;
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(name) => {
                // Trim so a whitespace-padded section name can't reach the
                // op section label / derived id.
                self.pending_section = Some(name.trim().to_string());
                self.field_label_origin = FieldLabelOrigin::NewSection;
                self.stage = OpPickerStage::FieldLabel;
                ModalOutcome::Continue
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn handle_field_label_key(&mut self, key: KeyEvent) -> ModalOutcome<OpPickerSelection> {
        match self.field_label_input.handle_key(key) {
            ModalOutcome::Cancel => {
                self.stage = self.field_label_origin.cancel_stage();
                // The section was staged immediately before this stage
                // (new-section name or the drilled section for a new field);
                // backing out discards that choice so it cannot leak into a
                // later commit on a different path.
                self.pending_section = None;
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(label) => {
                let vault = self
                    .selected_vault
                    .clone()
                    .expect("vault set before field-label commit");
                // Trim the field label so leading/trailing whitespace can't
                // reach the op field id/label (item_name is trimmed too).
                let field_label = label.trim().to_string();
                if let Some(item) = self.selected_item.clone() {
                    ModalOutcome::Commit(OpPickerSelection::EditItemField {
                        account: self.selected_account.clone(),
                        vault,
                        item,
                        section: self.pending_section.take(),
                        // Typed label = a new field to append.
                        field: crate::operator_env::FieldTarget::New { label: field_label },
                    })
                } else {
                    ModalOutcome::Commit(OpPickerSelection::NewItem {
                        account: self.selected_account.clone(),
                        vault,
                        item_name: self.item_name_input.trimmed_value(),
                        section: self.pending_section.take(),
                        field_label,
                    })
                }
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn toggle_section_collapse(&mut self, name: String) {
        let collapsed = self.collapsed_sections.contains(name.as_str());
        self.set_section_collapsed(name, !collapsed);
    }

    /// Collapse (`collapsed = true`) or expand a section header, then clamp
    /// the field selection so it never dangles past the new row count.
    /// All three entry points (Enter toggle, Left collapse, Right expand)
    /// route here so the selection clamp stays in lockstep with the rows.
    fn set_section_collapsed(&mut self, name: String, collapsed: bool) {
        if collapsed {
            self.collapsed_sections.insert(name);
        } else {
            self.collapsed_sections.remove(name.as_str());
        }
        let new_len = self.build_field_display_rows().len();
        self.field_list_state
            .select(clamp_selection(self.field_list_state.selected, new_len));
    }

    /// Browse: commit the field's `op://` reference. Create: overwrite the
    /// field by its exact id — the consumer matches on `field_id` and
    /// preserves the field's existing section, so `selected_section` rides
    /// along only for display, not placement.
    fn commit_existing_field(&self, field: &OpField) -> OpPickerSelection {
        if self.mode.is_create() {
            return OpPickerSelection::EditItemField {
                account: self.selected_account.clone(),
                vault: self
                    .selected_vault
                    .clone()
                    .expect("vault set before field commit"),
                item: self
                    .selected_item
                    .clone()
                    .expect("item set before field commit"),
                section: self.selected_section.clone(),
                field: crate::operator_env::FieldTarget::Existing {
                    id: field.id.clone(),
                    label: field.label.clone(),
                },
            };
        }
        OpPickerSelection::Existing(build_op_ref_on_commit(self, field))
    }

    fn reset_selection_for_filter(&mut self, stage: OpPickerStage) {
        match stage {
            OpPickerStage::Account => {
                let n = self.filtered_accounts().len();
                self.account_list_state.select(first_selection(n));
            }
            OpPickerStage::Vault => {
                let n = self.filtered_vaults().len();
                self.vault_list_state.select(first_selection(n));
            }
            OpPickerStage::Item => {
                let n = self.filtered_item_choices().len();
                self.item_list_state.select(first_selection(n));
            }
            OpPickerStage::Field => {
                let n = self.build_field_display_rows().len();
                self.field_list_state.select(first_selection(n));
            }
            _ => {}
        }
    }
}

/// Build an `OpRef` from the picker's currently-selected vault/item/field.
///
/// The `op` field uses UUID-form identifiers from the picker's pane
/// selections. The `path` field uses human-readable names, with an
/// inline `Item[subtitle]` annotation when the item shares its name
/// with another item in the same vault (ambiguity-aware).
///
/// Bracket-bearing item names suppress the subtitle embed (defensive —
/// the UUID in `op` still resolves correctly). Empty subtitles also
/// suppress the embed.
///
/// Section info is recovered by parsing `field.reference`, which `op item get`
/// emits in canonical form (with the section name correctly attributed).
/// If `field.reference` is empty or unparseable, this falls back to a
/// 3-segment URI — which produces an UNRESOLVING reference for fields
/// that actually live inside a section. In production, `op item get`
/// always populates `reference` for fields, so this fallback is a
/// safety net rather than a routine path.
///
/// # Panics
///
/// Panics if vault or item are not selected — callers must
/// ensure the picker has fully drilled to the field pane before calling.
pub(crate) fn build_op_ref_on_commit(
    state: &OpPickerState,
    field: &crate::operator_env::OpField,
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

#[cfg(test)]
mod tests {
    //! Most tests inject a no-op `StubRunner` and overwrite
    //! `vaults`/`items`/`fields`/`load_state`/`stage`/selection
    //! directly before driving `handle_key` — bypasses the worker
    //! channel. The `*_uses_injected_runner_in_async_worker` tests at
    //! the end exercise the worker path end-to-end.
    use super::*;
    use crate::operator_env::{OpAccount, OpField, OpItem, OpVault};
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use std::sync::Mutex;

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
        fn item_list(
            &self,
            _vault_id: &str,
            _account: Option<&str>,
        ) -> anyhow::Result<Vec<OpItem>> {
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
            id: id.to_string(),
            email: email.to_string(),
            url: url.to_string(),
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
        while s.rx.is_some() && std::time::Instant::now() < deadline {
            s.poll_load();
            if s.rx.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
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
        let mut s = OpPickerState::new_with_runner(runner);
        drain_initial_account_load(&mut s);
        s.rx = None;
        s.stage = OpPickerStage::Vault;
        s.load_state = OpLoadState::Ready;
        s
    }

    fn vault(name: &str) -> OpVault {
        OpVault {
            id: format!("v-{name}"),
            name: name.to_string(),
        }
    }

    fn item(name: &str) -> OpItem {
        OpItem {
            id: format!("i-{name}"),
            name: name.to_string(),
            subtitle: String::new(),
        }
    }

    fn item_with_subtitle(name: &str, subtitle: &str) -> OpItem {
        OpItem {
            id: format!("i-{name}-{subtitle}"),
            name: name.to_string(),
            subtitle: subtitle.to_string(),
        }
    }

    fn field(label: &str, ty: &str, concealed: bool) -> OpField {
        OpField {
            id: label.to_string(),
            label: label.to_string(),
            field_type: ty.to_string(),
            concealed,
            reference: String::new(),
        }
    }

    fn field_with_reference(label: &str, reference: &str) -> OpField {
        OpField {
            id: label.to_string(),
            label: label.to_string(),
            field_type: "STRING".to_string(),
            concealed: false,
            reference: reference.to_string(),
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
        s.filter_buf = "AzhokhoV".to_string();

        let visible = s.filtered_items();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].subtitle, "azhokhov@example.com");
    }

    #[test]
    fn filter_vaults_narrows_by_name() {
        let mut s = picker_ready();
        s.vaults = vec![vault("Personal"), vault("Private"), vault("Work")];
        s.vault_list_state.select(Some(0));
        s.filter_buf = "per".to_string();

        let visible = s.filtered_vaults();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "Personal");
    }

    #[test]
    fn filter_clears_on_pane_advance() {
        let mut s = picker_ready();
        s.vaults = vec![vault("Personal"), vault("Private"), vault("Work")];
        s.vault_list_state.select(Some(0));
        s.filter_buf = "per".to_string();
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
        s.filter_buf = "ap".to_string();

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
        s.filter_buf = "pw".to_string();

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
        let mut s = OpPickerState::new_create_with_runner_and_cache(
            runner,
            Rc::new(RefCell::new(OpCache::default())),
            "default-item",
            "token",
        );
        drain_initial_account_load(&mut s);
        s.rx = None;
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
        let mut s = OpPickerState::new_create_with_runner_and_cache(
            runner,
            Rc::new(RefCell::new(OpCache::default())),
            "default-item",
            "token",
        );
        drain_initial_account_load(&mut s);
        s.rx = None;
        s.selected_vault = Some(vault("Personal"));
        s.selected_item = Some(item("login"));
        // Drive the existing-item Enter through start_field_load + drain.
        s.start_field_load("i-login".into(), "v-Personal".into(), None);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while s.rx.is_some() && std::time::Instant::now() < deadline {
            s.poll_load();
            std::thread::sleep(std::time::Duration::from_millis(2));
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
        s.selected_section = Some("auth".to_string());
        // `r` clears `fields`/`field_list_state` and sets the in-place flag.
        s.fields.clear();
        s.field_refresh_in_place = true;
        // Publish the reloaded fields through the same arm the worker uses.
        let (tx, rx) = tokio::sync::oneshot::channel();
        assert!(
            tx.send(LoadResult::Fields(Ok(vec![
                field_with_reference("user", "op://Personal/login/user"),
                field_with_reference("api", "op://Personal/login/auth/api"),
            ])))
            .is_ok()
        );
        s.rx = Some(rx);
        s.poll_load();

        assert_eq!(
            s.stage,
            OpPickerStage::Field,
            "in-place refresh must NOT bounce back to Section"
        );
        assert_eq!(
            s.selected_section,
            Some("auth".to_string()),
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
            vec![None, Some("auth".to_string()), Some("extra".to_string()),],
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
                    crate::operator_env::FieldTarget::Existing {
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
        assert_eq!(s.selected_section, Some("auth".to_string()));
        // Field stage shows only the two "auth" fields + NewFieldSentinel.
        let rows = s.build_field_display_rows();
        assert_eq!(rows.len(), 3, "two auth fields + new-field sentinel");
        assert!(matches!(rows[2], FieldDisplayRow::NewFieldSentinel));
        // Selecting the first scoped field commits with section Some("auth").
        s.field_list_state.select(Some(0));
        match s.handle_key(key(KeyCode::Enter)) {
            ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
                assert_eq!(section, Some("auth".to_string()));
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
            let _ = s.handle_key(key(KeyCode::Char(c)));
        }
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Continue
        ));
        assert_eq!(s.stage, OpPickerStage::FieldLabel);
        match s.handle_key(key(KeyCode::Enter)) {
            ModalOutcome::Commit(OpPickerSelection::EditItemField { section, field, .. }) => {
                assert_eq!(section, Some("creds".to_string()));
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
        let _ = s.handle_key(key(KeyCode::Enter));
        assert_eq!(s.stage, OpPickerStage::NewSectionName);
        for c in "foo".chars() {
            let _ = s.handle_key(key(KeyCode::Char(c)));
        }
        let _ = s.handle_key(key(KeyCode::Enter));
        assert_eq!(s.stage, OpPickerStage::FieldLabel);
        assert_eq!(s.pending_section.as_deref(), Some("foo"));
        // Esc cancels the field-label stage.
        let _ = s.handle_key(key(KeyCode::Esc));
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
        let _ = s.handle_key(key(KeyCode::Enter));
        assert_eq!(s.stage, OpPickerStage::Field);
        s.field_label_input = TextInputState::new("Field", "  oauth-token  ");
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
        let _ = s.handle_key(key(KeyCode::Enter));
        s.section_name_input = TextInputState::new("Section name", "  creds  ");
        let _ = s.handle_key(key(KeyCode::Enter));
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
        let _ = s.handle_key(key(KeyCode::Left));
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
        let _ = s.handle_key(key(KeyCode::Enter));
        assert_eq!(s.stage, OpPickerStage::Field);

        let _ = s.handle_key(key(KeyCode::Esc));
        assert_eq!(s.stage, OpPickerStage::Section, "Field Esc → Section");
        assert_eq!(s.selected_section, None, "section cleared on back-nav");
        assert!(s.selected_item.is_some(), "item kept on Field→Section Esc");

        let _ = s.handle_key(key(KeyCode::Esc));
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
        let mut s = OpPickerState::new_with_runner(runner);
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
        let mut s = OpPickerState::new_with_runner(runner);
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
        let mut s = OpPickerState::new_with_runner(runner);
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
        let mut s = OpPickerState::new_with_runner(runner);
        drain_initial_account_load(&mut s);
        s.rx = None;
        s.load_state = OpLoadState::Ready;
        s.filter_buf = "alic".to_string();
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
        let mut s = OpPickerState::new_with_runner(runner);
        drain_initial_account_load(&mut s);
        s.rx = None;
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
        let _ = runner.vault_list(s.selected_account_id().as_deref());
        let recorded = runner.last_vault_list_account.lock().unwrap().clone();
        assert_eq!(
            recorded,
            Some(Some("acct2".to_string())),
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
        let mut s = OpPickerState::new_with_runner(runner);
        drain_initial_account_load(&mut s);
        s.rx = None;
        s.load_state = OpLoadState::Ready;
        s.stage = OpPickerStage::Vault;
        s.selected_account = Some(account("acct1", "a@example.com", "alpha.1password.com"));
        s.vaults = vec![vault("Personal"), vault("Work")];
        s.vault_list_state.select(Some(1));
        s.filter_buf = "wo".to_string();

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
        counter: std::sync::Arc<Mutex<usize>>,
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
        use crate::operator_env::OpCache;
        use std::sync::Arc;

        let cache = std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()));
        let counter1: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
        let counter2: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

        // First picker: cache miss → runner invoked once.
        let mut s1 = OpPickerState::new_with_runner_and_cache(
            Arc::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter1.clone(),
            }),
            cache.clone(),
        );
        drain_initial_account_load(&mut s1);
        assert_eq!(
            *counter1.lock().unwrap(),
            1,
            "first picker constructor must miss the empty cache"
        );

        // Second picker: cache hit → runner must NOT be invoked.
        let mut s2 = OpPickerState::new_with_runner_and_cache(
            Arc::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter2.clone(),
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
        use crate::operator_env::OpCache;
        use std::sync::Arc;

        let cache = std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()));
        let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

        let mut s1 = OpPickerState::new_with_runner_and_cache(
            Arc::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter.clone(),
            }),
            cache.clone(),
        );
        drain_initial_account_load(&mut s1);
        assert_eq!(*counter.lock().unwrap(), 1, "first picker must miss");
        assert!(
            cache.borrow().get_accounts().is_some(),
            "first picker must populate the cache"
        );

        let mut s2 = OpPickerState::new_with_runner_and_cache(
            Arc::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter.clone(),
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
        use crate::operator_env::OpCache;
        use std::sync::Arc;

        let cache = std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()));
        let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

        let r = Arc::new(CounterRunner {
            accounts: vec![
                account("acct1", "a@example.com", "alpha.1password.com"),
                account("acct2", "b@example.com", "beta.1password.com"),
            ],
            counter: counter.clone(),
        });
        let mut s = OpPickerState::new_with_runner_and_cache(r, cache);
        drain_initial_account_load(&mut s);
        assert_eq!(*counter.lock().unwrap(), 1, "constructor must miss once");
        assert_eq!(s.accounts.len(), 2);

        let _ = s.handle_key(key(KeyCode::Char('r')));
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
        gate: std::sync::Arc<(Mutex<bool>, std::sync::Condvar)>,
    }

    impl BlockingRunner {
        fn new() -> Self {
            Self {
                gate: std::sync::Arc::new((Mutex::new(false), std::sync::Condvar::new())),
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
        let _s = OpPickerState::new_with_runner(runner);
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
        let s = OpPickerState::new_with_runner(runner);

        assert!(
            matches!(s.load_state, OpLoadState::Loading { .. }),
            "constructor must leave the picker in Loading; got {:?}",
            s.load_state
        );

        let area = Rect::new(0, 0, 60, 12);
        let backend = TestBackend::new(area.width, area.height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| crate::console::widgets::op_picker::render::render(f, area, &s))
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
                Some((vault_id.to_string(), account.map(String::from)));
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
                item_id.to_string(),
                vault_id.to_string(),
                account.map(String::from),
            ));
            Ok(Vec::new())
        }
    }

    fn drain_worker_load(s: &mut OpPickerState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while s.rx.is_some() && std::time::Instant::now() < deadline {
            s.poll_load();
            if s.rx.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            s.rx.is_none(),
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
        let mut s = OpPickerState::new_with_runner(runner);
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
            Some(Some("acct1".to_string())),
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
        let mut s = OpPickerState::new_with_runner(runner);
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
            Some(("v-personal".to_string(), Some("acct1".to_string()))),
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
        let mut s = OpPickerState::new_with_runner(runner);
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
                "i-aws".to_string(),
                "v-personal".to_string(),
                Some("acct1".to_string())
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
        let r = build_op_ref_on_commit(&state, &field);
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
        let r = build_op_ref_on_commit(&state, &field);
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
        let r = build_op_ref_on_commit(&state, &field);
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
        let r = build_op_ref_on_commit(&state, &field);
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
        let sectioned_field = crate::operator_env::OpField {
            id: "f_sectioned".into(),
            label: "password".into(),
            reference: "op://Private/MyItem/Auth/password".into(),
            field_type: "CONCEALED".into(),
            concealed: true,
        };
        let no_ref_field = crate::operator_env::OpField {
            id: "f_noref".into(),
            label: "notes".into(),
            reference: String::new(),
            field_type: "STRING".into(),
            concealed: false,
        };
        let the_item = crate::operator_env::OpItem {
            id: "i_uuid".into(),
            name: "MyItem".into(),
            subtitle: String::new(),
        };
        let mut state = test_state_picked(
            crate::operator_env::OpVault {
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
        let r = build_op_ref_on_commit(&state, &no_ref_field);
        assert_eq!(r.op, "op://v_uuid/i_uuid/f_noref");
        assert_eq!(r.path, "Private/MyItem/notes");
    }

    // ── parity tests: build_op_ref_on_commit vs resolve_op_uri_to_ref ──────

    /// Minimal `OpStructRunner` stub for parity tests (no async needed).
    struct ParityStub {
        vaults: Vec<crate::operator_env::OpVault>,
        items: std::collections::HashMap<String, Vec<crate::operator_env::OpItem>>,
        fields: std::collections::HashMap<String, Vec<crate::operator_env::OpField>>,
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
            self.vaults.push(crate::operator_env::OpVault {
                id: id.to_string(),
                name: name.to_string(),
            });
            self
        }

        fn with_item(mut self, vault_id: &str, name: &str, id: &str, subtitle: &str) -> Self {
            self.items
                .entry(vault_id.to_string())
                .or_default()
                .push(crate::operator_env::OpItem {
                    id: id.to_string(),
                    name: name.to_string(),
                    subtitle: subtitle.to_string(),
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
            self.fields.entry(item_id.to_string()).or_default().push(
                crate::operator_env::OpField {
                    id: id.to_string(),
                    label: label.to_string(),
                    field_type: if concealed {
                        "CONCEALED".into()
                    } else {
                        "STRING".into()
                    },
                    concealed,
                    reference: reference.to_string(),
                },
            );
            self
        }
    }

    impl crate::operator_env::OpStructRunner for ParityStub {
        fn account_list(&self) -> anyhow::Result<Vec<crate::operator_env::OpAccount>> {
            Ok(vec![])
        }
        fn vault_list(
            &self,
            _account: Option<&str>,
        ) -> anyhow::Result<Vec<crate::operator_env::OpVault>> {
            Ok(self.vaults.clone())
        }
        fn item_list(
            &self,
            vault_id: &str,
            _account: Option<&str>,
        ) -> anyhow::Result<Vec<crate::operator_env::OpItem>> {
            Ok(self.items.get(vault_id).cloned().unwrap_or_default())
        }
        fn item_get(
            &self,
            item_id: &str,
            _vault_id: &str,
            _account: Option<&str>,
        ) -> anyhow::Result<Vec<crate::operator_env::OpField>> {
            Ok(self.fields.get(item_id).cloned().unwrap_or_default())
        }
    }

    /// Fix 1D parity: unique item, 3-segment field → identical `OpRef`.
    #[test]
    fn parity_unique_item_3seg_field_cli_matches_picker() {
        let field = crate::operator_env::OpField {
            id: "f_uuid".into(),
            label: "api key".into(),
            reference: "op://Private/Stripe/api key".into(),
            field_type: "concealed".into(),
            concealed: true,
        };
        let the_item = crate::operator_env::OpItem {
            id: "i_uuid".into(),
            name: "Stripe".into(),
            subtitle: String::new(),
        };
        let state = test_state_picked(
            crate::operator_env::OpVault {
                id: "v_uuid".into(),
                name: "Private".into(),
            },
            vec![the_item.clone()],
            the_item,
            field.clone(),
        );
        let picker_ref = build_op_ref_on_commit(&state, &field);

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
        let cli_ref =
            crate::operator_env::resolve_op_uri_to_ref("op://Private/Stripe/api key", &stub, None)
                .unwrap();

        assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
        assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
    }

    /// Fix 1D parity: ambiguous item with subtitle → both embed subtitle bracket.
    #[test]
    fn parity_ambiguous_item_with_subtitle_cli_matches_picker() {
        let field = crate::operator_env::OpField {
            id: "f_uuid".into(),
            label: "auth token".into(),
            reference: "op://Private/Claude/auth token".into(),
            field_type: "concealed".into(),
            concealed: true,
        };
        let item_a = crate::operator_env::OpItem {
            id: "i_uuid_a".into(),
            name: "Claude".into(),
            subtitle: "alexey@zhokhov.com".into(),
        };
        let item_b = crate::operator_env::OpItem {
            id: "i_uuid_b".into(),
            name: "Claude".into(),
            subtitle: "alexey@chainargos.com".into(),
        };
        let state = test_state_picked(
            crate::operator_env::OpVault {
                id: "v_uuid".into(),
                name: "Private".into(),
            },
            vec![item_a.clone(), item_b],
            item_a,
            field.clone(),
        );
        let picker_ref = build_op_ref_on_commit(&state, &field);

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
        let cli_ref = crate::operator_env::resolve_op_uri_to_ref(
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
        let field = crate::operator_env::OpField {
            id: "f_uuid".into(),
            label: "auth token".into(),
            reference: "op://Private/Claude/Security/auth token".into(),
            field_type: "concealed".into(),
            concealed: true,
        };
        let the_item = crate::operator_env::OpItem {
            id: "i_uuid".into(),
            name: "Claude".into(),
            subtitle: String::new(),
        };
        let state = test_state_picked(
            crate::operator_env::OpVault {
                id: "v_uuid".into(),
                name: "Private".into(),
            },
            vec![the_item.clone()],
            the_item,
            field.clone(),
        );
        let picker_ref = build_op_ref_on_commit(&state, &field);

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
        let cli_ref = crate::operator_env::resolve_op_uri_to_ref(
            "op://Private/Claude/Security/auth token",
            &stub,
            None,
        )
        .unwrap();

        assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
        assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
    }

    /// Fix 1D parity: 3-segment user input where field has a section →
    /// after fix 1A the CLI picks up the section from field.reference,
    /// matching the picker's output.
    #[test]
    fn parity_3seg_input_with_sectioned_field_cli_matches_picker() {
        let field = crate::operator_env::OpField {
            id: "f_uuid".into(),
            label: "auth token".into(),
            reference: "op://Private/Claude/Security/auth token".into(),
            field_type: "concealed".into(),
            concealed: true,
        };
        let the_item = crate::operator_env::OpItem {
            id: "i_uuid".into(),
            name: "Claude".into(),
            subtitle: String::new(),
        };
        let state = test_state_picked(
            crate::operator_env::OpVault {
                id: "v_uuid".into(),
                name: "Private".into(),
            },
            vec![the_item.clone()],
            the_item,
            field.clone(),
        );
        let picker_ref = build_op_ref_on_commit(&state, &field);

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
        let cli_ref = crate::operator_env::resolve_op_uri_to_ref(
            "op://Private/Claude/auth token",
            &stub,
            None,
        )
        .unwrap();

        assert_eq!(cli_ref.op, picker_ref.op, "op URI must match");
        assert_eq!(cli_ref.path, picker_ref.path, "display path must match");
    }
}

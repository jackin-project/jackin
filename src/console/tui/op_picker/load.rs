//! Construction and background-load completion for the 1Password picker.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
#[cfg(test)]
use std::sync::Arc;

use jackin_console::tui::components::list_helpers::{first_selection, list_state_for_count};
use jackin_tui::components::TextInputState;
use jackin_tui::runtime::{BlockingSubscription, Subscription, SubscriptionPoll};

use super::{
    AccountsLoadedPlan, FieldLabelOrigin, LoadRequest, LoadResult, OpCache, OpLoadState,
    OpPickerAccount, OpPickerError, OpPickerFatalState, OpPickerMode, OpPickerStage, OpPickerState,
    OpPickerPendingLoad,
};
#[cfg(test)]
use crate::operator_env::OpStructRunner;

#[cfg(test)]
type PickerRunner = Arc<dyn OpStructRunner + Send + Sync>;
#[cfg(not(test))]
type PickerRunner = ();

impl OpPickerState {
    pub fn new() -> Self {
        Self::new_with_cache(Rc::new(RefCell::new(OpCache::default())))
    }

    pub fn new_with_cache(op_cache: Rc<RefCell<OpCache>>) -> Self {
        Self::new_with_mode(op_cache, OpPickerMode::Browse)
    }

    #[cfg(test)]
    pub fn new_with_runner(runner: PickerRunner) -> Self {
        Self::new_with_runner_and_cache(runner, Rc::new(RefCell::new(OpCache::default())))
    }

    #[cfg(test)]
    pub fn new_with_runner_and_cache(
        runner: PickerRunner,
        op_cache: Rc<RefCell<OpCache>>,
    ) -> Self {
        Self::new_with_mode_and_runner(op_cache, OpPickerMode::Browse, runner)
    }

    pub fn new_create_with_cache(
        op_cache: Rc<RefCell<OpCache>>,
        item_name_default: impl Into<String>,
        field_label_default: impl Into<String>,
    ) -> Self {
        Self::new_with_mode(
            op_cache,
            OpPickerMode::Create {
                item_name_default: item_name_default.into(),
                field_label_default: field_label_default.into(),
            },
        )
    }

    #[cfg(test)]
    pub fn new_create_with_runner_and_cache(
        runner: PickerRunner,
        op_cache: Rc<RefCell<OpCache>>,
        item_name_default: impl Into<String>,
        field_label_default: impl Into<String>,
    ) -> Self {
        Self::new_with_mode_and_runner(
            op_cache,
            OpPickerMode::Create {
                item_name_default: item_name_default.into(),
                field_label_default: field_label_default.into(),
            },
            runner,
        )
    }

    fn new_with_mode(
        op_cache: Rc<RefCell<OpCache>>,
        mode: OpPickerMode,
    ) -> Self {
        #[cfg(test)]
        let runner = crate::operator_env::default_op_struct_runner();
        #[cfg(not(test))]
        let runner = ();
        Self::build(op_cache, mode, runner)
    }

    #[cfg(test)]
    fn new_with_mode_and_runner(
        op_cache: Rc<RefCell<OpCache>>,
        mode: OpPickerMode,
        runner: PickerRunner,
    ) -> Self {
        Self::build(op_cache, mode, runner)
    }

    fn build(
        op_cache: Rc<RefCell<OpCache>>,
        mode: OpPickerMode,
        #[cfg_attr(not(test), allow(unused_variables))] runner: PickerRunner,
    ) -> Self {
        let (item_default, field_default) = match &mode {
            OpPickerMode::Browse => (String::new(), String::new()),
            OpPickerMode::Create {
                item_name_default,
                field_label_default,
            } => (item_name_default.clone(), field_label_default.clone()),
        };
        let mut state = Self {
            // Start on Account so the loading-panel descriptor says
            // "loading accounts..." until poll_load routes to Vault
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
            #[cfg(test)]
            runner,
            rx: None,
            pending_load: None,
            op_cache,
        };
        state.start_account_load();
        state
    }

    /// Async (not synchronous in the constructor) so a network-stalled
    /// or biometric-blocked `op` doesn't freeze the TUI render loop.
    /// Cache hits and misses both route through one-shot subscriptions so
    /// `poll_load` stays the single completion path.
    pub(super) fn start_account_load(&mut self) {
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let request = LoadRequest::Accounts;
        let cached = self
            .op_cache
            .borrow()
            .get_accounts()
            .map(|accounts| LoadResult::Accounts(Ok(accounts)));
        self.start_worker_load(cached, request);
    }

    fn handle_accounts_loaded(&mut self, accounts: Vec<OpPickerAccount>) {
        self.op_cache.borrow_mut().put_accounts(accounts.clone());
        match jackin_console::tui::components::op_picker::accounts_loaded_plan(accounts.len()) {
            AccountsLoadedPlan::NotSignedIn => {
                // Empty list is functionally "not signed in"; same panel,
                // same recovery (`op signin` in the host shell).
                self.load_state =
                    OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn));
            }
            AccountsLoadedPlan::SelectSingleAccount => {
                // Single-account fast path: skip the Account pane entirely.
                // `self.accounts` stays empty; the Esc guard in
                // `handle_vault_key` uses `accounts.len() > 1` as a proxy for
                // "multi-account session".
                let account = accounts.into_iter().next().expect("len == 1");
                let account_id = account.id.clone();
                self.selected_account = Some(account);
                self.start_vault_load(Some(account_id));
            }
            AccountsLoadedPlan::ShowAccountPane => {
                self.accounts = accounts;
                self.account_list_state = list_state_for_count(self.accounts.len());
                self.stage = OpPickerStage::Account;
                self.load_state = OpLoadState::Ready;
            }
        }
    }

    /// Stage advances at request time (not result time) so the
    /// loading-panel breadcrumb reflects the in-flight load, not the
    /// previous stage. Filter cleared for the new pane.
    pub(super) fn start_vault_load(&mut self, account_id: Option<String>) {
        self.stage = OpPickerStage::Vault;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_vaults(account_id.as_deref())
            .map(|vaults| LoadResult::Vaults(Ok(vaults)));
        let request = LoadRequest::Vaults { account_id };
        self.start_worker_load(cached, request);
    }

    pub(super) fn start_item_load(&mut self, vault_id: String, account_id: Option<String>) {
        self.stage = OpPickerStage::Item;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_items(account_id.as_deref(), &vault_id)
            .map(|items| LoadResult::Items(Ok(items)));
        let request = LoadRequest::Items {
            account_id,
            vault_id,
        };
        self.start_worker_load(cached, request);
    }

    pub(super) fn start_field_load(
        &mut self,
        item_id: String,
        vault_id: String,
        account_id: Option<String>,
    ) {
        self.stage = OpPickerStage::Field;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_fields(account_id.as_deref(), &vault_id, &item_id)
            .map(|fields| LoadResult::Fields(Ok(fields)));
        let request = LoadRequest::Fields {
            account_id,
            vault_id,
            item_id,
        };
        self.start_worker_load(cached, request);
    }

    fn start_worker_load(&mut self, cached: Option<LoadResult>, request: LoadRequest) {
        self.rx = None;
        self.pending_load = Some(OpPickerPendingLoad {
            cached,
            request,
            #[cfg(test)]
            runner: self.runner_clone_for_worker(),
            #[cfg(not(test))]
            runner: (),
        });
    }

    pub(in crate::console) fn take_pending_load(&mut self) -> Option<OpPickerPendingLoad> {
        if self.rx.is_some() {
            return None;
        }
        self.pending_load.take()
    }

    pub(in crate::console) fn attach_load_receiver(
        &mut self,
        rx: BlockingSubscription<LoadResult>,
    ) {
        self.rx = Some(rx);
    }

    pub(super) fn selected_account_id(&self) -> Option<String> {
        self.selected_account.as_ref().map(|account| account.id.clone())
    }

    fn selected_account_id_ref(&self) -> Option<&str> {
        self.selected_account
            .as_ref()
            .map(|account| account.id.as_str())
    }

    #[cfg(test)]
    fn runner_clone_for_worker(&self) -> Arc<dyn OpStructRunner + Send + Sync> {
        Arc::clone(&self.runner)
    }

    /// Public so the outer console event loop can drain pending
    /// results every tick; keeps the picker responsive without keystrokes.
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
            SubscriptionPoll::Ready(LoadResult::Accounts(Err(err)) | LoadResult::Vaults(Err(err))) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(
                    jackin_console::tui::components::op_picker::classify_probe_error_message(
                        err.to_string(),
                    ),
                );
                true
            }
            SubscriptionPoll::Ready(LoadResult::Items(Ok(items))) => {
                self.rx = None;
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|vault| vault.id.clone())
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
            SubscriptionPoll::Ready(LoadResult::Items(Err(err)) | LoadResult::Fields(Err(err))) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(OpPickerError::Recoverable {
                    message: err.to_string(),
                });
                true
            }
            SubscriptionPoll::Ready(LoadResult::Fields(Ok(mut fields))) => {
                self.rx = None;
                // Concealed first; cache the sorted vec so cache hits
                // are already presentation-ordered.
                fields.sort_by_key(|field| !field.concealed);
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|vault| vault.id.clone())
                    .unwrap_or_default();
                let item_id = self
                    .selected_item
                    .as_ref()
                    .map(|item| item.id.clone())
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

    /// Discard any in-flight load result and force the picker into `Ready`.
    pub fn cancel_in_flight_load(&mut self) {
        self.rx = None;
        self.load_state = OpLoadState::Ready;
    }

    pub fn tick(&mut self) -> bool {
        if let OpLoadState::Loading { spinner_tick } = &mut self.load_state {
            *spinner_tick = spinner_tick.wrapping_add(1);
            true
        } else {
            false
        }
    }
}

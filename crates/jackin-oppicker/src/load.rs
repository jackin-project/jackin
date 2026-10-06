//! Construction and background-load completion for the 1Password picker.

use std::cell::RefCell;
use std::rc::Rc;

use crate::LoadSubscription;
use crate::adapters::LoadPoll;

use crate::{
    AccountsLoadedPlan, FieldsLoadedPlan, OpLoadState, OpPickerAccount, OpPickerCache,
    OpPickerError, OpPickerFatalState, OpPickerLoadRequest, OpPickerMode, OpPickerPendingLoad,
    OpPickerStage, VaultsLoadedPlan, accounts_loaded_plan, disconnected_worker_error_state,
    field_label_input_state, fields_loaded_plan, item_name_input_state, items_loaded_plan,
    normalize_field_section_ids, probe_load_error_from_anyhow, recoverable_load_error_state,
    section_name_input_state, sort_fields_by_concealed_first, vaults_loaded_plan,
};

use crate::state::{LoadResult, OpPickerState, collection_state_for_count};

impl Default for OpPickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl OpPickerState {
    pub fn new() -> Self {
        Self::new_with_cache(Rc::new(RefCell::new(OpPickerCache::default())))
    }

    pub fn new_with_cache(op_cache: Rc<RefCell<OpPickerCache>>) -> Self {
        Self::new_with_mode(op_cache, OpPickerMode::Browse)
    }

    pub fn new_create_with_cache(
        op_cache: Rc<RefCell<OpPickerCache>>,
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

    pub(super) fn new_with_mode(op_cache: Rc<RefCell<OpPickerCache>>, mode: OpPickerMode) -> Self {
        let (item_default, field_default) = match &mode {
            OpPickerMode::Browse => (String::new(), String::new()),
            OpPickerMode::Create {
                item_name_default,
                field_label_default,
            } => (item_name_default.clone(), field_label_default.clone()),
        };
        let mut state = Self {
            stage: OpPickerStage::Account,
            filter_buf: String::new(),
            accounts: Vec::new(),
            account_list_state: collection_state_for_count(0),
            selected_account: None,
            vaults: Vec::new(),
            vault_list_state: collection_state_for_count(0),
            selected_vault: None,
            items: Vec::new(),
            item_list_state: collection_state_for_count(0),
            selected_item: None,
            fields: Vec::new(),
            sections: Vec::new(),
            field_list_state: collection_state_for_count(0),
            section_list_state: collection_state_for_count(0),
            selected_section: None,
            collapsed_sections: std::collections::HashSet::new(),
            load_state: OpLoadState::Loading { spinner_tick: 0 },
            mode,
            item_name_input: item_name_input_state(item_default),
            field_label_input: field_label_input_state(field_default),
            section_name_input: section_name_input_state(""),
            pending_section: None,
            field_label_origin: crate::FieldLabelOrigin::NewItem,
            field_refresh_in_place: false,
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
    pub fn start_account_load(&mut self) {
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let request = OpPickerLoadRequest::Accounts;
        let cached = self
            .op_cache
            .borrow()
            .get_accounts()
            .map(|accounts| LoadResult::Accounts(Ok(accounts)));
        self.start_worker_load(cached, request);
    }

    fn handle_accounts_loaded(&mut self, accounts: Vec<OpPickerAccount>) {
        self.op_cache.borrow_mut().put_accounts(accounts.clone());
        match accounts_loaded_plan(accounts.len()) {
            AccountsLoadedPlan::NotSignedIn => {
                self.load_state =
                    OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn));
            }
            AccountsLoadedPlan::SelectSingleAccount => {
                let Some(account) = accounts.into_iter().next() else {
                    self.load_state =
                        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn));
                    return;
                };
                let account_id = account.id.clone();
                self.selected_account = Some(account);
                self.start_vault_load(Some(account_id));
            }
            AccountsLoadedPlan::ShowAccountPane => {
                self.accounts = accounts;
                self.account_list_state = collection_state_for_count(self.accounts.len());
                self.stage = OpPickerStage::Account;
                self.load_state = OpLoadState::Ready;
            }
        }
    }

    /// Stage advances at request time (not result time) so the
    /// loading-panel breadcrumb reflects the in-flight load, not the
    /// previous stage. Filter cleared for the new pane.
    pub fn start_vault_load(&mut self, account_id: Option<String>) {
        self.stage = OpPickerStage::Vault;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_vaults(account_id.as_deref())
            .map(|vaults| LoadResult::Vaults(Ok(vaults)));
        let request = OpPickerLoadRequest::Vaults { account_id };
        self.start_worker_load(cached, request);
    }

    pub fn start_item_load(&mut self, vault_id: String, account_id: Option<String>) {
        self.stage = OpPickerStage::Item;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = self
            .op_cache
            .borrow()
            .get_items(account_id.as_deref(), &vault_id)
            .map(|items| LoadResult::Items(Ok(items)));
        let request = OpPickerLoadRequest::Items {
            account_id,
            vault_id,
        };
        self.start_worker_load(cached, request);
    }

    pub fn start_field_load(
        &mut self,
        item_id: String,
        vault_id: String,
        account_id: Option<String>,
    ) {
        self.stage = OpPickerStage::Field;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let cached = {
            let cache = self.op_cache.borrow();
            cache
                .get_fields(account_id.as_deref(), &vault_id, &item_id)
                .map(|fields| {
                    let sections = cache
                        .get_sections(account_id.as_deref(), &vault_id, &item_id)
                        .unwrap_or_default();
                    LoadResult::Fields(Ok(jackin_core::OpItemDetail { fields, sections }))
                })
        };
        let request = OpPickerLoadRequest::Fields {
            account_id,
            vault_id,
            item_id,
        };
        self.start_worker_load(cached, request);
    }

    fn start_worker_load(&mut self, cached: Option<LoadResult>, request: OpPickerLoadRequest) {
        self.rx = None;
        self.pending_load = Some(OpPickerPendingLoad {
            cached,
            request,
            runner: (),
        });
    }

    #[expect(
        clippy::missing_const_for_fn,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    pub fn take_pending_load(
        &mut self,
    ) -> Option<OpPickerPendingLoad<LoadResult, OpPickerLoadRequest, ()>> {
        if self.rx.is_some() {
            return None;
        }
        self.pending_load.take()
    }

    pub fn attach_load_receiver(&mut self, rx: LoadSubscription<LoadResult>) {
        self.rx = Some(rx);
    }

    fn selected_account_id_ref(&self) -> Option<&str> {
        crate::selected_account_id_ref(self.selected_account.as_ref(), |account| {
            account.id.as_str()
        })
    }

    pub fn selected_vault_id_or_default(&self) -> String {
        crate::selected_entity_id_or_default(self.selected_vault.as_ref(), |vault| {
            vault.id.as_str()
        })
    }

    pub fn selected_item_id_or_default(&self) -> String {
        crate::selected_entity_id_or_default(self.selected_item.as_ref(), |item| item.id.as_str())
    }

    /// Public so the outer console event loop can drain pending
    /// results every tick; keeps the picker responsive without keystrokes.
    pub fn poll_load(&mut self) -> bool {
        let Some(rx) = self.rx.as_mut() else {
            return false;
        };
        match rx.poll_next() {
            LoadPoll::Ready(LoadResult::Accounts(Ok(accounts))) => {
                self.rx = None;
                self.handle_accounts_loaded(accounts);
                true
            }
            LoadPoll::Ready(LoadResult::Vaults(Ok(vaults))) => {
                self.rx = None;
                let plan = vaults_loaded_plan(vaults.len());
                if matches!(plan, VaultsLoadedPlan::NoVaults) {
                    self.load_state =
                        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults));
                    return true;
                }
                self.op_cache
                    .borrow_mut()
                    .put_vaults(self.selected_account_id_ref(), vaults.clone());
                self.vaults = vaults;
                if let VaultsLoadedPlan::ShowVaultPane { selected } = plan {
                    self.vault_list_state.set_active(selected);
                }
                self.load_state = OpLoadState::Ready;
                true
            }
            LoadPoll::Ready(LoadResult::Accounts(Err(err)) | LoadResult::Vaults(Err(err))) => {
                self.rx = None;
                self.load_state = probe_load_error_from_anyhow(&err);
                true
            }
            LoadPoll::Ready(LoadResult::Items(Ok(items))) => {
                self.rx = None;
                let vault_id = self.selected_vault_id_or_default();
                self.op_cache.borrow_mut().put_items(
                    self.selected_account_id_ref(),
                    &vault_id,
                    items.clone(),
                );
                let plan = items_loaded_plan(items.len());
                self.items = items;
                self.item_list_state.set_active(plan.selected);
                self.load_state = OpLoadState::Ready;
                true
            }
            LoadPoll::Ready(LoadResult::Items(Err(err)) | LoadResult::Fields(Err(err))) => {
                self.rx = None;
                self.load_state = recoverable_load_error_state(err.to_string());
                true
            }
            LoadPoll::Ready(LoadResult::Fields(Ok(mut detail))) => {
                self.rx = None;
                if let Err(error) =
                    normalize_field_section_ids(&mut detail.fields, &detail.sections)
                {
                    self.fields.clear();
                    self.sections.clear();
                    self.load_state = recoverable_load_error_state(error);
                    return true;
                }
                sort_fields_by_concealed_first(&mut detail.fields, |field| field.concealed);
                let vault_id = self.selected_vault_id_or_default();
                let item_id = self.selected_item_id_or_default();
                self.op_cache.borrow_mut().put_fields(
                    self.selected_account_id_ref(),
                    &vault_id,
                    &item_id,
                    detail.fields.clone(),
                );
                self.op_cache.borrow_mut().put_sections(
                    self.selected_account_id_ref(),
                    &vault_id,
                    &item_id,
                    detail.sections.clone(),
                );
                self.fields = detail.fields;
                self.sections = detail.sections;
                self.collapsed_sections.clear();
                let section_choice_count = self.section_choices().len();
                let field_display_count = self.build_field_display_rows().len();
                match fields_loaded_plan(
                    &self.mode,
                    self.field_refresh_in_place,
                    section_choice_count,
                    field_display_count,
                ) {
                    FieldsLoadedPlan::RefreshFieldPane {
                        field_selected,
                        clear_refresh_in_place,
                    } => {
                        if clear_refresh_in_place {
                            self.field_refresh_in_place = false;
                        }
                        self.field_list_state.set_active(field_selected);
                    }
                    FieldsLoadedPlan::ShowSectionPane {
                        stage,
                        section_selected,
                        clear_selected_section,
                    } => {
                        if clear_selected_section {
                            self.selected_section = None;
                        }
                        self.stage = stage;
                        self.section_list_state.set_active(section_selected);
                    }
                    FieldsLoadedPlan::ShowFieldPane {
                        field_selected,
                        clear_selected_section,
                    } => {
                        if clear_selected_section {
                            self.selected_section = None;
                        }
                        self.field_list_state.set_active(field_selected);
                    }
                }
                self.load_state = OpLoadState::Ready;
                true
            }
            LoadPoll::Pending => false,
            LoadPoll::Closed => {
                self.rx = None;
                self.load_state = disconnected_worker_error_state();
                true
            }
        }
    }

    /// Discard any in-flight load result and force the picker into `Ready`.
    pub fn cancel_in_flight_load(&mut self) {
        self.rx = None;
        self.load_state = OpLoadState::Ready;
    }

    pub const fn tick(&mut self) -> bool {
        if let OpLoadState::Loading { spinner_tick } = &mut self.load_state {
            *spinner_tick = spinner_tick.wrapping_add(1);
            true
        } else {
            false
        }
    }
}

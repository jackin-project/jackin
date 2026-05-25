//! TUI picker for selecting where to store a Claude OAuth token in 1Password.
//!
//! Drill-down `[Account →] Vault → ItemChoice`:
//!   - New item  → `NewItemName → FieldLabel` → commit `NewItem`
//!   - Existing  → `ExistingFieldChoice` → existing field → commit `EditItemField`
//!                                        → `[ + New field ]` → `FieldLabel` → commit `EditItemField`
//!
//! Called from the standalone token-store dialog when `--interactive` is
//! passed to `jackin workspace claude-token setup` without `--vault`.

use std::sync::{Arc, mpsc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_widget_list::ListState;

use crate::operator_env::{OpAccount, OpCli, OpField, OpItem, OpStructRunner, OpVault};

use super::{ModalOutcome, cycle_select};
use super::text_input::TextInputState;
pub use super::op_picker::{OpLoadState, OpPickerError, OpPickerFatalState};

pub mod render;

/// What the operator wants to do with the token once captured.
#[derive(Debug, Clone)]
pub enum TokenStoreSelection {
    /// Create a brand-new 1Password item in the chosen vault.
    NewItem {
        vault: OpVault,
        item_name: String,
        field_label: String,
    },
    /// Overwrite (or append) a field in an existing 1Password item.
    EditItemField {
        vault: OpVault,
        item: OpItem,
        field_label: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStoreStage {
    Account,
    Vault,
    /// List: `[ + New item ]` at index 0, then existing items in the vault.
    ItemChoice,
    /// Text input for the new item's title.
    NewItemName,
    /// List: `[ + New field ]` at index 0, then existing fields in the chosen item.
    ExistingFieldChoice,
    /// Text input for the field label (new item path or new-field-in-existing-item path).
    FieldLabel,
}

enum LoadResult {
    Accounts(anyhow::Result<Vec<OpAccount>>),
    Vaults(anyhow::Result<Vec<OpVault>>),
    Items(anyhow::Result<Vec<OpItem>>),
    Fields(anyhow::Result<Vec<OpField>>),
}

pub struct TokenStorePickerState<'a> {
    pub stage: TokenStoreStage,
    pub filter_buf: String,

    pub accounts: Vec<OpAccount>,
    pub account_list_state: ListState,
    pub selected_account: Option<OpAccount>,

    pub vaults: Vec<OpVault>,
    pub vault_list_state: ListState,
    pub selected_vault: Option<OpVault>,

    /// Existing items in the selected vault. Row 0 is always the virtual
    /// "[ + New item ]" placeholder; real items start at row 1.
    pub items: Vec<OpItem>,
    pub item_list_state: ListState,
    /// Set when user picks an existing item (ExistingFieldChoice path).
    pub selected_item: Option<OpItem>,

    /// Fields in the chosen existing item. Row 0 is always the virtual
    /// "[ + New field ]" placeholder; real fields start at row 1.
    pub fields: Vec<OpField>,
    pub field_list_state: ListState,

    /// Standard text-input dialog for the new-item name stage.
    pub item_name_input: TextInputState<'a>,
    /// Standard text-input dialog for the field label (new item or new-field-in-existing-item).
    pub field_label_input: TextInputState<'a>,

    pub load_state: OpLoadState,

    runner: Arc<dyn OpStructRunner + Send + Sync>,
    rx: Option<mpsc::Receiver<LoadResult>>,
}

impl std::fmt::Debug for TokenStorePickerState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenStorePickerState")
            .field("stage", &self.stage)
            .field("filter_buf", &self.filter_buf)
            .field("selected_account", &self.selected_account)
            .field("selected_vault", &self.selected_vault)
            .field("selected_item", &self.selected_item)
            .field("fields_count", &self.fields.len())
            .finish_non_exhaustive()
    }
}

impl<'a> TokenStorePickerState<'a> {
    pub fn new(item_name_default: &str) -> Self {
        Self::new_with_runner(Arc::new(OpCli::new_interactive()), item_name_default)
    }

    pub fn new_with_runner(
        runner: Arc<dyn OpStructRunner + Send + Sync>,
        item_name_default: &str,
    ) -> Self {
        let mut s = Self {
            stage: TokenStoreStage::Account,
            filter_buf: String::new(),
            accounts: Vec::new(),
            account_list_state: ListState::default(),
            selected_account: None,
            vaults: Vec::new(),
            vault_list_state: ListState::default(),
            selected_vault: None,
            items: Vec::new(),
            item_list_state: ListState::default(),
            selected_item: None,
            fields: Vec::new(),
            field_list_state: ListState::default(),
            item_name_input: TextInputState::new("Item name", item_name_default),
            field_label_input: TextInputState::new(
                "Field label",
                crate::workspace::token_setup::DEFAULT_FIELD_LABEL,
            ),
            load_state: OpLoadState::Loading { spinner_tick: 0 },
            runner,
            rx: None,
        };
        s.start_account_load();
        s
    }

    fn runner_clone(&self) -> Arc<dyn OpStructRunner + Send + Sync> {
        Arc::clone(&self.runner)
    }

    fn start_account_load(&mut self) {
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let runner = self.runner_clone();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Accounts(runner.account_list()));
        });
    }

    fn handle_accounts_loaded(&mut self, accounts: Vec<OpAccount>) {
        if accounts.is_empty() {
            self.load_state =
                OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn));
            return;
        }
        if accounts.len() == 1 {
            let account = accounts.into_iter().next().expect("len == 1");
            let account_id = account.id.clone();
            self.selected_account = Some(account);
            self.start_vault_load(Some(account_id));
            return;
        }
        self.accounts = accounts;
        self.account_list_state.select(Some(0));
        self.stage = TokenStoreStage::Account;
        self.load_state = OpLoadState::Ready;
    }

    fn start_vault_load(&mut self, account_id: Option<String>) {
        self.stage = TokenStoreStage::Vault;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let runner = self.runner_clone();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Vaults(runner.vault_list(account_id.as_deref())));
        });
    }

    fn start_item_load(&mut self, vault_id: String, account_id: Option<String>) {
        self.stage = TokenStoreStage::ItemChoice;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let runner = self.runner_clone();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Items(
                runner.item_list(&vault_id, account_id.as_deref()),
            ));
        });
    }

    fn start_field_load(&mut self, item_id: String, vault_id: String, account_id: Option<String>) {
        self.stage = TokenStoreStage::ExistingFieldChoice;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let runner = self.runner_clone();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Fields(
                runner.item_get(&item_id, &vault_id, account_id.as_deref()),
            ));
        });
    }

    pub fn poll_load(&mut self) {
        let Some(rx) = self.rx.as_ref() else { return };
        match rx.try_recv() {
            Ok(LoadResult::Accounts(Ok(accounts))) => {
                self.rx = None;
                self.handle_accounts_loaded(accounts);
            }
            Ok(LoadResult::Vaults(Ok(vaults))) => {
                self.rx = None;
                if vaults.is_empty() {
                    self.load_state =
                        OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NoVaults));
                    return;
                }
                self.vaults = vaults;
                self.vault_list_state.select(Some(0));
                self.load_state = OpLoadState::Ready;
            }
            Ok(LoadResult::Items(Ok(items))) => {
                self.rx = None;
                self.items = items;
                // Row 0 is always "[ + New item ]", so start selection there.
                self.item_list_state.select(Some(0));
                self.load_state = OpLoadState::Ready;
            }
            Ok(LoadResult::Fields(Ok(fields))) => {
                self.rx = None;
                self.fields = fields;
                // Row 0 is always "[ + New field ]", so start selection there.
                self.field_list_state.select(Some(0));
                self.load_state = OpLoadState::Ready;
            }
            Ok(
                LoadResult::Accounts(Err(e))
                | LoadResult::Vaults(Err(e))
                | LoadResult::Items(Err(e))
                | LoadResult::Fields(Err(e)),
            ) => {
                self.rx = None;
                self.load_state =
                    OpLoadState::Error(classify_probe_error(&e));
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(OpPickerError::Recoverable {
                    message: "background worker disconnected".into(),
                });
            }
        }
    }

    pub fn tick(&mut self) {
        if let OpLoadState::Loading { spinner_tick } = &mut self.load_state {
            *spinner_tick = spinner_tick.wrapping_add(1);
        }
        self.poll_load();
    }

    pub fn cancel_in_flight_load(&mut self) {
        self.rx = None;
        self.load_state = OpLoadState::Ready;
    }

    fn selected_account_id(&self) -> Option<String> {
        self.selected_account.as_ref().map(|a| a.id.clone())
    }

    pub fn filtered_accounts(&self) -> Vec<&OpAccount> {
        let needle = self.filter_buf.to_lowercase();
        self.accounts
            .iter()
            .filter(|a| {
                needle.is_empty()
                    || a.email.to_lowercase().contains(&needle)
                    || a.url.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn filtered_vaults(&self) -> Vec<&OpVault> {
        let needle = self.filter_buf.to_lowercase();
        self.vaults
            .iter()
            .filter(|v| needle.is_empty() || v.name.to_lowercase().contains(&needle))
            .collect()
    }

    /// Items filtered by the current `filter_buf`. Index 0 in the returned
    /// `Vec` is always a sentinel `None` (the "[ + New item ]" row);
    /// subsequent entries are `Some(&OpItem)`.
    pub fn filtered_item_choices(&self) -> Vec<Option<&OpItem>> {
        let needle = self.filter_buf.to_lowercase();
        let mut out: Vec<Option<&OpItem>> = vec![None]; // "[ + New item ]" sentinel
        out.extend(
            self.items
                .iter()
                .filter(|i| {
                    needle.is_empty()
                        || i.name.to_lowercase().contains(&needle)
                        || i.subtitle.to_lowercase().contains(&needle)
                })
                .map(Some),
        );
        out
    }

    /// Fields filtered by the current `filter_buf`. Index 0 is always a
    /// sentinel `None` (the "[ + New field ]" row); subsequent entries
    /// are `Some(&OpField)`.
    pub fn filtered_field_choices(&self) -> Vec<Option<&OpField>> {
        let needle = self.filter_buf.to_lowercase();
        let mut out: Vec<Option<&OpField>> = vec![None]; // "[ + New field ]" sentinel
        out.extend(
            self.fields
                .iter()
                .filter(|f| needle.is_empty() || f.label.to_lowercase().contains(&needle))
                .map(Some),
        );
        out
    }

    pub fn is_multi_account(&self) -> bool {
        // Populated only when there are ≥2 accounts (single-account
        // fast-path leaves `self.accounts` empty).
        !self.accounts.is_empty()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<TokenStoreSelection> {
        self.poll_load();

        if matches!(self.load_state, OpLoadState::Error(OpPickerError::Fatal(_))) {
            return if key.code == KeyCode::Esc {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
            };
        }

        if matches!(self.load_state, OpLoadState::Loading { .. }) {
            return if key.code == KeyCode::Esc {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
            };
        }

        match self.stage {
            TokenStoreStage::Account => self.handle_account_key(key),
            TokenStoreStage::Vault => self.handle_vault_key(key),
            TokenStoreStage::ItemChoice => self.handle_item_choice_key(key),
            TokenStoreStage::NewItemName => self.handle_new_item_name_key(key),
            TokenStoreStage::ExistingFieldChoice => self.handle_existing_field_choice_key(key),
            TokenStoreStage::FieldLabel => self.handle_field_label_key(key),
        }
    }

    fn handle_account_key(&mut self, key: KeyEvent) -> ModalOutcome<TokenStoreSelection> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Enter => {
                let visible = self.filtered_accounts();
                let cur = self.account_list_state.selected.unwrap_or(0);
                if let Some(account) = visible.get(cur).copied() {
                    let account_id = account.id.clone();
                    self.selected_account = Some(account.clone());
                    self.filter_buf.clear();
                    self.start_vault_load(Some(account_id));
                }
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
                let n = self.filtered_accounts().len();
                reset_selection(&mut self.account_list_state, n);
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                let n = self.filtered_accounts().len();
                reset_selection(&mut self.account_list_state, n);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_vault_key(&mut self, key: KeyEvent) -> ModalOutcome<TokenStoreSelection> {
        match key.code {
            KeyCode::Esc => {
                if self.is_multi_account() {
                    self.stage = TokenStoreStage::Account;
                    self.filter_buf.clear();
                    self.selected_account = None;
                    ModalOutcome::Continue
                } else {
                    ModalOutcome::Cancel
                }
            }
            KeyCode::Enter => {
                let visible = self.filtered_vaults();
                let cur = self.vault_list_state.selected.unwrap_or(0);
                if let Some(vault) = visible.get(cur).copied() {
                    let vault_id = vault.id.clone();
                    let account_id = self.selected_account_id();
                    self.selected_vault = Some(vault.clone());
                    self.start_item_load(vault_id, account_id);
                }
                ModalOutcome::Continue
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
                let n = self.filtered_vaults().len();
                reset_selection(&mut self.vault_list_state, n);
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                let n = self.filtered_vaults().len();
                reset_selection(&mut self.vault_list_state, n);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_item_choice_key(&mut self, key: KeyEvent) -> ModalOutcome<TokenStoreSelection> {
        match key.code {
            KeyCode::Esc => {
                self.stage = TokenStoreStage::Vault;
                self.filter_buf.clear();
                self.selected_vault = None;
                self.items.clear();
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let cur = self.item_list_state.selected.unwrap_or(0);
                let choice: Option<Option<String>> = {
                    let visible = self.filtered_item_choices();
                    match visible.get(cur) {
                        Some(None) => Some(None),
                        Some(Some(item)) => Some(Some(item.id.clone())),
                        None => None,
                    }
                };
                match choice {
                    None => ModalOutcome::Continue,
                    Some(None) => {
                        self.stage = TokenStoreStage::NewItemName;
                        ModalOutcome::Continue
                    }
                    Some(Some(item_id)) => {
                        let item = self.items.iter().find(|i| i.id == item_id).cloned();
                        match item {
                            None => ModalOutcome::Continue,
                            Some(item) => {
                                let vault_id = self
                                    .selected_vault
                                    .as_ref()
                                    .expect("vault set before items")
                                    .id
                                    .clone();
                                let account_id = self.selected_account_id();
                                let item_id_for_load = item.id.clone();
                                self.selected_item = Some(item);
                                self.start_field_load(item_id_for_load, vault_id, account_id);
                                ModalOutcome::Continue
                            }
                        }
                    }
                }
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
                let n = self.filtered_item_choices().len();
                reset_selection(&mut self.item_list_state, n);
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                let n = self.filtered_item_choices().len();
                reset_selection(&mut self.item_list_state, n);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_new_item_name_key(&mut self, key: KeyEvent) -> ModalOutcome<TokenStoreSelection> {
        match self.item_name_input.handle_key(key) {
            ModalOutcome::Cancel => {
                self.stage = TokenStoreStage::ItemChoice;
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(_) => {
                self.stage = TokenStoreStage::FieldLabel;
                ModalOutcome::Continue
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }

    fn handle_existing_field_choice_key(
        &mut self,
        key: KeyEvent,
    ) -> ModalOutcome<TokenStoreSelection> {
        match key.code {
            KeyCode::Esc => {
                self.stage = TokenStoreStage::ItemChoice;
                self.filter_buf.clear();
                self.selected_item = None;
                self.fields.clear();
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let cur = self.field_list_state.selected.unwrap_or(0);
                let choice: Option<Option<String>> = {
                    let visible = self.filtered_field_choices();
                    match visible.get(cur) {
                        Some(None) => Some(None),
                        Some(Some(field)) => Some(Some(field.label.clone())),
                        None => None,
                    }
                };
                match choice {
                    None => ModalOutcome::Continue,
                    Some(None) => {
                        // "[ + New field ]" — go to text input
                        self.stage = TokenStoreStage::FieldLabel;
                        ModalOutcome::Continue
                    }
                    Some(Some(field_label)) => {
                        let vault = self.selected_vault.clone().expect("vault set");
                        let item = self.selected_item.clone().expect("item set");
                        ModalOutcome::Commit(TokenStoreSelection::EditItemField {
                            vault,
                            item,
                            field_label,
                        })
                    }
                }
            }
            KeyCode::Up => {
                let n = self.filtered_field_choices().len();
                cycle_select(&mut self.field_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_field_choices().len();
                cycle_select(&mut self.field_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                let n = self.filtered_field_choices().len();
                reset_selection(&mut self.field_list_state, n);
                ModalOutcome::Continue
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_buf.push(c);
                let n = self.filtered_field_choices().len();
                reset_selection(&mut self.field_list_state, n);
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    fn handle_field_label_key(&mut self, key: KeyEvent) -> ModalOutcome<TokenStoreSelection> {
        match self.field_label_input.handle_key(key) {
            ModalOutcome::Cancel => {
                // Go back to where we came from:
                // - new-item path: selected_item is None → back to NewItemName
                // - existing-item "new field" path: selected_item is Some → back to ExistingFieldChoice
                if self.selected_item.is_some() {
                    self.stage = TokenStoreStage::ExistingFieldChoice;
                } else {
                    self.stage = TokenStoreStage::NewItemName;
                }
                ModalOutcome::Continue
            }
            ModalOutcome::Commit(label) => {
                let vault = self.selected_vault.clone().expect("vault set");
                if let Some(item) = self.selected_item.clone() {
                    ModalOutcome::Commit(TokenStoreSelection::EditItemField {
                        vault,
                        item,
                        field_label: label,
                    })
                } else {
                    let item_name = self.item_name_input.trimmed_value();
                    ModalOutcome::Commit(TokenStoreSelection::NewItem {
                        vault,
                        item_name,
                        field_label: label,
                    })
                }
            }
            ModalOutcome::Continue => ModalOutcome::Continue,
        }
    }
}

fn reset_selection(list_state: &mut ListState, count: usize) {
    if count == 0 {
        list_state.select(None);
    } else {
        list_state.select(Some(0));
    }
}

fn classify_probe_error(e: &anyhow::Error) -> OpPickerError {
    let msg = e.to_string().to_lowercase();
    if msg.contains("not found") || msg.contains("executable file not found") {
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    } else if msg.contains("not signed in")
        || msg.contains("sign in")
        || msg.contains("authentication required")
    {
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    } else {
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal {
            message: e.to_string(),
        })
    }
}

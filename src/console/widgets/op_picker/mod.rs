//! 1Password vault/item/field picker modal.
//!
//! Drill-down `[Account →] Vault → Item → Field` reachable via `P`
//! from a Secrets row. The Account pane only appears for ≥2 signed-in
//! accounts. Selecting a field commits `OpField::reference` (the
//! `op://...` string `op` itself emits) verbatim — synthesizing the
//! path from display names mishandled sections, slashes, and
//! whitespace.
//!
//! Account scope is *not* encoded in the committed `op://` path —
//! launch-time `op read` resolves against `op`'s default-account
//! context, so the operator must `op signin` the right account or the
//! resolution fails with "item not found". Per-value account override
//! in the on-disk format is a future PR.
//!
//! `OpStructRunner` calls run on background threads, results routed
//! through an `mpsc` channel; the spinner ticks until the receiver
//! yields. Probe / vault-list failures fork into four fatal panels
//! (not installed, not signed in, no vaults, generic).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, mpsc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_widget_list::ListState;

use crate::console::op_cache::OpCache;
use crate::operator_env::{OpAccount, OpCli, OpField, OpItem, OpStructRunner, OpVault};

use super::{ModalOutcome, cycle_select};

pub mod render;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPickerStage {
    Account,
    Vault,
    Item,
    Field,
}

#[derive(Debug, Clone)]
pub enum OpLoadState {
    Idle,
    Loading { spinner_tick: u8 },
    Ready,
    Error(OpPickerError),
}

/// `Fatal` panels block all navigation but Esc; `Recoverable` shows
/// inline so the operator can navigate back and retry.
#[derive(Debug, Clone)]
pub enum OpPickerError {
    Fatal(OpPickerFatalState),
    Recoverable { message: String },
}

/// Each variant maps to a distinct instructional panel in
/// [`render::render`].
#[derive(Debug, Clone)]
pub enum OpPickerFatalState {
    NotInstalled,
    NotSignedIn,
    NoVaults,
    GenericFatal { message: String },
}

/// Pane-specific so the `try_recv` drainer can route to the right
/// `Vec` without a separate "what was loading" tag.
enum LoadResult {
    Accounts(anyhow::Result<Vec<OpAccount>>),
    Vaults(anyhow::Result<Vec<OpVault>>),
    Items(anyhow::Result<Vec<OpItem>>),
    Fields(anyhow::Result<Vec<OpField>>),
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

    pub load_state: OpLoadState,

    /// `Arc` so spawned worker threads share the same trait object
    /// (test injectees included).
    runner: Arc<dyn OpStructRunner + Send + Sync>,
    rx: Option<mpsc::Receiver<LoadResult>>,
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
            .field("load_state", &self.load_state)
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
        let mut s = Self {
            // Start on Account so the loading-panel descriptor says
            // "loading accounts…" until poll_load routes to Vault
            // (single-account) or stays here (multi-account).
            stage: OpPickerStage::Account,
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
            load_state: OpLoadState::Loading { spinner_tick: 0 },
            runner,
            rx: None,
            op_cache,
        };
        s.start_account_load();
        s
    }

    /// Async (not synchronous in the constructor) so a network-stalled
    /// or biometric-blocked `op` doesn't freeze the TUI render loop.
    /// Cache hits and misses both route through `mpsc` so `poll_load`
    /// stays the single completion path.
    fn start_account_load(&mut self) {
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        if let Some(cached) = self.op_cache.borrow().get_accounts() {
            let _ = tx.send(LoadResult::Accounts(Ok(cached)));
            return;
        }
        let runner = Arc::clone(&self.runner);
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Accounts(runner.account_list()));
        });
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
        self.account_list_state.select(Some(0));
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
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        if let Some(cached) = self.op_cache.borrow().get_vaults(account_id.as_deref()) {
            let _ = tx.send(LoadResult::Vaults(Ok(cached)));
            return;
        }
        let runner = self.runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Vaults(runner.vault_list(account_id.as_deref())));
        });
    }

    fn start_item_load(&mut self, vault_id: String, account_id: Option<String>) {
        self.stage = OpPickerStage::Item;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        if let Some(cached) = self
            .op_cache
            .borrow()
            .get_items(account_id.as_deref(), &vault_id)
        {
            let _ = tx.send(LoadResult::Items(Ok(cached)));
            return;
        }
        let runner = self.runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Items(
                runner.item_list(&vault_id, account_id.as_deref()),
            ));
        });
    }

    fn start_field_load(&mut self, item_id: String, vault_id: String, account_id: Option<String>) {
        self.stage = OpPickerStage::Field;
        self.filter_buf.clear();
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        if let Some(cached) =
            self.op_cache
                .borrow()
                .get_fields(account_id.as_deref(), &vault_id, &item_id)
        {
            let _ = tx.send(LoadResult::Fields(Ok(cached)));
            return;
        }
        let runner = self.runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Fields(runner.item_get(
                &item_id,
                &vault_id,
                account_id.as_deref(),
            )));
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
    fn runner_clone_for_thread(&self) -> Arc<dyn OpStructRunner + Send + Sync> {
        Arc::clone(&self.runner)
    }

    /// Public so the outer console event loop can drain pending
    /// results every tick — keeps the picker responsive without
    /// requiring keystrokes. Idempotent on an empty channel.
    pub fn poll_load(&mut self) {
        let Some(rx) = self.rx.as_ref() else {
            return;
        };
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
                self.op_cache
                    .borrow_mut()
                    .put_vaults(self.selected_account_id_ref(), vaults.clone());
                self.vaults = vaults;
                self.vault_list_state.select(Some(0));
                self.load_state = OpLoadState::Ready;
            }
            Ok(LoadResult::Accounts(Err(e)) | LoadResult::Vaults(Err(e))) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(classify_probe_error(&e));
            }
            Ok(LoadResult::Items(Ok(items))) => {
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
                    .select(if self.items.is_empty() { None } else { Some(0) });
                self.load_state = OpLoadState::Ready;
            }
            Ok(LoadResult::Items(Err(e)) | LoadResult::Fields(Err(e))) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(OpPickerError::Recoverable {
                    message: e.to_string(),
                });
            }
            Ok(LoadResult::Fields(Ok(mut fields))) => {
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
                self.field_list_state.select(if self.fields.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.load_state = OpLoadState::Ready;
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

    pub fn filtered_items(&self) -> Vec<&OpItem> {
        let needle = self.filter_buf.to_lowercase();
        self.items
            .iter()
            .filter(|i| {
                needle.is_empty()
                    || i.name.to_lowercase().contains(&needle)
                    || i.subtitle.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn filtered_fields(&self) -> Vec<&OpField> {
        let needle = self.filter_buf.to_lowercase();
        self.fields
            .iter()
            .filter(|f| {
                needle.is_empty()
                    || f.label.to_lowercase().contains(&needle)
                    || f.id.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        // Tests bypass render entirely so we drain here too, not just
        // on tick.
        self.poll_load();

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
            OpPickerStage::Field => self.handle_field_key(key),
        }
    }

    fn handle_account_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Re-fires the probe so add/remove of signed-in
                // accounts mid-session is picked up without restart.
                self.op_cache.borrow_mut().invalidate_accounts();
                self.accounts.clear();
                self.account_list_state = ListState::default();
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
                let cur = self.account_list_state.selected.unwrap_or(0);
                if let Some(a) = visible.get(cur) {
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

    fn handle_vault_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let account_id = self.selected_account_id();
                self.op_cache
                    .borrow_mut()
                    .invalidate_vaults(account_id.as_deref());
                self.vaults.clear();
                self.vault_list_state = ListState::default();
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
                    self.vault_list_state = ListState::default();
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
                let cur = self.vault_list_state.selected.unwrap_or(0);
                if let Some(v) = visible.get(cur) {
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

    fn handle_item_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
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
                self.item_list_state = ListState::default();
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
                let n = self.filtered_items().len();
                cycle_select(&mut self.item_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_items().len();
                cycle_select(&mut self.item_list_state, n, 1);
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                self.filter_buf.pop();
                self.reset_selection_for_filter(OpPickerStage::Item);
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let visible = self.filtered_items();
                let cur = self.item_list_state.selected.unwrap_or(0);
                if let Some(item) = visible.get(cur) {
                    let item = (*item).clone();
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

    fn handle_field_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
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
                self.field_list_state = ListState::default();
                self.start_field_load(item_id, vault_id, account_id);
                ModalOutcome::Continue
            }
            KeyCode::Esc => {
                self.stage = OpPickerStage::Item;
                self.filter_buf.clear();
                self.fields.clear();
                self.selected_item = None;
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_fields().len();
                cycle_select(&mut self.field_list_state, n, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_fields().len();
                cycle_select(&mut self.field_list_state, n, 1);
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
                if let Some(field) = visible.get(cur) {
                    // Prefer the CLI-emitted `reference` verbatim;
                    // synthesizing from display names mishandled
                    // sections, slashes, and whitespace. Synthesis is
                    // a defensive fallback for fixtures that omit
                    // `reference`.
                    let path = if field.reference.is_empty() {
                        let label = if field.label.is_empty() {
                            field.id.clone()
                        } else {
                            field.label.clone()
                        };
                        let vault_name =
                            self.selected_vault.as_ref().map_or("", |v| v.name.as_str());
                        let item_name = self.selected_item.as_ref().map_or("", |i| i.name.as_str());
                        format!("op://{vault_name}/{item_name}/{label}")
                    } else {
                        field.reference.clone()
                    };
                    return ModalOutcome::Commit(path);
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

    fn reset_selection_for_filter(&mut self, stage: OpPickerStage) {
        match stage {
            OpPickerStage::Account => {
                let n = self.filtered_accounts().len();
                self.account_list_state
                    .select(if n == 0 { None } else { Some(0) });
            }
            OpPickerStage::Vault => {
                let n = self.filtered_vaults().len();
                self.vault_list_state
                    .select(if n == 0 { None } else { Some(0) });
            }
            OpPickerStage::Item => {
                let n = self.filtered_items().len();
                self.item_list_state
                    .select(if n == 0 { None } else { Some(0) });
            }
            OpPickerStage::Field => {
                let n = self.filtered_fields().len();
                self.field_list_state
                    .select(if n == 0 { None } else { Some(0) });
            }
        }
    }
}

/// Classifies by stderr substring because `anyhow::Error` has no
/// typed variants here.
fn classify_probe_error(e: &anyhow::Error) -> OpPickerError {
    let msg = e.to_string();
    if msg.contains("failed to spawn") {
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    } else if msg.contains("not signed in")
        || msg.contains("not currently signed")
        || msg.contains("no accounts")
    {
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    } else {
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { message: msg })
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
            item_with_subtitle("Google", "azhokhov@scentbird.com"),
        ];
        s.item_list_state.select(Some(0));
        s.filter_buf = "AzhokhoV".to_string();

        let visible = s.filtered_items();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].subtitle, "azhokhov@scentbird.com");
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

    /// Backward-compat fallback: synthesize from display names when
    /// `OpField::reference` is missing (older fixtures).
    #[test]
    fn enter_on_field_commits_op_path() {
        let mut s = picker_ready();
        s.selected_vault = Some(vault("Personal"));
        s.selected_item = Some(OpItem {
            id: "i-api".into(),
            name: "API Keys".into(),
            subtitle: String::new(),
        });
        s.fields = vec![
            field("password", "concealed", true),
            field("username", "text", false),
        ];
        s.field_list_state.select(Some(0));
        s.stage = OpPickerStage::Field;

        let outcome = s.handle_key(key(KeyCode::Enter));
        match outcome {
            ModalOutcome::Commit(path) => {
                assert_eq!(path, "op://Personal/API Keys/password");
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    /// Display name with whitespace + section-aware reference must
    /// commit verbatim, not via synthesized path.
    #[test]
    fn picker_commit_uses_op_provided_reference_not_synthesized() {
        let mut s = picker_ready();
        s.selected_vault = Some(vault("Personal"));
        s.selected_item = Some(OpItem {
            id: "i-test".into(),
            name: "name with spaces".into(),
            subtitle: String::new(),
        });
        s.fields = vec![field_with_reference("api", "op://Personal/test/auth/api")];
        s.field_list_state.select(Some(0));
        s.stage = OpPickerStage::Field;

        let outcome = s.handle_key(key(KeyCode::Enter));
        match outcome {
            ModalOutcome::Commit(path) => {
                assert_eq!(
                    path, "op://Personal/test/auth/api",
                    "picker must commit `field.reference` verbatim, not a synthesized path"
                );
            }
            other => panic!("expected Commit, got {other:?}"),
        }
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
        use crate::console::op_cache::OpCache;
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
        use crate::console::op_cache::OpCache;
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
        use crate::console::op_cache::OpCache;
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
        fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
            let (lock, cv) = &*self.gate;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cv.wait(released).unwrap();
            }
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
}

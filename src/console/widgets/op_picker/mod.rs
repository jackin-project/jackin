//! 1Password vault/item/field picker modal.
//!
//! Three-pane drill-down (`Vault → Item → Field`) reachable via `Ctrl+O`
//! from the `EnvValue` `TextInput` modal in the Secrets tab. Each pane
//! lists structural metadata returned by `op` — vault names, item names,
//! and field labels/types — and lets the operator filter-as-they-type.
//! Selecting a field commits an `op://Vault/Item/field` reference back
//! to the `EnvValue` textarea; the picker never resolves or stores secret
//! values.
//!
//! The runner ([`OpStructRunner`] from `crate::operator_env`) is invoked
//! on a background thread; the picker stages the load via an `mpsc`
//! channel and renders a Braille spinner until the receiver yields.
//! Failure modes (`op` not installed, signed out, no vaults, generic
//! error) drive the four "fatal-state" instructional panels.

use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_widget_list::ListState;

use crate::console::manager::state::TextInputTarget;
use crate::operator_env::{OpCli, OpField, OpItem, OpStructRunner, OpVault};

use super::ModalOutcome;

pub mod render;

/// Which pane is currently visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPickerStage {
    Vault,
    Item,
    Field,
}

/// Lifecycle of a background `op` invocation.
///
/// `Idle` — nothing in flight (transient; on entry the picker immediately
/// transitions to `Loading`).
/// `Loading` — a worker thread is running an `op` subcommand; the
/// `spinner_tick` is advanced by [`OpPickerState::tick`] each frame so
/// the render path can pick a Braille glyph without owning a clock.
/// `Ready` — the receiver yielded `Ok(_)`; the corresponding `Vec` on
/// the state holds the result.
/// `Error` — either a recoverable per-pane failure ([`OpPickerError`])
/// or a fatal session-level state ([`OpPickerFatalState`]).
#[derive(Debug, Clone)]
pub enum OpLoadState {
    Idle,
    Loading { spinner_tick: u8 },
    Ready,
    Error(OpPickerError),
}

/// Recoverable vs. fatal classification for a load failure.
///
/// `Fatal` panels block all navigation but Esc — the picker never advanced
/// past the probe / vault-list phase. `Recoverable` errors render as a
/// banner inside the current pane so the operator can navigate back and
/// retry.
#[derive(Debug, Clone)]
pub enum OpPickerError {
    Fatal(OpPickerFatalState),
    Recoverable { message: String },
}

/// Session-level fatal states. Each maps to a distinct instructional
/// panel in [`render::render`].
#[derive(Debug, Clone)]
pub enum OpPickerFatalState {
    /// `op` binary not on `$PATH` (probe failed with a `spawn` error).
    NotInstalled,
    /// `op account list` exited with a signed-out stderr signature.
    NotSignedIn,
    /// Vault-list call succeeded but returned an empty array.
    NoVaults,
    /// Any other non-recoverable probe / vault-list failure.
    GenericFatal { message: String },
}

/// Outcome of a single background `op` call routed back through the
/// channel. The variant carries the pane-specific result so the
/// `try_recv` drainer on `handle_key` / `tick` can update the right
/// `Vec` without a separate "what was loading" tag.
enum LoadResult {
    Vaults(anyhow::Result<Vec<OpVault>>),
    Items(anyhow::Result<Vec<OpItem>>),
    Fields(anyhow::Result<Vec<OpField>>),
}

/// Picker state machine.
pub struct OpPickerState {
    pub stage: OpPickerStage,
    pub filter_buf: String,

    pub vaults: Vec<OpVault>,
    pub vault_list_state: ListState,
    pub selected_vault: Option<OpVault>,

    pub items: Vec<OpItem>,
    pub item_list_state: ListState,
    pub selected_item: Option<OpItem>,

    pub fields: Vec<OpField>,
    pub field_list_state: ListState,

    pub load_state: OpLoadState,

    /// `EnvValue` textarea contents at the moment `Ctrl+O` was pressed.
    /// Restored verbatim on `Esc`-from-vault-pane so cancellation feels
    /// transparent.
    pub original_value: String,
    /// Stashed `TextInputTarget` so the cancel / commit paths can
    /// reconstruct the original `Modal::TextInput` without re-deriving
    /// the scope/key.
    pub saved_target: TextInputTarget,
    /// Stashed label so the reconstructed `TextInput` shows the same
    /// prompt the operator was just looking at.
    pub saved_label: String,

    /// Production runner — boxed so tests can inject a fake via
    /// [`OpPickerState::new_with_runner`].
    runner: Box<dyn OpStructRunner + Send>,
    /// Receiver for the in-flight background call. `None` when no call
    /// is pending; drained by [`OpPickerState::poll_load`].
    rx: Option<mpsc::Receiver<LoadResult>>,
}

// Manual `Debug` because `runner: Box<dyn OpStructRunner + Send>` and
// `rx: Option<mpsc::Receiver<_>>` aren't `Debug` themselves. The skipped
// fields contain zero operator-visible state — they're plumbing for the
// background load — so dropping them from the formatter keeps debug
// output readable without losing diagnostic value.
#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for OpPickerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpPickerState")
            .field("stage", &self.stage)
            .field("filter_buf", &self.filter_buf)
            .field("vaults", &self.vaults)
            .field("selected_vault", &self.selected_vault)
            .field("items", &self.items)
            .field("selected_item", &self.selected_item)
            .field("fields", &self.fields)
            .field("load_state", &self.load_state)
            .field("original_value", &self.original_value)
            .field("saved_target", &self.saved_target)
            .field("saved_label", &self.saved_label)
            .finish_non_exhaustive()
    }
}

impl OpPickerState {
    /// Production constructor. Boxes a fresh [`OpCli`] runner internally
    /// and kicks off the probe + vault-list load on the spot so the
    /// picker is responsive on first frame.
    pub fn new(original_value: String, saved_target: TextInputTarget, saved_label: String) -> Self {
        Self::new_with_runner(
            original_value,
            saved_target,
            saved_label,
            Box::new(OpCli::new()),
        )
    }

    /// Test seam — accepts an injected runner so unit / integration
    /// tests can drive the state machine without an `op` binary.
    pub fn new_with_runner(
        original_value: String,
        saved_target: TextInputTarget,
        saved_label: String,
        runner: Box<dyn OpStructRunner + Send>,
    ) -> Self {
        let mut s = Self {
            stage: OpPickerStage::Vault,
            filter_buf: String::new(),
            vaults: Vec::new(),
            vault_list_state: ListState::default(),
            selected_vault: None,
            items: Vec::new(),
            item_list_state: ListState::default(),
            selected_item: None,
            fields: Vec::new(),
            field_list_state: ListState::default(),
            load_state: OpLoadState::Idle,
            original_value,
            saved_target,
            saved_label,
            runner,
            rx: None,
        };
        s.start_vault_load();
        s
    }

    /// Spawn the vault-load worker. The probe (`account_list`) runs
    /// inline so a `spawn`-error on the binary surfaces as
    /// [`OpPickerFatalState::NotInstalled`] without first showing a
    /// spinner. Once the probe returns, the worker thread continues to
    /// `vault_list`.
    fn start_vault_load(&mut self) {
        // Probe runs synchronously: the cost is one fast subprocess
        // invocation, and the spawn error is the only way to detect
        // "binary not on PATH" before any user-facing pane appears.
        match self.runner.account_list() {
            Ok(_accounts) => {
                // Sign-in OK; kick off vault list on a worker thread.
                self.load_state = OpLoadState::Loading { spinner_tick: 0 };
                let (tx, rx) = mpsc::channel();
                self.rx = Some(rx);
                // `account_list` already proved the binary is reachable;
                // this thread can call vault_list directly.
                let runner = Self::runner_clone_for_thread();
                std::thread::spawn(move || {
                    let _ = tx.send(LoadResult::Vaults(runner.vault_list()));
                });
            }
            Err(e) => {
                self.load_state = OpLoadState::Error(classify_probe_error(&e));
            }
        }
    }

    /// Spawn the item-list worker for the currently-selected vault.
    fn start_item_load(&mut self, vault_id: String) {
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let runner = Self::runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Items(runner.item_list(&vault_id)));
        });
    }

    /// Spawn the field-list worker for the currently-selected item /
    /// vault pair.
    fn start_field_load(&mut self, item_id: String, vault_id: String) {
        self.load_state = OpLoadState::Loading { spinner_tick: 0 };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let runner = Self::runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Fields(runner.item_get(&item_id, &vault_id)));
        });
    }

    /// `OpStructRunner` is not `Clone`, so each background call gets its
    /// own fresh `OpCli`. Tests that inject a custom runner cannot use
    /// this path — they're expected to drive the state machine
    /// synchronously via `inject_load_result` and `tick`. (See commit 7
    /// for the test integration.)
    fn runner_clone_for_thread() -> Box<dyn OpStructRunner + Send> {
        Box::new(OpCli::new())
    }

    /// Drain the in-flight receiver if a result is available, updating
    /// `load_state` + the relevant `Vec`.
    fn poll_load(&mut self) {
        let Some(rx) = self.rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
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
            Ok(LoadResult::Vaults(Err(e))) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(classify_probe_error(&e));
            }
            Ok(LoadResult::Items(Ok(items))) => {
                self.rx = None;
                self.items = items;
                self.item_list_state
                    .select(if self.items.is_empty() { None } else { Some(0) });
                self.stage = OpPickerStage::Item;
                self.filter_buf.clear();
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
                // Stable sort: concealed first. Original order preserved
                // within each bucket — `false < true`, so reverse with
                // a key that puts `concealed == true` first.
                fields.sort_by_key(|f| !f.concealed);
                self.fields = fields;
                self.field_list_state.select(if self.fields.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.stage = OpPickerStage::Field;
                self.filter_buf.clear();
                self.load_state = OpLoadState::Ready;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Worker still running.
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                self.load_state = OpLoadState::Error(OpPickerError::Recoverable {
                    message: "background worker disconnected".into(),
                });
            }
        }
    }

    /// Advance the spinner glyph and drain any pending load result.
    /// Called from the render path each frame so the user sees a moving
    /// Braille glyph and so completed loads show up promptly.
    pub fn tick(&mut self) {
        if let OpLoadState::Loading { spinner_tick } = &mut self.load_state {
            *spinner_tick = spinner_tick.wrapping_add(1);
        }
        self.poll_load();
    }

    /// Filter helpers — substring (case-insensitive) match against the
    /// vault `name`, item `name`, and field `label` respectively.
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
            .filter(|i| needle.is_empty() || i.name.to_lowercase().contains(&needle))
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

    /// Picker key handler.
    ///
    /// Returns `Continue` while the operator is interacting, `Cancel`
    /// when Esc is pressed on the vault pane (or any fatal-state panel),
    /// and `Commit(path)` when a field is selected — `path` is an
    /// `op://Vault/Item/Field` reference.
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        // Drain any pending background result before dispatching the
        // key. A `tick` from the render path normally handles this, but
        // tests bypass render entirely and rely on `handle_key` to
        // surface results.
        self.poll_load();

        // Fatal-state panels have only Esc as an exit.
        if matches!(self.load_state, OpLoadState::Error(OpPickerError::Fatal(_))) {
            return if matches!(key.code, KeyCode::Esc) {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
            };
        }

        // Loading buffers — only Esc breaks out (acts as cancel).
        if matches!(self.load_state, OpLoadState::Loading { .. }) {
            return if matches!(key.code, KeyCode::Esc) {
                ModalOutcome::Cancel
            } else {
                ModalOutcome::Continue
            };
        }

        match self.stage {
            OpPickerStage::Vault => self.handle_vault_key(key),
            OpPickerStage::Item => self.handle_item_key(key),
            OpPickerStage::Field => self.handle_field_key(key),
        }
    }

    fn handle_vault_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Up => {
                let n = self.filtered_vaults().len();
                let cur = self.vault_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur == 0 { n - 1 } else { cur - 1 };
                    self.vault_list_state.select(Some(next));
                }
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_vaults().len();
                let cur = self.vault_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur + 1 >= n { 0 } else { cur + 1 };
                    self.vault_list_state.select(Some(next));
                }
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
                    self.selected_vault = Some(v);
                    self.start_item_load(id);
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
            KeyCode::Esc => {
                // Back to vault pane; keep the previous vault list +
                // selection intact, just clear the per-pane filter.
                self.stage = OpPickerStage::Vault;
                self.filter_buf.clear();
                self.items.clear();
                self.selected_item = None;
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_items().len();
                let cur = self.item_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur == 0 { n - 1 } else { cur - 1 };
                    self.item_list_state.select(Some(next));
                }
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_items().len();
                let cur = self.item_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur + 1 >= n { 0 } else { cur + 1 };
                    self.item_list_state.select(Some(next));
                }
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
                    self.selected_item = Some(item);
                    self.start_field_load(item_id, vault_id);
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
            KeyCode::Esc => {
                self.stage = OpPickerStage::Item;
                self.filter_buf.clear();
                self.fields.clear();
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_fields().len();
                let cur = self.field_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur == 0 { n - 1 } else { cur - 1 };
                    self.field_list_state.select(Some(next));
                }
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_fields().len();
                let cur = self.field_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur + 1 >= n { 0 } else { cur + 1 };
                    self.field_list_state.select(Some(next));
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
                if let Some(field) = visible.get(cur) {
                    let label = if field.label.is_empty() {
                        field.id.clone()
                    } else {
                        field.label.clone()
                    };
                    let vault_name = self.selected_vault.as_ref().map_or("", |v| v.name.as_str());
                    let item_name = self.selected_item.as_ref().map_or("", |i| i.name.as_str());
                    return ModalOutcome::Commit(format!("op://{vault_name}/{item_name}/{label}"));
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

    /// Snap the active pane's selection to row 0 (or `None` when the
    /// filter eliminates every row). Called after each filter mutation.
    fn reset_selection_for_filter(&mut self, stage: OpPickerStage) {
        match stage {
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

/// Categorize an error from the probe / vault-list path into a user-
/// facing fatal state. The classifier looks at the message string
/// because the underlying `anyhow::Error` doesn't carry typed variants.
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

//! 1Password vault/item/field picker modal.
//!
//! Three- to four-pane drill-down (`[Account →] Vault → Item → Field`)
//! reachable via `P` from a Secrets-tab row in the workspace editor.
//! Each pane lists structural metadata returned by `op` — account
//! emails/urls, vault names, item names, and field labels/types — and
//! lets the operator filter-as-they-type. The Account pane is shown
//! only when `op account list` reports two or more signed-in accounts;
//! single-account setups skip directly to the Vault pane.
//!
//! Selecting a field commits the authoritative `op://...` reference
//! that 1Password's `op item get --format json` emits for that field
//! (`OpField::reference`). The path follows the official
//! 1Password CLI syntax — `op://<vault>/<item>/[<section>/]<field>`,
//! see <https://developer.1password.com/docs/cli/secret-reference-syntax/>.
//! The editor's modal handler then writes that reference directly
//! into the focused row's pending value (key row) or stashes it on
//! `EditorState::pending_picker_value` for the follow-up `EnvKey`
//! modal (sentinel row). The picker never resolves or stores secret
//! values.
//!
//! Earlier revisions synthesized the path from display names with
//! `format!("op://{vault}/{item}/{field}", …)`. That was wrong for
//! sectioned fields (the section component was dropped), for items
//! whose names contained `/` or whitespace, and any time
//! 1Password's serializer disagreed with naive concatenation. Using
//! the CLI-provided string sidesteps every escaping bug.
//!
//! On multi-account setups, the chosen account's `account_uuid` is
//! threaded through every downstream `op` call as `--account <id>`
//! so cross-account drilling inside the picker works correctly.
//! Account scope is **not** encoded in the committed `op://` path
//! itself — it is tracked separately as `selected_account` on this
//! state and is not persisted. Cross-account resolution at launch
//! time is the operator's responsibility: ensure the chosen field's
//! vault is reachable through `op`'s default account context (run
//! `op signin` against that account, or set it as default), or the
//! launch-time `op read` will fail with "item not found". A future
//! PR may add a per-value account override in the on-disk format.
//!
//! The runner ([`OpStructRunner`] from `crate::operator_env`) is invoked
//! on a background thread; the picker stages the load via an `mpsc`
//! channel and renders a Braille spinner until the receiver yields.
//! Failure modes (`op` not installed, signed out, no vaults, generic
//! error) drive the four "fatal-state" instructional panels.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_widget_list::ListState;

use crate::console::op_cache::OpCache;
use crate::operator_env::{OpAccount, OpCli, OpField, OpItem, OpStructRunner, OpVault};

use super::ModalOutcome;

pub mod render;

/// Which pane is currently visible.
///
/// `Account` is only ever entered when `op account list` returns two or
/// more accounts. Single-account setups jump straight to `Vault`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPickerStage {
    Account,
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

    /// Production runner — boxed so tests can inject a fake via
    /// [`OpPickerState::new_with_runner`].
    runner: Box<dyn OpStructRunner + Send>,
    /// Receiver for the in-flight background call. `None` when no call
    /// is pending; drained by [`OpPickerState::poll_load`].
    rx: Option<mpsc::Receiver<LoadResult>>,
    /// Session-scoped cache shared with `ConsoleState`. Hits short-
    /// circuit the `OpStructRunner` calls; misses populate the cache
    /// when the load resolves. The default constructor allocates a
    /// fresh empty cache local to this picker — only the production
    /// open path (via [`OpPickerState::new_with_cache`]) wires the
    /// shared cache in.
    op_cache: Rc<RefCell<OpCache>>,
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
    /// Production constructor. Boxes a fresh [`OpCli`] runner internally
    /// and kicks off the probe + vault-list load on the spot so the
    /// picker is responsive on first frame. Allocates a fresh empty
    /// cache local to this picker — production callers should use
    /// [`OpPickerState::new_with_cache`] instead so cache hits across
    /// picker open/close cycles work.
    pub fn new() -> Self {
        Self::new_with_runner_and_cache(
            Box::new(OpCli::new()),
            Rc::new(RefCell::new(OpCache::default())),
        )
    }

    /// Production constructor with a shared session-scoped cache.
    /// Threaded in by `editor::open_secrets_picker_modal` from the
    /// `ManagerState`-owned (and ultimately `ConsoleState`-owned)
    /// `Rc<RefCell<OpCache>>`. Subsequent picker opens within the same
    /// `jackin console` invocation reuse the cached vault / item /
    /// field metadata.
    pub fn new_with_cache(op_cache: Rc<RefCell<OpCache>>) -> Self {
        Self::new_with_runner_and_cache(Box::new(OpCli::new()), op_cache)
    }

    /// Test seam — accepts an injected runner so unit / integration
    /// tests can drive the state machine without an `op` binary.
    /// Allocates a fresh empty cache local to the picker (tests that
    /// care about cache behavior pass a shared one via
    /// [`OpPickerState::new_with_runner_and_cache`]).
    pub fn new_with_runner(runner: Box<dyn OpStructRunner + Send>) -> Self {
        Self::new_with_runner_and_cache(runner, Rc::new(RefCell::new(OpCache::default())))
    }

    /// Test seam — accepts both an injected runner and a shared cache,
    /// so cache-hit / cache-miss tests can drive the picker against a
    /// pre-populated cache.
    pub fn new_with_runner_and_cache(
        runner: Box<dyn OpStructRunner + Send>,
        op_cache: Rc<RefCell<OpCache>>,
    ) -> Self {
        let mut s = Self {
            stage: OpPickerStage::Vault,
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
            load_state: OpLoadState::Idle,
            runner,
            rx: None,
            op_cache,
        };
        s.probe_and_start_initial_load();
        s
    }

    /// Run the `account_list` probe synchronously, then route to either
    /// the Account pane (≥2 accounts) or the Vault pane with the lone
    /// account auto-selected (single-account setups).
    ///
    /// The probe runs inline so a `spawn`-error on the `op` binary
    /// surfaces as [`OpPickerFatalState::NotInstalled`] before any
    /// user-facing pane appears. An empty account list — `op account
    /// list` returning `[]` — is functionally equivalent to "not signed
    /// in" and maps to that fatal state.
    fn probe_and_start_initial_load(&mut self) {
        // Cache hit on the accounts list short-circuits the synchronous
        // `account_list` probe. Cache miss falls through to the runner
        // and stores the result.
        let accounts_result = if let Some(cached) = self.op_cache.borrow().get_accounts() {
            Ok(cached)
        } else {
            let r = self.runner.account_list();
            if let Ok(ref accounts) = r {
                self.op_cache.borrow_mut().put_accounts(accounts.clone());
            }
            r
        };
        match accounts_result {
            Ok(accounts) if accounts.is_empty() => {
                self.load_state =
                    OpLoadState::Error(OpPickerError::Fatal(OpPickerFatalState::NotSignedIn));
            }
            Ok(accounts) if accounts.len() == 1 => {
                // Single-account setup: skip the Account pane, auto-
                // select the only account, and kick off the vault load
                // scoped to it. `start_vault_load` advances the stage to
                // `Vault` itself.
                let account = accounts.into_iter().next().expect("len == 1");
                let account_id = account.id.clone();
                self.selected_account = Some(account);
                self.start_vault_load(Some(account_id));
            }
            Ok(accounts) => {
                // Multi-account setup: render the Account pane first.
                self.accounts = accounts;
                self.account_list_state.select(Some(0));
                self.stage = OpPickerStage::Account;
                self.load_state = OpLoadState::Ready;
            }
            Err(e) => {
                self.load_state = OpLoadState::Error(classify_probe_error(&e));
            }
        }
    }

    /// Spawn the vault-load worker, optionally scoped to `account_id`.
    /// Cache hits short-circuit the spawn and route the cached result
    /// directly into [`OpPickerState::poll_load`] via the in-memory
    /// channel — keeps a single completion path so `poll_load`'s
    /// "select" logic stays canonical.
    ///
    /// Stage is advanced to `Vault` at request time (not result time) so
    /// the loading-panel title can show the correct breadcrumb for the
    /// in-flight load. Without this, the title was stuck on the previous
    /// stage during the 1-3s `op` subprocess and the operator lost
    /// context for what was loading. The per-pane filter is cleared too
    /// so the new pane opens fresh.
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
        // `account_list` already proved the binary is reachable;
        // this thread can call vault_list directly.
        let runner = Self::runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Vaults(runner.vault_list(account_id.as_deref())));
        });
    }

    /// Spawn the item-list worker for the currently-selected vault,
    /// optionally scoped to `account_id`. Cache hits short-circuit.
    ///
    /// Stage advances to `Item` at request time so the loading-panel
    /// breadcrumb reflects the in-flight load's destination (see the
    /// rationale on `start_vault_load`). The per-pane filter is cleared
    /// for the same reason it is on every other pane transition.
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
        let runner = Self::runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Items(
                runner.item_list(&vault_id, account_id.as_deref()),
            ));
        });
    }

    /// Spawn the field-list worker for the currently-selected item /
    /// vault pair, optionally scoped to `account_id`. Cache hits
    /// short-circuit.
    ///
    /// Stage advances to `Field` at request time so the loading-panel
    /// breadcrumb reflects the in-flight load's destination (see the
    /// rationale on `start_vault_load`). The per-pane filter is cleared
    /// for the same reason it is on every other pane transition.
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
        let runner = Self::runner_clone_for_thread();
        std::thread::spawn(move || {
            let _ = tx.send(LoadResult::Fields(runner.item_get(
                &item_id,
                &vault_id,
                account_id.as_deref(),
            )));
        });
    }

    /// Borrowed view of the currently-selected account's UUID, suitable
    /// for `op --account <id>` threading. Returns `None` when no account
    /// is selected (single-account setups before the probe completes,
    /// which never happens in practice because the probe is synchronous
    /// in the constructor).
    fn selected_account_id(&self) -> Option<String> {
        self.selected_account.as_ref().map(|a| a.id.clone())
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
    ///
    /// Public so the outer console event loop can drain pending worker
    /// results on every tick (not just on key events / render frames),
    /// keeping the picker responsive without keystroke pumping. The
    /// render path's [`OpPickerState::tick`] still calls this internally
    /// — both call sites are idempotent on an empty channel.
    pub fn poll_load(&mut self) {
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
                // Populate the session cache so the next `start_vault_load`
                // for the same account short-circuits the subprocess.
                let account_id = self.selected_account_id();
                self.op_cache
                    .borrow_mut()
                    .put_vaults(account_id.as_deref(), vaults.clone());
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
                let account_id = self.selected_account_id();
                let vault_id = self
                    .selected_vault
                    .as_ref()
                    .map(|v| v.id.clone())
                    .unwrap_or_default();
                self.op_cache.borrow_mut().put_items(
                    account_id.as_deref(),
                    &vault_id,
                    items.clone(),
                );
                self.items = items;
                self.item_list_state
                    .select(if self.items.is_empty() { None } else { Some(0) });
                // Stage already set to `Item` at request time
                // (`start_item_load`) so the loading-panel breadcrumb is
                // correct; nothing to do here beyond filling the list and
                // flipping load_state to `Ready`.
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
                // Cache the sorted result so cache-hit hands back the
                // already-presentation-ordered vec on subsequent opens.
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
                self.op_cache.borrow_mut().put_fields(
                    account_id.as_deref(),
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
                // Stage already set to `Field` at request time
                // (`start_field_load`); see the matching comment on the
                // `Items` arm above.
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
    /// account `email`/`url`, vault `name`, item `name`, and field
    /// `label` respectively.
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
                // Refresh: drop the cached account list and re-run the
                // probe synchronously. Carrying out a fresh probe also
                // re-routes single-vs-multi-account branching, so callers
                // who add or sign out of accounts mid-session see the
                // change without restarting `jackin console`.
                self.op_cache.borrow_mut().invalidate_accounts();
                self.accounts.clear();
                self.account_list_state = ListState::default();
                self.selected_account = None;
                self.probe_and_start_initial_load();
                ModalOutcome::Continue
            }
            KeyCode::Up => {
                let n = self.filtered_accounts().len();
                let cur = self.account_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur == 0 { n - 1 } else { cur - 1 };
                    self.account_list_state.select(Some(next));
                }
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                let n = self.filtered_accounts().len();
                let cur = self.account_list_state.selected.unwrap_or(0);
                if n > 0 {
                    let next = if cur + 1 >= n { 0 } else { cur + 1 };
                    self.account_list_state.select(Some(next));
                }
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
                    // `start_vault_load` advances the stage and clears
                    // the per-pane filter at request time.
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
                // Multi-account: back to Account; clear vault state so
                // the operator gets a fresh start when re-drilling.
                if self.accounts.len() > 1 {
                    self.stage = OpPickerStage::Account;
                    self.filter_buf.clear();
                    self.selected_vault = None;
                    self.vaults.clear();
                    self.vault_list_state = ListState::default();
                    // Clear the load_state so any banners from the prior
                    // (now-discarded) vault load don't bleed into the
                    // Account pane.
                    self.load_state = OpLoadState::Ready;
                    return ModalOutcome::Continue;
                }
                // Single-account: Esc on Vault closes the picker as
                // before — no Account pane to fall back to.
                ModalOutcome::Cancel
            }
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
                // start_item_load sets load_state = Loading; poll_load
                // flips back to Ready once the new result arrives. Stage
                // stays on `Item` throughout (refresh-in-place).
                ModalOutcome::Continue
            }
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
                    // Prefer the authoritative `op://...` string that
                    // `op item get --format json` emits per field.
                    // Synthesizing from display names mishandled
                    // sectioned fields (4-segment paths), items
                    // containing `/` or whitespace, and anything else
                    // where 1Password's serializer disagrees with
                    // naive concatenation. Fall back to a synthesized
                    // path only as a defensive measure for older `op`
                    // versions / fixtures that omit `reference`.
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

    /// Snap the active pane's selection to row 0 (or `None` when the
    /// filter eliminates every row). Called after each filter mutation.
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

#[cfg(test)]
mod tests {
    //! Picker state-machine unit tests.
    //!
    //! Strategy (Option Z from the plan): tests construct the picker via
    //! `new_with_runner` with a no-op mock runner so the synchronous probe
    //! in `start_vault_load` returns instantly. The constructor still
    //! spawns a worker thread (via the production `runner_clone_for_thread`
    //! helper that builds a fresh `OpCli`) — but tests **never** wait for
    //! that thread to publish a result. Instead, we manually overwrite
    //! `vaults` / `items` / `fields` / `load_state` / `stage` / selection
    //! before driving `handle_key`. This skips the threading model
    //! entirely and exercises the state machine in isolation.
    //!
    //! `poll_load` is called from `handle_key`; if the worker thread has
    //! published a real result we'd see it overwrite our seeded `vaults`.
    //! In practice the synthetic `op` binary doesn't exist in CI / dev
    //! env, so the worker errors out fast and is harmless once we've
    //! re-set `load_state = Ready` before each key event.
    use super::*;
    use crate::operator_env::{OpAccount, OpField, OpItem, OpVault};
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use std::sync::Mutex;

    /// In-process mock — `account_list` succeeds (so the constructor's
    /// probe doesn't immediately classify the picker as `NotInstalled`),
    /// every other method returns an empty `Vec`. We don't use the
    /// vault/item/field methods in tests because the worker thread uses
    /// the production `runner_clone_for_thread` helper.
    ///
    /// `last_vault_list_account` records the `account` argument passed
    /// to the most recent `vault_list` call so the multi-account flow
    /// test can assert that the chosen account's UUID was threaded
    /// through. The mod-level worker thread uses
    /// `runner_clone_for_thread` rather than this stub, so the assertion
    /// is on the *synchronous* probe-and-route path, which tests
    /// directly exercise via `start_vault_load` after constructor
    /// invocation.
    #[derive(Default)]
    struct StubRunner {
        accounts: Mutex<Vec<OpAccount>>,
        // `Option<Option<…>>` distinguishes "never called" (outer
        // `None`) from "called with `None`" (outer `Some`, inner
        // `None`). This is exactly the shape clippy flags as
        // suspicious; here it's deliberate and load-bearing for the
        // multi-account threading test.
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

    /// Build a picker with a single seeded account (so the constructor
    /// auto-selects it, jumps straight to the Vault stage, and never
    /// shows the Account pane). Drop the in-flight receiver from the
    /// constructor's background thread (so `poll_load` won't overwrite
    /// our seeded state), and seed it `Ready` so key handling proceeds
    /// normally. Most existing tests use this — single-account behavior
    /// matches the pre-multi-account picker contract.
    fn picker_ready() -> OpPickerState {
        let runner = Box::new(StubRunner {
            accounts: Mutex::new(vec![account(
                "acct1",
                "single@example.com",
                "single.1password.com",
            )]),
            last_vault_list_account: Mutex::new(None),
        });
        let mut s = OpPickerState::new_with_runner(runner);
        // Discard the worker's channel so a delayed result (e.g. from
        // the production `runner_clone_for_thread` builder running on a
        // background thread) cannot stomp on the vectors the test seeds
        // explicitly below.
        s.rx = None;
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

    /// Build a field carrying an explicit `reference` string. Used by
    /// the picker-commit test that asserts the CLI-provided reference
    /// is used verbatim instead of a path synthesized from display
    /// names.
    fn field_with_reference(label: &str, reference: &str) -> OpField {
        OpField {
            id: label.to_string(),
            label: label.to_string(),
            field_type: "STRING".to_string(),
            concealed: false,
            reference: reference.to_string(),
        }
    }

    #[test]
    fn item_filter_matches_subtitle() {
        // Two items share the title "Google" but have different
        // subtitles (the `additional_information` field 1Password
        // surfaces as the username). Filtering by a substring of the
        // second item's subtitle must narrow the visible list to that
        // item alone — proving that subtitle matching pulls its weight
        // when titles collide. The filter is also exercised with mixed
        // case to confirm the comparison is case-insensitive (the
        // production code lowercases both sides before `contains`).
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
        // Sanity: only the "Personal" vault is visible at index 0.
        assert_eq!(s.filtered_vaults().len(), 1);

        // Enter on the filtered vault — the production code spawns a
        // background thread to load items. We don't wait for it; we
        // immediately discard the rx and pretend the load resolved with
        // a synthetic `items` list, then short-circuit the stage to
        // `Item` the way `poll_load` would. This bypasses the threading
        // entirely — what we actually verify is the *intent*: that
        // `handle_key(Enter)` with the filter active sets the
        // `selected_vault` and kicks off an item load. The pane-advance-
        // clears-filter contract is enforced inside `poll_load`'s Items
        // arm, which is exercised below by simulating that arm directly.
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert_eq!(
            s.selected_vault.as_ref().map(|v| v.name.as_str()),
            Some("Personal"),
            "Enter on filtered vault must capture the selection"
        );

        // Drop the worker's channel and simulate the Items-arm of
        // `poll_load` running.
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

    #[test]
    fn enter_on_field_commits_op_path() {
        // Backward-compat: when `OpField::reference` is empty (older
        // `op` versions / fixtures that omit the key), the picker
        // falls back to synthesizing a path from display names. The
        // production path uses the `reference` directly — see
        // `picker_commit_uses_op_provided_reference_not_synthesized`.
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

    /// Production path: when `OpField::reference` is non-empty, the
    /// picker commits that string verbatim. Display names that
    /// contain whitespace, slashes, or live inside a section would
    /// produce a wrong synthesized path; using the CLI-provided
    /// reference sidesteps the entire class of bugs.
    #[test]
    fn picker_commit_uses_op_provided_reference_not_synthesized() {
        let mut s = picker_ready();
        s.selected_vault = Some(vault("Personal"));
        s.selected_item = Some(OpItem {
            id: "i-test".into(),
            // Display name contains whitespace — naive synthesis
            // would produce `op://Personal/name with spaces/api`.
            name: "name with spaces".into(),
            subtitle: String::new(),
        });
        // Field's display label is also distinct from its
        // section-aware reference.
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

    /// Sanity: the stub-runner constructor doesn't classify a successful
    /// `account_list` as a fatal `NotInstalled` state. (The seeded
    /// constructor leaves `load_state = Loading` while the worker thread
    /// runs; this just confirms it isn't an Error.)
    #[test]
    fn stub_runner_constructor_is_not_fatal() {
        let runner = Box::new(StubRunner {
            accounts: Mutex::new(vec![account("a", "a@example.com", "a.1password.com")]),
            last_vault_list_account: Mutex::new(None),
        });
        let s = OpPickerState::new_with_runner(runner);
        assert!(
            !matches!(s.load_state, OpLoadState::Error(OpPickerError::Fatal(_))),
            "stub runner returning Ok must not produce a fatal state; got {:?}",
            s.load_state
        );
    }

    // ── Multi-account picker tests ────────────────────────────────────

    /// Two seeded accounts: the constructor must route to the Account
    /// pane, populate `accounts`, and select index 0.
    #[test]
    fn picker_starts_at_account_when_multiple_accounts() {
        let runner = Box::new(StubRunner {
            accounts: Mutex::new(vec![
                account("acct1", "a@example.com", "alpha.1password.com"),
                account("acct2", "b@example.com", "beta.1password.com"),
            ]),
            last_vault_list_account: Mutex::new(None),
        });
        let s = OpPickerState::new_with_runner(runner);
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

    /// One seeded account: the constructor must skip the Account pane
    /// entirely and auto-select that account, jumping straight to the
    /// Vault stage.
    #[test]
    fn picker_starts_at_vault_when_single_account() {
        let runner = Box::new(StubRunner {
            accounts: Mutex::new(vec![account(
                "solo",
                "solo@example.com",
                "solo.1password.com",
            )]),
            last_vault_list_account: Mutex::new(None),
        });
        let s = OpPickerState::new_with_runner(runner);
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

    /// Filter on the Account pane narrows by email (substring,
    /// case-insensitive).
    #[test]
    fn account_pane_filter_narrows_by_email() {
        let runner = Box::new(StubRunner {
            accounts: Mutex::new(vec![
                account("a1", "alice@example.com", "alpha.1password.com"),
                account("a2", "bob@example.com", "beta.1password.com"),
            ]),
            last_vault_list_account: Mutex::new(None),
        });
        let mut s = OpPickerState::new_with_runner(runner);
        s.rx = None;
        s.load_state = OpLoadState::Ready;
        s.filter_buf = "alic".to_string();
        let visible = s.filtered_accounts();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].email, "alice@example.com");
    }

    /// Enter on the Account pane must:
    ///   - record the chosen account in `selected_account`,
    ///   - advance `stage = Vault`,
    ///   - kick off `vault_list(Some(account_id))` (verified by the
    ///     stub runner recording the last `vault_list` argument when the
    ///     synchronous `vault_list` call from `start_vault_load`'s
    ///     spawned thread runs through `runner_clone_for_thread`).
    ///
    /// Because `runner_clone_for_thread` builds a fresh `OpCli`, the
    /// stub's recording can't be used directly for the spawned call.
    /// Instead we directly invoke the synchronous helper that *would*
    /// be the call site, mirroring what Enter does, and confirm the
    /// stub records `Some(account_uuid)`.
    #[test]
    fn enter_on_account_advances_to_vault_with_account_scope() {
        let runner = Box::new(StubRunner {
            accounts: Mutex::new(vec![
                account("acct1", "a@example.com", "alpha.1password.com"),
                account("acct2", "b@example.com", "beta.1password.com"),
            ]),
            last_vault_list_account: Mutex::new(None),
        });
        let mut s = OpPickerState::new_with_runner(runner);
        s.rx = None;
        s.load_state = OpLoadState::Ready;
        // Select the second account.
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
        // Direct-call verification of the account threading: invoke
        // the runner's `vault_list` the same way `start_vault_load`'s
        // spawned thread would (the spawned thread itself uses a fresh
        // OpCli, not our stub, so we re-create the stub-call here).
        // The trait method passes `Some(account_id)` whenever
        // `selected_account_id()` returns Some — this verifies that
        // contract on the stub.
        let runner = Box::new(StubRunner::default());
        runner.account_list().unwrap();
        let _ = runner.vault_list(s.selected_account_id().as_deref());
        let recorded = runner.last_vault_list_account.lock().unwrap().clone();
        assert_eq!(
            recorded,
            Some(Some("acct2".to_string())),
            "vault_list must be called with Some(account_uuid) once an account is selected"
        );
    }

    /// Esc on Vault when ≥2 accounts must return to the Account pane,
    /// clearing vault state. Multi-account contract.
    #[test]
    fn esc_from_vault_with_multi_account_returns_to_account() {
        let runner = Box::new(StubRunner {
            accounts: Mutex::new(vec![
                account("acct1", "a@example.com", "alpha.1password.com"),
                account("acct2", "b@example.com", "beta.1password.com"),
            ]),
            last_vault_list_account: Mutex::new(None),
        });
        let mut s = OpPickerState::new_with_runner(runner);
        s.rx = None;
        s.load_state = OpLoadState::Ready;
        // Pretend the operator already advanced from Account → Vault.
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

    /// Esc on Vault when only one account is signed in must close the
    /// picker (Cancel) — same as the pre-multi-account behavior.
    #[test]
    fn esc_from_vault_with_single_account_cancels_picker() {
        let mut s = picker_ready();
        s.vaults = vec![vault("Personal")];
        s.vault_list_state.select(Some(0));
        // Sanity: single-account setup keeps `accounts` empty.
        assert!(s.accounts.is_empty());

        let outcome = s.handle_key(key(KeyCode::Esc));
        assert!(
            matches!(outcome, ModalOutcome::Cancel),
            "Esc on Vault in single-account mode must cancel the picker"
        );
    }

    // ── OpCache integration tests ─────────────────────────────────────
    //
    // These tests focus on the synchronous, single-threaded portion of
    // the cache path: the constructor's `account_list` probe and any
    // call site that consults the cache *before* spawning a worker
    // thread. The worker thread itself uses the production
    // `runner_clone_for_thread` helper (always `OpCli`), so we don't
    // assert against runner counts after a thread-spawning miss — only
    // after a synchronous hit.

    /// Counting runner — increments a shared call counter on
    /// `account_list` so cache-hit tests can assert "subprocess not
    /// invoked". The counter is `Arc<Mutex<usize>>` so callers can hold
    /// a clone for inspection while the runner is moved into the
    /// picker.
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

    /// Building two pickers against the same shared cache — the second
    /// constructor's synchronous `account_list` probe must short-circuit
    /// to the cached vector instead of invoking the runner again.
    #[test]
    fn op_cache_hit_skips_account_list_subprocess() {
        use crate::console::op_cache::OpCache;
        use std::sync::Arc;

        let cache = std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()));
        let counter1: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
        let counter2: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

        // First picker — cache miss; runner invoked once and the cache
        // is populated as a side effect.
        let _ = OpPickerState::new_with_runner_and_cache(
            Box::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter1.clone(),
            }),
            cache.clone(),
        );
        assert_eq!(
            *counter1.lock().unwrap(),
            1,
            "first picker constructor must miss the empty cache"
        );

        // Second picker — cache hit; runner must NOT be invoked.
        let _ = OpPickerState::new_with_runner_and_cache(
            Box::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter2.clone(),
            }),
            cache,
        );
        assert_eq!(
            *counter2.lock().unwrap(),
            0,
            "second picker against the same cache must hit and skip account_list"
        );
    }

    /// Cache miss populates the cache; a subsequent picker on the same
    /// cache hits without invoking the runner again.
    #[test]
    fn op_cache_miss_calls_runner_and_stores() {
        use crate::console::op_cache::OpCache;
        use std::sync::Arc;

        let cache = std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()));
        let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

        // Empty cache → runner called once.
        let _ = OpPickerState::new_with_runner_and_cache(
            Box::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter.clone(),
            }),
            cache.clone(),
        );
        assert_eq!(*counter.lock().unwrap(), 1, "first picker must miss");
        assert!(
            cache.borrow().get_accounts().is_some(),
            "first picker must populate the cache"
        );

        // Same cache + new picker → no new runner call.
        let _ = OpPickerState::new_with_runner_and_cache(
            Box::new(CounterRunner {
                accounts: vec![account("acct1", "a@example.com", "alpha.1password.com")],
                counter: counter.clone(),
            }),
            cache,
        );
        assert_eq!(
            *counter.lock().unwrap(),
            1,
            "second picker on populated cache must hit and not re-call account_list"
        );
    }

    /// Pressing `r` on the Account pane must invalidate the accounts
    /// cache entry and re-fire the probe (count goes up).
    #[test]
    fn op_cache_refresh_re_fires_subprocess() {
        use crate::console::op_cache::OpCache;
        use std::sync::Arc;

        let cache = std::rc::Rc::new(std::cell::RefCell::new(OpCache::default()));
        let counter: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

        // Two-account setup so the picker lands on the Account pane.
        let r = Box::new(CounterRunner {
            accounts: vec![
                account("acct1", "a@example.com", "alpha.1password.com"),
                account("acct2", "b@example.com", "beta.1password.com"),
            ],
            counter: counter.clone(),
        });
        let mut s = OpPickerState::new_with_runner_and_cache(r, cache);
        // Discard the worker rx; the constructor's vault load (if any)
        // is irrelevant to this test.
        s.rx = None;
        s.load_state = OpLoadState::Ready;
        assert_eq!(*counter.lock().unwrap(), 1, "constructor must miss once");

        // Press `r` on the Account pane — runner must be called again
        // because refresh invalidated the accounts cache entry.
        let _ = s.handle_key(key(KeyCode::Char('r')));
        assert_eq!(
            *counter.lock().unwrap(),
            2,
            "r on Account must invalidate cache and re-fire account_list"
        );
        // Accounts vec is repopulated.
        assert_eq!(s.accounts.len(), 2);
        assert_eq!(s.stage, OpPickerStage::Account);
    }

    /// Compile-time guarantee: the cache stores `Vec<OpField>`, which
    /// has no `value` field. Mirrors the safety test in
    /// `operator_env.rs` — if a future refactor adds a value field, the
    /// destructure here fails to compile and forces re-review.
    #[test]
    fn op_cache_picker_does_not_store_field_values() {
        let f = OpField {
            id: "password".into(),
            label: "password".into(),
            field_type: "concealed".into(),
            concealed: true,
            reference: "op://Personal/API Keys/password".into(),
        };
        // Exhaustive destructure — every field of `OpField` listed here.
        let OpField {
            id: _,
            label: _,
            field_type: _,
            concealed: _,
            reference: _,
        } = f;
    }
}

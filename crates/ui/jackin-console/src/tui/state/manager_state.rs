// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` struct and modal presence.

use super::{
    AccountPickerState, AgentChoiceState, DragState, ManagerConfigSaveResult, ManagerEffect,
    ManagerHoverTarget, ManagerInstanceRefreshSnapshot, ManagerStage, Modal, MountInfoCache,
    MountScrollFocus, PendingFileBrowserCommit, PendingFileBrowserListing, PendingMountInfoRefresh,
    RolePickerState, UsageRouteState, UsageScreenState, WorkspaceSummary,
};
use ratatui::layout::Rect;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::tui::focus::TabFocus;
use crate::tui::runtime::BlockingSubscription;

use jackin_env::OpCache;

#[derive(Debug)]
pub struct ManagerState<'a> {
    pub stage: ManagerStage<'a>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub instances: Vec<jackin_core::InstanceIndexEntry>,
    pub current_dir: String,
    pub selected: usize,
    /// Modal slot at the list level (e.g. `Modal::GithubPicker`); the
    /// Editor / `CreatePrelude` stages own their own modal slots.
    pub list_modal: Option<Modal<'a>>,
    /// Passive overlay drawn on top of `list_modal` for the duration of
    /// a single frame while a blocking async operation runs (currently
    /// the console role-resolution path). Input handlers do not see it.
    pub status_overlay: Option<crate::tui::components::StatusPopupState>,
    /// Keyboard-help overlay opened by `?` from any console stage. Owns all
    /// input while `Some` (dispatch's top arm); focus restore is automatic —
    /// the stage state underneath is untouched.
    pub keyboard_help: Option<termrock::widgets::KeyboardHelpState>,
    pub inline_role_picker: Option<RolePickerState>,
    pub inline_agent_picker: Option<(jackin_core::RoleSelector, AgentChoiceState)>,
    /// Agent picker opened when the operator presses `N` on an instance row
    /// to start a new session in the running container. Carries the target
    /// `container_base`, the agent picker, and a provider list. The list is
    /// currently always empty: host config cannot prove which `ZAI_API_KEY`
    /// the already-running daemon captured, so provider choice for a running
    /// container is made from the last live manifest admission refresh.
    pub inline_new_session_picker: Option<(
        String,
        AgentChoiceState,
        Vec<crate::services::launch::AccountChoice>,
    )>,
    /// Provider picker shown after the agent is committed in
    /// `inline_new_session_picker` when its live admission list has 2+
    /// entries. Context is the target `container`.
    pub inline_account_picker: Option<AccountPickerState<String>>,
    /// Provider picker for the initial workspace launch (before the container
    /// exists). Shown after the operator commits an agent choice and
    /// `ZAI_API_KEY` is configured. Context is the `RoleSelector`.
    pub launch_account_picker: Option<AccountPickerState<jackin_core::RoleSelector>>,
    pub list_mounts_scroll: termrock::widgets::ScrollAreaState,
    pub list_global_mounts_scroll: termrock::widgets::ScrollAreaState,
    pub list_role_global_mounts_scroll: termrock::widgets::ScrollAreaState,
    pub list_roles_scroll: termrock::widgets::ScrollAreaState,
    pub list_focus_owner: TabFocus<MountScrollFocus>,
    pub list_names_scroll: termrock::widgets::ScrollAreaState,
    pub list_split_pct: u16,
    pub drag_state: Option<DragState>,
    pub hover_target: Option<ManagerHoverTarget>,
    /// Consumer hover cache over per-event `HitRegion`s (mouse matrix
    /// row 15) — the source the Moved arm resolves stage hovers from.
    pub hover: termrock::interaction::HoverState<crate::tui::input::mouse::ConsoleHoverTarget>,
    pub mount_info_cache: MountInfoCache,
    /// Process-lifetime cache of `op` structural metadata, threaded
    /// into the picker on open. Carries no credentials — see
    /// `op_cache.rs`.
    pub op_cache: Rc<RefCell<OpCache>>,
    /// Mirrored from `ConsoleState::op_available` (probed once at
    /// startup) so the Secrets-tab editor can disable the
    /// source-picker's 1Password choice without re-probing.
    pub op_available: bool,
    /// Typed non-TUI work requested by input/update code. The root run loop
    /// drains and executes these outside the input dispatcher.
    pub(in crate::tui) pending_effects: Vec<ManagerEffect>,
    /// Last known terminal size, updated at the top of every render
    /// frame. Used by keyboard handlers to compute `viewport_h` for
    /// cursor-to-viewport scroll adjustment without needing a render pass.
    pub cached_term_size: Rect,
    /// Throttle the per-tick `InstanceIndex::read_or_rebuild` poll —
    /// state on disk can't change at the 20 Hz render cadence and the
    /// rebuild path walks every container directory.
    pub instances_last_refresh: Option<std::time::Instant>,
    pub(in crate::tui) instances_refresh_interval: std::time::Duration,
    pub(in crate::tui) instances_refresh_generation: u64,
    pub(in crate::tui) instances_refresh_rx:
        Option<BlockingSubscription<(u64, Result<ManagerInstanceRefreshSnapshot, String>)>>,
    pub(in crate::tui) mount_info_refresh_rx: Option<BlockingSubscription<PendingMountInfoRefresh>>,
    pub(in crate::tui) file_browser_listing_rx:
        Option<BlockingSubscription<PendingFileBrowserListing>>,
    pub(in crate::tui) file_browser_commit_rx:
        Option<BlockingSubscription<PendingFileBrowserCommit>>,
    pub(in crate::tui) config_save_rx: Option<BlockingSubscription<ManagerConfigSaveResult>>,
    pub(in crate::tui) account_scan_rx: Option<
        BlockingSubscription<(
            u64,
            Result<crate::tui::screens::settings::model::AccountScanOutcome, String>,
        )>,
    >,
    /// Dedup gate: last error string from `refresh_instances`. Without
    /// this, a persistent parse error would reopen the popup on every
    /// 20 Hz tick — operators would never be able to dismiss it.
    pub(in crate::tui) instances_last_error: Option<String>,
    /// Which saved-workspace indices are expanded in the tree view.
    /// Indices are positions in `self.workspaces` and are only valid for
    /// the lifetime of this `ManagerState` instance — workspace changes
    /// always fully rebuild state, clearing this set.
    pub expanded_workspaces: BTreeSet<usize>,
    /// Whether the synthetic "Current directory" row is expanded to
    /// show its active instances. Mirrors `expanded_workspaces` for
    /// the one-off cwd row, which has no index into `workspaces`.
    pub current_dir_expanded: bool,
    /// Cached sessions per active instance keyed by `container_base`.
    /// Populated from manifests during `refresh_instances`.
    pub instance_sessions: HashMap<String, Vec<jackin_core::SessionRecord>>,
    /// Containers whose manifests could not be read during the last
    /// `refresh_instances` pass. Cleared on every successful index load.
    pub(in crate::tui) instance_session_errors: HashSet<String>,
    /// Exact account/agent/config-ID admissions read from live manifests,
    /// keyed by `container_base`. Missing means the live manifest could not
    /// prove an admission set, so the new-session picker must offer nothing.
    pub live_instance_admissions:
        HashMap<String, Vec<crate::services::launch::LiveInstanceAdmission>>,
    /// Live tab/pane snapshot per running instance keyed by
    /// `container_base`. Populated each `refresh_instances` tick by
    /// fetching from the daemon's bind-mounted socket at
    /// `~/.jackin/sockets/<container>/jackin.sock`. Missing keys mean
    /// the snapshot is unavailable (container not running, socket
    /// pre-dates the bind-mount, or the fetch failed).
    pub instance_snapshots: HashMap<String, jackin_protocol::InstanceSnapshot>,
    /// `true` when the operator has dropped cursor focus into the
    /// snapshot preview pane via Tab / →. While set, ↑/↓ navigates
    /// `preview_pane_cursor` through the flattened pane list and
    /// Enter attaches with the selected pane's focus id. Esc / ← /
    /// `BackTab` pops focus back to the workspace tree.
    pub preview_focused: bool,
    /// Operator-selected pane index within the flattened pane list
    /// of the focused instance, keyed by `container_base`. Persists
    /// across re-entries to the preview pane so the operator's last
    /// selection survives a `Esc → ↑/↓ → Tab` round-trip.
    pub preview_pane_cursor: HashMap<String, usize>,
    /// Console-owned Usage route state (screen + visibility). The screen is
    /// created on first open and kept alive afterwards so the heartbeat
    /// keeps refreshing broker data while the route is offscreen; closing
    /// the route hides it without destroying screen state, so periodic
    /// refreshes continue offscreen and selection survives a close/reopen
    /// round-trip.
    pub usage: UsageRouteState,
    /// Complete usage publication staged before the route is opened.
    pub usage_snapshot: UsageScreenState,
}

// ── Impls ───────────────────────────────────────────────────────────────────

impl crate::tui::model::ConsoleManagerModalBlockPresence for ManagerState<'_> {
    fn list_modal_open(&self) -> bool {
        self.list_modal.is_some()
    }

    fn editor_modal_open(&self) -> bool {
        matches!(&self.stage, ManagerStage::Editor(editor) if editor.modal.is_some())
    }
}

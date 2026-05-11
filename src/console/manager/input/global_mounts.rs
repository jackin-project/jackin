use crossterm::event::{KeyCode, KeyEvent};

use super::super::state::{
    GlobalMountConfirm, GlobalMountDraft, GlobalMountModal, GlobalMountTextTarget, ManagerStage,
    ManagerState, Toast, ToastKind,
};

const NO_MOUNT_SELECTED: &str = "No mount selected.";
const MOUNT_NAME_EMPTY: &str = "Mount name cannot be empty.";
const MOUNT_GONE: &str = "Mount no longer exists; selection was cleared.";
const ADD_DRAFT_LOST: &str = "Add-mount draft was lost; press 'a' to start over.";
use crate::config::AppConfig;
use crate::console::widgets::ModalOutcome;
use crate::console::widgets::confirm::ConfirmState;
use crate::console::widgets::text_input::TextInputState;
use crate::paths::JackinPaths;
use crate::workspace::{MountConfig, resolve_path};

pub(super) fn handle_global_mounts_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::GlobalMounts(global) = &mut state.stage else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if global.is_dirty() {
                global.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                state.stage = ManagerStage::List;
            }
        }
        KeyCode::Left | KeyCode::Char('h' | 'H') => {
            global.scroll_x = global.scroll_x.saturating_sub(8);
        }
        KeyCode::Right | KeyCode::Char('l' | 'L') => {
            global.scroll_x = global.scroll_x.saturating_add(8);
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            global.selected = global.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            let max = global.pending.len().saturating_sub(1);
            global.selected = (global.selected + 1).min(max);
        }
        KeyCode::Char('a' | 'A') => {
            global.add_draft = Some(GlobalMountDraft::default());
            global.modal = Some(text_modal(GlobalMountTextTarget::AddName, "Mount name", ""));
        }
        KeyCode::Char('s' | 'S') => {
            let action = if has_sensitive_mount(&global.pending) {
                GlobalMountConfirm::Sensitive
            } else {
                GlobalMountConfirm::Save
            };
            global.modal = Some(confirm_modal(action));
        }
        KeyCode::Char('d' | 'D') => {
            if global.pending.is_empty() {
                set_toast(state, "Nothing to remove.", ToastKind::Error);
            } else if let ManagerStage::GlobalMounts(global) = &mut state.stage {
                global.modal = Some(confirm_modal(GlobalMountConfirm::Remove));
            }
        }
        KeyCode::Char('r' | 'R') => {
            if let Some(row) = global.pending.get_mut(global.selected) {
                row.mount.readonly = !row.mount.readonly;
            } else {
                set_toast(state, NO_MOUNT_SELECTED, ToastKind::Error);
            }
        }
        KeyCode::Char('n' | 'N') => open_edit_text(state, GlobalMountTextTarget::Rename),
        KeyCode::Char('1') => open_edit_text(state, GlobalMountTextTarget::Source),
        KeyCode::Char('2') => open_edit_text(state, GlobalMountTextTarget::Destination),
        KeyCode::Char('3') => open_edit_text(state, GlobalMountTextTarget::Scope),
        _ => {}
    }
}

pub(super) fn handle_global_mounts_modal(
    global: &mut super::super::state::GlobalMountsState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) {
    let Some(mut modal) = global.modal.take() else {
        return;
    };
    match &mut modal {
        GlobalMountModal::Text { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => commit_text(global, target, &value),
            ModalOutcome::Cancel => {
                if global.add_draft.take().is_some() {
                    global.error = Some("Add mount cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => global.modal = Some(modal),
        },
        GlobalMountModal::Confirm { action, state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => commit_confirm(global, *action, config, paths),
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
                if matches!(action, GlobalMountConfirm::Sensitive) {
                    global.error = Some("Save aborted: sensitive paths not confirmed.".into());
                }
            }
            ModalOutcome::Continue => global.modal = Some(modal),
        },
    }
}

fn commit_confirm(
    global: &mut super::super::state::GlobalMountsState<'_>,
    action: GlobalMountConfirm,
    config: &mut AppConfig,
    paths: &JackinPaths,
) {
    match action {
        GlobalMountConfirm::Remove => {
            if global.selected < global.pending.len() {
                global.pending.remove(global.selected);
                global.selected = global.selected.min(global.pending.len().saturating_sub(1));
            }
        }
        GlobalMountConfirm::Save => match global.save_to_config(paths) {
            Ok(saved) => {
                *config = saved;
                global.success = Some("Global mounts saved.".into());
                global.exit_requested = true;
            }
            Err(err) => global.error = Some(err.to_string()),
        },
        GlobalMountConfirm::Sensitive => {
            global.modal = Some(confirm_modal(GlobalMountConfirm::Save));
        }
        GlobalMountConfirm::Discard => {
            global.discard();
            global.exit_requested = true;
        }
    }
}

fn commit_text(
    global: &mut super::super::state::GlobalMountsState<'_>,
    target: &GlobalMountTextTarget,
    value: &str,
) {
    let trimmed = value.trim();
    match target {
        GlobalMountTextTarget::AddName => {
            if trimmed.is_empty() {
                global.error = Some(MOUNT_NAME_EMPTY.into());
                global.modal = Some(text_modal(GlobalMountTextTarget::AddName, "Mount name", ""));
                return;
            }
            let Some(draft) = global.add_draft.as_mut() else {
                global.error = Some(ADD_DRAFT_LOST.into());
                return;
            };
            draft.name = trimmed.to_string();
            global.modal = Some(text_modal(GlobalMountTextTarget::AddSource, "Source", ""));
        }
        GlobalMountTextTarget::AddSource => {
            let Some(draft) = global.add_draft.as_mut() else {
                global.error = Some(ADD_DRAFT_LOST.into());
                return;
            };
            draft.src = resolve_path(trimmed);
            global.modal = Some(text_modal(
                GlobalMountTextTarget::AddDestination,
                "Destination",
                "",
            ));
        }
        GlobalMountTextTarget::AddDestination => {
            let Some(draft) = global.add_draft.as_mut() else {
                global.error = Some(ADD_DRAFT_LOST.into());
                return;
            };
            draft.dst = trimmed.to_string();
            global.modal = Some(text_modal(
                GlobalMountTextTarget::AddScope,
                "Scope (empty = global)",
                "",
            ));
        }
        GlobalMountTextTarget::AddScope => {
            let Some(draft) = global.add_draft.take() else {
                global.error = Some(ADD_DRAFT_LOST.into());
                return;
            };
            global.pending.push(crate::config::GlobalMountRow {
                scope: scope_value(trimmed),
                name: draft.name,
                mount: MountConfig {
                    src: draft.src,
                    dst: draft.dst,
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            });
            global.selected = global.pending.len().saturating_sub(1);
        }
        GlobalMountTextTarget::Source => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.mount.src = resolve_path(trimmed);
        }
        GlobalMountTextTarget::Destination => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.mount.dst = trimmed.to_string();
        }
        GlobalMountTextTarget::Scope => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.scope = scope_value(trimmed);
        }
        GlobalMountTextTarget::Rename => {
            if trimmed.is_empty() {
                global.error = Some(MOUNT_NAME_EMPTY.into());
                return;
            }
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.name = trimmed.to_string();
        }
    }
}

fn open_edit_text(state: &mut ManagerState<'_>, target: GlobalMountTextTarget) {
    let ManagerStage::GlobalMounts(global) = &mut state.stage else {
        return;
    };
    let Some(row) = global.pending.get(global.selected) else {
        set_toast(state, NO_MOUNT_SELECTED, ToastKind::Error);
        return;
    };
    let (label, initial) = match target {
        GlobalMountTextTarget::Rename => ("Rename mount", row.name.clone()),
        GlobalMountTextTarget::Source => ("Source", row.mount.src.clone()),
        GlobalMountTextTarget::Destination => ("Destination", row.mount.dst.clone()),
        GlobalMountTextTarget::Scope => (
            "Scope (empty = global)",
            row.scope.clone().unwrap_or_default(),
        ),
        // Add-flow targets are driven by the four-step text wizard, not this entry point.
        GlobalMountTextTarget::AddName
        | GlobalMountTextTarget::AddSource
        | GlobalMountTextTarget::AddDestination
        | GlobalMountTextTarget::AddScope => return,
    };
    global.modal = Some(text_modal(target, label, &initial));
}

/// Promote pending error/success messages to toasts; pop back to the
/// workspace list when the handler set `exit_requested`.
pub(super) fn after_global_mounts_event(state: &mut ManagerState<'_>) {
    let ManagerStage::GlobalMounts(global) = &mut state.stage else {
        return;
    };
    let error = global.error.take();
    let success = global.success.take();
    let exit = std::mem::take(&mut global.exit_requested);
    if let Some(err) = error {
        set_toast(state, &err, ToastKind::Error);
    } else if let Some(msg) = success {
        set_toast(state, &msg, ToastKind::Success);
    }
    if exit {
        state.stage = ManagerStage::List;
    }
}

fn set_toast(state: &mut ManagerState<'_>, msg: &str, kind: ToastKind) {
    state.toast = Some(Toast {
        message: msg.to_string(),
        kind,
        shown_at: std::time::Instant::now(),
    });
}

fn confirm_modal(action: GlobalMountConfirm) -> GlobalMountModal<'static> {
    let prompt = match action {
        GlobalMountConfirm::Save => "Save global mounts to ~/.config/jackin/config.toml?",
        GlobalMountConfirm::Sensitive => "Sensitive global mount path detected. Save anyway?",
        GlobalMountConfirm::Remove => "Remove selected global mount?",
        GlobalMountConfirm::Discard => "Discard unsaved global mount changes?",
    };
    GlobalMountModal::Confirm {
        action,
        state: ConfirmState::new(prompt),
    }
}

fn text_modal(
    target: GlobalMountTextTarget,
    label: &str,
    initial: &str,
) -> GlobalMountModal<'static> {
    GlobalMountModal::Text {
        target,
        state: Box::new(TextInputState::new(label, initial)),
    }
}

fn scope_value(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn has_sensitive_mount(rows: &[crate::config::GlobalMountRow]) -> bool {
    let mounts: Vec<MountConfig> = rows.iter().map(|row| row.mount.clone()).collect();
    !crate::workspace::find_sensitive_mounts(&mounts).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_mount_save_detects_sensitive_sources() {
        let rows = vec![crate::config::GlobalMountRow {
            scope: None,
            name: "ssh".into(),
            mount: MountConfig {
                src: "/home/user/.ssh".into(),
                dst: "/ssh".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        }];

        assert!(has_sensitive_mount(&rows));
    }
}

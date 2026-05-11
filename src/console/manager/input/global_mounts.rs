use crossterm::event::{KeyCode, KeyEvent};

use super::super::state::{
    GlobalMountDraft, GlobalMountModal, GlobalMountTextTarget, ManagerStage, ManagerState, Toast,
    ToastKind,
};
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
            state.stage = ManagerStage::List;
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
            if has_sensitive_mount(&global.pending) {
                global.modal = Some(GlobalMountModal::ConfirmSensitive {
                    state: ConfirmState::new("Sensitive global mount path detected. Save anyway?"),
                });
            } else {
                global.modal = Some(GlobalMountModal::ConfirmSave {
                    state: ConfirmState::new("Save global mounts to ~/.config/jackin/config.toml?"),
                });
            }
        }
        KeyCode::Char('d' | 'D') if !global.pending.is_empty() => {
            global.modal = Some(GlobalMountModal::ConfirmRemove {
                state: ConfirmState::new("Remove selected global mount?"),
            });
        }
        KeyCode::Char('r' | 'R') => {
            if let Some(row) = global.pending.get_mut(global.selected) {
                row.mount.readonly = !row.mount.readonly;
            }
        }
        KeyCode::Char('n' | 'N') => {
            if let Some(row) = global.pending.get(global.selected) {
                global.modal = Some(text_modal(
                    GlobalMountTextTarget::Rename,
                    "Rename mount",
                    &row.name,
                ));
            }
        }
        KeyCode::Char('1') => {
            if let Some(row) = global.pending.get(global.selected) {
                global.modal = Some(text_modal(
                    GlobalMountTextTarget::Source,
                    "Source",
                    &row.mount.src,
                ));
            }
        }
        KeyCode::Char('2') => {
            if let Some(row) = global.pending.get(global.selected) {
                global.modal = Some(text_modal(
                    GlobalMountTextTarget::Destination,
                    "Destination",
                    &row.mount.dst,
                ));
            }
        }
        KeyCode::Char('3') => {
            if let Some(row) = global.pending.get(global.selected) {
                global.modal = Some(text_modal(
                    GlobalMountTextTarget::Scope,
                    "Scope (empty = global)",
                    row.scope.as_deref().unwrap_or(""),
                ));
            }
        }
        _ => {}
    }
    if let ManagerStage::GlobalMounts(global) = &mut state.stage
        && let Some(err) = global.error.take()
    {
        state.toast = Some(Toast {
            message: err,
            kind: ToastKind::Error,
            shown_at: std::time::Instant::now(),
        });
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
            ModalOutcome::Cancel => global.add_draft = None,
            ModalOutcome::Continue => global.modal = Some(modal),
        },
        GlobalMountModal::ConfirmRemove { state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => {
                if global.selected < global.pending.len() {
                    global.pending.remove(global.selected);
                    global.selected = global.selected.min(global.pending.len().saturating_sub(1));
                }
            }
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {}
            ModalOutcome::Continue => global.modal = Some(modal),
        },
        GlobalMountModal::ConfirmSave { state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => match global.save_to_config(paths) {
                Ok(saved) => *config = saved,
                Err(err) => global.error = Some(err.to_string()),
            },
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {}
            ModalOutcome::Continue => global.modal = Some(modal),
        },
        GlobalMountModal::ConfirmSensitive { state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => {
                global.modal = Some(GlobalMountModal::ConfirmSave {
                    state: ConfirmState::new("Save global mounts to ~/.config/jackin/config.toml?"),
                });
            }
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {}
            ModalOutcome::Continue => global.modal = Some(modal),
        },
    }
}

fn commit_text(
    global: &mut super::super::state::GlobalMountsState<'_>,
    target: &GlobalMountTextTarget,
    value: &str,
) {
    match target {
        GlobalMountTextTarget::AddName => {
            if let Some(draft) = global.add_draft.as_mut() {
                draft.name = value.trim().to_string();
                global.modal = Some(text_modal(GlobalMountTextTarget::AddSource, "Source", ""));
            }
        }
        GlobalMountTextTarget::AddSource => {
            if let Some(draft) = global.add_draft.as_mut() {
                draft.src = resolve_path(value.trim());
                global.modal = Some(text_modal(
                    GlobalMountTextTarget::AddDestination,
                    "Destination",
                    "",
                ));
            }
        }
        GlobalMountTextTarget::AddDestination => {
            if let Some(draft) = global.add_draft.as_mut() {
                draft.dst = value.trim().to_string();
                global.modal = Some(text_modal(
                    GlobalMountTextTarget::AddScope,
                    "Scope (empty = global)",
                    "",
                ));
            }
        }
        GlobalMountTextTarget::AddScope => {
            if let Some(draft) = global.add_draft.take() {
                global.pending.push(crate::config::GlobalMountRow {
                    scope: scope_value(value.trim()),
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
        }
        GlobalMountTextTarget::Source => {
            if let Some(row) = global.pending.get_mut(global.selected) {
                row.mount.src = resolve_path(value.trim());
            }
        }
        GlobalMountTextTarget::Destination => {
            if let Some(row) = global.pending.get_mut(global.selected) {
                row.mount.dst = value.trim().to_string();
            }
        }
        GlobalMountTextTarget::Scope => {
            if let Some(row) = global.pending.get_mut(global.selected) {
                row.scope = scope_value(value.trim());
            }
        }
        GlobalMountTextTarget::Rename => {
            if let Some(row) = global.pending.get_mut(global.selected) {
                row.name = value.trim().to_string();
            }
        }
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

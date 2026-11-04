// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{
    ChangeSet, Completion, Effect, FieldDescriptor, FolderProbe, OnePassword, OpAccount, OpError,
    OpField, OpItem, OpSession, OpVault, SecretClass,
};
use jackin_console::domain::instance::AgentState;
use jackin_console::domain::onepassword::OpReference;
use jackin_console::domain::vocabulary::{PaneWords, Unbuilt, Vocabulary};
use jackin_console::panes::{PaneBody, PaneKey, PaneKindTag, PaneNote, PaneSpawn, PasteOutcome};
use jackin_console::world::{Msg, SaveFailure};
use junie_tui::core::event::Key;
use junie_tui::core::id::WidgetId;
use junie_tui::theme::Tone;
use junie_tui::widgets::viewport::TextViewport;
use junie_tui::widgets::viewport_ops::ViewportOps;
use ratatui::layout::Rect;

pub(crate) fn fails(effect: &Effect, words: &Vocabulary) -> Option<Completion> {
    let no = |what: Unbuilt| words.unbuilt(what);
    let save = |what: Unbuilt| SaveFailure {
        error: no(what),
        next: None,
    };
    let msg = match effect {
        Effect::SaveWorkspace { id, .. } => Msg::WorkspaceSaved {
            id: *id,
            result: Err(save(Unbuilt::SaveWorkspace)),
        },
        Effect::SaveGlobal { .. } => Msg::GlobalSaved {
            result: Err(save(Unbuilt::SaveSettings)),
        },
        Effect::SaveAccounts { .. } => Msg::AccountsSaved {
            added: None,
            result: Err(no(Unbuilt::SaveAccounts)),
        },
        Effect::DeleteWorkspace { id } => Msg::WorkspaceDeleted {
            id: *id,
            result: Err(no(Unbuilt::DeleteWorkspace)),
        },
        Effect::Restore { instance } => Msg::Restored {
            instance: instance.clone(),
            result: Err(no(Unbuilt::RestoreSandbox)),
        },
        Effect::TrustRole { role } => Msg::RoleTrusted {
            role: role.clone(),
            result: Err(no(Unbuilt::TrustRole)),
        },
        Effect::Prewarm { workspace } => Msg::Prewarmed {
            workspace: *workspace,
            result: Err(no(Unbuilt::PrewarmImage)),
        },
        Effect::Stop { instance } => Msg::Stopped {
            instance: instance.clone(),
            result: Err(no(Unbuilt::StopSandbox)),
        },
        Effect::Purge { instance } => Msg::Purged {
            instance: instance.clone(),
            result: Err(no(Unbuilt::PurgeSandbox)),
        },
        Effect::RefreshAccount { account, .. } => Msg::AccountRefreshed {
            account: account.clone(),
            result: Err(no(Unbuilt::RefreshUsage)),
        },
        Effect::ValidateAccount { account } => Msg::AccountValidated {
            account: account.clone(),
            result: Err(no(Unbuilt::ValidateAccount)),
        },
        Effect::LoadRole { role } => Msg::RoleLoaded {
            role: role.clone(),
            result: Err(no(Unbuilt::LoadRole)),
        },
        Effect::CheckRole { role, .. } => Msg::RoleChecked {
            role: role.clone(),
            result: Err(no(Unbuilt::CheckRole)),
        },
        Effect::SaveRole { role, .. } => Msg::RoleSaved {
            role: role.clone(),
            result: Err(no(Unbuilt::SaveRole)),
        },
        Effect::BindRole { role, agent, .. } => Msg::RoleBound {
            role: role.clone(),
            agent: *agent,
            result: Err(no(Unbuilt::BindRole)),
        },
        Effect::AddRole { location } => Msg::RoleAdded {
            location: location.clone(),
            result: Err(no(Unbuilt::AddRole)),
        },
        Effect::RemoveRole { role } => Msg::RoleRemoved {
            role: role.clone(),
            result: Err(no(Unbuilt::RemoveRole)),
        },
        Effect::CheckSource { ticket, .. } => Msg::SourceChecked {
            ticket: *ticket,
            result: Err(no(Unbuilt::ValidateAccount)),
        },
        Effect::ProbeFolder { ticket, .. } => Msg::FolderProbed {
            ticket: *ticket,
            result: Ok(FolderProbe::Unreadable),
        },
        Effect::ReadChanges {
            ticket, unpushed, ..
        } => Msg::ChangesRead {
            ticket: *ticket,
            result: Ok(ChangeSet {
                files: Vec::new(),
                unpushed: *unpushed,
            }),
        },
        Effect::LoadOp { ticket, .. } => Msg::OpLoaded {
            ticket: *ticket,
            result: Err(OpError::NotAvailable),
        },
        Effect::Refresh | Effect::RunDoctor => return None,
    };
    Some(Completion::ready(0, msg))
}

#[derive(Debug)]
pub(crate) struct UnbuiltPane {
    viewport: TextViewport,
    note: String,
}

impl UnbuiltPane {
    pub(crate) fn new(spawn: &PaneSpawn, words: &Vocabulary) -> UnbuiltPane {
        let id = WidgetId::of(&spawn.workspace).child(usize::try_from(spawn.id).unwrap_or(0));
        UnbuiltPane {
            viewport: TextViewport::new(id),
            note: words.unbuilt(Unbuilt::Pane),
        }
    }
}

impl PaneBody for UnbuiltPane {
    fn kind(&self) -> PaneKindTag {
        PaneKindTag::Agent
    }

    fn state(&self) -> AgentState {
        AgentState::Unknown
    }

    fn title(&self) -> Option<&str> {
        None
    }

    fn note(&self, _words: &PaneWords) -> Option<PaneNote> {
        Some(PaneNote {
            text: self.note.clone(),
            tone: Tone::Error,
        })
    }

    fn viewport(&self) -> &dyn ViewportOps {
        &self.viewport
    }

    fn viewport_mut(&mut self) -> &mut dyn ViewportOps {
        &mut self.viewport
    }

    fn key(&mut self, _key: &Key, _now_ms: i64) -> PaneKey {
        PaneKey::Consumed
    }

    fn paste(&mut self, _text: &str, _now_ms: i64) -> PasteOutcome {
        PasteOutcome::Sent
    }

    fn tick(&mut self, _now_ms: i64) -> bool {
        false
    }

    fn sync_size(&mut self, inner: Rect) {
        self.viewport.set_area(inner);
    }

    fn focus_changed(&mut self, _focused: bool, _now_ms: i64) {}

    fn redraw(&mut self) {}

    fn clear(&mut self) {}

    fn touched(&self) -> &[String] {
        &[]
    }

    fn sends_prefix(&self) -> bool {
        false
    }

    fn close_needs_confirm(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SignedInOnly;

impl OnePassword for SignedInOnly {
    fn session(&self) -> OpSession {
        OpSession::SignedIn
    }

    fn set_session(&mut self, _session: OpSession) {}

    fn accounts(&self) -> &[OpAccount] {
        &[]
    }

    fn latency_ms(&self) -> i64 {
        0
    }

    fn list_accounts(&self) -> Result<Vec<&OpAccount>, OpError> {
        Ok(Vec::new())
    }

    fn list_vaults(&self, account: &str) -> Result<Vec<&OpVault>, OpError> {
        Err(OpError::MissingAccount {
            account: account.to_owned(),
        })
    }

    fn list_items(&self, _account: &str, vault: &str) -> Result<Vec<&OpItem>, OpError> {
        Err(OpError::MissingVault {
            vault: vault.to_owned(),
        })
    }

    fn list_fields(
        &self,
        _account: &str,
        vault: &str,
        item: &str,
    ) -> Result<Vec<&OpField>, OpError> {
        Err(OpError::MissingItem {
            item: item.to_owned(),
            vault: vault.to_owned(),
        })
    }

    fn reference(
        &self,
        _account: &str,
        vault: &str,
        item: &str,
        _field: &str,
    ) -> Result<OpReference, OpError> {
        Err(OpError::MissingItem {
            item: item.to_owned(),
            vault: vault.to_owned(),
        })
    }

    fn describe(&self, reference: &OpReference) -> Result<FieldDescriptor, OpError> {
        Err(OpError::MissingField {
            field: reference.field_label.clone(),
            item: reference.item_title.clone(),
        })
    }

    fn endpoint_of(&self, _reference: &OpReference) -> Option<String> {
        None
    }

    fn classify(&self, reference: &OpReference) -> Result<SecretClass, OpError> {
        Err(OpError::MissingField {
            field: reference.field_label.clone(),
            item: reference.item_title.clone(),
        })
    }

    fn box_clone(&self) -> Box<dyn OnePassword> {
        Box::new(self.clone())
    }
}

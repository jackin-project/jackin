// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use jackin_console::backend::{
    ChangeSet, CheckRow, Completion, Effect, FieldDescriptor, FolderProbe, OnePassword, OpAccount,
    OpError, OpField, OpItem, OpSession, OpVault, SecretClass, ValidationOutcome,
};
use jackin_console::domain::account::{
    AccountIdentity, Confidence, IssueCode, Lifecycle, Recoverability, RecoverableIssue,
};
use jackin_console::domain::instance::{AgentState, Instance, InstanceStatus};
use jackin_console::domain::launch::LaunchFailure;
use jackin_console::domain::onepassword::OpReference;
use jackin_console::domain::openshell as normalize;
use jackin_console::launch::{FailedLaunch, LaunchEvent, LaunchFeed, LaunchTarget, TypedKey};
use jackin_console::panes::{PaneBody, PaneKey, PaneKindTag, PaneNote, PaneSpawn, PasteOutcome};
use jackin_console::world::{Msg, World};
use jackin_openshell::GatewayError;
use junie_tui::core::event::Key;
use junie_tui::core::id::WidgetId;
use junie_tui::theme::Tone;
use junie_tui::widgets::viewport::TextViewport;
use junie_tui::widgets::viewport_ops::ViewportOps;
use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unbuilt {
    SaveWorkspace,
    SaveSettings,
    SaveAccounts,
    DeleteWorkspace,
    RestoreSandbox,
    TrustRole,
    PrewarmImage,
    StopSandbox,
    PurgeSandbox,
    RefreshUsage,
    ValidateAccount,
    LoadRole,
    CheckRole,
    SaveRole,
    BindRole,
    AddRole,
    RemoveRole,
    Launch,
    Pane,
}

impl fmt::Display for Unbuilt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            Unbuilt::SaveWorkspace => "Saving a workspace",
            Unbuilt::SaveSettings => "Saving settings",
            Unbuilt::SaveAccounts => "Saving accounts",
            Unbuilt::DeleteWorkspace => "Deleting a workspace",
            Unbuilt::RestoreSandbox => "Restoring a sandbox",
            Unbuilt::TrustRole => "Trusting a role source",
            Unbuilt::PrewarmImage => "Building an image",
            Unbuilt::StopSandbox => "Stopping a sandbox",
            Unbuilt::PurgeSandbox => "Purging a sandbox",
            Unbuilt::RefreshUsage => "Reading usage",
            Unbuilt::ValidateAccount => "Validating an account",
            Unbuilt::LoadRole => "Loading a role",
            Unbuilt::CheckRole => "Checking a role",
            Unbuilt::SaveRole => "Saving a role",
            Unbuilt::BindRole => "Binding a role",
            Unbuilt::AddRole => "Adding a role",
            Unbuilt::RemoveRole => "Removing a role",
            Unbuilt::Launch => "Launching a sandbox",
            Unbuilt::Pane => "A terminal pane",
        };
        write!(f, "{what} is not available in this build yet")
    }
}

impl Unbuilt {
    fn text(self) -> String {
        self.to_string()
    }
}

pub(crate) fn fails(effect: &Effect) -> Option<Completion> {
    let msg = match effect {
        Effect::SaveWorkspace { id, .. } => Msg::WorkspaceSaved {
            id: *id,
            result: Err(Unbuilt::SaveWorkspace.text()),
        },
        Effect::SaveGlobal { .. } => Msg::GlobalSaved {
            result: Err(Unbuilt::SaveSettings.text()),
        },
        Effect::SaveAccounts { .. } => Msg::AccountsSaved {
            result: Err(Unbuilt::SaveAccounts.text()),
        },
        Effect::DeleteWorkspace { id } => Msg::WorkspaceDeleted {
            id: *id,
            result: Err(Unbuilt::DeleteWorkspace.text()),
        },
        Effect::Restore { instance } => Msg::Restored {
            instance: instance.clone(),
            result: Err(Unbuilt::RestoreSandbox.text()),
        },
        Effect::TrustRole { role } => Msg::RoleTrusted {
            role: role.clone(),
            result: Err(Unbuilt::TrustRole.text()),
        },
        Effect::Prewarm { workspace } => Msg::Prewarmed {
            workspace: *workspace,
            result: Err(Unbuilt::PrewarmImage.text()),
        },
        Effect::Stop { instance } => Msg::Stopped {
            instance: instance.clone(),
            result: Err(Unbuilt::StopSandbox.text()),
        },
        Effect::Purge { instance } => Msg::Purged {
            instance: instance.clone(),
            result: Err(Unbuilt::PurgeSandbox.text()),
        },
        Effect::RefreshAccount { account, .. } => Msg::AccountRefreshed {
            account: account.clone(),
            result: Err(Unbuilt::RefreshUsage.text()),
        },
        Effect::ValidateAccount { account } => Msg::AccountValidated {
            account: account.clone(),
            result: Err(Unbuilt::ValidateAccount.text()),
        },
        Effect::LoadRole { role } => Msg::RoleLoaded {
            role: role.clone(),
            result: Err(Unbuilt::LoadRole.text()),
        },
        Effect::CheckRole { role, .. } => Msg::RoleChecked {
            role: role.clone(),
            result: Err(Unbuilt::CheckRole.text()),
        },
        Effect::SaveRole { role, .. } => Msg::RoleSaved {
            role: role.clone(),
            result: Err(Unbuilt::SaveRole.text()),
        },
        Effect::BindRole { role, agent, .. } => Msg::RoleBound {
            role: role.clone(),
            agent: *agent,
            result: Err(Unbuilt::BindRole.text()),
        },
        Effect::AddRole { location } => Msg::RoleAdded {
            location: location.clone(),
            result: Err(Unbuilt::AddRole.text()),
        },
        Effect::RemoveRole { role } => Msg::RoleRemoved {
            role: role.clone(),
            result: Err(Unbuilt::RemoveRole.text()),
        },
        Effect::Refresh | Effect::RunDoctor => return None,
    };
    Some(Completion::ready(0, msg))
}

pub(crate) fn validation() -> ValidationOutcome {
    let text = Unbuilt::ValidateAccount.text();
    ValidationOutcome {
        level: None,
        identity: AccountIdentity::default(),
        confidence: Confidence::None,
        lifecycle: Lifecycle::Unavailable,
        issue: Some(RecoverableIssue::new(
            IssueCode::ProviderUnavailable,
            text.clone(),
            Recoverability::ActionRequired,
        )),
        usage: None,
        material: CheckRow::Skipped(text.clone()),
        identity_row: CheckRow::Skipped(text.clone()),
        quota_row: CheckRow::Failed(text),
    }
}

pub(crate) fn folder_probe() -> FolderProbe {
    FolderProbe::Unreadable
}

pub(crate) fn changes(unpushed: usize) -> ChangeSet {
    ChangeSet {
        files: Vec::new(),
        unpushed,
    }
}

pub(crate) fn launch_failure() -> LaunchFailure {
    LaunchFailure {
        stage: 0,
        summary: Unbuilt::Launch.text(),
        next_step: String::new(),
        detail: Vec::new(),
    }
}

pub(crate) fn gateway_failure(base: &LaunchFailure, error: &GatewayError) -> LaunchFailure {
    LaunchFailure {
        detail: vec![error.to_string()],
        ..base.clone()
    }
}

#[derive(Debug)]
pub(crate) struct UnbuiltLaunch {
    run_id: String,
    failed: Option<Box<FailedLaunch>>,
}

impl UnbuiltLaunch {
    pub(crate) fn new(
        world: &World,
        target: &LaunchTarget,
        failure: LaunchFailure,
    ) -> UnbuiltLaunch {
        let workspace = target.workspace.and_then(|id| world.workspace(id));
        let run_id = world.now_secs().to_string();
        let instance = Instance {
            id: run_id.clone(),
            container: workspace.map(|w| w.name.clone()).unwrap_or_default(),
            workspace: target.workspace,
            workdir: String::new(),
            role: target.role.clone(),
            agent: target.agent,
            status: InstanceStatus::FailedSetup,
            created_secs: world.now_secs(),
            last_seen_secs: world.now_secs(),
            run_id: run_id.clone(),
            sessions: Ok(Vec::new()),
            daemon: jackin_console::domain::instance::DaemonSnapshot::NoTabs,
            branch: None,
            pr: None,
            default_branch: String::new(),
            github: None,
            uncommitted: 0,
            unpushed: 0,
            account: target.account.clone(),
            accounts: target.account.iter().cloned().collect(),
        };
        let mut instance = instance;
        normalize::instance(&mut instance);
        UnbuiltLaunch {
            run_id,
            failed: Some(Box::new(FailedLaunch { failure, instance })),
        }
    }
}

impl LaunchFeed for UnbuiltLaunch {
    fn run_id(&self) -> &str {
        &self.run_id
    }

    fn poll(&mut self, _world: &World) -> Vec<LaunchEvent> {
        self.failed
            .take()
            .map(LaunchEvent::Failed)
            .into_iter()
            .collect()
    }

    fn retry_credentials(&mut self, _op: &mut dyn OnePassword) {}

    fn use_plain_text(&mut self) -> Option<TypedKey> {
        None
    }

    fn cancel(&mut self, _world: &World) -> Option<Instance> {
        None
    }
}

#[derive(Debug)]
pub(crate) struct UnbuiltPane {
    viewport: TextViewport,
}

impl UnbuiltPane {
    pub(crate) fn new(spawn: &PaneSpawn) -> UnbuiltPane {
        let id = WidgetId::of(&spawn.workspace).child(usize::try_from(spawn.id).unwrap_or(0));
        UnbuiltPane {
            viewport: TextViewport::new(id),
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

    fn note(&self) -> Option<PaneNote> {
        Some(PaneNote {
            text: Unbuilt::Pane.text(),
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

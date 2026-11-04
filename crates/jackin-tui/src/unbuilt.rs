// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{
    ChangeSet, Completion, Effect, FieldDescriptor, FolderProbe, OnePassword, OpAccount, OpError,
    OpField, OpItem, OpSession, OpVault, SecretClass,
};
use jackin_console::domain::instance::Instance;
use jackin_console::domain::launch::LaunchFailure;
use jackin_console::domain::onepassword::OpReference;
use jackin_console::domain::vocabulary::{Unbuilt, Vocabulary};
use jackin_console::launch::{EnvAnswer, FailedLaunch, LaunchEvent, LaunchFeed, TypedKey};
use jackin_console::world::{Msg, World};
use jackin_openshell::GatewayError;

pub(crate) fn fails(effect: &Effect, words: &Vocabulary) -> Option<Completion> {
    let no = |what: Unbuilt| words.unbuilt(what);
    let msg = match effect {
        Effect::Restore { instance } => Msg::Restored {
            instance: instance.clone(),
            result: Err(no(Unbuilt::RestoreSandbox)),
        },
        Effect::Prewarm { workspace } => Msg::Prewarmed {
            workspace: *workspace,
            result: Err(no(Unbuilt::PrewarmImage)),
        },
        Effect::RefreshAccount { account, .. } => Msg::AccountRefreshed {
            account: account.clone(),
            result: Err(no(Unbuilt::RefreshUsage)),
        },
        Effect::ValidateAccount { account } => Msg::AccountValidated {
            account: account.clone(),
            result: Err(no(Unbuilt::ValidateAccount)),
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
        _ => return None,
    };
    Some(Completion::ready(0, msg))
}

pub(crate) fn launch_failure(words: &Vocabulary) -> LaunchFailure {
    LaunchFailure {
        stage: 0,
        summary: words.unbuilt(Unbuilt::Launch),
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
    pub(crate) fn new(world: &World, failure: LaunchFailure) -> UnbuiltLaunch {
        UnbuiltLaunch {
            run_id: world.now_secs().to_string(),
            failed: Some(Box::new(FailedLaunch {
                failure,
                instance: None,
            })),
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

    fn answer_env(&mut self, _answers: Vec<EnvAnswer>) {}

    fn cancel(&mut self, _world: &World) -> Option<Instance> {
        None
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

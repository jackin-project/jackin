// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{Effect, OpError, Ticket};
use jackin_console::domain::doctor::DoctorFailure;
use jackin_console::world::{Msg, SaveFailure};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("the work stopped unexpectedly")]
pub(crate) struct Stopped;

fn stopped() -> String {
    Stopped.to_string()
}

pub(crate) fn refresh(ticket: Ticket) -> Msg {
    Msg::Refreshed {
        ticket,
        result: Err(stopped()),
    }
}

pub(crate) fn doctor() -> Msg {
    Msg::DoctorFinished(Err(DoctorFailure::Failed(stopped())))
}

pub(crate) fn reply(effect: &Effect) -> Msg {
    let error = stopped();
    match effect {
        Effect::SaveWorkspace { .. } => Msg::WorkspaceSaved {
            id: None,
            result: Err(SaveFailure { error, next: None }),
        },
        Effect::SaveGlobal { .. } => Msg::GlobalSaved {
            result: Err(SaveFailure { error, next: None }),
        },
        Effect::SaveAccounts { ticket, .. } => Msg::AccountsSaved {
            ticket: *ticket,
            added: None,
            result: Err(error),
        },
        Effect::DeleteWorkspace { id } => Msg::WorkspaceDeleted {
            id: *id,
            result: Err(error),
        },
        Effect::Stop { instance } => Msg::Stopped {
            instance: instance.clone(),
            result: Err(error),
        },
        Effect::PowerStart { instance } => Msg::PowerStarted {
            instance: instance.clone(),
            result: Err(error),
        },
        Effect::PowerStop { instance } => Msg::PowerStopped {
            instance: instance.clone(),
            result: Err(error),
        },
        Effect::Purge { instance } => Msg::Purged {
            instance: instance.clone(),
            result: Err(error),
        },
        Effect::TrustRole { role } => Msg::RoleTrusted {
            role: role.clone(),
            result: Err(error),
        },
        Effect::LoadRole { role } => Msg::RoleLoaded {
            role: role.clone(),
            result: Err(error),
        },
        Effect::CheckRole { role, ticket, .. } => Msg::RoleChecked {
            ticket: *ticket,
            role: role.clone(),
            result: Err(error),
        },
        Effect::SaveRole { role, .. } => Msg::RoleSaved {
            role: role.clone(),
            result: Err(error),
        },
        Effect::BindRole { role, agent, .. } => Msg::RoleBound {
            role: role.clone(),
            agent: *agent,
            result: Err(error),
        },
        Effect::AddRole { location } => Msg::RoleAdded {
            location: location.clone(),
            result: Err(error),
        },
        Effect::RemoveRole { role } => Msg::RoleRemoved {
            role: role.clone(),
            result: Err(error),
        },
        Effect::RefreshAccount {
            account, source, ..
        } => Msg::AccountRefreshed {
            account: account.clone(),
            source: Box::new(source.clone()),
            result: Err(error),
        },
        Effect::ValidateAccount { account, source } => Msg::AccountValidated {
            account: account.clone(),
            source: Box::new(source.clone()),
            result: Err(error),
        },
        Effect::CheckSource { ticket, .. } => Msg::SourceChecked {
            ticket: *ticket,
            result: Err(error),
        },
        Effect::ProbeFolder { ticket, .. } => Msg::FolderProbed {
            ticket: *ticket,
            result: Err(error),
        },
        Effect::ReadChanges { ticket, .. } => Msg::ChangesRead {
            ticket: *ticket,
            result: Err(error),
        },
        Effect::LoadOp { ticket, .. } => Msg::OpLoaded {
            ticket: *ticket,
            result: Err(OpError::NotAvailable),
        },
        Effect::Refresh { ticket } => refresh(*ticket),
        Effect::RunDoctor => doctor(),
        Effect::Restore { instance } => Msg::Restored {
            instance: instance.clone(),
            result: Err(error),
        },
        Effect::Prewarm { workspace } => Msg::Prewarmed {
            workspace: *workspace,
            result: Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use jackin_console::backend::OpScope;
    use jackin_console::domain::account::{CredentialSource, DetectedKind};
    use jackin_console::domain::role::RoleManifestDoc;

    use super::*;

    #[test]
    fn a_ticketed_effect_gets_its_ticket_back_with_a_failure() {
        let load = Effect::LoadOp {
            ticket: 7,
            scope: OpScope::Accounts,
        };
        assert_eq!(
            reply(&load),
            Msg::OpLoaded {
                ticket: 7,
                result: Err(OpError::NotAvailable),
            }
        );
        let probe = Effect::ProbeFolder {
            ticket: 8,
            path: "~/code".to_owned(),
        };
        assert_eq!(
            reply(&probe),
            Msg::FolderProbed {
                ticket: 8,
                result: Err(Stopped.to_string()),
            }
        );
        assert_eq!(reply(&Effect::Refresh { ticket: 9 }), refresh(9));
    }

    #[test]
    fn a_crashed_reply_echoes_the_ticket_and_the_request_sources() {
        let check = Effect::CheckRole {
            ticket: 11,
            role: "agent-smith".to_owned(),
            doc: RoleManifestDoc {
                name: "agent-smith".to_owned(),
                dockerfile: String::new(),
                policy: String::new(),
                codex_model: None,
                docker: false,
                hook_setup: None,
                hook_preflight: None,
                env: Vec::new(),
            },
        };
        assert_eq!(
            reply(&check),
            Msg::RoleChecked {
                ticket: 11,
                role: "agent-smith".to_owned(),
                result: Err(Stopped.to_string()),
            }
        );
        let source = CredentialSource::HostEnv {
            var: "OPENAI_API_KEY".to_owned(),
            detected: DetectedKind::CodexAuthJson,
        };
        let validate = Effect::ValidateAccount {
            account: "personal".to_owned(),
            source: source.clone(),
        };
        assert_eq!(
            reply(&validate),
            Msg::AccountValidated {
                account: "personal".to_owned(),
                source: Box::new(source.clone()),
                result: Err(Stopped.to_string()),
            }
        );
        let refresh = Effect::RefreshAccount {
            account: "personal".to_owned(),
            source: source.clone(),
            stagger_ms: 250,
        };
        assert_eq!(
            reply(&refresh),
            Msg::AccountRefreshed {
                account: "personal".to_owned(),
                source: Box::new(source),
                result: Err(Stopped.to_string()),
            }
        );
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{
    CheckRow, FolderProbe, OpError, PlainKey, Ticket, ValidationOutcome,
};
use jackin_console::domain::account::{
    AccountIdentity, Confidence, CredentialSource, DetectedKind, IssueCode, Lifecycle,
    Recoverability, RecoverableIssue, ValidationLevel,
};
use jackin_console::domain::agent::Provider;
use jackin_console::domain::reason::{Found, IssueReason};
use jackin_console::domain::vocabulary::{Rejection, SaveField};
use jackin_console::world::Msg;
use jackin_core::HostPath;
use jackin_session::{
    AUTH_FILE, FolderFacts, Source, SourceDesk, SourceFault, SourceFound, error_chain,
};

use crate::bridge::Settled;
use crate::observe::Inputs;
use crate::onepassword::op_error;
use crate::persist::sentence;
use crate::project::core_provider;

struct Failure {
    code: IssueCode,
    lifecycle: Lifecycle,
    recoverability: Recoverability,
    reason: IssueReason,
}

fn failure(
    code: IssueCode,
    lifecycle: Lifecycle,
    recoverability: Recoverability,
    reason: IssueReason,
) -> Failure {
    Failure {
        code,
        lifecycle,
        recoverability,
        reason,
    }
}

fn op_failure(error: OpError) -> Failure {
    let (code, recoverability) = match &error {
        OpError::Locked => (IssueCode::OpLocked, Recoverability::Retryable),
        OpError::NotAvailable => (IssueCode::ProviderUnavailable, Recoverability::Retryable),
        OpError::AuthorizationRequired { .. } => (
            IssueCode::OpAuthorizationRequired,
            Recoverability::ActionRequired,
        ),
        OpError::PermissionDenied { .. } => (
            IssueCode::OpPermissionDenied,
            Recoverability::ActionRequired,
        ),
        OpError::MissingAccount { .. }
        | OpError::MissingVault { .. }
        | OpError::MissingItem { .. } => (IssueCode::OpItemMissing, Recoverability::ActionRequired),
        OpError::MissingField { .. } | OpError::WrongFieldShape { .. } => {
            (IssueCode::OpFieldMissing, Recoverability::ActionRequired)
        }
        OpError::EmptyMaterial { .. } => (IssueCode::ApiKeyEmpty, Recoverability::ActionRequired),
    };
    failure(
        code,
        Lifecycle::NeedsSecret,
        recoverability,
        IssueReason::OnePassword(error),
    )
}

fn failure_of(source: &CredentialSource, fault: &SourceFault) -> Failure {
    let action = Recoverability::ActionRequired;
    let chosen = source.safe_detail();
    let login_invalid = || IssueReason::LoginInvalid {
        file: AUTH_FILE.to_owned(),
    };
    let unreadable = |path: String| IssueReason::FolderUnreadable {
        path,
        detail: error_chain(fault),
    };
    match fault {
        SourceFault::EnvName(name) => failure(
            IssueCode::CredentialFileMissing,
            Lifecycle::NeedsSecret,
            action,
            IssueReason::EnvInvalid { name: name.clone() },
        ),
        SourceFault::EnvUnset(name) => failure(
            IssueCode::CredentialFileMissing,
            Lifecycle::NeedsSecret,
            action,
            IssueReason::EnvMissing { name: name.clone() },
        ),
        SourceFault::EnvEmpty(name) => failure(
            IssueCode::ApiKeyEmpty,
            Lifecycle::NeedsSecret,
            action,
            IssueReason::EnvEmpty { name: name.clone() },
        ),
        SourceFault::FileMissing(path) => failure(
            IssueCode::CredentialFileMissing,
            Lifecycle::NeedsSecret,
            action,
            IssueReason::FileMissing {
                path: path.to_string(),
            },
        ),
        SourceFault::FileNotRegular(path) => failure(
            IssueCode::CredentialMalformed,
            Lifecycle::Error,
            action,
            IssueReason::FileNotRegular {
                path: path.to_string(),
            },
        ),
        SourceFault::Unreadable(path) => failure(
            IssueCode::FolderUnreadable,
            Lifecycle::Error,
            action,
            unreadable(path.to_string()),
        ),
        SourceFault::Login(_) | SourceFault::Fs(_) => failure(
            IssueCode::CredentialMalformed,
            Lifecycle::Error,
            action,
            unreadable(chosen),
        ),
        SourceFault::LoginInvalid(_) => failure(
            IssueCode::CredentialMalformed,
            Lifecycle::Error,
            action,
            login_invalid(),
        ),
        SourceFault::NotChatgpt(path) => failure(
            IssueCode::FolderWrongProvider,
            Lifecycle::Error,
            action,
            IssueReason::LoginMissing {
                path: path.to_string(),
                file: format!("ChatGPT login in {AUTH_FILE}"),
            },
        ),
        SourceFault::FolderMissing(path) => failure(
            IssueCode::FolderMissing,
            Lifecycle::Error,
            action,
            IssueReason::FolderMissing {
                path: path.to_string(),
            },
        ),
        SourceFault::NoLogin(path) => failure(
            IssueCode::CredentialFileMissing,
            Lifecycle::NeedsLogin,
            action,
            IssueReason::LoginMissing {
                path: path.to_string(),
                file: AUTH_FILE.to_owned(),
            },
        ),
        SourceFault::Reference(reference) => failure(
            IssueCode::OpFieldMissing,
            Lifecycle::NeedsSecret,
            action,
            IssueReason::BadReference {
                reference: reference.clone(),
            },
        ),
        SourceFault::Op(fault) => op_failure(op_error(fault)),
        SourceFault::NotConcealed(field) => op_failure(OpError::WrongFieldShape {
            field: field.clone(),
        }),
        SourceFault::EmptyField(field) => op_failure(OpError::EmptyMaterial {
            field: field.clone(),
        }),
    }
}

pub(crate) fn found_of(source: &CredentialSource) -> Found {
    match source {
        CredentialSource::HostEnv { var, .. } => Found::EnvSet { name: var.clone() },
        CredentialSource::LocalFolder { path, .. } => Found::FolderFound { path: path.clone() },
        CredentialSource::OnePassword(_) => Found::ReferenceResolved,
        CredentialSource::PlainApiKey { .. } => Found::KeyPresent,
    }
}

fn found_material(source: &CredentialSource, found: &SourceFound) -> Found {
    match found {
        SourceFound::Login { account } => Found::Login {
            account: account.to_string(),
        },
        SourceFound::Env | SourceFound::File | SourceFound::Folder | SourceFound::OnePassword => {
            found_of(source)
        }
    }
}

pub(crate) fn skipped() -> CheckRow {
    CheckRow::Skipped(IssueReason::Skipped)
}

pub(crate) fn outcome(
    source: &CredentialSource,
    checked: &Result<SourceFound, SourceFault>,
) -> ValidationOutcome {
    match checked {
        Ok(found) => ValidationOutcome {
            level: Some(ValidationLevel::MaterialDiscovered),
            identity: AccountIdentity::default(),
            confidence: Confidence::PresenceOnly,
            lifecycle: Lifecycle::Available,
            issue: None,
            usage: None,
            material: CheckRow::Ok(found_material(source, found)),
            identity_row: skipped(),
            quota_row: skipped(),
        },
        Err(fault) => {
            let Failure {
                code,
                lifecycle,
                recoverability,
                reason,
            } = failure_of(source, fault);
            ValidationOutcome {
                level: None,
                identity: AccountIdentity::default(),
                confidence: Confidence::None,
                lifecycle,
                issue: Some(RecoverableIssue::new(code, reason.clone(), recoverability)),
                usage: None,
                material: CheckRow::Failed(reason),
                identity_row: skipped(),
                quota_row: skipped(),
            }
        }
    }
}

pub async fn folder_probe(sources: &SourceDesk, folder: &HostPath) -> FolderProbe {
    match sources.probe_folder(folder).await {
        FolderFacts::Missing => FolderProbe::Missing,
        FolderFacts::Unreadable => FolderProbe::Unreadable,
        FolderFacts::NoCredential => FolderProbe::NoCredential,
        FolderFacts::Malformed => FolderProbe::Malformed,
        FolderFacts::Unrecognised => FolderProbe::Found(DetectedKind::Unknown),
        FolderFacts::CodexLogin => FolderProbe::Found(DetectedKind::CodexAuthJson),
    }
}

impl Inputs {
    pub(crate) async fn check_source(
        &self,
        ticket: Ticket,
        provider: Provider,
        source: &CredentialSource,
        _key: Option<&PlainKey>,
    ) -> Settled {
        let result = self.source_checked(provider, source).await;
        Msg::SourceChecked { ticket, result }.into()
    }

    async fn source_checked(
        &self,
        provider: Provider,
        source: &CredentialSource,
    ) -> Result<Box<ValidationOutcome>, String> {
        let reference;
        let folder;
        let checked = match source {
            CredentialSource::PlainApiKey { .. } => {
                return Err(sentence(&Rejection::CannotSave(SaveField::ApiKey)));
            }
            CredentialSource::HostEnv { var, .. } => {
                self.sources.check(Source::Env { var }, &self.cancel).await
            }
            CredentialSource::LocalFolder { path, .. } => {
                folder = match self.expanded(path) {
                    Ok(folder) => folder,
                    Err(error) => return Err(error_chain(&error)),
                };
                self.sources
                    .check(
                        Source::Folder {
                            path: &folder,
                            login: core_provider(provider) == jackin_core::Provider::Codex,
                        },
                        &self.cancel,
                    )
                    .await
            }
            CredentialSource::OnePassword(found) => {
                reference = found.canonical();
                self.sources
                    .check(
                        Source::OnePassword {
                            account: &found.account,
                            reference: &reference,
                        },
                        &self.cancel,
                    )
                    .await
            }
        };
        Ok(Box::new(outcome(source, &checked)))
    }

    pub(crate) async fn probe_folder(&self, ticket: Ticket, path: &str) -> Settled {
        let probe = match self.expanded(path) {
            Ok(folder) => folder_probe(&self.sources, &folder).await,
            Err(_) => FolderProbe::Missing,
        };
        Msg::FolderProbed {
            ticket,
            result: Ok(probe),
        }
        .into()
    }
}

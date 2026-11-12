// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{
    ChatgptAccountId, CredentialFileProblem, EnvName, EnvSnapshot, FileKind, FileSystem, FsError,
    HostPath, OpReference, plain, read_credential_file,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::credentials::{CredentialError, read_chatgpt_login};
use crate::op::{OnePasswordCli, OpFault, OpFieldInfo, OpFieldKind};

pub const AUTH_FILE: &str = "auth.json";
pub const CHATGPT_LOGIN: &str = "ChatGPT login in auth.json";
const OP_SCHEME: &str = "op://";

#[derive(Clone, Copy, Debug)]
pub enum Source<'a> {
    Env {
        var: &'a str,
    },
    File {
        path: &'a HostPath,
    },
    Folder {
        path: &'a HostPath,
        login: bool,
    },
    OnePassword {
        account: &'a str,
        reference: &'a str,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceFound {
    Env,
    File,
    Folder,
    Login { account: ChatgptAccountId },
    OnePassword,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderFacts {
    Missing,
    Unreadable,
    NoCredential,
    Malformed,
    Unrecognised,
    CodexLogin,
}

#[derive(Debug, Error)]
pub enum SourceFault {
    #[error("{} is not a valid environment variable name", plain(.0))]
    EnvName(String),
    #[error("environment variable {} is not set", plain(.0))]
    EnvUnset(String),
    #[error("environment variable {} is empty", plain(.0))]
    EnvEmpty(String),
    #[error("no file at {0}")]
    FileMissing(HostPath),
    #[error("{0} is not a regular file")]
    FileNotRegular(HostPath),
    #[error("{0} is larger than the credential file limit")]
    FileTooLarge(HostPath),
    #[error("cannot read {0}")]
    Unreadable(HostPath),
    #[error("no folder at {0}")]
    FolderMissing(HostPath),
    #[error("{0} has no auth.json")]
    NoLogin(HostPath),
    #[error("the auth.json in {0} is not valid")]
    LoginInvalid(HostPath),
    #[error("the auth.json in {0} is not a ChatGPT login")]
    NotChatgpt(HostPath),
    #[error("{} is not a 1Password field reference", plain(.0))]
    Reference(String),
    #[error(transparent)]
    Op(OpFault),
    #[error("1Password field {} is not concealed", plain(.0))]
    NotConcealed(String),
    #[error("1Password field {} is empty", plain(.0))]
    EmptyField(String),
    #[error(transparent)]
    Login(CredentialError),
    #[error(transparent)]
    Fs(FsError),
}

#[derive(Clone, Debug)]
pub struct SourceDesk {
    fs: Arc<dyn FileSystem>,
    env: EnvSnapshot,
    op: OnePasswordCli,
}

struct FieldPath<'a> {
    vault: &'a str,
    item: &'a str,
    section: Option<&'a str>,
    field: &'a str,
}

fn field_path(reference: &str) -> Result<FieldPath<'_>, SourceFault> {
    let bad = || SourceFault::Reference(reference.to_owned());
    OpReference::parse(reference).map_err(|_| bad())?;
    let rest = reference.strip_prefix(OP_SCHEME).ok_or_else(bad)?;
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.iter().any(|segment| segment.starts_with('-')) {
        return Err(bad());
    }
    match segments.as_slice() {
        [vault, item, field] => Ok(FieldPath {
            vault,
            item,
            section: None,
            field,
        }),
        [vault, item, section, field] => Ok(FieldPath {
            vault,
            item,
            section: Some(section),
            field,
        }),
        _ => Err(bad()),
    }
}

impl SourceDesk {
    pub fn new(fs: Arc<dyn FileSystem>, env: EnvSnapshot, op: OnePasswordCli) -> SourceDesk {
        SourceDesk { fs, env, op }
    }

    pub async fn check(
        &self,
        source: Source<'_>,
        cancel: &CancellationToken,
    ) -> Result<SourceFound, SourceFault> {
        match source {
            Source::Env { var } => self.env(var),
            Source::File { path } => self.file(path).await,
            Source::Folder { path, login } => self.folder(path, login).await,
            Source::OnePassword { account, reference } => {
                self.reference(account, reference, cancel).await
            }
        }
    }

    fn env(&self, var: &str) -> Result<SourceFound, SourceFault> {
        let name = EnvName::parse(var).map_err(|_| SourceFault::EnvName(var.to_owned()))?;
        match self.env.get(&name) {
            None => Err(SourceFault::EnvUnset(var.to_owned())),
            Some(value) if value.trim().is_empty() => Err(SourceFault::EnvEmpty(var.to_owned())),
            Some(_) => Ok(SourceFound::Env),
        }
    }

    async fn file(&self, path: &HostPath) -> Result<SourceFound, SourceFault> {
        match read_credential_file(self.fs.as_ref(), path)
            .await
            .map_err(SourceFault::Fs)?
        {
            Ok(_) => Ok(SourceFound::File),
            Err(CredentialFileProblem::Missing) => Err(SourceFault::FileMissing(path.clone())),
            Err(CredentialFileProblem::NotARegularFile) => {
                Err(SourceFault::FileNotRegular(path.clone()))
            }
            Err(CredentialFileProblem::TooLarge) => Err(SourceFault::FileTooLarge(path.clone())),
        }
    }

    async fn folder(&self, path: &HostPath, login: bool) -> Result<SourceFound, SourceFault> {
        let facts = self.probe_folder(path).await;
        match facts {
            FolderFacts::Missing => Err(SourceFault::FolderMissing(path.clone())),
            FolderFacts::Unreadable => Err(SourceFault::Unreadable(path.clone())),
            _ if !login => Ok(SourceFound::Folder),
            FolderFacts::NoCredential => Err(SourceFault::NoLogin(path.clone())),
            FolderFacts::Malformed => Err(SourceFault::LoginInvalid(path.clone())),
            FolderFacts::Unrecognised => Err(SourceFault::NotChatgpt(path.clone())),
            FolderFacts::CodexLogin => {
                let found = read_chatgpt_login(self.fs.as_ref(), path)
                    .await
                    .map_err(SourceFault::Login)?;
                Ok(SourceFound::Login {
                    account: found.account_id,
                })
            }
        }
    }

    pub async fn probe_folder(&self, path: &HostPath) -> FolderFacts {
        let folder = match self.fs.canonicalize(path).await {
            Ok(Some(folder)) => folder,
            Ok(None) => return FolderFacts::Missing,
            Err(_) => return FolderFacts::Unreadable,
        };
        if !matches!(
            self.fs.stat_following(&folder).await,
            Ok(Some(FileKind::Dir))
        ) {
            return FolderFacts::Unreadable;
        }
        match read_chatgpt_login(self.fs.as_ref(), &folder).await {
            Ok(_) => FolderFacts::CodexLogin,
            Err(CredentialError::NotAChatgptLogin) => FolderFacts::Unrecognised,
            Err(CredentialError::AuthJsonInvalid { .. }) => FolderFacts::Malformed,
            Err(CredentialError::LoginUnreadable { .. }) => FolderFacts::NoCredential,
            Err(
                CredentialError::LoginNotRegular { .. }
                | CredentialError::LoginTooLarge { .. }
                | CredentialError::KeychainUnavailable
                | CredentialError::KeychainLoginMissing
                | CredentialError::Fs(_)
                | CredentialError::MissingStaticKey,
            ) => FolderFacts::Unreadable,
        }
    }

    async fn reference(
        &self,
        account: &str,
        reference: &str,
        cancel: &CancellationToken,
    ) -> Result<SourceFound, SourceFault> {
        let path = field_path(reference)?;
        let detail = self
            .op
            .item(account, path.vault, path.item, cancel)
            .await
            .map_err(SourceFault::Op)?;
        let field = resolve_field(&detail.fields, &path)?;
        if field.kind != OpFieldKind::Concealed {
            return Err(SourceFault::NotConcealed(field.label.clone()));
        }
        if field.empty {
            return Err(SourceFault::EmptyField(field.label.clone()));
        }
        Ok(SourceFound::OnePassword)
    }
}

fn section_matches(field: &OpFieldInfo, section: Option<&str>) -> bool {
    match (section, &field.section) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(_), None) => false,
        (Some(wanted), Some(found)) => found.id == wanted || found.label == wanted,
    }
}

fn resolve_field<'a>(
    fields: &'a [OpFieldInfo],
    path: &FieldPath<'_>,
) -> Result<&'a OpFieldInfo, SourceFault> {
    let mut found: Vec<&OpFieldInfo> = Vec::new();
    for field in fields
        .iter()
        .filter(|field| section_matches(field, path.section))
    {
        if (field.id == path.field || field.label == path.field)
            && !found.iter().any(|seen| seen.id == field.id)
        {
            found.push(field);
        }
    }
    match found.len() {
        0 => Err(SourceFault::Op(OpFault::MissingField {
            field: path.field.to_owned(),
            item: path.item.to_owned(),
        })),
        1 => Ok(found[0]),
        _ => Err(SourceFault::Op(OpFault::AmbiguousField {
            field: path.field.to_owned(),
            item: path.item.to_owned(),
        })),
    }
}

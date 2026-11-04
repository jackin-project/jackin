// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{
    ChatgptAccountId, EnvName, EnvSnapshot, FileKind, FileSystem, FsError, HostPath, OpReference,
    plain,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::credentials::{CredentialError, read_chatgpt_login};
use crate::op::{OnePasswordCli, OpFault, OpFieldKind};

pub const AUTH_FILE: &str = "auth.json";
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
        match self
            .fs
            .stat_following(path)
            .await
            .map_err(SourceFault::Fs)?
        {
            None => Err(SourceFault::FileMissing(path.clone())),
            Some(FileKind::File) => Ok(SourceFound::File),
            Some(_) => Err(SourceFault::FileNotRegular(path.clone())),
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
        let Ok(Some(folder)) = self.fs.canonicalize(path).await else {
            return FolderFacts::Missing;
        };
        if !matches!(
            self.fs.stat_following(&folder).await,
            Ok(Some(FileKind::Dir))
        ) {
            return FolderFacts::Unreadable;
        }
        let Ok(file) = HostPath::resolve(Path::new(AUTH_FILE), &folder) else {
            return FolderFacts::Unreadable;
        };
        match self.fs.stat_following(&file).await {
            Ok(None) => FolderFacts::NoCredential,
            Ok(Some(FileKind::File)) => match read_chatgpt_login(self.fs.as_ref(), &folder).await {
                Ok(_) => FolderFacts::CodexLogin,
                Err(CredentialError::NotAChatgptLogin) => FolderFacts::Unrecognised,
                Err(CredentialError::AuthJsonInvalid { .. }) => FolderFacts::Malformed,
                Err(CredentialError::LoginUnreadable { .. }) => FolderFacts::NoCredential,
                Err(CredentialError::Fs(_)) => FolderFacts::Unreadable,
            },
            Ok(Some(_)) | Err(_) => FolderFacts::Unreadable,
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
        let field = detail
            .fields
            .iter()
            .find(|field| field.id == path.field && field.section.as_deref() == path.section)
            .ok_or_else(|| {
                SourceFault::Op(OpFault::MissingField {
                    field: path.field.to_owned(),
                    item: path.item.to_owned(),
                })
            })?;
        if field.kind != OpFieldKind::Concealed {
            return Err(SourceFault::NotConcealed(field.label.clone()));
        }
        if field.empty {
            return Err(SourceFault::EmptyField(field.label.clone()));
        }
        Ok(SourceFound::OnePassword)
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::time::Duration;

use jackin_core::{
    CredentialSource, FsError, HostPath, ProcessSpec, ProcessStatus, ProcessStdin, Program,
    RegularFileProblem, RepoPath, Secret,
};
use jackin_usage_provider_core::Credential;
use tokio_util::sync::CancellationToken;

use crate::deps::UsageDeps;
use crate::request::UsageAccount;

const OP_TIMEOUT: Duration = Duration::from_secs(30);
const CODEX_LOGIN_FILE: &str = "auth.json";
const CREDENTIAL_FILE_MAX_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Token,
    Login,
}

pub(crate) struct Material {
    pub(crate) secret: Secret,
    pub(crate) shape: Shape,
}

impl Material {
    fn token(text: &str) -> Material {
        Material {
            secret: Secret::new(text.to_owned()),
            shape: Shape::Token,
        }
    }

    pub(crate) fn credential(&self) -> Credential<'_> {
        match self.shape {
            Shape::Token => Credential::Token(self.secret.expose()),
            Shape::Login => Credential::Login(self.secret.expose()),
        }
    }

    fn login(text: String) -> Material {
        Material {
            secret: Secret::new(text),
            shape: Shape::Login,
        }
    }
}

pub(crate) enum Unresolved {
    Missing,
    Unreadable(&'static str),
}

pub(crate) async fn resolve(
    deps: &UsageDeps,
    account: &UsageAccount,
    cancel: &CancellationToken,
) -> Result<Material, Unresolved> {
    match &account.credential {
        CredentialSource::Env { var } => deps
            .env
            .get(var)
            .and_then(non_empty)
            .map(Material::token)
            .ok_or(Unresolved::Missing),
        CredentialSource::OnePassword { reference, account } => {
            let mut args = vec!["read".to_owned()];
            if let Some(account) = account {
                args.extend(["--account".to_owned(), account.as_str().to_owned()]);
            }
            args.push(reference.as_str().to_owned());
            let spec = ProcessSpec {
                program: Program::Op,
                args,
                cwd: None,
                env: Vec::new(),
                env_remove_prefixes: Vec::new(),
                stdin: ProcessStdin::Null,
                timeout: OP_TIMEOUT,
            };
            let output = deps
                .runner
                .run_secret(&spec, cancel)
                .await
                .map_err(|_| Unresolved::Missing)?;
            if output.status != ProcessStatus::Exited(0) {
                return Err(Unresolved::Missing);
            }
            let text = std::str::from_utf8(output.stdout.expose())
                .map_err(|_| Unresolved::Unreadable("the 1Password value is not text"))?;
            non_empty(text)
                .map(Material::token)
                .ok_or(Unresolved::Missing)
        }
        CredentialSource::File { path } => {
            let unusable = || Unresolved::Unreadable("the credential file is not a usable path");
            let root = path
                .as_path()
                .parent()
                .and_then(|parent| HostPath::parse(parent).ok())
                .ok_or_else(unusable)?;
            let name = path
                .as_path()
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| RepoPath::parse(name).ok())
                .ok_or_else(unusable)?;
            let text = read_text(deps, &root, &name).await?;
            Ok(if text.starts_with('{') {
                Material::login(text)
            } else {
                Material::token(&text)
            })
        }
        CredentialSource::CodexLogin { home, .. } => {
            let name = RepoPath::parse(CODEX_LOGIN_FILE)
                .map_err(|_| Unresolved::Unreadable("the Codex login file name is not usable"))?;
            Ok(Material::login(read_text(deps, home, &name).await?))
        }
    }
}

async fn read_text(
    deps: &UsageDeps,
    root: &HostPath,
    name: &RepoPath,
) -> Result<String, Unresolved> {
    let read = match deps
        .fs
        .read_regular(root, name, CREDENTIAL_FILE_MAX_BYTES)
        .await
    {
        Err(FsError::Io {
            kind: ErrorKind::NotFound,
            ..
        }) => return Err(Unresolved::Missing),
        Err(_) => {
            return Err(Unresolved::Unreadable(
                "the credential file could not be read",
            ));
        }
        Ok(read) => read,
    };
    let file = match read {
        Ok(file) => file,
        Err(RegularFileProblem::Missing) => return Err(Unresolved::Missing),
        Err(RegularFileProblem::Symlink) => {
            return Err(Unresolved::Unreadable(
                "the credential file is a symbolic link",
            ));
        }
        Err(RegularFileProblem::NotARegularFile) => {
            return Err(Unresolved::Unreadable(
                "the credential file is not a regular file",
            ));
        }
    };
    if file.bytes.len() > CREDENTIAL_FILE_MAX_BYTES {
        return Err(Unresolved::Unreadable("the credential file is too large"));
    }
    let text = String::from_utf8(file.bytes)
        .map_err(|_| Unresolved::Unreadable("the credential file is not text"))?;
    non_empty(&text)
        .map(str::to_owned)
        .ok_or(Unresolved::Missing)
}

fn non_empty(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::time::Duration;

use jackin_core::{
    CredentialFileProblem, CredentialSource, HostPath, ProcessSpec, ProcessStatus, ProcessStdin,
    Program, Secret, read_credential_file,
};
use jackin_usage_provider_core::Credential;
use tokio_util::sync::CancellationToken;

use crate::deps::UsageDeps;
use crate::request::UsageAccount;

const OP_TIMEOUT: Duration = Duration::from_secs(30);
const CODEX_LOGIN_FILE: &str = "auth.json";

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
        CredentialSource::OmpHost | CredentialSource::HostLogin { .. } => Err(Unresolved::Missing),
        CredentialSource::File { path } => {
            let text = read_text(deps, path).await?;
            Ok(if text.starts_with('{') {
                Material::login(text)
            } else {
                Material::token(&text)
            })
        }
        CredentialSource::CodexLogin { home, .. } => {
            let path = HostPath::resolve(Path::new(CODEX_LOGIN_FILE), home)
                .map_err(|_| Unresolved::Unreadable("the Codex login file name is not usable"))?;
            Ok(Material::login(read_text(deps, &path).await?))
        }
    }
}

async fn read_text(deps: &UsageDeps, path: &HostPath) -> Result<String, Unresolved> {
    let bytes = match read_credential_file(deps.fs.as_ref(), path).await {
        Err(_) => {
            return Err(Unresolved::Unreadable(
                "the credential file could not be read",
            ));
        }
        Ok(Err(CredentialFileProblem::Missing)) => return Err(Unresolved::Missing),
        Ok(Err(CredentialFileProblem::NotARegularFile)) => {
            return Err(Unresolved::Unreadable(
                "the credential file is not a regular file",
            ));
        }
        Ok(Err(CredentialFileProblem::TooLarge)) => {
            return Err(Unresolved::Unreadable("the credential file is too large"));
        }
        Ok(Ok(bytes)) => bytes,
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| Unresolved::Unreadable("the credential file is not text"))?;
    non_empty(&text)
        .map(str::to_owned)
        .ok_or(Unresolved::Missing)
}

fn non_empty(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

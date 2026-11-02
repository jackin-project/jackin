// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{
    BranchName, CommitId, EnvName, SandboxName, SandboxPath, TerminalText, sandbox_layout,
};
use jackin_openshell::{Gateway, GatewayError, ProgramOutcome, ProgramRun, ProgramStdin};
use tokio_util::sync::CancellationToken;

const HEADS: &str = "refs/heads/";
const STASH: &str = "refs/stash";
const NOT_A_REPOSITORY: &str = "not a git repository";
static MKDIR: SandboxPath = SandboxPath::from_static("/usr/bin/mkdir");
static RM: SandboxPath = SandboxPath::from_static("/usr/bin/rm");

#[derive(Debug)]
pub(super) enum RemoteError {
    Gateway(GatewayError),
    NoRepository,
    Git {
        exit_code: i32,
        output: TerminalText,
    },
    Malformed,
}

impl From<GatewayError> for RemoteError {
    fn from(error: GatewayError) -> RemoteError {
        RemoteError::Gateway(error)
    }
}

#[derive(Debug)]
pub(super) struct Repository {
    pub(super) branches: BTreeMap<BranchName, CommitId>,
    pub(super) unusable: Vec<String>,
    pub(super) detached: Option<CommitId>,
    pub(super) uncommitted: bool,
}

pub(super) struct Remote<'a> {
    pub(super) gateway: &'a dyn Gateway,
    pub(super) name: &'a SandboxName,
    pub(super) timeout: Duration,
    pub(super) cancel: &'a CancellationToken,
}

fn failure(outcome: &ProgramOutcome) -> RemoteError {
    let text = if outcome.stderr.is_empty() {
        &outcome.stdout
    } else {
        &outcome.stderr
    };
    RemoteError::Git {
        exit_code: outcome.exit_code,
        output: TerminalText::sanitize(text),
    }
}

impl Remote<'_> {
    async fn exec(
        &self,
        program: &SandboxPath,
        args: Vec<String>,
    ) -> Result<ProgramOutcome, GatewayError> {
        let env = EnvName::parse("LC_ALL")
            .map(|name| BTreeMap::from([(name, "C".to_owned())]))
            .unwrap_or_default();
        self.gateway
            .run_program(
                &ProgramRun {
                    sandbox: self.name.clone(),
                    program: program.clone(),
                    args,
                    workdir: Some(sandbox_layout::PROJECT.clone()),
                    env,
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: self.timeout,
                },
                self.cancel,
            )
            .await
    }

    async fn git(&self, args: &[&str]) -> Result<ProgramOutcome, GatewayError> {
        self.exec(
            &sandbox_layout::GIT,
            args.iter().map(|arg| (*arg).to_owned()).collect(),
        )
        .await
    }

    async fn require(&self, args: &[&str]) -> Result<Vec<u8>, RemoteError> {
        let outcome = self.git(args).await?;
        if outcome.exit_code == 0 {
            Ok(outcome.stdout)
        } else {
            Err(failure(&outcome))
        }
    }

    async fn require_repository(&self) -> Result<(), RemoteError> {
        let outcome = self.git(&["rev-parse", "--git-dir"]).await?;
        match outcome.exit_code {
            0 if outcome.stdout.trim_ascii() == b".git" => Ok(()),
            0 => Err(RemoteError::NoRepository),
            128 if String::from_utf8_lossy(&outcome.stderr).contains(NOT_A_REPOSITORY) => {
                Err(RemoteError::NoRepository)
            }
            _ => Err(failure(&outcome)),
        }
    }

    async fn detached(&self) -> Result<Option<CommitId>, RemoteError> {
        let symbolic = self.git(&["symbolic-ref", "-q", "HEAD"]).await?;
        match symbolic.exit_code {
            0 => return Ok(None),
            1 => {}
            _ => return Err(failure(&symbolic)),
        }
        let resolved = self.git(&["rev-parse", "-q", "--verify", "HEAD"]).await?;
        match resolved.exit_code {
            0 => CommitId::parse(String::from_utf8_lossy(&resolved.stdout).trim())
                .map(Some)
                .map_err(|_| RemoteError::Malformed),
            1 => Ok(None),
            _ => Err(failure(&resolved)),
        }
    }

    pub(super) async fn repository(&self) -> Result<Repository, RemoteError> {
        self.require_repository().await?;
        let listed = self
            .require(&[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)",
                "refs/heads",
                STASH,
            ])
            .await?;
        let mut repository = Repository {
            branches: BTreeMap::new(),
            unusable: Vec::new(),
            detached: self.detached().await?,
            uncommitted: false,
        };
        for line in String::from_utf8_lossy(&listed).lines() {
            let (name, oid) = line.split_once('\0').ok_or(RemoteError::Malformed)?;
            if name == STASH {
                repository.uncommitted = true;
            } else if let Some(branch) = name.strip_prefix(HEADS) {
                let tip = CommitId::parse(oid).map_err(|_| RemoteError::Malformed)?;
                match BranchName::parse(branch) {
                    Ok(branch) => {
                        repository.branches.insert(branch, tip);
                    }
                    Err(_) => repository.unusable.push(branch.to_owned()),
                }
            }
        }
        let status = self
            .require(&["--no-optional-locks", "status", "--porcelain"])
            .await?;
        repository.uncommitted |= !status.is_empty();
        Ok(repository)
    }

    pub(super) async fn discard(&self, target: &SandboxPath) {
        drop(
            self.exec(
                &RM,
                vec!["-f".to_owned(), "--".to_owned(), target.as_str().to_owned()],
            )
            .await,
        );
    }

    pub(super) async fn bundle(
        &self,
        target: &SandboxPath,
        refs: &[String],
    ) -> Result<(), RemoteError> {
        let made = self
            .exec(
                &MKDIR,
                vec![
                    "-p".to_owned(),
                    "--".to_owned(),
                    sandbox_layout::BUNDLE_DIR.as_str().to_owned(),
                ],
            )
            .await?;
        if made.exit_code != 0 {
            return Err(failure(&made));
        }
        let mut args = vec![
            "bundle".to_owned(),
            "create".to_owned(),
            "--quiet".to_owned(),
            target.as_str().to_owned(),
        ];
        args.extend_from_slice(refs);
        let outcome = self.exec(&sandbox_layout::GIT, args).await?;
        if outcome.exit_code == 0 {
            Ok(())
        } else {
            Err(failure(&outcome))
        }
    }
}

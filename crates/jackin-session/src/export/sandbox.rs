// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use jackin_core::{
    BranchName, CommitId, EnvName, SandboxName, SandboxPath, TerminalText, sandbox_layout,
};

use super::diff::{ChangedFile, Unread, added_file, parse_patch};
use jackin_openshell::{Gateway, GatewayError, ProgramOutcome, ProgramRun, ProgramStdin};
use tokio_util::sync::CancellationToken;

const HEADS: &str = "refs/heads/";
const NOT_A_REPOSITORY: &str = "not a git repository";
const CLEANUP: Duration = Duration::from_secs(10);
const UNTRACKED_FILES: usize = 500;
const UNTRACKED_BYTES: usize = 256 * 1024;
const NOT_READ: i32 = 66;
static SH: SandboxPath = SandboxPath::from_static("/bin/sh");
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
    Malformed(&'static str),
}

impl From<GatewayError> for RemoteError {
    fn from(error: GatewayError) -> RemoteError {
        RemoteError::Gateway(error)
    }
}

#[derive(Debug)]
pub(super) struct LinkedHead {
    pub(super) name: BranchName,
    pub(super) tip: CommitId,
}

#[derive(Debug)]
pub(super) struct Repository {
    pub(super) branches: BTreeMap<BranchName, CommitId>,
    pub(super) unusable: Vec<TerminalText>,
    pub(super) detached: Option<CommitId>,
    pub(super) linked: Vec<LinkedHead>,
    checkouts: Vec<String>,
}

#[derive(Debug)]
struct Checkout {
    path: String,
    head: Option<CommitId>,
    off_heads: bool,
    prunable: bool,
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

fn lines(output: &[u8]) -> usize {
    output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .count()
}

fn hidden(listed: &[u8]) -> usize {
    listed
        .split(|byte| *byte == 0)
        .filter(|entry| {
            entry
                .first()
                .is_some_and(|tag| *tag == b'S' || tag.is_ascii_lowercase())
        })
        .count()
}

fn checkouts(listed: &[u8]) -> Result<Vec<Checkout>, RemoteError> {
    const MALFORMED: RemoteError =
        RemoteError::Malformed("git worktree list printed unreadable output");
    let mut found: Vec<Checkout> = Vec::new();
    for field in listed.split(|byte| *byte == 0) {
        if field.is_empty() {
            continue;
        }
        let text = std::str::from_utf8(field).map_err(|_| MALFORMED)?;
        if let Some(path) = text.strip_prefix("worktree ") {
            found.push(Checkout {
                path: path.to_owned(),
                head: None,
                off_heads: false,
                prunable: false,
            });
            continue;
        }
        let current = found.last_mut().ok_or(MALFORMED)?;
        if let Some(head) = text.strip_prefix("HEAD ") {
            current.head = CommitId::parse(head).ok();
        } else if text == "detached" {
            current.off_heads = true;
        } else if let Some(branch) = text.strip_prefix("branch ") {
            current.off_heads = !branch.starts_with(HEADS);
        } else if text == "prunable" || text.starts_with("prunable ") {
            current.prunable = true;
        }
    }
    if found.is_empty() {
        return Err(MALFORMED);
    }
    Ok(found)
}

fn worktree_id(git_dir: &[u8]) -> Result<BranchName, RemoteError> {
    const MALFORMED: RemoteError =
        RemoteError::Malformed("git printed an unreadable worktree directory");
    let text = std::str::from_utf8(git_dir).map_err(|_| MALFORMED)?;
    let dir = Path::new(text.trim_end());
    let id = dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(MALFORMED)?;
    let parent = dir.parent().and_then(Path::file_name);
    if parent != Some("worktrees".as_ref()) {
        return Err(MALFORMED);
    }
    BranchName::parse(id).map_err(|_| MALFORMED)
}

impl Remote<'_> {
    async fn exec_within(
        &self,
        timeout: Duration,
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
                    timeout,
                },
                self.cancel,
            )
            .await
    }

    async fn exec(
        &self,
        program: &SandboxPath,
        args: Vec<String>,
    ) -> Result<ProgramOutcome, GatewayError> {
        self.exec_within(self.timeout, program, args).await
    }

    async fn git(&self, args: &[&str]) -> Result<ProgramOutcome, GatewayError> {
        self.exec(
            &sandbox_layout::GIT,
            std::iter::once("--no-optional-locks")
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
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

    pub(super) async fn repository(&self) -> Result<Repository, RemoteError> {
        self.require_repository().await?;
        let listed = self
            .require(&["worktree", "list", "--porcelain", "-z"])
            .await?;
        let mut repository = Repository {
            branches: BTreeMap::new(),
            unusable: Vec::new(),
            detached: None,
            linked: Vec::new(),
            checkouts: Vec::new(),
        };
        for (index, checkout) in checkouts(&listed)?.into_iter().enumerate() {
            if checkout.prunable {
                continue;
            }
            repository.checkouts.push(checkout.path.clone());
            if !checkout.off_heads {
                continue;
            }
            let tip = checkout.head.ok_or(RemoteError::Malformed(
                "git worktree list printed a worktree outside refs/heads without a commit",
            ))?;
            if index == 0 {
                repository.detached = Some(tip);
                continue;
            }
            let git_dir = self
                .require(&["-C", &checkout.path, "rev-parse", "--absolute-git-dir"])
                .await?;
            match worktree_id(&git_dir) {
                Ok(name) => repository.linked.push(LinkedHead { name, tip }),
                Err(_) => repository.unusable.push(TerminalText::sanitize(
                    format!("worktree at {}", checkout.path).as_bytes(),
                )),
            }
        }
        repository
            .linked
            .sort_by(|left, right| left.name.cmp(&right.name));
        let listed = self
            .require(&[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)",
                "refs/heads",
            ])
            .await?;
        for line in String::from_utf8_lossy(&listed).lines() {
            let (name, oid) = line.split_once('\0').ok_or(RemoteError::Malformed(
                "git for-each-ref printed a line without an object name",
            ))?;
            let Some(branch) = name.strip_prefix(HEADS) else {
                continue;
            };
            let tip = CommitId::parse(oid).map_err(|_| {
                RemoteError::Malformed("git for-each-ref printed an unreadable object name")
            })?;
            match BranchName::parse(branch) {
                Ok(branch) => {
                    repository.branches.insert(branch, tip);
                }
                Err(_) => repository
                    .unusable
                    .push(TerminalText::sanitize(branch.as_bytes())),
            }
        }
        Ok(repository)
    }

    pub(super) async fn uncommitted(&self, repository: &Repository) -> Result<usize, RemoteError> {
        let stash = self
            .git(&["rev-parse", "--verify", "--quiet", "refs/stash"])
            .await?;
        let mut count = match stash.exit_code {
            0 => lines(&self.require(&["stash", "list"]).await?).max(1),
            1 => 0,
            _ => return Err(failure(&stash)),
        };
        for path in &repository.checkouts {
            let status = self
                .require(&[
                    "-C",
                    path,
                    "-c",
                    "core.fileMode=true",
                    "-c",
                    "core.trustctime=true",
                    "-c",
                    "core.checkStat=default",
                    "status",
                    "--porcelain",
                    "--untracked-files=all",
                    "--ignore-submodules=none",
                ])
                .await?;
            count += lines(&status);
            let flagged = self.require(&["-C", path, "ls-files", "-v", "-z"]).await?;
            count += hidden(&flagged);
        }
        Ok(count)
    }

    pub(super) async fn changes(
        &self,
        repository: &Repository,
    ) -> Result<(Vec<ChangedFile>, usize), RemoteError> {
        let root = repository.checkouts.first().ok_or(RemoteError::Malformed(
            "git worktree list printed no checkout",
        ))?;
        let head = self
            .git(&["-C", root, "rev-parse", "--verify", "--quiet", "HEAD"])
            .await?;
        let mut files = if head.exit_code == 0 {
            let patch = self
                .require(&[
                    "-C",
                    root,
                    "-c",
                    "core.quotepath=false",
                    "diff-index",
                    "-p",
                    "--no-color",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--find-renames",
                    "-U3",
                    "HEAD",
                    "--",
                ])
                .await?;
            parse_patch(&patch).map_err(RemoteError::Malformed)?
        } else {
            Vec::new()
        };
        let listed = self
            .require(&[
                "-C",
                root,
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
            ])
            .await?;
        let mut names = listed
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| String::from_utf8_lossy(name).into_owned());
        for name in names.by_ref().take(UNTRACKED_FILES) {
            let content = self.read_untracked(root, &name).await?;
            files.push(added_file(
                &name,
                content.as_deref().map_err(|unread| *unread),
            ));
        }
        Ok((files, names.count()))
    }

    async fn read_untracked(
        &self,
        root: &str,
        name: &str,
    ) -> Result<Result<Vec<u8>, Unread>, RemoteError> {
        let script = format!(
            "[ -f \"$1\" ] && [ ! -L \"$1\" ] || exit {NOT_READ}; head -c {} -- \"$1\" 2>/dev/null || exit {NOT_READ}",
            UNTRACKED_BYTES + 1
        );
        let outcome = self
            .exec(
                &SH,
                vec![
                    "-c".to_owned(),
                    script,
                    "jackin-read".to_owned(),
                    format!("{root}/{name}"),
                ],
            )
            .await?;
        match outcome.exit_code {
            0 if outcome.stdout.len() > UNTRACKED_BYTES => Ok(Err(Unread::TooLarge)),
            0 => Ok(Ok(outcome.stdout)),
            NOT_READ => Ok(Err(Unread::NotRegular)),
            _ => Err(failure(&outcome)),
        }
    }

    pub(super) async fn discard(&self, target: &SandboxPath) {
        drop(
            self.exec_within(
                CLEANUP,
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

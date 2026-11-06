// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{
    EnvName, FileMode, SandboxName, SandboxPath, SecretBytes, TerminalText, sandbox_layout,
};
use jackin_openshell::{Gateway, ProgramOutcome, ProgramRun, ProgramStdin, SandboxFile};
use tokio_util::sync::CancellationToken;

use crate::launch::LaunchError;
use crate::project::facts::{Facts, Head};
use crate::project::git::words;
use crate::project::listing::Listing;
use crate::project::{ProjectStep, TreeId};

static FIND: SandboxPath = SandboxPath::from_static("/usr/bin/find");
static MKDIR: SandboxPath = SandboxPath::from_static("/usr/bin/mkdir");
static RM: SandboxPath = SandboxPath::from_static("/usr/bin/rm");
static SHALLOW: SandboxPath = SandboxPath::from_static("/sandbox/project/.git/shallow");
static EXCLUDE: SandboxPath = SandboxPath::from_static("/sandbox/project/.git/info/exclude");
static OBJECTS: SandboxPath = SandboxPath::from_static("/sandbox/project/.git/objects");
const FILE_MODE: FileMode = FileMode(0o644);
const GATE_INDEX: &str = "GIT_INDEX_FILE";
const GATE_OBJECTS: &str = "GIT_OBJECT_DIRECTORY";
const GATE_ALTERNATES: &str = "GIT_ALTERNATE_OBJECT_DIRECTORIES";

const REFSPECS: [&str; 4] = [
    "+refs/heads/*:refs/heads/*",
    "+refs/tags/*:refs/tags/*",
    "+refs/remotes/*:refs/remotes/*",
    "^refs/heads/jackin/*",
];

pub(super) struct Sandbox<'a> {
    pub(super) gateway: &'a dyn Gateway,
    pub(super) name: &'a SandboxName,
    pub(super) timeout: Duration,
    pub(super) cancel: &'a CancellationToken,
}

fn failed(step: ProjectStep, outcome: &ProgramOutcome) -> LaunchError {
    let text = if outcome.stderr.is_empty() {
        &outcome.stdout
    } else {
        &outcome.stderr
    };
    LaunchError::ProjectFailed {
        step,
        exit_code: outcome.exit_code,
        output: TerminalText::sanitize(text),
    }
}

fn path_arg(path: &SandboxPath) -> String {
    path.as_str().to_owned()
}

impl Sandbox<'_> {
    async fn exec(
        &self,
        step: ProjectStep,
        program: &SandboxPath,
        args: Vec<String>,
        env: BTreeMap<EnvName, String>,
        workdir: bool,
    ) -> Result<ProgramOutcome, LaunchError> {
        let outcome = self
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: self.name.clone(),
                    program: program.clone(),
                    args,
                    workdir: workdir.then(|| sandbox_layout::PROJECT.clone()),
                    env,
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: self.timeout,
                },
                self.cancel,
            )
            .await?;
        if outcome.exit_code == 0 {
            Ok(outcome)
        } else {
            Err(failed(step, &outcome))
        }
    }

    async fn git(
        &self,
        step: ProjectStep,
        args: &[&str],
        env: BTreeMap<EnvName, String>,
    ) -> Result<ProgramOutcome, LaunchError> {
        self.exec(step, &sandbox_layout::GIT, words(args), env, true)
            .await
    }

    async fn write(&self, path: &SandboxPath, content: &[u8]) -> Result<(), LaunchError> {
        self.gateway
            .write_file(
                self.name,
                &SandboxFile {
                    path: path.clone(),
                    mode: FILE_MODE,
                    content: SecretBytes::new(content.to_vec()),
                },
                self.cancel,
            )
            .await?;
        Ok(())
    }

    pub(super) async fn empty(&self) -> Result<(), LaunchError> {
        let project = path_arg(&sandbox_layout::PROJECT);
        self.exec(
            ProjectStep::Empty,
            &MKDIR,
            vec!["-p".to_owned(), "--".to_owned(), project.clone()],
            BTreeMap::new(),
            false,
        )
        .await?;
        self.exec(
            ProjectStep::Empty,
            &FIND,
            vec![
                project,
                "-mindepth".to_owned(),
                "1".to_owned(),
                "-delete".to_owned(),
            ],
            BTreeMap::new(),
            false,
        )
        .await?;
        Ok(())
    }

    pub(super) async fn repository(
        &self,
        facts: &Facts,
        bundle: Option<&SandboxPath>,
    ) -> Result<(), LaunchError> {
        let format = format!("--object-format={}", facts.format.as_str());
        self.git(ProjectStep::Init, &["init", "-q", &format], BTreeMap::new())
            .await?;
        if let Some(shallow) = &facts.shallow {
            self.write(&SHALLOW, shallow).await?;
        }
        if let Some(bundle) = bundle {
            self.fetch(bundle).await?;
        }
        self.point(&facts.head).await?;
        for (key, value) in [
            ("user.name", &facts.identity.name),
            ("user.email", &facts.identity.email),
        ] {
            if let Some(value) = value {
                self.git(
                    ProjectStep::Configure,
                    &["config", "--local", "--", key, value],
                    BTreeMap::new(),
                )
                .await?;
            }
        }
        if let Some(exclude) = &facts.exclude {
            self.write(&EXCLUDE, exclude).await?;
        }
        if let Some(bundle) = bundle {
            self.exec(
                ProjectStep::RemoveBundle,
                &RM,
                vec!["-f".to_owned(), "--".to_owned(), path_arg(bundle)],
                BTreeMap::new(),
                false,
            )
            .await?;
        }
        Ok(())
    }

    async fn fetch(&self, bundle: &SandboxPath) -> Result<(), LaunchError> {
        let mut args = words(&["fetch", "--quiet", "--update-head-ok", "--no-tags"]);
        args.push(path_arg(bundle));
        args.extend(REFSPECS.map(str::to_owned));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.git(ProjectStep::Fetch, &refs, BTreeMap::new()).await?;
        Ok(())
    }

    async fn point(&self, head: &Head) -> Result<(), LaunchError> {
        match head {
            Head::Branch { reference, .. } | Head::Unborn { reference } => {
                self.git(
                    ProjectStep::SetHead,
                    &["symbolic-ref", "HEAD", reference],
                    BTreeMap::new(),
                )
                .await?;
            }
            Head::Detached { commit } => {
                self.git(
                    ProjectStep::SetHead,
                    &["update-ref", "--no-deref", "HEAD", commit.as_str()],
                    BTreeMap::new(),
                )
                .await?;
            }
        }
        if head.is_born() {
            self.git(ProjectStep::Reset, &["reset", "-q"], BTreeMap::new())
                .await?;
        }
        Ok(())
    }

    fn gate_env(gate: &SandboxPath) -> Result<BTreeMap<EnvName, String>, LaunchError> {
        let pairs = [
            (GATE_INDEX, path_arg(&gate.join("index")?)),
            (GATE_OBJECTS, path_arg(&gate.join("objects")?)),
            (GATE_ALTERNATES, path_arg(&OBJECTS)),
        ];
        pairs
            .into_iter()
            .map(|(name, value)| {
                EnvName::parse(name)
                    .map(|name| (name, value))
                    .map_err(|error| LaunchError::ProbeInvalid {
                        message: error.to_string(),
                    })
            })
            .collect()
    }

    pub(super) async fn tree(
        &self,
        gate: &SandboxPath,
        listing: &Listing,
    ) -> Result<TreeId, LaunchError> {
        self.clear(gate).await?;
        self.exec(
            ProjectStep::SandboxTree,
            &MKDIR,
            vec![
                "-p".to_owned(),
                "--".to_owned(),
                path_arg(&gate.join("objects")?),
            ],
            BTreeMap::new(),
            false,
        )
        .await?;
        let paths = gate.join("paths")?;
        self.write(&paths, listing.as_bytes()).await?;
        let env = Self::gate_env(gate)?;
        let mut add = words(&["--literal-pathspecs", "add", "-f", "--pathspec-file-nul"]);
        add.push(format!("--pathspec-from-file={}", path_arg(&paths)));
        let add: Vec<&str> = add.iter().map(String::as_str).collect();
        self.git(ProjectStep::SandboxTree, &add, env.clone())
            .await?;
        let written = self
            .git(ProjectStep::SandboxTree, &["write-tree"], env)
            .await?;
        TreeId::parse(String::from_utf8_lossy(&written.stdout).trim()).map_err(|error| {
            LaunchError::ProjectOutput {
                step: ProjectStep::SandboxTree,
                message: error.to_string(),
            }
        })
    }

    pub(super) async fn entries(
        &self,
        gate: &SandboxPath,
        tree: &TreeId,
    ) -> Result<Vec<u8>, LaunchError> {
        let env = Self::gate_env(gate)?;
        let listed = self
            .git(
                ProjectStep::TreeList,
                &["ls-tree", "-r", "-z", tree.as_str()],
                env,
            )
            .await?;
        Ok(listed.stdout)
    }

    pub(super) async fn clear(&self, gate: &SandboxPath) -> Result<(), LaunchError> {
        self.exec(
            ProjectStep::SandboxTree,
            &RM,
            vec!["-rf".to_owned(), "--".to_owned(), path_arg(gate)],
            BTreeMap::new(),
            false,
        )
        .await?;
        Ok(())
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io::Write;
use std::num::NonZeroU8;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jackin_core::{
    AccountName, Agent, ConfigStore, FileMode, FileSystem, GatewayName, HostPath, ImageId,
    ImageTag, LaunchId, NameSuffix, OpenShellWorkspace, Phase, PlannedLabels, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, Program, ProjectRoot, RoleName, SandboxName,
    SandboxPath, SandboxSlug, SecretBytes, StdFileSystem, WorkspaceName,
};
use jackin_openshell::contract::{ContractFixture, ContractGroup, SHELL_END_TIMEOUT, run};
use jackin_openshell::{
    ClosedPolicy, Gateway, GrpcGateway, GrpcGatewayConfig, MainProcess, ProgramRun, ProgramStdin,
    SandboxCreate, SandboxFile, SandboxQuery, resolve_gateway,
};
use jackin_session::{
    DEFAULT_BUNDLE_BYTES, DEFAULT_EXPORT_ENTRY_BYTES, DEFAULT_EXPORT_OBJECTS,
    DEFAULT_EXPORT_TOTAL_BYTES, HostPorts, LaunchLimits, Session, SessionDeps, SystemClock,
    TokioProcessRunner,
};
use jackin_testkit::FakeEngine;
use tokio_util::sync::CancellationToken;

const POLICY: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
    - /usr
    - /lib
    - /proc
    - /dev/urandom
    - /etc
  read_write:
    - /tmp
    - /sandbox
    - /dev/null
landlock:
  compatibility: hard_requirement
process:
  run_as_user: sandbox
  run_as_group: sandbox
";
const GIT: &str = "/usr/bin/git";
const SH: &str = "/bin/sh";
const EXEC_TIMEOUT: Duration = Duration::from_secs(300);
const READY_ATTEMPTS: usize = 240;
const SYNTHETIC_MARK: &str = "jackin-p51b-driver";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Flag {
    Stale,
    Export,
    Keep,
    CleanupRefs,
}

#[derive(Clone, Copy)]
enum Command {
    Upload,
    Contract,
}

struct Options {
    command: Command,
    gateway: String,
    workspace: String,
    image: String,
    project: PathBuf,
    openshell_home: PathBuf,
    work_dir: PathBuf,
    flags: BTreeSet<Flag>,
    rounds: u32,
}

fn home_dir() -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").ok_or("HOME is not set")?,
    ))
}

fn options() -> Result<Options, Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let command = match args.next().as_deref() {
        Some("upload") => Command::Upload,
        Some("contract") => Command::Contract,
        other => {
            return Err(format!(
                "the first argument is upload or contract, not {other:?}; flags: --gateway --workspace --image --project --openshell-home --work-dir [--stale] [--rounds N] [--export] [--keep] [--cleanup-refs]"
            )
            .into());
        }
    };
    let mut options = Options {
        command,
        gateway: "jackin-local".to_owned(),
        workspace: "default".to_owned(),
        image: String::new(),
        project: PathBuf::from("."),
        openshell_home: home_dir()?.join(".config").join("openshell"),
        work_dir: std::env::temp_dir().join("jackin-p51b-driver"),
        flags: BTreeSet::new(),
        rounds: 1,
    };
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--gateway" => options.gateway = value("--gateway")?,
            "--workspace" => options.workspace = value("--workspace")?,
            "--image" => options.image = value("--image")?,
            "--project" => options.project = PathBuf::from(value("--project")?),
            "--openshell-home" => {
                options.openshell_home = PathBuf::from(value("--openshell-home")?);
            }
            "--work-dir" => options.work_dir = PathBuf::from(value("--work-dir")?),
            "--rounds" => options.rounds = value("--rounds")?.parse()?,
            "--stale" => {
                options.flags.insert(Flag::Stale);
            }
            "--export" => {
                options.flags.insert(Flag::Export);
            }
            "--keep" => {
                options.flags.insert(Flag::Keep);
            }
            "--cleanup-refs" => {
                options.flags.insert(Flag::CleanupRefs);
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    if options.image.is_empty() {
        return Err("--image names an image that the gateway already holds".into());
    }
    Ok(options)
}

fn say(text: &str) -> Result<(), Box<dyn Error>> {
    writeln!(std::io::stdout().lock(), "{text}")?;
    Ok(())
}

struct Live {
    gateway: Arc<GrpcGateway>,
    session: Session,
    runner: Arc<TokioProcessRunner>,
    project: ProjectRoot,
    options: Options,
}

async fn host_git(
    runner: &dyn ProcessRunner,
    root: &HostPath,
    args: &[&str],
) -> Result<String, Box<dyn Error>> {
    let spec = ProcessSpec {
        program: Program::Git,
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: Some(root.clone()),
        env: Vec::new(),
        env_remove_prefixes: vec!["GIT_".to_owned()],
        stdin: ProcessStdin::Null,
        timeout: Duration::from_secs(300),
    };
    let output = runner.run(&spec, None, &CancellationToken::new()).await?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(format!(
            "git {args:?} ended with {:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

impl Live {
    async fn connect(options: Options) -> Result<Live, Box<dyn Error>> {
        let runner = Arc::new(TokioProcessRunner);
        let port: Arc<dyn ProcessRunner> = Arc::<TokioProcessRunner>::clone(&runner);
        let fs: Arc<dyn FileSystem> = Arc::new(StdFileSystem::default());
        let cancel = CancellationToken::new();
        let resolved = resolve_gateway(
            runner.as_ref(),
            fs.as_ref(),
            &HostPath::parse(&options.openshell_home)?,
            &GatewayName::parse(&options.gateway)?,
            &cancel,
        )
        .await?;
        let gateway = Arc::new(
            GrpcGateway::connect(GrpcGatewayConfig {
                gateway: resolved,
                workspace: OpenShellWorkspace::parse(&options.workspace)?,
                runner: Arc::clone(&port),
                transfer_timeout: Duration::from_mins(30),
                call_timeout: Duration::from_secs(120),
            })
            .await?,
        );
        std::fs::create_dir_all(&options.work_dir)?;
        let work = options.work_dir.canonicalize()?;
        let session = Session::new(SessionDeps {
            gateway: Arc::<GrpcGateway>::clone(&gateway),
            engine: Arc::new(FakeEngine::new()),
            host: HostPorts {
                clock: Arc::new(SystemClock),
                fs: Arc::clone(&fs),
                runner: Arc::clone(&port),
            },
            cache_dir: HostPath::parse(&work.join("cache"))?,
            config: ConfigStore::new(
                Arc::clone(&fs),
                Arc::new(SystemClock),
                HostPath::parse(&work.join("config.toml"))?,
            ),
            limits: LaunchLimits {
                ready: Duration::from_secs(120),
                supervisor: Duration::from_secs(60),
                attempt: Duration::from_secs(60),
                hook: Duration::from_secs(600),
                upload: Duration::from_secs(3600),
                create_attempts: NonZeroU8::MIN,
                bundle_bytes: DEFAULT_BUNDLE_BYTES,
                export_entry_bytes: DEFAULT_EXPORT_ENTRY_BYTES,
                export_objects: DEFAULT_EXPORT_OBJECTS,
                export_total_bytes: DEFAULT_EXPORT_TOTAL_BYTES,
            },
        });
        let project = ProjectRoot::open(
            fs.as_ref(),
            &HostPath::parse(&options.project.canonicalize()?)?,
        )
        .await?;
        Ok(Live {
            gateway,
            session,
            runner,
            project,
            options,
        })
    }

    async fn sandbox(&self, launch: &LaunchId) -> Result<SandboxName, Box<dyn Error>> {
        let name = SandboxName::new(&SandboxSlug::parse("p5b")?, &NameSuffix::for_launch(launch));
        let planned = PlannedLabels {
            workspace: WorkspaceName::parse("p51b")?,
            role: RoleName::parse("p51b")?,
            agent: Agent::Codex,
            account: AccountName::parse("p51b")?,
            launch: launch.clone(),
        };
        let image_id = ImageId::parse(&format!("sha256:{}", "0".repeat(64)))?;
        let create = SandboxCreate::new(
            name.clone(),
            ImageTag::parse(&self.options.image)?,
            planned.complete(&image_id, jackin_core::DockerLabel::Off),
            Vec::new(),
            ClosedPolicy::parse(POLICY)?,
            MainProcess {
                program: SandboxPath::parse(SH)?,
                args: vec!["-c".to_owned(), "sleep 3600".to_owned()],
            },
        );
        self.gateway.create_sandbox(&create).await?;
        for _ in 0..READY_ATTEMPTS {
            let found = self
                .gateway
                .find_sandboxes(&SandboxQuery::Name(name.clone()))
                .await?;
            if found.first().is_some_and(|view| view.phase == Phase::Ready) {
                return Ok(name);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(format!("{name} never reached Ready").into())
    }

    async fn exec(
        &self,
        sandbox: &SandboxName,
        program: &str,
        args: &[&str],
        workdir: Option<&str>,
        env: &[(&str, &str)],
    ) -> Result<(i32, String), Box<dyn Error>> {
        let mut variables = BTreeMap::new();
        for (name, value) in env {
            variables.insert(jackin_core::EnvName::parse(name)?, (*value).to_owned());
        }
        let outcome = self
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::parse(program)?,
                    args: args.iter().map(|arg| (*arg).to_owned()).collect(),
                    workdir: workdir.map(SandboxPath::parse).transpose()?,
                    env: variables,
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: EXEC_TIMEOUT,
                },
                &CancellationToken::new(),
            )
            .await?;
        let mut text = String::from_utf8_lossy(&outcome.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&outcome.stderr));
        Ok((outcome.exit_code, text))
    }

    async fn show(
        &self,
        sandbox: &SandboxName,
        title: &str,
        script: &str,
    ) -> Result<(), Box<dyn Error>> {
        let (code, text) = self
            .exec(sandbox, SH, &["-c", script], Some("/sandbox/project"), &[])
            .await?;
        say(&format!("--- {title} (exit {code})\n{}", text.trim_end()))
    }

    async fn seed_stale(&self, sandbox: &SandboxName) -> Result<(), Box<dyn Error>> {
        for (path, content) in [
            ("/sandbox/project/stale/deep/file.txt", "stale"),
            ("/sandbox/project/.stale-dotfile", "stale"),
            ("/sandbox/project/.git/HEAD", "ref: refs/heads/stale\n"),
        ] {
            self.gateway
                .write_file(
                    sandbox,
                    &SandboxFile {
                        path: SandboxPath::parse(path)?,
                        mode: FileMode(0o644),
                        content: SecretBytes::new(content.as_bytes().to_vec()),
                    },
                )
                .await?;
        }
        say(
            "seeded /sandbox/project/stale/deep/file.txt, /sandbox/project/.stale-dotfile and a leftover /sandbox/project/.git/HEAD",
        )
    }

    async fn host_facts(&self) -> Result<(), Box<dyn Error>> {
        let root = self.project.path();
        let runner = self.runner.as_ref();
        let head = host_git(runner, root, &["symbolic-ref", "-q", "HEAD"])
            .await
            .unwrap_or_else(|_| "detached\n".to_owned());
        let commit = host_git(runner, root, &["rev-parse", "HEAD"])
            .await
            .unwrap_or_default();
        let format = host_git(runner, root, &["rev-parse", "--show-object-format"]).await?;
        let version = host_git(runner, root, &["--version"]).await?;
        say(&format!(
            "host: root={} head={} commit={} format={} {}",
            root.as_path().display(),
            head.trim(),
            commit.trim(),
            format.trim(),
            version.trim()
        ))
    }

    async fn sandbox_facts(&self, sandbox: &SandboxName) -> Result<(), Box<dyn Error>> {
        self.show(
            sandbox,
            "sandbox repository",
            "git --version; git symbolic-ref -q HEAD || echo detached; git rev-parse HEAD; git rev-parse --show-object-format; git rev-parse --is-shallow-repository; git status --porcelain --untracked-files=all | head -20; git config --local --list | head -20",
        )
        .await?;
        self.show(
            sandbox,
            "tree id of the sandbox working files (temporary index and object directory)",
            "d=$(mktemp -d /sandbox/.jackin/driver-gate.XXXXXX) && mkdir -p \"$d/objects\" && GIT_INDEX_FILE=\"$d/index\" GIT_OBJECT_DIRECTORY=\"$d/objects\" GIT_ALTERNATE_OBJECT_DIRECTORIES=/sandbox/project/.git/objects sh -c 'git add -A && git write-tree'; rm -rf \"$d\"",
        )
        .await?;
        self.show(
            sandbox,
            "ownership and modes (L3), safe.directory, exec user",
            "id; stat -c '%U:%G %a %n' /sandbox/project /sandbox/project/.git; find . -maxdepth 2 -type f ! -path './.git/*' -printf '%U:%G %m %p\\n' | sort | head -20; git config --show-origin --get-all safe.directory || echo 'no safe.directory'; git status >/dev/null && echo 'git status as the exec user: ok'",
        )
        .await?;
        self.show(
            sandbox,
            "file kinds (L11): symlinks and non-ASCII names",
            "find . -path ./.git -prune -o -type l -print | head -20; find . -path ./.git -prune -o -name '*[! -~]*' -print | head -20",
        )
        .await
    }

    async fn upload(&self, launch: &LaunchId) -> Result<Option<SandboxName>, Box<dyn Error>> {
        self.host_facts().await?;
        let sandbox = self.sandbox(launch).await?;
        say(&format!("sandbox {sandbox} is Ready"))?;
        if self.options.flags.contains(&Flag::Stale) {
            self.seed_stale(&sandbox).await?;
        }
        self.show(&sandbox, "disk before (L10)", "df -h /sandbox | tail -1")
            .await?;
        let started = Instant::now();
        let result = self
            .session
            .upload_project(&self.project, &sandbox, launch, &CancellationToken::new())
            .await;
        say(&format!(
            "upload_project took {:.1} s",
            started.elapsed().as_secs_f64()
        ))?;
        match result {
            Ok(()) => say("upload_project: Ok (the tree gate passed)")?,
            Err(error) => {
                say(&format!("upload_project: Err\n  {error}\n  {error:?}"))?;
                return Ok(Some(sandbox));
            }
        }
        self.show(&sandbox, "disk after (L10)", "df -h /sandbox | tail -1")
            .await?;
        self.sandbox_facts(&sandbox).await?;
        if self.options.flags.contains(&Flag::Stale) {
            self.show(
                &sandbox,
                "stale files after the upload (none expected)",
                "ls -A stale .stale-dotfile 2>&1; git rev-parse --abbrev-ref HEAD",
            )
            .await?;
        }
        Ok(Some(sandbox))
    }

    async fn sandbox_refs(&self, sandbox: &SandboxName) -> Result<String, Box<dyn Error>> {
        let (_, refs) = self
            .exec(
                sandbox,
                GIT,
                &["for-each-ref", "--format=%(refname)", "refs/"],
                Some("/sandbox/project"),
                &[],
            )
            .await?;
        Ok(refs)
    }

    async fn export(&self, sandbox: &SandboxName, round: u32) -> Result<(), Box<dyn Error>> {
        let root = self.project.path();
        let runner = self.runner.as_ref();
        let before = (
            host_git(
                runner,
                root,
                &["status", "--porcelain", "-z", "--untracked-files=all"],
            )
            .await?,
            host_git(runner, root, &["rev-parse", "HEAD"])
                .await
                .unwrap_or_default(),
        );
        let refs = self.sandbox_refs(sandbox).await?;
        say(&format!(
            "sandbox refs before the commit (round {round}):\n{}",
            refs.trim_end()
        ))?;
        if refs
            .lines()
            .any(|line| line.starts_with("refs/heads/jackin/"))
        {
            return Err("a refs/heads/jackin/* ref reached the sandbox".into());
        }
        let work = format!(
            "echo {SYNTHETIC_MARK}-{round} >> driver-note.txt && git add driver-note.txt && git -c user.name=driver -c user.email=driver@example.invalid commit -q -m 'driver commit {round}'"
        );
        self.show(sandbox, "commit in the sandbox", &work).await?;
        let remote = "/sandbox/.jackin/export.bundle";
        let (code, text) = self
            .exec(
                sandbox,
                GIT,
                &["bundle", "create", remote, "--branches"],
                Some("/sandbox/project"),
                &[],
            )
            .await?;
        say(&format!(
            "git bundle create in the sandbox: exit {code} {}",
            text.trim()
        ))?;
        let local = HostPath::parse(
            &self
                .options
                .work_dir
                .canonicalize()?
                .join(format!("export-{round}.bundle")),
        )?;
        self.gateway
            .download_file(
                sandbox,
                &SandboxPath::parse(remote)?,
                &local,
                &CancellationToken::new(),
            )
            .await?;
        say(&format!(
            "download_file: Ok, digests equal, {}",
            local.as_path().display()
        ))?;
        let target = format!("+refs/heads/*:refs/heads/jackin/{sandbox}/*");
        let bundle = local.as_path().display().to_string();
        host_git(runner, root, &["fetch", "-q", &bundle, &target]).await?;
        let listed = host_git(
            runner,
            root,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/heads/jackin/",
            ],
        )
        .await?;
        say(&format!(
            "host refs/heads/jackin/* after round {round}:\n{}",
            listed.trim_end()
        ))?;
        let after = (
            host_git(
                runner,
                root,
                &["status", "--porcelain", "-z", "--untracked-files=all"],
            )
            .await?,
            host_git(runner, root, &["rev-parse", "HEAD"])
                .await
                .unwrap_or_default(),
        );
        say(&format!(
            "host worktree and HEAD unchanged: {}",
            before == after
        ))
    }

    async fn drop_refs(&self) -> Result<(), Box<dyn Error>> {
        let root = self.project.path();
        let runner = self.runner.as_ref();
        let listed = host_git(
            runner,
            root,
            &["for-each-ref", "--format=%(refname)", "refs/heads/jackin/"],
        )
        .await?;
        for name in listed.lines() {
            host_git(runner, root, &["update-ref", "-d", name]).await?;
        }
        say("deleted the refs/heads/jackin/* refs of this run")
    }

    async fn upload_command(&self) -> Result<(), Box<dyn Error>> {
        for round in 1..=self.options.rounds {
            say(&format!("== round {round}"))?;
            let launch = LaunchId::generate()?;
            let Some(sandbox) = self.upload(&launch).await? else {
                continue;
            };
            let landed = self
                .exec(
                    &sandbox,
                    SH,
                    &["-c", "test -d .git"],
                    Some("/sandbox/project"),
                    &[],
                )
                .await
                .is_ok_and(|(code, _)| code == 0);
            if self.options.flags.contains(&Flag::Export) && landed {
                self.export(&sandbox, round).await?;
            }
            if !self.options.flags.contains(&Flag::Keep) {
                self.gateway.delete_sandbox(&sandbox).await?;
                say(&format!("deleted {sandbox}"))?;
            }
        }
        if self.options.flags.contains(&Flag::CleanupRefs) {
            self.drop_refs().await?;
        }
        Ok(())
    }

    async fn contract_command(&self) -> Result<(), Box<dyn Error>> {
        let fixture = ContractFixture {
            image: ImageTag::parse(&self.options.image)?,
            project: self.project.clone(),
            name_prefix: SandboxSlug::parse("ctr")?,
            shell_end_timeout: SHELL_END_TIMEOUT,
        };
        for (label, group) in [
            ("Core", ContractGroup::Core),
            ("Transfer", ContractGroup::Transfer),
            ("Terminal", ContractGroup::Terminal),
        ] {
            let report = run(self.gateway.as_ref(), &fixture, group).await;
            let mut failed = 0;
            for check in &report.checks {
                match &check.outcome {
                    Ok(()) => say(&format!("CHECK {label} {} Ok", check.name))?,
                    Err(message) => {
                        failed += 1;
                        say(&format!("CHECK {label} {} Err {message}", check.name))?;
                    }
                }
            }
            say(&format!(
                "GROUP {label} total={} failed={failed}",
                report.checks.len()
            ))?;
        }
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = options()?;
    let command = options.command;
    let live = Live::connect(options).await?;
    match command {
        Command::Upload => live.upload_command().await,
        Command::Contract => live.contract_command().await,
    }
}

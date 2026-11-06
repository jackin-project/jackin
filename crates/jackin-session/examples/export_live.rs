// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::io::Write;
use std::num::NonZeroU8;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    AccountName, Agent, Config, ConfigStore, FileSystem, GatewayName, HostPath, ImageId, ImageTag,
    LaunchId, NameSuffix, OpenShellWorkspace, Phase, PlannedLabels, PolicyLabel, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, Program, ProjectRoot, RoleName, SandboxName,
    SandboxPath, SandboxSlug, StdFileSystem, WorkspaceName,
};
use jackin_openshell::contract::{ContractFixture, ContractGroup, SHELL_END_TIMEOUT, run};
use jackin_openshell::{
    ClosedPolicy, Gateway, GrpcGateway, GrpcGatewayConfig, MainProcess, ProgramRun, ProgramStdin,
    SandboxCreate, SandboxQuery, resolve_gateway,
};
use jackin_session::{
    DEFAULT_BUNDLE_BYTES, DEFAULT_EXPORT_ENTRY_BYTES, DEFAULT_EXPORT_OBJECTS,
    DEFAULT_EXPORT_TOTAL_BYTES, HostPorts, LaunchLimits, PurgeMode, Session, SessionDeps,
    SystemClock, TokioProcessRunner,
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
const WORKSPACE_LABEL: &str = "p52";
const SYNTHETIC_MARK: &str = "jackin-p52-driver";
const EXEC_TIMEOUT: Duration = Duration::from_secs(300);
const READY_ATTEMPTS: usize = 240;

#[derive(Clone, Copy)]
enum Command {
    Scenario,
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
    cleanup_refs: bool,
}

fn options() -> Result<Options, Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let command = match args.next().as_deref() {
        Some("scenario") => Command::Scenario,
        Some("contract") => Command::Contract,
        other => {
            return Err(format!(
                "the first argument is scenario or contract, not {other:?}; flags: --gateway --workspace --image --project --openshell-home --work-dir [--cleanup-refs]"
            )
            .into());
        }
    };
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    let mut options = Options {
        command,
        gateway: "jackin-local".to_owned(),
        workspace: "default".to_owned(),
        image: String::new(),
        project: PathBuf::from("."),
        openshell_home: home.join(".config").join("openshell"),
        work_dir: std::env::temp_dir().join("jackin-p52-driver"),
        cleanup_refs: false,
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
            "--cleanup-refs" => options.cleanup_refs = true,
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

async fn host_read(
    runner: &dyn ProcessRunner,
    root: &HostPath,
    args: &[&str],
) -> Result<String, Box<dyn Error>> {
    let mut full = vec!["--no-optional-locks"];
    full.extend_from_slice(args);
    host_git(runner, root, &full).await
}

#[derive(PartialEq, Eq)]
struct HostState {
    status: String,
    head: String,
    index: String,
    refs: String,
}

impl HostState {
    fn compare(&self, other: &Self) -> [(&'static str, bool); 4] {
        [
            ("status", self.status == other.status),
            ("HEAD", self.head == other.head),
            ("index digest", self.index == other.index),
            ("non-jackin refs", self.refs == other.refs),
        ]
    }
}

struct Live {
    gateway: Arc<GrpcGateway>,
    session: Session,
    cancel: CancellationToken,
    runner: Arc<TokioProcessRunner>,
    project: ProjectRoot,
    config: Config,
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
        let cancel = CancellationToken::new();
        let port: Arc<dyn ProcessRunner> = Arc::<TokioProcessRunner>::clone(&runner);
        let fs: Arc<dyn FileSystem> = Arc::new(StdFileSystem::default());
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
                cancel: cancel.clone(),
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
        let root = HostPath::parse(&options.project.canonicalize()?)?;
        let project = ProjectRoot::open(fs.as_ref(), &root).await?;
        let config = Config::parse(&format!(
            "[workspaces.{WORKSPACE_LABEL}]\npath = \"{}\"\n",
            root.as_path().display()
        ))?;
        Ok(Live {
            gateway,
            session,
            cancel,
            runner,
            project,
            config,
            options,
        })
    }

    async fn sandbox(&self, launch: &LaunchId) -> Result<SandboxName, Box<dyn Error>> {
        let name = SandboxName::new(&SandboxSlug::parse("p52")?, &NameSuffix::for_launch(launch));
        let planned = PlannedLabels {
            workspace: WorkspaceName::parse(WORKSPACE_LABEL)?,
            role: RoleName::parse("p52")?,
            agent: Agent::Codex,
            account: AccountName::parse("p52")?,
            policy: PolicyLabel::of("policy"),
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
                program: SandboxPath::parse("/bin/sh")?,
                args: vec!["-c".to_owned(), "sleep 3600".to_owned()],
            },
        );
        self.gateway.create_sandbox(&create).await?;
        for _ in 0..READY_ATTEMPTS {
            let found = self
                .gateway
                .find_sandboxes(&SandboxQuery::Name(name.clone()), &self.cancel)
                .await?;
            if found.first().is_some_and(|view| view.phase == Phase::Ready) {
                return Ok(name);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(format!("{name} never reached Ready").into())
    }

    async fn sandbox_git(&self, sandbox: &SandboxName, script: &str) -> Result<(), Box<dyn Error>> {
        let outcome = self
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::parse("/bin/sh")?,
                    args: vec!["-c".to_owned(), script.to_owned()],
                    workdir: Some(SandboxPath::parse("/sandbox/project")?),
                    env: std::collections::BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: EXEC_TIMEOUT,
                },
                &self.cancel,
            )
            .await?;
        say(&format!(
            "sandbox: {script}\n  exit {} {}",
            outcome.exit_code,
            String::from_utf8_lossy(&outcome.stdout).trim()
        ))
    }

    async fn host_state(&self) -> Result<HostState, Box<dyn Error>> {
        let root = self.project.path();
        let runner = self.runner.as_ref();
        let index = host_read(
            runner,
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        )
        .await?;
        let index = std::fs::read(index.trim()).map_or_else(
            |_| "no index".to_owned(),
            |bytes| jackin_core::Sha256::of(&bytes).to_string(),
        );
        let status = host_read(
            runner,
            root,
            &["status", "--porcelain", "-z", "--untracked-files=all"],
        )
        .await?;
        let head = host_read(runner, root, &["rev-parse", "HEAD"])
            .await
            .unwrap_or_default();
        let refs = host_read(
            runner,
            root,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/heads",
                "refs/tags",
            ],
        )
        .await?
        .lines()
        .filter(|line| !line.starts_with("refs/heads/jackin/"))
        .collect::<Vec<_>>()
        .join("\n");
        Ok(HostState {
            status,
            head,
            index,
            refs,
        })
    }

    async fn exported_refs(&self) -> Result<String, Box<dyn Error>> {
        host_git(
            self.runner.as_ref(),
            self.project.path(),
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/heads/jackin/",
            ],
        )
        .await
    }

    async fn scenario(&self) -> Result<(), Box<dyn Error>> {
        let launch = LaunchId::generate()?;
        let sandbox = self.sandbox(&launch).await?;
        say(&format!("sandbox {sandbox} is Ready"))?;
        self.session
            .upload_project(&self.project, &sandbox, &launch, &self.cancel)
            .await?;
        let state = self
            .session
            .work_state(&self.config, &sandbox, &self.cancel)
            .await;
        say(&format!(
            "work_state after the upload (a fresh sandbox): {state:?}"
        ))?;
        let work = format!(
            "echo {SYNTHETIC_MARK}-main >> driver-note.txt && git add driver-note.txt && git -c user.name=driver -c user.email=driver@example.invalid commit -q -m 'driver main' && git checkout -q -b driver/feature && echo {SYNTHETIC_MARK}-feature >> driver-feature.txt && git add driver-feature.txt && git -c user.name=driver -c user.email=driver@example.invalid commit -q -m 'driver feature' && git checkout -q -"
        );
        self.sandbox_git(&sandbox, &work).await?;
        let state = self
            .session
            .work_state(&self.config, &sandbox, &self.cancel)
            .await;
        say(&format!("work_state after the sandbox commits: {state:?}"))?;
        let before = self.host_state().await?;
        let report = self
            .session
            .export(&self.config, &sandbox, self.cancel.clone())
            .await;
        say(&format!("export: {report:?}"))?;
        let after = self.host_state().await?;
        say(&format!(
            "host index, HEAD, status and non-jackin refs unchanged: {}",
            before == after
        ))?;
        for (name, same) in before.compare(&after) {
            say(&format!("  {name} unchanged: {same}"))?;
        }
        say(&format!(
            "host refs/heads/jackin/*:\n{}",
            self.exported_refs().await?.trim_end()
        ))?;
        let state = self
            .session
            .work_state(&self.config, &sandbox, &self.cancel)
            .await;
        say(&format!("work_state after the export: {state:?}"))?;
        self.sandbox_git(
            &sandbox,
            &format!("echo {SYNTHETIC_MARK}-late >> driver-late.txt && git add driver-late.txt && git -c user.name=driver -c user.email=driver@example.invalid commit -q -m 'driver late'"),
        )
        .await?;
        let refused = self
            .session
            .purge(&self.config, &sandbox, PurgeMode::Guarded, &self.cancel)
            .await;
        say(&format!("purge (guarded): {refused:?}"))?;
        let still = self
            .gateway
            .find_sandboxes(&SandboxQuery::Name(sandbox.clone()), &self.cancel)
            .await?;
        say(&format!(
            "sandbox still present after the refused purge: {}",
            !still.is_empty()
        ))?;
        let forced = self
            .session
            .purge(&self.config, &sandbox, PurgeMode::Forced, &self.cancel)
            .await;
        say(&format!("purge (forced): {forced:?}"))?;
        let gone = self
            .gateway
            .find_sandboxes(&SandboxQuery::Name(sandbox.clone()), &self.cancel)
            .await?;
        say(&format!(
            "sandbox gone after the forced purge: {}",
            gone.is_empty()
        ))?;
        let again = self
            .session
            .purge(&self.config, &sandbox, PurgeMode::Guarded, &self.cancel)
            .await;
        say(&format!("purge (repeated): {again:?}"))?;
        if self.options.cleanup_refs {
            for line in self.exported_refs().await?.lines() {
                if let Some((name, _)) = line.split_once(' ') {
                    host_git(
                        self.runner.as_ref(),
                        self.project.path(),
                        &["update-ref", "-d", name],
                    )
                    .await?;
                }
            }
            say("deleted the refs/heads/jackin/* refs of this run")?;
        }
        Ok(())
    }

    async fn contract(&self) -> Result<(), Box<dyn Error>> {
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
        Command::Scenario => live.scenario().await,
        Command::Contract => live.contract().await,
    }
}

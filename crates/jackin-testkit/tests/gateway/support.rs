// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Debug;
use std::num::NonZeroU16;
use std::time::Duration;

use jackin_core::{
    AccountName, Agent, EnvName, FileMode, GatewayName, HostPath, ImageId, ImageTag, LaunchId,
    NameSuffix, OpenShellWorkspace, Phase, PlannedLabels, PolicyLabel, ProjectRoot, ProviderName,
    RoleName, SandboxName, SandboxPath, SandboxSlug, SecretBytes, StdFileSystem, WorkspaceName,
};
use jackin_openshell::contract::{ContractFixture, SHELL_END_TIMEOUT};
use jackin_openshell::{
    AuthoredPolicy, Gateway, InputMode, MainProcess, NetworkRule, ProgramOutcome, ProgramRun,
    ProgramStdin, RepaintPolicy, SandboxCreate, SandboxFile, SandboxQuery, SandboxView, TermSize,
    TerminalEvent, TerminalKind, TerminalRequest, TerminalSession,
};
use jackin_testkit::{FakeGateway, SandboxHandle};
use tempfile::TempDir;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const IMAGE: &str = "jackin.invalid/contract-base:0123456789abcdef";
const AUTHORED_POLICY: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
    - /usr
  read_write:
    - /tmp
    - /sandbox
landlock:
  compatibility: hard_requirement
process:
  run_as_user: sandbox
  run_as_group: sandbox
";

const WAIT: Duration = Duration::from_secs(3);

pub(crate) use jackin_testkit::guard;

pub(crate) fn fake() -> Result<FakeGateway, Box<dyn Error>> {
    let fake = FakeGateway::new(
        GatewayName::parse("fake-gateway")?,
        OpenShellWorkspace::parse("default")?,
    );
    fake.add_image(&ImageTag::parse(IMAGE)?);
    Ok(fake)
}

pub(crate) fn path(text: &str) -> Result<SandboxPath, Box<dyn Error>> {
    Ok(SandboxPath::parse(text)?)
}

pub(crate) fn size(cols: u16, rows: u16) -> Result<TermSize, Box<dyn Error>> {
    Ok(TermSize {
        cols: NonZeroU16::new(cols).ok_or("zero columns")?,
        rows: NonZeroU16::new(rows).ok_or("zero rows")?,
    })
}

pub(crate) fn sh(script: &str) -> Result<MainProcess, Box<dyn Error>> {
    Ok(MainProcess {
        program: path("/bin/sh")?,
        args: vec!["-c".to_owned(), script.to_owned()],
    })
}

pub(crate) fn supervised() -> Result<MainProcess, Box<dyn Error>> {
    Ok(MainProcess {
        program: path("/usr/local/bin/jackin-start")?,
        args: vec!["codex".to_owned()],
    })
}

pub(crate) fn spec_with(
    launch: &LaunchId,
    image: &str,
    providers: Vec<ProviderName>,
    main: MainProcess,
) -> Result<SandboxCreate, Box<dyn Error>> {
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("work")?,
        role: RoleName::parse("backend")?,
        agent: Agent::Codex,
        account: AccountName::parse("work")?,
        policy: PolicyLabel::of("policy"),
        launch: launch.clone(),
    };
    let image_id = ImageId::parse(&format!("sha256:{}", "ba7816bf".repeat(8)))?;
    let name = SandboxName::new(
        &SandboxSlug::parse("work")?,
        &NameSuffix::for_launch(launch),
    );
    Ok(SandboxCreate::new(
        name,
        ImageTag::parse(image)?,
        planned.complete(&image_id, jackin_core::DockerLabel::Off),
        providers,
        AuthoredPolicy::parse(AUTHORED_POLICY)?
            .with_rule(NetworkRule::docker_engine(
                NonZeroU16::new(12376).ok_or("zero port")?,
            ))?
            .close()?,
        main,
    ))
}

pub(crate) fn spec(main: MainProcess) -> Result<SandboxCreate, Box<dyn Error>> {
    spec_with(&LaunchId::generate()?, IMAGE, Vec::new(), main)
}

pub(crate) async fn observe(
    fake: &FakeGateway,
    name: &SandboxName,
) -> Result<SandboxView, Box<dyn Error>> {
    fake.find_sandboxes(&SandboxQuery::Name(name.clone()))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| format!("{name} is not listed").into())
}

pub(crate) async fn ready(
    fake: &FakeGateway,
    main: MainProcess,
) -> Result<SandboxHandle, Box<dyn Error>> {
    let spec = spec(main)?;
    fake.create_sandbox(&spec).await?;
    let view = observe(fake, spec.name()).await?;
    if view.phase != Phase::Ready {
        return Err(format!("{} is {:?}", view.name, view.phase).into());
    }
    fake.sandbox(spec.name())
        .ok_or_else(|| "the sandbox is not listed".into())
}

pub(crate) fn program_run(
    sandbox: &SandboxName,
    program: &str,
    args: &[&str],
) -> Result<ProgramRun, Box<dyn Error>> {
    Ok(ProgramRun {
        sandbox: sandbox.clone(),
        program: path(program)?,
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        workdir: None,
        env: BTreeMap::<EnvName, String>::new(),
        stdin: ProgramStdin::Closed,
        touch_on_success: None,
        timeout: Duration::from_secs(30),
    })
}

pub(crate) async fn run(
    fake: &FakeGateway,
    run: &ProgramRun,
) -> Result<ProgramOutcome, Box<dyn Error>> {
    Ok(fake.run_program(run, &CancellationToken::new()).await?)
}

pub(crate) async fn write(
    fake: &FakeGateway,
    sandbox: &SandboxName,
    target: &str,
    content: &str,
) -> Result<(), Box<dyn Error>> {
    let file = SandboxFile {
        path: path(target)?,
        mode: FileMode(0o600),
        content: SecretBytes::new(content.as_bytes().to_vec()),
    };
    Ok(fake.write_file(sandbox, &file).await?)
}

pub(crate) async fn read(
    fake: &FakeGateway,
    sandbox: &SandboxName,
    target: &str,
) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
    let mut files = fake.read_files(sandbox, &[path(target)?]).await?;
    Ok(files.pop().flatten())
}

pub(crate) fn main_request(
    sandbox: &SandboxName,
    input: InputMode,
) -> Result<TerminalRequest, Box<dyn Error>> {
    Ok(TerminalRequest {
        sandbox: sandbox.clone(),
        kind: TerminalKind::MainProcess {
            input,
            repaint: RepaintPolicy::Never,
        },
        size: size(100, 30)?,
    })
}

pub(crate) async fn attach(
    fake: &FakeGateway,
    sandbox: &SandboxName,
    input: InputMode,
) -> Result<TerminalSession, Box<dyn Error>> {
    Ok(fake.open_terminal(&main_request(sandbox, input)?).await?)
}

pub(crate) async fn collect_until(
    label: &str,
    session: &mut TerminalSession,
    last: impl Fn(&TerminalEvent) -> bool,
) -> Result<Vec<TerminalEvent>, Box<dyn Error>> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match tokio::time::timeout_at(deadline, session.next_event()).await {
            Err(_) => return Err(format!("timed out waiting for {label}; saw {events:?}").into()),
            Ok(None) => {
                return Err(
                    format!("the session closed waiting for {label}; saw {events:?}").into(),
                );
            }
            Ok(Some(event)) => {
                let done = last(&event);
                events.push(event);
                if done {
                    return Ok(events);
                }
            }
        }
    }
}

pub(crate) async fn drain(
    label: &str,
    session: &mut TerminalSession,
) -> Result<Vec<TerminalEvent>, Box<dyn Error>> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match tokio::time::timeout_at(deadline, session.next_event()).await {
            Err(_) => return Err(format!("timed out waiting for {label}; saw {events:?}").into()),
            Ok(None) => return Ok(events),
            Ok(Some(event)) => events.push(event),
        }
    }
}

pub(crate) fn output_text(events: &[TerminalEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            TerminalEvent::Output(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect()
}

pub(crate) async fn eventually<T: PartialEq + Debug>(
    label: &str,
    observe: impl Fn() -> T,
    expected: T,
) -> TestResult {
    let waiting = async {
        while observe() != expected {
            tokio::task::yield_now().await;
        }
    };
    tokio::time::timeout(WAIT, waiting).await.map_err(|_| {
        format!(
            "timed out waiting for {label} to be {expected:?}; saw {:?}",
            observe()
        )
    })?;
    Ok(())
}

pub(crate) async fn git(dir: &std::path::Path, args: &[&str]) -> TestResult {
    let status = Command::from(jackin_testkit::git_command())
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("git {args:?} failed: {status}").into())
    }
}

pub(crate) async fn project() -> Result<(TempDir, ProjectRoot), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    git(dir.path(), &["init", "-q"]).await?;
    tokio::fs::write(dir.path().join("README.md"), b"fixture\n").await?;
    git(dir.path(), &["add", "."]).await?;
    git(dir.path(), &["commit", "-q", "-m", "fixture"]).await?;
    let root = ProjectRoot::open(&StdFileSystem::default(), &HostPath::parse(dir.path())?).await?;
    Ok((dir, root))
}

pub(crate) async fn fixture(
    fake: &FakeGateway,
) -> Result<(TempDir, ContractFixture), Box<dyn Error>> {
    let image = ImageTag::parse(IMAGE)?;
    fake.add_image(&image);
    let (dir, project) = project().await?;
    Ok((
        dir,
        ContractFixture {
            image,
            project,
            name_prefix: SandboxSlug::parse("ctr")?,
            shell_end_timeout: SHELL_END_TIMEOUT,
        },
    ))
}

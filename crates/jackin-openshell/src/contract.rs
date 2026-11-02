// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{
    AccountName, Agent, ChatgptAccountId, CredentialKeys, EnvName, FileMode, HostPath, ImageId,
    ImageTag, LabelSelector, LaunchId, NameSuffix, Phase, PlannedLabels, ProfileAsset, ProfileId,
    ProjectRoot, ProviderName, RoleName, SandboxName, SandboxPath, SandboxSlug, Secret,
    SecretBytes, WorkspaceName,
};
use tokio_util::sync::CancellationToken;

use crate::error::GatewayError;
use crate::gateway::{
    Gateway, MainProcess, PowerAction, ProfileSync, ProgramRun, ProgramStdin, ProjectUpload,
    ProviderMaterial, ProviderSpec, ProviderState, Provisioning, SandboxCreate, SandboxFile,
    SandboxQuery, SandboxView,
};
use crate::policy::{PolicyDocument, ProviderProfile};
use crate::terminal::{
    InputLease, InputMode, RepaintPolicy, TermSize, TerminalEvent, TerminalKind, TerminalRequest,
};

pub const CORE_CHECKS: [&str; 12] = [
    "create_find_by_label",
    "create_duplicate_name",
    "provisioning_failed_reported",
    "program_exit_code_and_stdin",
    "touch_on_success",
    "write_file_mode_and_atomicity",
    "read_missing_file",
    "profile_import_unchanged_updated",
    "provider_put_and_delete",
    "power_transitions",
    "ended_main_process_stop_start",
    "delete_sandbox",
];

pub const TRANSFER_CHECKS: [&str; 2] = ["upload_project", "download_with_digest"];

pub const TERMINAL_CHECKS: [&str; 2] = ["shell_exit_code", "main_attach_detach_replay"];

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const POLL_ATTEMPTS: usize = 240;
const PROGRAM_TIMEOUT: Duration = Duration::from_secs(60);
const MISSING_IMAGE: &str = "jackin/contract-missing:0000000000000000";
const BANNER: &str = "contract-banner";
const CONTRACT_POLICY: &str = "version: 1
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
  compatibility: best_effort
process:
  run_as_user: sandbox
  run_as_group: sandbox
";

fn profile_yaml(description: &str) -> String {
    format!(
        "id: jackin-contract
display_name: Contract
description: {description}
category: agent
inference_capable: false
credentials:
  - name: access_token
    description: contract access token
    env_vars: [JACKIN_CONTRACT_ACCESS]
    required: true
    auth_style: bearer
    header_name: authorization
    refresh:
      strategy: oauth2_refresh_token
      token_url: https://auth.example.invalid/oauth/token
      refresh_before_seconds: 300
      max_lifetime_seconds: 3600
      material:
        - name: client_id
          description: client id
          required: true
        - name: refresh_token
          description: refresh token
          required: true
          secret: true
  - name: account_id
    description: contract account
    env_vars: [JACKIN_CONTRACT_ACCOUNT]
    required: true
discovery:
  credentials: [access_token, account_id]
endpoints:
  - host: contract.example.invalid
    port: 443
    protocol: rest
    access: read-write
    enforcement: enforce
binaries:
  - /usr/bin/curl
"
    )
}

const CONTRACT_KEYS: CredentialKeys = CredentialKeys {
    access_token: "JACKIN_CONTRACT_ACCESS",
    refresh_token: "JACKIN_CONTRACT_REFRESH",
    account_id: "JACKIN_CONTRACT_ACCOUNT",
    id_token: "JACKIN_CONTRACT_ID",
    token_url: "https://auth.example.invalid/oauth/token",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContractGroup {
    Core,
    Transfer,
    Terminal,
}

#[derive(Clone, Debug)]
pub struct ContractFixture {
    pub image: ImageTag,
    pub project: ProjectRoot,
    pub name_prefix: SandboxSlug,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractReport {
    pub checks: Vec<ContractCheck>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractCheck {
    pub name: &'static str,
    pub outcome: Result<(), String>,
}

type Outcome<T> = Result<T, String>;

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn expect(condition: bool, message: &str) -> Outcome<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn path(value: &str) -> Outcome<SandboxPath> {
    SandboxPath::parse(value).map_err(text)
}

struct Ctx<'a> {
    gateway: &'a dyn Gateway,
    fixture: &'a ContractFixture,
}

impl Ctx<'_> {
    fn spec(
        &self,
        launch: &LaunchId,
        image: &ImageTag,
        main: MainProcess,
    ) -> Outcome<SandboxCreate> {
        let name = SandboxName::new(&self.fixture.name_prefix, &NameSuffix::for_launch(launch));
        let planned = PlannedLabels {
            workspace: WorkspaceName::parse("contract").map_err(text)?,
            role: RoleName::parse("contract").map_err(text)?,
            agent: Agent::Codex,
            account: AccountName::parse("contract").map_err(text)?,
            launch: launch.clone(),
        };
        let image_id = ImageId::parse(&format!("sha256:{}", "0".repeat(64))).map_err(text)?;
        Ok(SandboxCreate::new(
            name,
            image.clone(),
            planned.complete(&image_id),
            Vec::new(),
            PolicyDocument::parse_authored(CONTRACT_POLICY).map_err(text)?,
            main,
        ))
    }

    fn main(program: &str, args: &[&str]) -> Outcome<MainProcess> {
        Ok(MainProcess {
            program: path(program)?,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        })
    }

    async fn view(&self, name: &SandboxName) -> Outcome<SandboxView> {
        let found = self
            .gateway
            .find_sandboxes(&SandboxQuery::Name(name.clone()))
            .await
            .map_err(text)?;
        found
            .into_iter()
            .next()
            .ok_or_else(|| format!("{name} is not listed"))
    }

    async fn wait_for(
        &self,
        name: &SandboxName,
        what: &str,
        done: impl Fn(&SandboxView) -> bool,
    ) -> Outcome<SandboxView> {
        for _ in 0..POLL_ATTEMPTS {
            let view = self.view(name).await?;
            if done(&view) {
                return Ok(view);
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        Err(format!("{name} never reached {what}"))
    }

    async fn ready(&self, main: MainProcess) -> Outcome<SandboxName> {
        let launch = LaunchId::generate();
        let spec = self.spec(&launch, &self.fixture.image, main)?;
        self.gateway.create_sandbox(&spec).await.map_err(text)?;
        self.wait_for(spec.name(), "Ready", |view| view.phase == Phase::Ready)
            .await?;
        Ok(spec.name().clone())
    }

    async fn sleeper(&self) -> Outcome<SandboxName> {
        self.ready(Self::main("/bin/sh", &["-c", "sleep 3600"])?)
            .await
    }

    async fn cleanup(&self, name: &SandboxName) {
        drop(self.gateway.delete_sandbox(name).await);
    }

    async fn exec(
        &self,
        sandbox: &SandboxName,
        script: &str,
        arguments: &[&str],
        stdin: ProgramStdin,
        touch: Option<SandboxPath>,
    ) -> Outcome<crate::gateway::ProgramOutcome> {
        let mut args = vec!["-c".to_owned(), script.to_owned(), "contract".to_owned()];
        args.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        let run = ProgramRun {
            sandbox: sandbox.clone(),
            program: path("/bin/sh")?,
            args,
            workdir: None,
            env: BTreeMap::<EnvName, String>::new(),
            stdin,
            touch_on_success: touch,
            timeout: PROGRAM_TIMEOUT,
        };
        self.gateway
            .run_program(&run, &CancellationToken::new())
            .await
            .map_err(text)
    }
}

async fn create_find_by_label(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = LaunchId::generate();
    let spec = ctx.spec(
        &launch,
        &ctx.fixture.image,
        Ctx::main("/bin/sh", &["-c", "sleep 3600"])?,
    )?;
    ctx.gateway.create_sandbox(&spec).await.map_err(text)?;
    let found = ctx
        .gateway
        .find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(&launch)))
        .await
        .map_err(text);
    ctx.cleanup(spec.name()).await;
    let found = found?;
    expect(
        found.len() == 1,
        "the launch label does not select exactly one sandbox",
    )?;
    expect(
        found[0].name == *spec.name(),
        "the launch label selected another sandbox",
    )?;
    expect(
        jackin_core::SandboxLabels::from_labels(&found[0].labels)
            .is_ok_and(|labels| labels == *spec.labels()),
        "the stored labels differ from the requested labels",
    )
}

async fn create_duplicate_name(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = LaunchId::generate();
    let spec = ctx.spec(
        &launch,
        &ctx.fixture.image,
        Ctx::main("/bin/sh", &["-c", "sleep 3600"])?,
    )?;
    ctx.gateway.create_sandbox(&spec).await.map_err(text)?;
    let again = ctx.gateway.create_sandbox(&spec).await;
    ctx.cleanup(spec.name()).await;
    expect(
        matches!(again, Err(GatewayError::AlreadyExists { .. })),
        "a second create with the same name is not AlreadyExists",
    )
}

async fn provisioning_failed_reported(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = LaunchId::generate();
    let missing = ImageTag::parse(MISSING_IMAGE).map_err(text)?;
    let spec = ctx.spec(&launch, &missing, Ctx::main("/bin/sh", &["-c", "exit 0"])?)?;
    ctx.gateway.create_sandbox(&spec).await.map_err(text)?;
    let seen = ctx
        .wait_for(spec.name(), "a failed provisioning", |view| {
            matches!(view.provisioning, Provisioning::Failed { .. })
        })
        .await;
    ctx.cleanup(spec.name()).await;
    seen.map(|_| ())
}

async fn program_exit_code_and_stdin(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let outcome = ctx
        .exec(
            &sandbox,
            "cat; exit 3",
            &[],
            ProgramStdin::Data(SecretBytes::new(b"contract-stdin".to_vec())),
            None,
        )
        .await;
    let closed = ctx
        .exec(
            &sandbox,
            "cat; echo closed",
            &[],
            ProgramStdin::Closed,
            None,
        )
        .await;
    ctx.cleanup(&sandbox).await;
    let outcome = outcome?;
    expect(outcome.exit_code == 3, "the exit code is not 3")?;
    expect(
        outcome.stdout == b"contract-stdin",
        "stdin did not reach the program",
    )?;
    expect(
        closed?.stdout == b"closed\n",
        "a closed stdin did not end the program",
    )
}

async fn marker_exists(
    ctx: &Ctx<'_>,
    sandbox: &SandboxName,
    marker: &SandboxPath,
) -> Outcome<bool> {
    let files = ctx
        .gateway
        .read_files(sandbox, std::slice::from_ref(marker))
        .await
        .map_err(text)?;
    Ok(files.first().is_some_and(Option::is_some))
}

async fn touch_on_success(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        ctx.exec(
            &sandbox,
            "mkdir -p -- \"$1\"",
            &["/sandbox/.jackin"],
            ProgramStdin::Closed,
            None,
        )
        .await?;
        let ok = path("/sandbox/.jackin/contract-ok")?;
        let bad = path("/sandbox/.jackin/contract-bad")?;
        ctx.exec(
            &sandbox,
            "exit 0",
            &[],
            ProgramStdin::Closed,
            Some(ok.clone()),
        )
        .await?;
        ctx.exec(
            &sandbox,
            "exit 1",
            &[],
            ProgramStdin::Closed,
            Some(bad.clone()),
        )
        .await?;
        expect(
            marker_exists(ctx, &sandbox, &ok).await?,
            "no marker after exit 0",
        )?;
        expect(
            !marker_exists(ctx, &sandbox, &bad).await?,
            "a marker after exit 1",
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn write_file_mode_and_atomicity(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        let target = path("/sandbox/.contract/secret.json")?;
        for content in [&b"first"[..], &b"second"[..]] {
            let file = SandboxFile {
                path: target.clone(),
                mode: FileMode(0o600),
                content: SecretBytes::new(content.to_vec()),
            };
            ctx.gateway
                .write_file(&sandbox, &file)
                .await
                .map_err(text)?;
        }
        let read = ctx
            .gateway
            .read_files(&sandbox, std::slice::from_ref(&target))
            .await
            .map_err(text)?;
        expect(
            read == vec![Some(b"second".to_vec())],
            "the file content is not the last write",
        )?;
        let listing = ctx
            .exec(
                &sandbox,
                "find \"$1\" -perm 600 -type f; ls -A \"$1\"",
                &["/sandbox/.contract"],
                ProgramStdin::Closed,
                None,
            )
            .await?;
        let listing = String::from_utf8_lossy(&listing.stdout).into_owned();
        expect(listing.contains("secret.json"), "the mode is not 0600")?;
        expect(
            listing
                .lines()
                .filter(|line| line.contains("secret.json"))
                .count()
                == 2
                && listing.lines().count() == 2,
            "a temporary file is left behind",
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn read_missing_file(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let missing = path("/sandbox/.contract/absent")?;
    let read = ctx
        .gateway
        .read_files(&sandbox, std::slice::from_ref(&missing))
        .await
        .map_err(text);
    ctx.cleanup(&sandbox).await;
    expect(
        read? == vec![None],
        "a missing file is not reported as None",
    )
}

async fn profile_import_unchanged_updated(ctx: &Ctx<'_>) -> Outcome<()> {
    let sync = |description: &'static str| async move {
        let yaml: &'static str = Box::leak(profile_yaml(description).into_boxed_str());
        let asset: &'static ProfileAsset = Box::leak(Box::new(ProfileAsset {
            id: "jackin-contract",
            yaml,
        }));
        let profile = ProviderProfile::parse(asset).map_err(text)?;
        ctx.gateway.ensure_profile(&profile).await.map_err(text)
    };
    let first = sync("first").await?;
    expect(
        matches!(
            first,
            ProfileSync::Imported | ProfileSync::Updated | ProfileSync::Unchanged
        ),
        "the first sync has no outcome",
    )?;
    expect(
        sync("first").await? == ProfileSync::Unchanged,
        "an equal profile is not Unchanged",
    )?;
    expect(
        sync("second").await? == ProfileSync::Updated,
        "a changed profile is not Updated",
    )?;
    expect(
        sync("second").await? == ProfileSync::Unchanged,
        "the updated profile is not Unchanged",
    )
}

async fn provider_put_and_delete(ctx: &Ctx<'_>) -> Outcome<()> {
    profile_import_unchanged_updated(ctx).await?;
    let name = ProviderName::parse(&format!("jackin-{}-contract", ctx.fixture.name_prefix))
        .map_err(text)?;
    let spec = ProviderSpec {
        name: name.clone(),
        profile: ProfileId::parse("jackin-contract").map_err(text)?,
        keys: CONTRACT_KEYS,
        material: ProviderMaterial::ChatgptLogin {
            access_token: Secret::new("contract-access-token".to_owned()),
            refresh_token: Secret::new("contract-refresh-token".to_owned()),
            client_id: "contract-client".to_owned(),
            chatgpt_account_id: ChatgptAccountId::parse("0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9")
                .map_err(text)?,
        },
    };
    let state = |expected: ProviderState| {
        let name = name.clone();
        async move {
            ctx.gateway
                .provider_state(&name)
                .await
                .map_err(text)
                .map(|state| state == expected)
        }
    };
    ctx.gateway.put_provider(&spec).await.map_err(text)?;
    ctx.gateway.put_provider(&spec).await.map_err(text)?;
    expect(
        state(ProviderState::Present).await?,
        "the provider is absent after a put",
    )?;
    ctx.gateway.delete_provider(&name).await.map_err(text)?;
    expect(
        state(ProviderState::Absent).await?,
        "the provider is present after a delete",
    )
}

async fn power_transitions(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        let stopped = ctx
            .gateway
            .set_power(&sandbox, PowerAction::Stop)
            .await
            .map_err(text)?;
        expect(
            matches!(stopped.phase, Phase::Stopped | Phase::Stopping),
            "Stop does not stop the sandbox",
        )?;
        ctx.wait_for(&sandbox, "Stopped", |view| view.phase == Phase::Stopped)
            .await?;
        ctx.gateway
            .set_power(&sandbox, PowerAction::Start)
            .await
            .map_err(text)?;
        ctx.wait_for(&sandbox, "Ready", |view| view.phase == Phase::Ready)
            .await?;
        Ok(())
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn ended_main_process_stop_start(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = LaunchId::generate();
    let spec = ctx.spec(
        &launch,
        &ctx.fixture.image,
        Ctx::main("/bin/sh", &["-c", "exit 7"])?,
    )?;
    ctx.gateway.create_sandbox(&spec).await.map_err(text)?;
    let result = async {
        ctx.wait_for(spec.name(), "an ended main process", |view| {
            view.phase.main_process_ended()
        })
        .await?;
        let stop = ctx.gateway.set_power(spec.name(), PowerAction::Stop).await;
        expect(
            matches!(stop, Err(GatewayError::MainProcessFailed { .. })),
            "Stop on an ended main process is not MainProcessFailed",
        )?;
        let start = ctx.gateway.set_power(spec.name(), PowerAction::Start).await;
        expect(
            matches!(start, Err(GatewayError::FailedPrecondition { .. })),
            "Start on an ended main process is not FailedPrecondition",
        )
    }
    .await;
    ctx.cleanup(spec.name()).await;
    result
}

async fn delete_sandbox(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    ctx.gateway.delete_sandbox(&sandbox).await.map_err(text)?;
    for _ in 0..POLL_ATTEMPTS {
        if ctx
            .gateway
            .find_sandboxes(&SandboxQuery::Name(sandbox.clone()))
            .await
            .map_err(text)?
            .is_empty()
        {
            return ctx.gateway.delete_sandbox(&sandbox).await.map_err(text);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err("the sandbox is still listed after a delete".to_owned())
}

async fn upload_project(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let upload = ProjectUpload {
        source: ctx.fixture.project.clone(),
        dest: path("/sandbox/project")?,
    };
    let result = async {
        let cancel = CancellationToken::new();
        ctx.gateway
            .upload_project(&sandbox, &upload, &cancel)
            .await
            .map_err(text)?;
        ctx.gateway
            .upload_project(&sandbox, &upload, &cancel)
            .await
            .map_err(text)?;
        let listing = ctx
            .exec(
                &sandbox,
                "ls -A \"$1\"",
                &["/sandbox/project"],
                ProgramStdin::Closed,
                None,
            )
            .await?;
        expect(
            String::from_utf8_lossy(&listing.stdout)
                .lines()
                .any(|line| line == ".git"),
            "the git directory is not in the sandbox",
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn download_with_digest(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        let remote = path("/sandbox/.contract/export.bin")?;
        let file = SandboxFile {
            path: remote.clone(),
            mode: FileMode(0o600),
            content: SecretBytes::new(b"contract-export".to_vec()),
        };
        ctx.gateway
            .write_file(&sandbox, &file)
            .await
            .map_err(text)?;
        let local =
            std::env::temp_dir().join(format!("jackin-contract-{}.bin", LaunchId::generate()));
        let local = HostPath::parse(&local).map_err(text)?;
        ctx.gateway
            .download_file(&sandbox, &remote, &local, &CancellationToken::new())
            .await
            .map_err(text)?;
        let bytes = tokio::fs::read(local.as_path()).await.map_err(text)?;
        drop(tokio::fs::remove_file(local.as_path()).await);
        expect(bytes == b"contract-export", "the downloaded bytes differ")
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

fn term_size() -> Outcome<TermSize> {
    Ok(TermSize {
        cols: std::num::NonZeroU16::new(100).ok_or("zero columns")?,
        rows: std::num::NonZeroU16::new(30).ok_or("zero rows")?,
    })
}

async fn shell_exit_code(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        let request = TerminalRequest {
            sandbox: sandbox.clone(),
            kind: TerminalKind::Shell {
                workdir: path("/sandbox")?,
                env: BTreeMap::new(),
            },
            size: term_size()?,
        };
        let mut session = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        session.input(b"exit 3\n".to_vec()).await.map_err(text)?;
        let mut exit = None;
        while let Some(event) = session.next_event().await {
            if let TerminalEvent::Exited(code) = event {
                exit = code;
            }
        }
        expect(exit == Some(3), "the shell exit code is not 3")
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn read_until(
    session: &mut crate::terminal::TerminalSession,
    needle: &str,
) -> Outcome<InputLease> {
    let mut lease = InputLease::ReadOnly;
    let mut seen = String::new();
    tokio::time::timeout(PROGRAM_TIMEOUT, async {
        while let Some(event) = session.next_event().await {
            match event {
                TerminalEvent::Lease(granted) => lease = granted,
                TerminalEvent::Output(bytes) => {
                    seen.push_str(&String::from_utf8_lossy(&bytes));
                    if seen.contains(needle) {
                        return Ok(lease);
                    }
                }
                TerminalEvent::Ended(reason) => {
                    return Err(format!("the terminal ended: {reason:?}"));
                }
                TerminalEvent::Attached | TerminalEvent::Notice(_) | TerminalEvent::Exited(_) => {}
            }
        }
        Err("the terminal closed".to_owned())
    })
    .await
    .map_err(|_| format!("{needle} never arrived"))?
}

async fn main_attach_detach_replay(ctx: &Ctx<'_>) -> Outcome<()> {
    let script = format!("echo {BANNER}; sleep 3600");
    let sandbox = ctx.ready(Ctx::main("/bin/sh", &["-c", &script])?).await?;
    let result = async {
        let request = TerminalRequest {
            sandbox: sandbox.clone(),
            kind: TerminalKind::MainProcess {
                input: InputMode::Owner,
                repaint: RepaintPolicy::Never,
            },
            size: term_size()?,
        };
        let mut first = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        let lease = read_until(&mut first, BANNER).await?;
        expect(
            lease == InputLease::Owner,
            "the first attachment is not the owner",
        )?;
        first.detach().await.map_err(text)?;
        drop(first);
        let mut second = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        read_until(&mut second, BANNER).await?;
        second.detach().await.map_err(text)?;
        Ok(())
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

macro_rules! check {
    ($checks:ident, $ctx:ident, $name:literal, $run:ident) => {
        $checks.push(ContractCheck {
            name: $name,
            outcome: $run(&$ctx).await,
        });
    };
}

pub async fn run(
    gateway: &dyn Gateway,
    fixture: &ContractFixture,
    group: ContractGroup,
) -> ContractReport {
    let ctx = Ctx { gateway, fixture };
    let mut checks = Vec::new();
    match group {
        ContractGroup::Core => {
            check!(checks, ctx, "create_find_by_label", create_find_by_label);
            check!(checks, ctx, "create_duplicate_name", create_duplicate_name);
            check!(
                checks,
                ctx,
                "provisioning_failed_reported",
                provisioning_failed_reported
            );
            check!(
                checks,
                ctx,
                "program_exit_code_and_stdin",
                program_exit_code_and_stdin
            );
            check!(checks, ctx, "touch_on_success", touch_on_success);
            check!(
                checks,
                ctx,
                "write_file_mode_and_atomicity",
                write_file_mode_and_atomicity
            );
            check!(checks, ctx, "read_missing_file", read_missing_file);
            check!(
                checks,
                ctx,
                "profile_import_unchanged_updated",
                profile_import_unchanged_updated
            );
            check!(
                checks,
                ctx,
                "provider_put_and_delete",
                provider_put_and_delete
            );
            check!(checks, ctx, "power_transitions", power_transitions);
            check!(
                checks,
                ctx,
                "ended_main_process_stop_start",
                ended_main_process_stop_start
            );
            check!(checks, ctx, "delete_sandbox", delete_sandbox);
        }
        ContractGroup::Transfer => {
            check!(checks, ctx, "upload_project", upload_project);
            check!(checks, ctx, "download_with_digest", download_with_digest);
        }
        ContractGroup::Terminal => {
            check!(checks, ctx, "shell_exit_code", shell_exit_code);
            check!(
                checks,
                ctx,
                "main_attach_detach_replay",
                main_attach_detach_replay
            );
        }
    }
    ContractReport { checks }
}

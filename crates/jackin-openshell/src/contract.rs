// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jackin_core::{
    AccountName, Agent, ChatgptAccountId, CredentialKeys, EnvName, FileMode, HostPath, ImageId,
    ImageTag, LabelSelector, LaunchId, NameSuffix, Phase, PlannedLabels, ProcessStatus,
    ProfileAsset, ProfileId, ProjectRoot, ProviderName, RoleName, SandboxName, SandboxPath,
    SandboxSlug, Secret, SecretBytes, Sha256, StdFileSystem, WorkspaceName,
};
use tokio_util::sync::CancellationToken;

use crate::error::{GatewayError, Missing, TransferOp};
use crate::gateway::{
    Gateway, MainProcess, PowerAction, ProfileSync, ProgramRun, ProgramStdin, ProviderMaterial,
    ProviderSpec, ProviderState, Provisioning, SandboxCreate, SandboxFile, SandboxQuery,
    SandboxView,
};
use crate::policy::{ClosedPolicy, ProviderProfile};
use crate::terminal::{
    EndReason, InputLease, InputMode, RepaintPolicy, TermSize, TerminalEvent, TerminalKind,
    TerminalRequest, TerminalSession,
};

pub const CORE_CHECKS: [&str; 15] = [
    "create_find_by_label",
    "create_duplicate_name",
    "absent_sandbox_is_not_found",
    "provisioning_failed_reported",
    "program_exit_code_and_stdin",
    "touch_on_success",
    "write_file_mode_and_atomicity",
    "read_missing_file",
    "profile_import_unchanged_updated",
    "provider_put_and_delete",
    "delete_provider_while_attached_is_refused",
    "power_transitions",
    "stop_on_stopped_is_ok",
    "ended_main_process_stop_start",
    "delete_sandbox",
];

pub const TRANSFER_CHECKS: [&str; 8] = [
    "upload_tree",
    "upload_file_with_digest",
    "download_with_digest",
    "download_missing_remote_is_a_size_error",
    "download_pre_cancelled_is_cancelled",
    "upload_absent_sandbox_is_a_transfer_error",
    "upload_stopped_sandbox_is_a_transfer_error",
    "upload_through_a_symlinked_parent_is_a_transfer_error",
];

pub const TERMINAL_CHECKS: [&str; 6] = [
    "shell_exit_code",
    "main_attach_detach_replay",
    "main_second_attachment_read_only",
    "main_viewer_read_only",
    "main_read_only_takes_lease_after_owner_leaves",
    "main_ended_is_refused",
];

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const POLL_ATTEMPTS: usize = 240;
const PROGRAM_TIMEOUT: Duration = Duration::from_secs(60);
const MISSING_IMAGE: &str = "jackin.invalid/contract-missing:0000000000000000";
const BANNER: &str = "contract-banner";
const CTRL_C: u8 = 0x03;
const CTRL_D: u8 = 0x04;
const LEASE_RETRY: Duration = Duration::from_millis(500);
const VIEWER_SETTLE: Duration = Duration::from_millis(500);
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
  compatibility: hard_requirement
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

fn fresh_launch() -> Outcome<LaunchId> {
    LaunchId::generate().map_err(text)
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
        self.spec_with(launch, image, Vec::new(), main)
    }

    fn spec_with(
        &self,
        launch: &LaunchId,
        image: &ImageTag,
        providers: Vec<ProviderName>,
        main: MainProcess,
    ) -> Outcome<SandboxCreate> {
        let name = SandboxName::new(&self.fixture.name_prefix, &NameSuffix::for_launch(launch));
        let planned = PlannedLabels {
            workspace: WorkspaceName::parse("contract").map_err(text)?,
            role: RoleName::parse("contract").map_err(text)?,
            agent: Agent::Codex,
            account: AccountName::parse("contract").map_err(text)?,
            launch: launch.clone(),
            docker: jackin_core::DockerLabel::Off,
        };
        let image_id = ImageId::parse(&format!("sha256:{}", "0".repeat(64))).map_err(text)?;
        Ok(SandboxCreate::new(
            name,
            image.clone(),
            planned.complete(&image_id),
            providers,
            ClosedPolicy::parse(CONTRACT_POLICY).map_err(text)?,
            main,
        ))
    }

    fn main(program: &str, args: &[&str]) -> Outcome<MainProcess> {
        Ok(MainProcess {
            program: path(program)?,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        })
    }

    fn owner_kind() -> TerminalKind {
        TerminalKind::MainProcess {
            input: InputMode::Owner,
            repaint: RepaintPolicy::Never,
        }
    }

    fn terminal(sandbox: &SandboxName, kind: TerminalKind) -> Outcome<TerminalRequest> {
        Ok(TerminalRequest {
            sandbox: sandbox.clone(),
            kind,
            size: term_size()?,
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
        let launch = fresh_launch()?;
        let spec = self.spec(&launch, &self.fixture.image, main)?;
        self.gateway.create_sandbox(&spec).await.map_err(text)?;
        let name = spec.name().clone();
        match self
            .wait_for(&name, "Ready", |view| view.phase == Phase::Ready)
            .await
        {
            Ok(_) => Ok(name),
            Err(error) => {
                self.cleanup(&name).await;
                Err(error)
            }
        }
    }

    async fn sleeper(&self) -> Outcome<SandboxName> {
        self.ready(Self::main("/bin/sh", &["-c", "sleep 3600"])?)
            .await
    }

    async fn cleanup(&self, name: &SandboxName) {
        drop(self.gateway.delete_sandbox(name).await);
    }

    async fn gone(&self, name: &SandboxName) -> Outcome<()> {
        for _ in 0..POLL_ATTEMPTS {
            if self
                .gateway
                .find_sandboxes(&SandboxQuery::Name(name.clone()))
                .await
                .map_err(text)?
                .is_empty()
            {
                return Ok(());
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        Err("the sandbox is still listed after a delete".to_owned())
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
    let launch = fresh_launch()?;
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
    let launch = fresh_launch()?;
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

async fn absent_sandbox_is_not_found(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = fresh_launch()?;
    let name = SandboxName::new(&ctx.fixture.name_prefix, &NameSuffix::for_launch(&launch));
    let gateway = ctx.gateway;
    let run = ProgramRun {
        sandbox: name.clone(),
        program: path("/bin/true")?,
        args: Vec::new(),
        workdir: None,
        env: BTreeMap::new(),
        stdin: ProgramStdin::Closed,
        touch_on_success: None,
        timeout: PROGRAM_TIMEOUT,
    };
    let target = path("/sandbox/.contract/absent")?;
    let file = SandboxFile {
        path: target.clone(),
        mode: FileMode(0o600),
        content: SecretBytes::new(b"absent".to_vec()),
    };
    let shell = Ctx::terminal(
        &name,
        TerminalKind::Shell {
            workdir: path("/sandbox")?,
            env: BTreeMap::new(),
        },
    )?;
    let main = Ctx::terminal(&name, Ctx::owner_kind())?;
    let results = [
        (
            "run_program",
            gateway
                .run_program(&run, &CancellationToken::new())
                .await
                .map(drop),
        ),
        (
            "read_files",
            gateway
                .read_files(&name, std::slice::from_ref(&target))
                .await
                .map(drop),
        ),
        ("write_file", gateway.write_file(&name, &file).await),
        (
            "open_terminal(Shell)",
            gateway.open_terminal(&shell).await.map(drop),
        ),
        (
            "open_terminal(MainProcess)",
            gateway.open_terminal(&main).await.map(drop),
        ),
    ];
    for (entry, result) in results {
        match result {
            Err(GatewayError::NotFound {
                what: Missing::Sandbox(found),
            }) if found == name => {}
            other => {
                return Err(format!(
                    "{entry} on an absent sandbox is not NotFound for it: {other:?}"
                ));
            }
        }
    }
    Ok(())
}

async fn provisioning_failed_reported(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = fresh_launch()?;
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

fn contract_profile() -> Outcome<ProfileId> {
    ProfileId::parse("jackin-contract").map_err(text)
}

fn contract_provider(ctx: &Ctx<'_>) -> Outcome<ProviderName> {
    ProviderName::parse(&format!("jackin-{}-contract", ctx.fixture.name_prefix)).map_err(text)
}

async fn clear_contract_profile(ctx: &Ctx<'_>) -> Outcome<()> {
    ctx.gateway
        .delete_provider(&contract_provider(ctx)?)
        .await
        .map_err(text)?;
    ctx.gateway
        .delete_profile(&contract_profile()?)
        .await
        .map_err(text)
}

async fn sync_contract_profile(ctx: &Ctx<'_>, description: &'static str) -> Outcome<ProfileSync> {
    let yaml: &'static str = Box::leak(profile_yaml(description).into_boxed_str());
    let asset: &'static ProfileAsset = Box::leak(Box::new(ProfileAsset {
        id: "jackin-contract",
        yaml,
    }));
    let profile = ProviderProfile::parse(asset).map_err(text)?;
    ctx.gateway.ensure_profile(&profile).await.map_err(text)
}

async fn finish_contract_profile(ctx: &Ctx<'_>, result: Outcome<()>) -> Outcome<()> {
    let cleanup = clear_contract_profile(ctx).await;
    result.and(cleanup)
}

async fn profile_import_unchanged_updated(ctx: &Ctx<'_>) -> Outcome<()> {
    clear_contract_profile(ctx).await?;
    let result = async {
        expect(
            sync_contract_profile(ctx, "first").await? == ProfileSync::Imported,
            "the first sync is not Imported",
        )?;
        expect(
            sync_contract_profile(ctx, "first").await? == ProfileSync::Unchanged,
            "an equal profile is not Unchanged",
        )?;
        expect(
            sync_contract_profile(ctx, "second").await? == ProfileSync::Updated,
            "a changed profile is not Updated",
        )?;
        expect(
            sync_contract_profile(ctx, "second").await? == ProfileSync::Unchanged,
            "the updated profile is not Unchanged",
        )
    }
    .await;
    finish_contract_profile(ctx, result).await
}

fn contract_provider_spec(ctx: &Ctx<'_>) -> Outcome<ProviderSpec> {
    Ok(ProviderSpec {
        name: contract_provider(ctx)?,
        profile: contract_profile()?,
        keys: CONTRACT_KEYS,
        material: ProviderMaterial::ChatgptLogin {
            access_token: Secret::new("contract-access-token".to_owned()),
            refresh_token: Secret::new("contract-refresh-token".to_owned()),
            client_id: "contract-client".to_owned(),
            chatgpt_account_id: ChatgptAccountId::parse("0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9")
                .map_err(text)?,
        },
    })
}

async fn provider_state_is(
    ctx: &Ctx<'_>,
    name: &ProviderName,
    expected: ProviderState,
) -> Outcome<bool> {
    ctx.gateway
        .provider_state(name)
        .await
        .map_err(text)
        .map(|state| state == expected)
}

async fn provider_put_and_delete(ctx: &Ctx<'_>) -> Outcome<()> {
    clear_contract_profile(ctx).await?;
    let result = async {
        sync_contract_profile(ctx, "first").await?;
        let spec = contract_provider_spec(ctx)?;
        let name = spec.name.clone();
        ctx.gateway.put_provider(&spec).await.map_err(text)?;
        ctx.gateway.put_provider(&spec).await.map_err(text)?;
        expect(
            provider_state_is(ctx, &name, ProviderState::Present).await?,
            "the provider is absent after a put",
        )?;
        ctx.gateway.delete_provider(&name).await.map_err(text)?;
        expect(
            provider_state_is(ctx, &name, ProviderState::Absent).await?,
            "the provider is present after a delete",
        )?;
        ctx.gateway
            .delete_provider(&name)
            .await
            .map_err(|error| format!("the second delete_provider failed: {error}"))?;
        expect(
            provider_state_is(ctx, &name, ProviderState::Absent).await?,
            "the provider is present after the second delete",
        )
    }
    .await;
    finish_contract_profile(ctx, result).await
}

async fn delete_provider_while_attached_is_refused(ctx: &Ctx<'_>) -> Outcome<()> {
    clear_contract_profile(ctx).await?;
    let mut created = None;
    let result = async {
        sync_contract_profile(ctx, "first").await?;
        let provider = contract_provider_spec(ctx)?;
        let name = provider.name.clone();
        ctx.gateway.put_provider(&provider).await.map_err(text)?;
        let spec = ctx.spec_with(
            &fresh_launch()?,
            &ctx.fixture.image,
            vec![name.clone()],
            Ctx::main("/bin/sh", &["-c", "sleep 3600"])?,
        )?;
        ctx.gateway.create_sandbox(&spec).await.map_err(text)?;
        created = Some(spec.name().clone());
        let refused = ctx.gateway.delete_provider(&name).await;
        expect(
            matches!(refused, Err(GatewayError::FailedPrecondition { .. })),
            &format!(
                "delete_provider while a sandbox lists it is not FailedPrecondition: {refused:?}"
            ),
        )?;
        expect(
            provider_state_is(ctx, &name, ProviderState::Present).await?,
            "the provider is absent after a refused delete",
        )
    }
    .await;
    let released = match &created {
        Some(sandbox) => {
            ctx.cleanup(sandbox).await;
            ctx.gone(sandbox).await
        }
        None => Ok(()),
    };
    finish_contract_profile(ctx, result.and(released)).await
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

async fn stop_on_stopped_is_ok(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        ctx.gateway
            .set_power(&sandbox, PowerAction::Stop)
            .await
            .map_err(text)?;
        ctx.wait_for(&sandbox, "Stopped", |view| view.phase == Phase::Stopped)
            .await?;
        let again = ctx
            .gateway
            .set_power(&sandbox, PowerAction::Stop)
            .await
            .map_err(text)?;
        expect(
            again.phase == Phase::Stopped,
            "Stop on a Stopped sandbox does not return Stopped",
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn ended_main_process_stop_start(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = fresh_launch()?;
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
    ctx.gone(&sandbox).await?;
    ctx.gateway.delete_sandbox(&sandbox).await.map_err(text)
}

fn scratch_dir(kind: &str) -> Outcome<PathBuf> {
    Ok(std::env::temp_dir().join(format!("jackin-contract-{kind}-{}", fresh_launch()?)))
}

async fn host_git(dir: &Path, args: &[&str]) -> Outcome<()> {
    let (dir, args) = (
        dir.to_path_buf(),
        args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
    );
    let status = tokio::task::spawn_blocking(move || {
        std::process::Command::new("/usr/bin/git")
            .args(&args)
            .current_dir(&dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
    })
    .await
    .map_err(text)?
    .map_err(text)?;
    expect(
        status.success(),
        "the host git failed on the scratch project",
    )
}

async fn scratch_project(dir: &Path) -> Outcome<ProjectRoot> {
    tokio::fs::create_dir_all(dir.join("sub"))
        .await
        .map_err(text)?;
    for (name, content) in [
        ("tracked.txt", "tracked\n"),
        ("sub/untracked.txt", "untracked\n"),
        (".gitignore", "ignored.log\n"),
        ("ignored.log", "ignored\n"),
    ] {
        tokio::fs::write(dir.join(name), content)
            .await
            .map_err(text)?;
    }
    tokio::fs::symlink("tracked.txt", dir.join("link.txt"))
        .await
        .map_err(text)?;
    host_git(dir, &["init", "-q"]).await?;
    host_git(dir, &["add", "tracked.txt", ".gitignore"]).await?;
    ProjectRoot::open(
        &StdFileSystem::default(),
        &HostPath::parse(dir).map_err(text)?,
    )
    .await
    .map_err(text)
}

async fn project_files(ctx: &Ctx<'_>, sandbox: &SandboxName) -> Outcome<Vec<String>> {
    let listing = ctx
        .exec(
            sandbox,
            "cd \"$1\" && find . -type f | sort",
            &["/sandbox/project"],
            ProgramStdin::Closed,
            None,
        )
        .await?;
    expect(listing.exit_code == 0, "the project listing failed")?;
    Ok(String::from_utf8_lossy(&listing.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

async fn upload_tree_checks(
    ctx: &Ctx<'_>,
    sandbox: &SandboxName,
    project: &ProjectRoot,
) -> Outcome<()> {
    let cancel = CancellationToken::new();
    let dest = path("/sandbox/project")?;
    let expected = ["./.gitignore", "./sub/untracked.txt", "./tracked.txt"];
    for _ in 0..2 {
        ctx.gateway
            .upload_tree(sandbox, project, &dest, &cancel)
            .await
            .map_err(text)?;
        let files = project_files(ctx, sandbox).await?;
        expect(
            files == expected,
            &format!("the uploaded files are {files:?}, not {expected:?}"),
        )?;
    }
    let link = ctx
        .exec(
            sandbox,
            "cd \"$1\" && test -L link.txt && readlink link.txt",
            &["/sandbox/project"],
            ProgramStdin::Closed,
            None,
        )
        .await?;
    expect(
        link.exit_code == 0 && String::from_utf8_lossy(&link.stdout).trim() == "tracked.txt",
        "the uploaded link.txt is not a symlink to tracked.txt",
    )?;
    let stale = SandboxFile {
        path: path("/sandbox/project/stale.txt")?,
        mode: FileMode(0o600),
        content: SecretBytes::new(b"stale".to_vec()),
    };
    ctx.gateway
        .write_file(sandbox, &stale)
        .await
        .map_err(text)?;
    ctx.gateway
        .upload_tree(sandbox, project, &dest, &cancel)
        .await
        .map_err(text)?;
    let files = project_files(ctx, sandbox).await?;
    expect(
        files.iter().any(|file| file == "./stale.txt") && files.len() == expected.len() + 1,
        &format!("a repeated upload changed the stale file set: {files:?}"),
    )
}

async fn upload_tree(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let dir = scratch_dir("project")?;
    let result = async {
        let project = scratch_project(&dir).await?;
        upload_tree_checks(ctx, &sandbox, &project).await
    }
    .await;
    drop(tokio::fs::remove_dir_all(&dir).await);
    ctx.cleanup(&sandbox).await;
    result
}

async fn landed(ctx: &Ctx<'_>, sandbox: &SandboxName, file: &str) -> Outcome<()> {
    let found = ctx
        .exec(
            sandbox,
            "test -f \"$1\"",
            &[file],
            ProgramStdin::Closed,
            None,
        )
        .await?;
    expect(
        found.exit_code == 0,
        &format!("{file} is not a file at the exact name"),
    )
}

async fn upload_file_checks(ctx: &Ctx<'_>, sandbox: &SandboxName, dir: &Path) -> Outcome<()> {
    let cancel = CancellationToken::new();
    let local = HostPath::parse(&dir.join("payload.bin")).map_err(text)?;
    let big: Vec<u8> = (0..3 * 1024 * 1024_u32)
        .map(|index| u8::try_from(index % 251).unwrap_or(0))
        .collect();
    for (bytes, target) in [
        (b"first".to_vec(), "/sandbox/.contract-new/small.bin"),
        (big, "/sandbox/.contract-dot/big.bin"),
        (b"other".to_vec(), "/sandbox/.contract-new/other.bin"),
        (b"second".to_vec(), "/sandbox/.contract-new/small.bin"),
    ] {
        tokio::fs::write(local.as_path(), &bytes)
            .await
            .map_err(text)?;
        ctx.gateway
            .upload_file(sandbox, &local, &path(target)?, &cancel)
            .await
            .map_err(text)?;
        landed(ctx, sandbox, target).await?;
    }
    let replaced = path("/sandbox/.contract-new/small.bin")?;
    let read = ctx
        .gateway
        .read_files(sandbox, std::slice::from_ref(&replaced))
        .await
        .map_err(text)?;
    expect(
        read.first().and_then(Option::as_deref) == Some(b"second".as_slice()),
        "an upload over an existing file does not replace the content",
    )
}

async fn upload_file_with_digest(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let dir = scratch_dir("file")?;
    let result = async {
        tokio::fs::create_dir_all(&dir).await.map_err(text)?;
        upload_file_checks(ctx, &sandbox, &dir).await
    }
    .await;
    drop(tokio::fs::remove_dir_all(&dir).await);
    ctx.cleanup(&sandbox).await;
    result
}

fn refused_upload(
    result: &Result<(), GatewayError>,
    op: TransferOp,
    says: &str,
    not: &str,
    what: &str,
) -> Outcome<()> {
    expect(
        matches!(
            result,
            Err(GatewayError::Transfer {
                op: got,
                status: ProcessStatus::Exited(1),
                tail,
            }) if *got == op && tail.as_str().contains(says) && !tail.as_str().contains(not)
        ),
        &format!("{what} is not a Transfer error with exit 1 that says {says:?}: {result:?}"),
    )
}

async fn refused_uploads(
    ctx: &Ctx<'_>,
    sandbox: &SandboxName,
    dir: &Path,
    says: &str,
    not: &str,
    what: &str,
) -> Outcome<()> {
    tokio::fs::create_dir_all(dir).await.map_err(text)?;
    let local = HostPath::parse(&dir.join("payload.bin")).map_err(text)?;
    tokio::fs::write(local.as_path(), b"refused")
        .await
        .map_err(text)?;
    let cancel = CancellationToken::new();
    let tree = ctx
        .gateway
        .upload_tree(
            sandbox,
            &ctx.fixture.project,
            &path("/sandbox/project")?,
            &cancel,
        )
        .await;
    let file = ctx
        .gateway
        .upload_file(
            sandbox,
            &local,
            &path("/sandbox/.contract/refused.bin")?,
            &cancel,
        )
        .await;
    refused_upload(
        &tree,
        TransferOp::UploadFiltered,
        says,
        not,
        &format!("a tree upload to {what}"),
    )?;
    refused_upload(
        &file,
        TransferOp::UploadFile,
        says,
        not,
        &format!("a file upload to {what}"),
    )
}

async fn upload_absent_sandbox_is_a_transfer_error(ctx: &Ctx<'_>) -> Outcome<()> {
    let name = SandboxName::new(
        &ctx.fixture.name_prefix,
        &NameSuffix::for_launch(&fresh_launch()?),
    );
    let dir = scratch_dir("absent")?;
    let result = refused_uploads(ctx, &name, &dir, "found", "ready", "an absent sandbox").await;
    drop(tokio::fs::remove_dir_all(&dir).await);
    result
}

async fn upload_stopped_sandbox_is_a_transfer_error(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let dir = scratch_dir("stopped")?;
    let result = async {
        ctx.gateway
            .set_power(&sandbox, PowerAction::Stop)
            .await
            .map_err(text)?;
        ctx.wait_for(&sandbox, "Stopped", |view| view.phase == Phase::Stopped)
            .await?;
        refused_uploads(ctx, &sandbox, &dir, "ready", "found", "a Stopped sandbox").await
    }
    .await;
    drop(tokio::fs::remove_dir_all(&dir).await);
    ctx.cleanup(&sandbox).await;
    result
}

async fn symlinked_parent_project(dir: &Path) -> Outcome<ProjectRoot> {
    let project = dir.join("project");
    let outside = dir.join("outside");
    for place in [project.join("d"), outside.clone()] {
        tokio::fs::create_dir_all(place).await.map_err(text)?;
    }
    tokio::fs::write(project.join("d/f.txt"), "inside\n")
        .await
        .map_err(text)?;
    tokio::fs::write(outside.join("f.txt"), "outside\n")
        .await
        .map_err(text)?;
    host_git(&project, &["init", "-q"]).await?;
    host_git(&project, &["add", "d/f.txt"]).await?;
    tokio::fs::remove_dir_all(project.join("d"))
        .await
        .map_err(text)?;
    tokio::fs::symlink(&outside, project.join("d"))
        .await
        .map_err(text)?;
    ProjectRoot::open(
        &StdFileSystem::default(),
        &HostPath::parse(&project).map_err(text)?,
    )
    .await
    .map_err(text)
}

async fn upload_through_a_symlinked_parent_is_a_transfer_error(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let dir = scratch_dir("symlinked-parent")?;
    let result = async {
        let project = symlinked_parent_project(&dir).await?;
        let uploaded = ctx
            .gateway
            .upload_tree(
                &sandbox,
                &project,
                &path("/sandbox/project")?,
                &CancellationToken::new(),
            )
            .await;
        refused_upload(
            &uploaded,
            TransferOp::UploadFiltered,
            "tar",
            "ready",
            "a tree upload through a symlinked parent",
        )?;
        let listing = ctx
            .exec(
                &sandbox,
                "find \"$1\" -type f -name f.txt 2>/dev/null",
                &["/sandbox/project"],
                ProgramStdin::Closed,
                None,
            )
            .await?;
        expect(
            !String::from_utf8_lossy(&listing.stdout).contains("/d/f.txt"),
            "the file behind the symlinked parent landed in the sandbox",
        )
    }
    .await;
    drop(tokio::fs::remove_dir_all(&dir).await);
    ctx.cleanup(&sandbox).await;
    result
}

fn scratch_download() -> Outcome<HostPath> {
    let local = std::env::temp_dir().join(format!("jackin-contract-{}.bin", fresh_launch()?));
    HostPath::parse(&local).map_err(text)
}

async fn put_remote(
    ctx: &Ctx<'_>,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    bytes: &[u8],
) -> Outcome<()> {
    let file = SandboxFile {
        path: remote.clone(),
        mode: FileMode(0o600),
        content: SecretBytes::new(bytes.to_vec()),
    };
    ctx.gateway.write_file(sandbox, &file).await.map_err(text)
}

async fn download_with_digest(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        let remote = path("/sandbox/.contract/export.bin")?;
        put_remote(ctx, &sandbox, &remote, b"contract-export").await?;
        let local = scratch_download()?;
        let digest = ctx
            .gateway
            .download_file(&sandbox, &remote, &local, &CancellationToken::new())
            .await
            .map_err(text)?;
        let bytes = tokio::fs::read(local.as_path()).await.map_err(text)?;
        drop(tokio::fs::remove_file(local.as_path()).await);
        expect(bytes == b"contract-export", "the downloaded bytes differ")?;
        expect(
            digest == Sha256::of(&bytes),
            &format!("the returned digest {digest} is not the digest of the bytes"),
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

const KEPT: &[u8] = b"keep-me";

async fn failed_download_keeps_the_local_file(
    ctx: &Ctx<'_>,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    cancel: &CancellationToken,
    what: &str,
) -> Outcome<Result<(), GatewayError>> {
    let seeded = scratch_download()?;
    tokio::fs::write(seeded.as_path(), KEPT)
        .await
        .map_err(text)?;
    let failed = ctx
        .gateway
        .download_file(sandbox, remote, &seeded, cancel)
        .await
        .map(drop);
    let kept = tokio::fs::read(seeded.as_path()).await.map_err(text);
    drop(tokio::fs::remove_file(seeded.as_path()).await);
    expect(
        kept.as_deref().ok() == Some(KEPT),
        &format!("{what} did not leave the existing local file as it was: {kept:?}"),
    )?;
    let bare = scratch_download()?;
    let again = ctx
        .gateway
        .download_file(sandbox, remote, &bare, cancel)
        .await
        .map(drop);
    let local_left = tokio::fs::try_exists(bare.as_path()).await.map_err(text)?;
    drop(tokio::fs::remove_file(bare.as_path()).await);
    expect(!local_left, &format!("{what} left a local file"))?;
    expect(
        failed.is_err() && again.is_err(),
        &format!("{what} did not fail with and without a local file: {failed:?}, {again:?}"),
    )?;
    Ok(failed)
}

async fn download_missing_remote_is_a_size_error(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        let remote = path("/sandbox/.contract/missing.bin")?;
        let missing = failed_download_keeps_the_local_file(
            ctx,
            &sandbox,
            &remote,
            &CancellationToken::new(),
            "a download of a missing file",
        )
        .await?;
        expect(
            matches!(
                missing,
                Err(GatewayError::Transfer {
                    op: TransferOp::Size,
                    status: ProcessStatus::Exited(code),
                    ..
                }) if code != 0
            ),
            &format!("a download of a missing file is not a Size error: {missing:?}"),
        )?;
        put_remote(ctx, &sandbox, &remote, b"now-present").await?;
        let local = scratch_download()?;
        ctx.gateway
            .download_file(&sandbox, &remote, &local, &CancellationToken::new())
            .await
            .map_err(text)?;
        let bytes = tokio::fs::read(local.as_path()).await.map_err(text)?;
        drop(tokio::fs::remove_file(local.as_path()).await);
        expect(
            bytes == b"now-present",
            "the download after the file appeared has other bytes",
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn download_pre_cancelled_is_cancelled(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.sleeper().await?;
    let result = async {
        let remote = path("/sandbox/.contract/cancelled.bin")?;
        put_remote(ctx, &sandbox, &remote, b"cancel-control").await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = failed_download_keeps_the_local_file(
            ctx,
            &sandbox,
            &remote,
            &cancel,
            "a download with a cancelled token",
        )
        .await?;
        expect(
            matches!(cancelled, Err(GatewayError::Cancelled { .. })),
            &format!("a download with a cancelled token is not Cancelled: {cancelled:?}"),
        )?;
        let local = scratch_download()?;
        ctx.gateway
            .download_file(&sandbox, &remote, &local, &CancellationToken::new())
            .await
            .map_err(text)?;
        let bytes = tokio::fs::read(local.as_path()).await.map_err(text)?;
        drop(tokio::fs::remove_file(local.as_path()).await);
        expect(
            bytes == b"cancel-control",
            "the download with a fresh token has other bytes",
        )
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
        let exit = tokio::time::timeout(PROGRAM_TIMEOUT, async {
            let mut exit = None;
            while let Some(event) = session.next_event().await {
                if let TerminalEvent::Exited(code) = event {
                    exit = code;
                }
            }
            exit
        })
        .await
        .map_err(|_| "the shell never ended".to_owned())?;
        expect(exit == Some(3), "the shell exit code is not 3")
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn read_until(session: &mut TerminalSession, needle: &str) -> Outcome<(InputLease, String)> {
    let mut lease = InputLease::ReadOnly;
    let mut seen = String::new();
    tokio::time::timeout(PROGRAM_TIMEOUT, async {
        while let Some(event) = session.next_event().await {
            match event {
                TerminalEvent::Lease(granted) => lease = granted,
                TerminalEvent::Output(bytes) => {
                    seen.push_str(&String::from_utf8_lossy(&bytes));
                    if seen.contains(needle) {
                        return Ok((lease, seen));
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

async fn lease_of(session: &mut TerminalSession) -> Outcome<InputLease> {
    tokio::time::timeout(PROGRAM_TIMEOUT, async {
        while let Some(event) = session.next_event().await {
            match event {
                TerminalEvent::Lease(granted) => return Ok(granted),
                TerminalEvent::Ended(reason) => {
                    return Err(format!("the terminal ended: {reason:?}"));
                }
                _ => {}
            }
        }
        Err("the terminal closed".to_owned())
    })
    .await
    .map_err(|_| "no lease arrived".to_owned())?
}

async fn remaining(session: &mut TerminalSession) -> Outcome<Vec<TerminalEvent>> {
    tokio::time::timeout(PROGRAM_TIMEOUT, async {
        let mut events = Vec::new();
        while let Some(event) = session.next_event().await {
            events.push(event);
        }
        events
    })
    .await
    .map_err(|_| "the terminal never closed".to_owned())
}

fn expect_end_without_exit(events: &[TerminalEvent], reason: &EndReason) -> Outcome<()> {
    expect(
        !events
            .iter()
            .any(|event| matches!(event, TerminalEvent::Exited(_))),
        "an ended attachment reported a process exit",
    )?;
    expect(
        events.last() == Some(&TerminalEvent::Ended(reason.clone())),
        &format!("the attachment did not end with {reason:?}: {events:?}"),
    )
}

async fn take_lease(session: &mut TerminalSession) -> Outcome<()> {
    let deadline = tokio::time::Instant::now() + PROGRAM_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        session
            .input(b"contract-retry\n".to_vec())
            .await
            .map_err(text)?;
        let window = tokio::time::timeout(LEASE_RETRY, async {
            while let Some(event) = session.next_event().await {
                match event {
                    TerminalEvent::Lease(InputLease::Owner) => return Ok(()),
                    TerminalEvent::Ended(reason) => {
                        return Err(format!("the terminal ended: {reason:?}"));
                    }
                    _ => {}
                }
            }
            Err("the terminal closed".to_owned())
        })
        .await;
        match window {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(error)) => return Err(error),
            Err(_) => {}
        }
    }
    Err("the read-only attachment never took the lease".to_owned())
}

fn echo_main() -> Outcome<MainProcess> {
    let script = format!("echo {BANNER}; exec cat");
    Ctx::main("/bin/sh", &["-c", &script])
}

async fn main_attach_detach_replay(ctx: &Ctx<'_>) -> Outcome<()> {
    let script = format!("echo {BANNER}; sleep 3600");
    let sandbox = ctx.ready(Ctx::main("/bin/sh", &["-c", &script])?).await?;
    let result = async {
        let request = Ctx::terminal(&sandbox, Ctx::owner_kind())?;
        let mut first = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        let (lease, _) = read_until(&mut first, BANNER).await?;
        expect(
            lease == InputLease::Owner,
            "the first attachment is not the owner",
        )?;
        first.detach().await.map_err(text)?;
        drop(first);
        let mut second = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        read_until(&mut second, BANNER).await?;
        second.input(vec![CTRL_D]).await.map_err(text)?;
        let events = remaining(&mut second).await?;
        expect_end_without_exit(&events, &EndReason::Detached)?;
        expect(
            ctx.view(&sandbox).await?.phase == Phase::Ready,
            "Ctrl-D ended the main process",
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn main_second_attachment_read_only(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.ready(echo_main()?).await?;
    let result = async {
        let request = Ctx::terminal(&sandbox, Ctx::owner_kind())?;
        let mut first = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        let (lease, _) = read_until(&mut first, BANNER).await?;
        expect(
            lease == InputLease::Owner,
            "the first attachment is not the owner",
        )?;
        let mut second = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        expect(
            lease_of(&mut second).await? == InputLease::ReadOnly,
            "a second attachment is not read-only",
        )
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn main_viewer_read_only(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.ready(echo_main()?).await?;
    let result = async {
        let request = Ctx::terminal(&sandbox, Ctx::owner_kind())?;
        let mut owner = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        read_until(&mut owner, BANNER).await?;
        let viewer_request = Ctx::terminal(
            &sandbox,
            TerminalKind::MainProcess {
                input: InputMode::Viewer,
                repaint: RepaintPolicy::Never,
            },
        )?;
        let mut viewer = ctx
            .gateway
            .open_terminal(&viewer_request)
            .await
            .map_err(text)?;
        expect(
            lease_of(&mut viewer).await? == InputLease::ReadOnly,
            "a viewer is not read-only",
        )?;
        viewer
            .input(b"contract-viewer\n".to_vec())
            .await
            .map_err(text)?;
        tokio::time::sleep(VIEWER_SETTLE).await;
        owner
            .input(b"contract-owner\n".to_vec())
            .await
            .map_err(text)?;
        let (_, seen) = read_until(&mut owner, "contract-owner").await?;
        expect(
            !seen.contains("contract-viewer"),
            "viewer input reached the main process",
        )?;
        viewer.input(vec![CTRL_C]).await.map_err(text)?;
        let events = remaining(&mut viewer).await?;
        expect_end_without_exit(&events, &EndReason::StreamClosed)
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn main_read_only_takes_lease_after_owner_leaves(ctx: &Ctx<'_>) -> Outcome<()> {
    let sandbox = ctx.ready(echo_main()?).await?;
    let result = async {
        let request = Ctx::terminal(&sandbox, Ctx::owner_kind())?;
        let mut first = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        read_until(&mut first, BANNER).await?;
        let mut second = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        expect(
            lease_of(&mut second).await? == InputLease::ReadOnly,
            "a second attachment is not read-only",
        )?;
        first.detach().await.map_err(text)?;
        remaining(&mut first).await?;
        take_lease(&mut second).await?;
        second
            .input(b"contract-owned\n".to_vec())
            .await
            .map_err(text)?;
        read_until(&mut second, "contract-owned").await?;
        Ok(())
    }
    .await;
    ctx.cleanup(&sandbox).await;
    result
}

async fn main_ended_is_refused(ctx: &Ctx<'_>) -> Outcome<()> {
    let launch = fresh_launch()?;
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
        let request = Ctx::terminal(spec.name(), Ctx::owner_kind())?;
        let mut session = ctx.gateway.open_terminal(&request).await.map_err(text)?;
        let events = remaining(&mut session).await?;
        expect(
            matches!(
                events.as_slice(),
                [
                    TerminalEvent::Attached,
                    TerminalEvent::Ended(EndReason::Refused(_))
                ]
            ),
            &format!("an attach to an ended main process is not refused: {events:?}"),
        )
    }
    .await;
    ctx.cleanup(spec.name()).await;
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
                "absent_sandbox_is_not_found",
                absent_sandbox_is_not_found
            );
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
            check!(
                checks,
                ctx,
                "delete_provider_while_attached_is_refused",
                delete_provider_while_attached_is_refused
            );
            check!(checks, ctx, "power_transitions", power_transitions);
            check!(checks, ctx, "stop_on_stopped_is_ok", stop_on_stopped_is_ok);
            check!(
                checks,
                ctx,
                "ended_main_process_stop_start",
                ended_main_process_stop_start
            );
            check!(checks, ctx, "delete_sandbox", delete_sandbox);
        }
        ContractGroup::Transfer => {
            check!(checks, ctx, "upload_tree", upload_tree);
            check!(
                checks,
                ctx,
                "upload_file_with_digest",
                upload_file_with_digest
            );
            check!(checks, ctx, "download_with_digest", download_with_digest);
            check!(
                checks,
                ctx,
                "download_missing_remote_is_a_size_error",
                download_missing_remote_is_a_size_error
            );
            check!(
                checks,
                ctx,
                "download_pre_cancelled_is_cancelled",
                download_pre_cancelled_is_cancelled
            );
            check!(
                checks,
                ctx,
                "upload_absent_sandbox_is_a_transfer_error",
                upload_absent_sandbox_is_a_transfer_error
            );
            check!(
                checks,
                ctx,
                "upload_stopped_sandbox_is_a_transfer_error",
                upload_stopped_sandbox_is_a_transfer_error
            );
            check!(
                checks,
                ctx,
                "upload_through_a_symlinked_parent_is_a_transfer_error",
                upload_through_a_symlinked_parent_is_a_transfer_error
            );
        }
        ContractGroup::Terminal => {
            check!(checks, ctx, "shell_exit_code", shell_exit_code);
            check!(
                checks,
                ctx,
                "main_attach_detach_replay",
                main_attach_detach_replay
            );
            check!(
                checks,
                ctx,
                "main_second_attachment_read_only",
                main_second_attachment_read_only
            );
            check!(checks, ctx, "main_viewer_read_only", main_viewer_read_only);
            check!(
                checks,
                ctx,
                "main_read_only_takes_lease_after_owner_leaves",
                main_read_only_takes_lease_after_owner_leaves
            );
            check!(checks, ctx, "main_ended_is_refused", main_ended_is_refused);
        }
    }
    ContractReport { checks }
}

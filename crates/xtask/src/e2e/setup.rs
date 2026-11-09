// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use jackin_core::AccountName;
use jackin_openshell::ProviderState;

use super::a1::fixture_login;
use super::credential::CredentialSession;
use super::redact::Redactor;
use super::report::Printer;
use super::run::{Names, Projects, Run, tail_of};
use super::scratch::{Scratch, base_env, child_env, snapshot_tree};
use super::sweep::find_stale;
use super::target::E2eTarget;
use super::world::World;
use super::{Boundaries, E2eArgs};

#[derive(Debug)]
pub(crate) struct SetupError(pub String);

fn setup(message: impl Into<String>) -> SetupError {
    SetupError(message.into())
}

fn registry_dir(parent: &std::collections::BTreeMap<String, String>) -> Option<PathBuf> {
    parent
        .get("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(|base| Path::new(base).join("openshell"))
        .or_else(|| {
            parent
                .get("HOME")
                .map(|home| Path::new(home).join(".config/openshell"))
        })
}

pub(crate) async fn prepare<'a>(
    target: E2eTarget,
    args: &'a E2eArgs,
    b: &'a Boundaries,
    printer: &Printer,
) -> Result<Run<'a>, SetupError> {
    let credentials =
        CredentialSession::open(target, b).map_err(|error| setup(error.to_string()))?;
    let redactor = credentials.redactor();
    printer.set_redactor(redactor.clone());
    credentials
        .fresh_host_reply(b, "before")
        .await
        .map_err(|error| setup(credentials.scrubbed_failure(&error)))?;
    let health = b.gateway.health().await.map_err(|error| {
        setup(format!(
            "the gateway {} is unreachable: {error}",
            args.gateway
        ))
    })?;
    if !health.serving {
        return Err(setup(format!(
            "the gateway {} is not serving",
            args.gateway
        )));
    }
    let stale = find_stale(b, &base_env(&b.env), &args.gateway)
        .await
        .map_err(|error| {
            setup(format!(
                "cannot look for leftovers of earlier runs: {error}"
            ))
        })?;
    if !stale.is_empty() {
        return Err(setup(format!(
            "the gateway holds {}; run `xtask e2e --gateway {} --sweep` first",
            stale.describe(),
            args.gateway
        )));
    }
    let role_arg = args
        .role
        .clone()
        .ok_or_else(|| setup("--role is required"))?;
    let role = std::fs::canonicalize(&role_arg).map_err(|_| {
        setup(format!(
            "the role directory {} does not exist",
            role_arg.display()
        ))
    })?;
    let registry = registry_dir(&b.env)
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| setup("no OpenShell registry directory under XDG_CONFIG_HOME or HOME"))?;
    let scratch =
        Scratch::create(&b.scratch_root.join(&b.tag)).map_err(|error| setup(error.to_string()))?;
    let built = build_run(
        args,
        b,
        printer,
        BuildParts {
            role,
            credentials,
            redactor,
            registry,
            scratch: scratch.clone(),
        },
    )
    .await;
    if built.is_err() {
        drop(scratch.remove());
    }
    built
}

struct BuildParts {
    role: PathBuf,
    credentials: CredentialSession,
    redactor: Redactor,
    registry: PathBuf,
    scratch: Scratch,
}

async fn build_run<'a>(
    args: &'a E2eArgs,
    b: &'a Boundaries,
    printer: &Printer,
    parts: BuildParts,
) -> Result<Run<'a>, SetupError> {
    let BuildParts {
        role,
        credentials,
        redactor,
        registry,
        scratch,
    } = parts;
    let client_id = credentials
        .a1()
        .map(|login| login.client_id().to_owned())
        .ok_or_else(|| setup("T01 requires its ChatGPT credential session"))?;
    scratch
        .link_registry(&registry)
        .map_err(|error| setup(error.to_string()))?;
    let env = child_env(&b.env, &scratch, false);
    let pty_env = child_env(&b.env, &scratch, true);
    let names = Names::new(&b.tag);
    let fixture = |name: String| {
        let scratch = scratch.clone();
        let env = env.clone();
        async move {
            scratch
                .fixture_project(b.host.as_ref(), &env, &name, &b.cancel)
                .await
                .map_err(|error| setup(error.to_string()))
        }
    };
    let projects = Projects {
        main: fixture(names.main.clone()).await?,
        life: fixture(names.life.clone()).await?,
        dock: fixture(names.dock.clone()).await?,
    };
    for account in [&names.fxr, &names.fxw] {
        fixture_login(&scratch.codex_homes().join(account), &client_id)
            .map_err(|error| setup(error.to_string()))?;
    }
    let mut run = Run {
        args,
        b,
        printer: printer.clone(),
        redactor,
        scratch: scratch.clone(),
        env,
        pty_env,
        jackin: b.jackin.display().to_string(),
        role,
        names,
        credentials,
        projects,
        registry_before: snapshot_tree(&registry),
        registry,
        role_refs_before: String::new(),
        w: World::default(),
        leaks: Vec::new(),
        cleaning: false,
        shield: tokio_util::sync::CancellationToken::new(),
    };
    let role = run.role.clone();
    run.role_refs_before = match run.jackin_refs(&role).await {
        Ok(listing) => listing,
        Err(failure) => {
            drop(run.scratch.remove());
            return Err(setup(format!(
                "cannot list the refs of the role repository: {}",
                failure.0
            )));
        }
    };
    Ok(run)
}

impl Run<'_> {
    pub(crate) async fn configure(&mut self) -> Result<(), SetupError> {
        let main = self.projects.main.clone();
        let gateway = self.args.gateway.clone();
        let mut add: Vec<String> = ["gateway", "add", &gateway, "--default"]
            .map(str::to_owned)
            .to_vec();
        if let Some(builder) = &self.args.builder {
            add.extend(["--builder".to_owned(), builder.clone()]);
        }
        if let Some(platform) = &self.args.platform {
            add.extend(["--platform".to_owned(), platform.clone()]);
        }
        if let Some((port, certs)) = self.args.engine() {
            let certs = std::fs::canonicalize(certs).map_err(|_| {
                setup(format!(
                    "the engine certificate directory {} does not exist",
                    certs.display()
                ))
            })?;
            add.extend([
                "--engine-port".to_owned(),
                port.to_string(),
                "--engine-certs".to_owned(),
                certs.display().to_string(),
            ]);
        }
        self.setup_command("gateway add", &add, &main).await?;
        let role = self.role.display().to_string();
        self.setup_command(
            "role add",
            &strings(&["role", "add", "sentinel", &role]),
            &main,
        )
        .await?;
        let codex_home = self.b.codex_home.display().to_string();
        let accounts: Vec<(String, String)> = vec![
            (self.names.a1.clone(), codex_home),
            (
                self.names.fxr.clone(),
                self.scratch
                    .codex_homes()
                    .join(&self.names.fxr)
                    .display()
                    .to_string(),
            ),
            (
                self.names.fxw.clone(),
                self.scratch
                    .codex_homes()
                    .join(&self.names.fxw)
                    .display()
                    .to_string(),
            ),
        ];
        for (account, home) in accounts {
            let account_name =
                AccountName::parse(&account).map_err(|error| setup(error.to_string()))?;
            let provider = self.credentials.target.provider_name(&account_name);
            let state = self
                .b
                .gateway
                .provider_state(&provider)
                .await
                .map_err(|error| setup(error.to_string()))?;
            if state != ProviderState::Absent {
                return Err(setup(format!(
                    "provider {provider} already exists; the runner never overwrites one"
                )));
            }
            self.w.accounts_made.push(account.clone());
            self.setup_command(
                "account add",
                &strings(&[
                    "account",
                    "add",
                    "codex",
                    "--name",
                    &account,
                    "--chatgpt-from",
                    &home,
                ]),
                &main,
            )
            .await?;
        }
        let a1 = self.names.a1.clone();
        self.setup_command(
            "account default",
            &strings(&["account", "default", &a1]),
            &main,
        )
        .await?;
        for zai in ["zai", "zai-role", "zai-workspace"] {
            let provider = ProviderName::parse(&Names::provider(zai))
                .map_err(|error| setup(error.to_string()))?;
            let state = self
                .b
                .gateway
                .provider_state(&provider)
                .await
                .map_err(|error| setup(error.to_string()))?;
            if state != ProviderState::Absent {
                return Err(setup(format!(
                    "provider {provider} already exists; the runner never overwrites one"
                )));
            }
            let key = self.scratch.root().join(format!("{zai}.key"));
            std::fs::write(&key, format!("e2e-{zai}-test-key"))
                .map_err(|error| setup(error.to_string()))?;
            self.w.accounts_made.push(zai.to_owned());
            self.setup_command(
                "account add zai",
                &strings(&[
                    "account",
                    "add",
                    "zai",
                    "--name",
                    zai,
                    "--credential",
                    &format!("file:{}", key.display()),
                ]),
                &main,
            )
            .await?;
        }
        self.setup_command(
            "account default zai",
            &strings(&["account", "default", "zai"]),
            &main,
        )
        .await?;
        let workspaces = [
            (self.names.main.clone(), self.projects.main.clone(), false),
            (self.names.life.clone(), self.projects.life.clone(), false),
            (self.names.dock.clone(), self.projects.dock.clone(), true),
        ];
        for (name, path, docker) in workspaces {
            let mut command = strings(&["workspace", "add", &name, &path.display().to_string()]);
            if docker {
                command.push("--docker".to_owned());
            }
            self.setup_command("workspace add", &command, &main).await?;
        }
        Ok(())
    }

    async fn setup_command(
        &mut self,
        label: &str,
        args: &[String],
        cwd: &Path,
    ) -> Result<(), SetupError> {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .jackin(&format!("setup {label}"), &refs, cwd)
            .await
            .map_err(|failure| setup(format!("jackin {label} could not run: {}", failure.0)))?;
        if output.ok() {
            Ok(())
        } else {
            Err(setup(format!(
                "jackin {label} exited {}: {}",
                output.exit.describe(),
                tail_of(&self.redactor, &output.stderr)
            )))
        }
    }
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jackin_core::{EnvName, LabelSelector, SandboxName, SandboxPath};
use jackin_openshell::{
    GatewayError, ProgramOutcome, ProgramRun, ProgramStdin, SandboxQuery, SandboxView,
};

use super::a1::{A1Error, A1Lock, A1Login};
use super::host::{HostCommand, HostError, HostOutput};
use super::probe::{ProbeError, ProbeMode, ProbeReport, ProbeRequest};
use super::pty::{PtyError, PtySession, PtySpec};
use super::redact::Redactor;
use super::report::Printer;
use super::scratch::{Scratch, ScratchError};
use super::verdict::Verdict;
use super::world::World;
use super::{Boundaries, E2eArgs};

pub(crate) const PROBE: &str = "/usr/local/bin/jackin-sentinel-probe";
pub(crate) const ENGINE_HOST: &str = "host.openshell.internal";

#[derive(Debug)]
pub(crate) struct Fail(pub String);

pub(crate) type Res<T> = Result<T, Fail>;

pub(crate) fn fail(message: impl Into<String>) -> Fail {
    Fail(message.into())
}

macro_rules! fail_from {
    ($($error:ty),* $(,)?) => {
        $(impl From<$error> for Fail {
            fn from(error: $error) -> Fail {
                Fail(error.to_string())
            }
        })*
    };
}

fail_from!(
    HostError,
    GatewayError,
    PtyError,
    ProbeError,
    ScratchError,
    A1Error,
    std::io::Error,
    serde_json::Error,
    jackin_core::PathError,
);

impl Fail {
    pub(crate) fn verdict(self) -> Verdict {
        Verdict::fail(self.0)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Names {
    pub tag: String,
    pub a1: String,
    pub fxr: String,
    pub fxw: String,
    pub main: String,
    pub life: String,
    pub dock: String,
}

impl Names {
    pub(crate) fn new(tag: &str) -> Names {
        Names {
            tag: tag.to_owned(),
            a1: format!("e2e-{tag}-a1"),
            fxr: format!("e2e-{tag}-fxr"),
            fxw: format!("e2e-{tag}-fxw"),
            main: format!("e2e-{tag}-main"),
            life: format!("e2e-{tag}-life"),
            dock: format!("e2e-{tag}-dock"),
        }
    }

    pub(crate) fn provider(account: &str) -> String {
        format!("jackin-{account}-codex")
    }

    pub(crate) fn workspace_prefix(&self) -> String {
        format!("e2e-{}-", self.tag)
    }

    pub(crate) fn container(&self) -> String {
        format!("e2e-{}", self.tag)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Projects {
    pub main: PathBuf,
    pub life: PathBuf,
    pub dock: PathBuf,
}

pub(crate) struct Run<'a> {
    pub args: &'a E2eArgs,
    pub b: &'a Boundaries,
    pub printer: Printer,
    pub redactor: Redactor,
    pub scratch: Scratch,
    pub env: BTreeMap<String, String>,
    pub pty_env: BTreeMap<String, String>,
    pub jackin: String,
    pub role: PathBuf,
    pub names: Names,
    pub a1: A1Login,
    pub lock: Option<A1Lock>,
    pub projects: Projects,
    pub registry: PathBuf,
    pub registry_before: Vec<(String, u128, u64)>,
    pub role_refs_before: String,
    pub w: World,
    pub leaks: Vec<String>,
    pub cleaning: bool,
    pub shield: tokio_util::sync::CancellationToken,
}

pub(crate) fn labels_of(view: &SandboxView) -> BTreeMap<String, String> {
    view.labels
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

impl Run<'_> {
    pub(crate) fn token(&self) -> tokio_util::sync::CancellationToken {
        if self.cleaning {
            self.shield.clone()
        } else {
            self.b.cancel.clone()
        }
    }

    pub(crate) fn log(&self, name: &str, text: &str) {
        let dir = &self.b.log_dir;
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let path = dir.join(format!("{name}.log"));
        let scrubbed = self.redactor.scrub(text);
        let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
        existing.push_str(&scrubbed);
        existing.push('\n');
        drop(std::fs::write(path, existing));
    }

    pub(crate) fn scan(&mut self, channel: &str, bytes: &[u8]) {
        if let Some(offset) = self.redactor.scan(bytes) {
            self.leaks
                .push(format!("needle found in {channel} at byte {offset}"));
        }
    }

    pub(crate) fn capture(&mut self, channel: &str, bytes: &[u8]) {
        self.scan(channel, bytes);
        self.log(
            &channel.replace([' ', '/'], "-"),
            &String::from_utf8_lossy(bytes),
        );
    }

    pub(crate) async fn run_host(
        &mut self,
        label: &str,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> Res<HostOutput> {
        let mut command = HostCommand::new(program, timeout)
            .args(args.iter().cloned())
            .env(self.env.clone());
        if let Some(dir) = cwd {
            command = command.cwd(dir);
        }
        let output = self.b.host.run(&command, &self.token()).await?;
        self.capture(&format!("{label} stdout"), &output.stdout);
        self.capture(&format!("{label} stderr"), &output.stderr);
        Ok(output)
    }

    pub(crate) async fn jackin(
        &mut self,
        label: &str,
        args: &[&str],
        cwd: &Path,
    ) -> Res<HostOutput> {
        let mut full: Vec<String> = vec!["--gateway".to_owned(), self.args.gateway.clone()];
        full.extend(args.iter().map(|arg| (*arg).to_owned()));
        let program = self.jackin.clone();
        let timeout = self.b.budgets.command;
        self.run_host(label, &program, &full, Some(cwd), timeout)
            .await
    }

    pub(crate) async fn openshell(&mut self, label: &str, args: &[&str]) -> Res<HostOutput> {
        let mut full: Vec<String> = vec!["-g".to_owned(), self.args.gateway.clone()];
        full.extend(args.iter().map(|arg| (*arg).to_owned()));
        let timeout = self.b.budgets.exec;
        self.run_host(label, "openshell", &full, None, timeout)
            .await
    }

    pub(crate) async fn git(&mut self, dir: &Path, args: &[&str]) -> Res<HostOutput> {
        let mut full: Vec<String> = [
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ]
        .map(str::to_owned)
        .to_vec();
        full.extend(args.iter().map(|arg| (*arg).to_owned()));
        let timeout = self.b.budgets.command;
        self.run_host("git", "git", &full, Some(dir), timeout).await
    }

    pub(crate) async fn exec_with(
        &mut self,
        sandbox: &SandboxName,
        label: &str,
        program: &str,
        args: Vec<String>,
        env: BTreeMap<EnvName, String>,
    ) -> Res<ProgramOutcome> {
        let run = ProgramRun {
            sandbox: sandbox.clone(),
            program: SandboxPath::parse(program)?,
            args,
            workdir: None,
            env,
            stdin: ProgramStdin::Closed,
            touch_on_success: None,
            timeout: self.b.budgets.exec,
        };
        let outcome = self.b.gateway.run_program(&run, &self.token()).await?;
        self.capture(&format!("{label} stdout"), &outcome.stdout);
        self.capture(&format!("{label} stderr"), &outcome.stderr);
        Ok(outcome)
    }

    pub(crate) async fn exec(
        &mut self,
        sandbox: &SandboxName,
        label: &str,
        script: &str,
    ) -> Res<ProgramOutcome> {
        self.exec_with(
            sandbox,
            label,
            "/bin/sh",
            vec!["-c".to_owned(), script.to_owned()],
            BTreeMap::new(),
        )
        .await
    }

    pub(crate) async fn probe(
        &mut self,
        sandbox: &SandboxName,
        label: &str,
        docker: bool,
        engine: bool,
        only: &[&str],
    ) -> Res<ProbeReport> {
        let mut args: Vec<String> = vec!["--docker".to_owned(), flag_docker(docker).to_owned()];
        if engine {
            let port = self.engine_port()?;
            args.push("--engine".to_owned());
            args.push(format!("{ENGINE_HOST}:{port}"));
        }
        args.push("--only".to_owned());
        args.push(only.join(","));
        let outcome = self
            .exec_with(sandbox, label, PROBE, args, BTreeMap::new())
            .await?;
        self.read_probe(
            outcome.exit_code,
            &outcome.stdout,
            &outcome.stderr,
            docker,
            only,
        )
    }

    pub(crate) fn read_probe(
        &self,
        exit_code: i32,
        stdout: &[u8],
        stderr: &[u8],
        docker: bool,
        only: &[&str],
    ) -> Res<ProbeReport> {
        if exit_code != 0 {
            return Err(ProbeError::Exit {
                code: exit_code,
                tail: tail_of(&self.redactor, stderr),
            }
            .into());
        }
        let request = ProbeRequest {
            only: only.iter().map(|id| (*id).to_owned()).collect(),
            docker,
            mode: ProbeMode::Sandbox,
        };
        Ok(ProbeReport::parse(stdout, &request, &self.redactor)?)
    }

    pub(crate) fn engine_port(&self) -> Res<u16> {
        self.args
            .engine_port
            .map(std::num::NonZeroU16::get)
            .ok_or_else(|| {
                fail("P0.5 is done; start the engine and pass --engine-port and --engine-certs")
            })
    }

    pub(crate) async fn find_launch(
        &self,
        launch: &jackin_core::LaunchId,
    ) -> Res<Vec<SandboxView>> {
        let query = SandboxQuery::Labels(LabelSelector::launch(launch));
        Ok(self.b.gateway.find_sandboxes(&query).await?)
    }

    pub(crate) async fn managed(&self) -> Res<Vec<SandboxView>> {
        let query = SandboxQuery::Labels(LabelSelector::managed());
        Ok(self.b.gateway.find_sandboxes(&query).await?)
    }

    pub(crate) fn open_pty(&self, args: &[&str], cwd: &Path) -> Res<PtySession> {
        let mut full: Vec<String> = vec!["--gateway".to_owned(), self.args.gateway.clone()];
        full.extend(args.iter().map(|arg| (*arg).to_owned()));
        let spec = PtySpec {
            program: self.jackin.clone(),
            args: full,
            cwd: Some(cwd.to_path_buf()),
            env: self.pty_env.clone(),
            cols: 120,
            rows: 40,
        };
        let link = self.b.pty.open(&spec)?;
        Ok(PtySession::new(link, self.redactor.clone()))
    }

    pub(crate) fn finish_pty(&mut self, label: &str, session: &PtySession) {
        self.capture(&format!("{label} transcript"), session.transcript());
    }
}

pub(crate) fn flag_docker(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

pub(crate) fn tail_of(redactor: &Redactor, bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let chars: Vec<char> = text.trim().chars().collect();
    let start = chars.len().saturating_sub(300);
    redactor.scrub(&chars[start..].iter().collect::<String>())
}

pub(crate) fn text_of(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

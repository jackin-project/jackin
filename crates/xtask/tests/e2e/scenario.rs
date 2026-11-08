// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod gateway;
mod host;
mod pty;
pub(crate) mod report;
pub(crate) mod world;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use jackin_core::{GatewayName, OpenShellWorkspace};
use jackin_testkit::FakeGateway;
use jackin_xtask::{
    Boundaries, Budgets, Capture, Clock, E2eArgs, ExitStatus, HostEngine, Sink, run_e2e,
};
use serde_json::Value;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

pub(crate) use self::gateway::ScenarioGateway;
pub(crate) use self::host::ScenarioHost;
pub(crate) use self::pty::ScenarioPty;
pub(crate) use self::world::World;

pub(crate) const GATEWAY: &str = "jackin-local";
pub(crate) const TAG: &str = "abcd";

#[derive(Debug, Default)]
pub(crate) struct TestClock {
    millis: AtomicU64,
}

impl TestClock {
    pub(crate) fn advance(&self, by: Duration) {
        self.millis.fetch_add(
            u64::try_from(by.as_millis()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
    }
}

impl Clock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProbePatch {
    pub step: String,
    pub call: Option<u32>,
    pub pointer: String,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub needle: String,
    pub nth: Option<u32>,
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
    pub advance: Duration,
}

impl Rule {
    pub(crate) fn exit(needle: &str, exit: i32, stdout: &str) -> Rule {
        Rule {
            needle: needle.to_owned(),
            nth: None,
            exit,
            stdout: stdout.to_owned(),
            stderr: String::new(),
            advance: Duration::ZERO,
        }
    }
}

#[derive(Clone, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch for each scripted fault"
)]
pub(crate) struct Knobs {
    pub probes: Vec<ProbePatch>,
    pub drop_steps: Vec<String>,
    pub exec: Vec<Rule>,
    pub host: Vec<Rule>,
    pub pty_fault: Option<PtyFault>,
    pub pty_extra: BTreeMap<String, String>,
    pub shell_exit: Vec<(String, i32)>,
    pub leak_in_pty: Option<String>,
    pub doctor: Option<Vec<String>>,
    pub unserving_at_doctor: bool,
    pub second_build_id: Option<String>,
    pub first_build_without_install: bool,
    pub second_build_stage: Option<String>,
    pub duplicate_on_retry: bool,
    pub rename_on_retry: bool,
    pub drop_labels: Vec<String>,
    pub set_labels: Vec<(String, String)>,
    pub name_shape: Option<String>,
    pub export_mode: ExportMode,
    pub leak_in_load_stderr: Option<String>,
    pub leak_in_inspect: Option<String>,
    pub leak_in_env_file: Option<String>,
    pub leak_in_auth_file: Option<String>,
    pub usage: Option<Vec<UsageScript>>,
    pub purge_unguarded: bool,
    pub force_leaves_sandbox: bool,
    pub stop_changes_s: bool,
    pub start_moves_s_codex: bool,
    pub bound_ignored: bool,
    pub account_override: Option<(String, String)>,
    pub provider_account_override: Option<(String, String)>,
    pub boot_unchanged: bool,
    pub file_lost: bool,
    pub hang_probe: bool,
    pub panic_in_probe: Option<String>,
    pub foreign_run_at: Option<String>,
    pub warning_missing: bool,
    pub container_in_default_engine: bool,
    pub host_engine_unlistable: bool,
    pub same_engine_id: bool,
    pub registry_write: bool,
    pub preflight_fail_exits_zero: bool,
    pub hook_error_text: Option<String>,
    pub error_phase_after_failed_load: bool,
    pub stopped_after_quit: bool,
    pub restart_same_pid: bool,
    pub quit_supervisor: Option<String>,
    pub restart_supervisor: Option<String>,
    pub j05_fault: Option<J05Fault>,
    pub cancel_at: Option<String>,
    pub advance: Vec<(String, Duration)>,
    pub probe_raw: Option<ProbeRaw>,
    pub observe_symlink: bool,
    pub rewrite_codex_home: bool,
    pub self_signals: Vec<(String, i32)>,
    pub fail_delete_after: Option<(String, u32)>,
    pub stuck_provider: Option<String>,
    pub create_env_unreadable: bool,
    pub docker_label_off: bool,
    pub reflect_body: Option<String>,
    pub reflect_not_descendant: bool,
    pub reflect_never_done: bool,
    pub codex_young_polls: u32,
    pub create_env_drop_launch: bool,
    pub purge_vanishes: bool,
    pub git_fail: Option<(String, String)>,
    pub host_spawn_error: Option<(String, String)>,
    pub reflect_swallow_first_enter: bool,
    pub reflect_done_code: i32,
    pub auth_id_token: Option<String>,
    pub create_env_extra: Option<(String, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProbeRaw {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PtyFault {
    pub from: u32,
    pub header: Option<String>,
    pub exit: Option<i32>,
    pub never_exits: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum J05Fault {
    DropFile,
    ExtraFile,
    BadDigest,
    IncludeIgnored,
    NewlineName,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportMode {
    #[default]
    Normal,
    Silent,
    NoRef,
    MoveTip,
    ChangeFile,
    StageFile,
    RoleRepo,
}

#[derive(Clone, Debug)]
pub(crate) struct UsageScript {
    pub a1: Value,
    pub fixtures: Value,
    pub drop_account: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    pub accounts: Vec<String>,
    pub default_account: Option<String>,
    pub role_binding: Option<String>,
    pub workspace_bindings: BTreeMap<String, String>,
    pub workspaces: BTreeMap<String, (PathBuf, bool)>,
    pub role_dir: Option<PathBuf>,
    pub builds: u32,
    pub usage_calls: u32,
    pub sandboxes: BTreeMap<String, SandboxModel>,
    pub containers: Vec<String>,
    pub commands: Vec<String>,
    pub loads: u32,
    pub age_polls: u32,
    pub exports: u32,
    pub attaches: u32,
    pub rule_hits: BTreeMap<String, u32>,
    pub links: Vec<PathBuf>,
    pub tag: String,
}

#[derive(Clone, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the sandbox state is a set of independent facts"
)]
pub(crate) struct SandboxModel {
    pub workspace: String,
    pub account: String,
    pub launch: String,
    pub kind: String,
    pub supervisor: String,
    pub codex_pid: u32,
    pub setup_runs: u32,
    pub preflight_runs: u32,
    pub preflight_failures: u32,
    pub boot: u32,
    pub committed: bool,
    pub probe_calls: BTreeMap<String, u32>,
    pub stopped: bool,
    pub started_once: bool,
    pub kept: bool,
    pub crashed: bool,
}

#[derive(Debug)]
pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub knobs: Mutex<Knobs>,
    pub clock: Arc<TestClock>,
    pub cancel: CancellationToken,
}

impl Shared {
    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn knobs(&self) -> MutexGuard<'_, Knobs> {
        self.knobs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub(crate) struct ScenarioEngine {
    pub shared: Arc<Shared>,
}

#[async_trait::async_trait]
impl HostEngine for ScenarioEngine {
    async fn engine_id(&self) -> Result<String, String> {
        Ok(if self.shared.knobs().same_engine_id {
            "ENGINE-D24".to_owned()
        } else {
            "ENGINE-HOST".to_owned()
        })
    }

    async fn container_names(&self) -> Result<Vec<String>, String> {
        if self.shared.knobs().host_engine_unlistable {
            return Err("the engine did not answer".to_owned());
        }
        Ok(if self.shared.knobs().container_in_default_engine {
            vec![format!("e2e-{}", self.shared.state().tag)]
        } else {
            Vec::new()
        })
    }
}

#[derive(Debug)]
pub(crate) struct Ran {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl Ran {
    pub(crate) fn line(&self, id: &str) -> Option<(String, String)> {
        self.stdout.lines().find_map(|line| {
            let mut parts = line.splitn(4, ' ').filter(|part| !part.is_empty());
            let first = parts.next()?;
            if first != id {
                return None;
            }
            let status = parts.next()?.to_owned();
            let rest: Vec<&str> = line.split_whitespace().skip(3).collect();
            Some((status, rest.join(" ")))
        })
    }

    pub(crate) fn status_of(&self, id: &str) -> String {
        self.line(id).map(|(status, _)| status).unwrap_or_default()
    }

    pub(crate) fn reason_of(&self, id: &str) -> String {
        self.line(id).map(|(_, reason)| reason).unwrap_or_default()
    }

    pub(crate) fn statuses(&self) -> BTreeMap<String, String> {
        self.stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let id = parts.next()?;
                (id.starts_with('J') || id == "CLEAN")
                    .then(|| (id.to_owned(), parts.next().unwrap_or("").to_owned()))
            })
            .collect()
    }
}

pub(crate) struct Scenario {
    pub _dir: TempDir,
    pub shared: Arc<Shared>,
    pub fake: Arc<FakeGateway>,
    pub gateway: Arc<ScenarioGateway>,
    pub world: World,
    pub args: E2eArgs,
    pub budgets: Budgets,
    pub cancel: CancellationToken,
    pub tag: String,
}

impl Scenario {
    pub(crate) async fn new() -> Scenario {
        let dir = tempfile::tempdir().expect("tempdir");
        let world = World::build(dir.path());
        let clock = Arc::new(TestClock::default());
        let cancel = CancellationToken::new();
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            knobs: Mutex::new(Knobs::default()),
            clock,
            cancel: cancel.clone(),
        });
        let fake = Arc::new(FakeGateway::new(
            GatewayName::parse(GATEWAY).expect("gateway name"),
            OpenShellWorkspace::parse("default").expect("workspace"),
        ));
        let gateway = Arc::new(ScenarioGateway::new(
            Arc::clone(&fake),
            Arc::clone(&shared),
            &world,
        ));
        gateway.seed_foreign().await;
        let args = world::args(&world, true);
        Scenario {
            _dir: dir,
            shared,
            fake,
            gateway,
            world,
            args,
            budgets: {
                let mut budgets = Budgets::uniform(Duration::from_secs(5));
                budgets.cleanup_settle = Duration::from_millis(300);
                budgets.expect = Duration::from_millis(400);
                budgets
            },
            cancel,
            tag: TAG.to_owned(),
        }
    }

    pub(crate) fn knobs(&self) -> MutexGuard<'_, Knobs> {
        self.shared.knobs()
    }

    pub(crate) fn probe_json(&self, steps: &[&str], docker: bool) -> Value {
        let repo = self.world.root.join("b-fixture");
        if !repo.exists() {
            std::fs::create_dir_all(&repo).expect("mkdir");
            std::fs::write(repo.join("README.md"), "fixture\n").expect("write");
            for args in [
                vec!["init", "-q", "-b", "main"],
                vec!["add", "-A"],
                vec![
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-q",
                    "-m",
                    "x",
                ],
            ] {
                let status = jackin_testkit::git_command()
                    .args(&args)
                    .current_dir(&repo)
                    .status()
                    .expect("git");
                assert!(status.success());
            }
        }
        let name = "jackin-w-0001";
        let model = SandboxModel {
            workspace: "w".to_owned(),
            launch: "launch1".to_owned(),
            boot: 1,
            ..SandboxModel::default()
        };
        self.state()
            .workspaces
            .insert("w".to_owned(), (repo, false));
        self.state()
            .sandboxes
            .insert(name.to_owned(), model.clone());
        let args: Vec<String> = [
            "--docker",
            if docker { "on" } else { "off" },
            "--engine",
            "host.openshell.internal:2376",
            "--only",
            &steps.join(","),
        ]
        .map(str::to_owned)
        .to_vec();
        let sandbox = jackin_core::SandboxName::parse(name).expect("name");
        report::build_report(&self.gateway, &sandbox, &model, &args)
    }

    pub(crate) fn patch(&self, step: &str, call: Option<u32>, pointer: &str, value: Value) {
        self.knobs().probes.push(ProbePatch {
            step: step.to_owned(),
            call,
            pointer: pointer.to_owned(),
            value,
        });
    }

    pub(crate) fn exec_rule(&self, needle: &str, exit: i32, stdout: &str) {
        self.knobs().exec.push(Rule::exit(needle, exit, stdout));
    }

    pub(crate) fn host_rule(&self, needle: &str, nth: Option<u32>, exit: i32, stderr: &str) {
        let mut rule = Rule::exit(needle, exit, "");
        rule.nth = nth;
        rule.stderr = stderr.to_owned();
        self.knobs().host.push(rule);
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.shared.state()
    }

    pub(crate) fn boundaries(&self) -> (Boundaries, Capture, Capture) {
        let (stdout, out) = Sink::capture();
        let (stderr, err) = Sink::capture();
        let host = Arc::new(ScenarioHost::new(
            Arc::clone(&self.shared),
            Arc::clone(&self.gateway),
            Arc::clone(&self.fake),
            self.world.clone(),
        ));
        let pty = Arc::new(ScenarioPty::new(
            Arc::clone(&self.shared),
            Arc::clone(&self.gateway),
            self.world.clone(),
        ));
        let mut env = BTreeMap::new();
        env.insert("HOME".to_owned(), self.world.home.display().to_string());
        env.insert("PATH".to_owned(), std::env::var("PATH").unwrap_or_default());
        env.insert("USER".to_owned(), "tester".to_owned());
        env.insert("LANG".to_owned(), "C".to_owned());
        let gateway: Arc<ScenarioGateway> = Arc::clone(&self.gateway);
        let clock: Arc<TestClock> = Arc::clone(&self.shared.clock);
        let boundaries = Boundaries {
            host,
            pty,
            gateway,
            engine: Arc::new(ScenarioEngine {
                shared: Arc::clone(&self.shared),
            }),
            clock,
            budgets: self.budgets.clone(),
            env,
            jackin: PathBuf::from("jackin"),
            repo_root: self.world.repo_root.clone(),
            codex_home: self.world.codex_home.clone(),
            scratch_root: self.world.scratch_root.clone(),
            log_dir: self.world.log_dir.clone(),
            lock_dir: self.world.lock_dir.clone(),
            tag: self.tag.clone(),
            cancel: self.cancel.clone(),
            stdout,
            stderr,
        };
        (boundaries, out, err)
    }

    pub(crate) async fn run(&self) -> Ran {
        self.state().tag.clone_from(&self.tag);
        let (boundaries, out, err) = self.boundaries();
        let status = run_e2e(&self.args, boundaries).await;
        Ran {
            status,
            stdout: out.text(),
            stderr: err.text(),
        }
    }
}

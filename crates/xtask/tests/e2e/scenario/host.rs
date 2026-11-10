// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    AccountName, Agent, CaFingerprint, Compatibility, DockerLabel, ImageId, InspectView, LaunchId,
    Phase, PlannedLabels, PolicyLabel, PolicyStatus, ProviderName, RoleName, SandboxName,
    SandboxSummary, SupervisorReading, SupervisorState, WorkspaceName,
};
use jackin_openshell::Gateway;
use jackin_testkit::FakeGateway;
use jackin_xtask::{Exit, HostCommand, HostError, HostOutput, HostProcess, RealHost};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::gateway::ScenarioGateway;
use super::world::{POLICY, World};
use super::{ExportMode, Rule, SandboxModel, Shared};
use crate::support::capture;

pub(crate) const IMAGE_ID: &str =
    "sha256:0f3a9c1d5e7b2a4c6e8f0a1b3c5d7e9f1a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d";

#[derive(Debug)]
pub(crate) struct ScenarioHost {
    shared: Arc<Shared>,
    gateway: Arc<ScenarioGateway>,
    fake: Arc<FakeGateway>,
    world: World,
    real: RealHost,
}

fn done(code: i32, stdout: &str, stderr: &str) -> Result<HostOutput, HostError> {
    Ok(HostOutput::new(
        Exit::Code(code),
        stdout.as_bytes().to_vec(),
        stderr.as_bytes().to_vec(),
    ))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = capture(
        jackin_testkit::git_command()
            .args([
                "-c",
                "user.name=scenario",
                "-c",
                "user.email=scenario@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir),
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

impl ScenarioHost {
    pub(crate) fn new(
        shared: Arc<Shared>,
        gateway: Arc<ScenarioGateway>,
        fake: Arc<FakeGateway>,
        world: World,
    ) -> ScenarioHost {
        ScenarioHost {
            shared,
            gateway,
            fake,
            world,
            real: RealHost::default(),
        }
    }

    fn account_for(&self, workspace: &str) -> String {
        let knobs = self.shared.knobs().clone();
        let state = self.shared.state();
        let bound = if knobs.bound_ignored {
            None
        } else {
            state.workspace_bindings.get(workspace)
        };
        let chosen = bound
            .or(state.role_binding.as_ref())
            .or(state.default_account.as_ref())
            .cloned()
            .unwrap_or_default();
        match knobs.account_override {
            Some((from, to)) if from == chosen && state.sandboxes.len() >= 2 => to,
            _ => chosen,
        }
    }

    fn docker_value(&self, workspace: &str) -> String {
        let docker = self
            .shared
            .state()
            .workspaces
            .get(workspace)
            .is_some_and(|(_, docker)| *docker);
        if docker && !self.shared.knobs().docker_label_off {
            "engine-2376-0123456789abcdef".to_owned()
        } else {
            "off".to_owned()
        }
    }

    fn labels_for(&self, workspace: &str, account: &str, launch: &str) -> Vec<(String, String)> {
        let first = self.shared.state().sandboxes.is_empty();
        let knobs = self.shared.knobs().clone();
        let mut pairs = vec![
            ("jackin.dev/managed".to_owned(), "true".to_owned()),
            ("jackin.dev/workspace".to_owned(), workspace.to_owned()),
            ("jackin.dev/role".to_owned(), "sentinel".to_owned()),
            (
                "jackin.dev/agents".to_owned(),
                jackin_core::Sha256::of(format!("codex:zai:{account}:glm-5.3").as_bytes()).as_str()
                    [..32]
                    .to_owned(),
            ),
            (
                "jackin.dev/policy".to_owned(),
                PolicyLabel::of(POLICY).to_string(),
            ),
            ("jackin.dev/image".to_owned(), IMAGE_ID[7..39].to_owned()),
            ("jackin.dev/launch".to_owned(), launch.to_owned()),
            ("jackin.dev/docker".to_owned(), self.docker_value(workspace)),
        ];
        if first {
            pairs.retain(|(key, _)| !knobs.drop_labels.contains(key));
            for (key, value) in knobs.set_labels {
                if let Some(slot) = pairs.iter_mut().find(|(k, _)| *k == key) {
                    slot.1 = value;
                }
            }
        }
        pairs
    }

    fn make_sandbox(
        &self,
        workspace: &str,
        launch: &str,
        account: &str,
        suffix: &str,
    ) -> Result<String, String> {
        let kind = workspace.rsplit('-').next().unwrap_or("main").to_owned();
        let shape = if self.shared.state().sandboxes.is_empty() {
            self.shared.knobs().name_shape.clone()
        } else {
            None
        };
        let name = shape.unwrap_or_else(|| format!("jackin-{kind}-{suffix}"));
        self.gateway
            .add_sandbox(&name, self.labels_for(workspace, account, launch))?;
        self.shared.state().sandboxes.insert(
            name.clone(),
            SandboxModel {
                workspace: workspace.to_owned(),
                account: account.to_owned(),
                launch: launch.to_owned(),
                kind,
                supervisor: "waiting".to_owned(),
                codex_pid: 0,
                boot: 1,
                ..SandboxModel::default()
            },
        );
        Ok(name)
    }

    fn doctor(&self) -> Result<HostOutput, HostError> {
        let knobs = self.shared.knobs().clone();
        let pinned = Compatibility::pinned().expect("pins").openshell_version;
        if knobs.rewrite_codex_home {
            let auth = self.world.codex_home.join("auth.json");
            let text = std::fs::read_to_string(&auth).unwrap_or_default();
            drop(std::fs::write(&auth, text.replace("rt_a1", "rt_zz")));
        }
        if knobs.unserving_at_doctor {
            self.fake.set_serving(false);
        }
        let lines = knobs.doctor.unwrap_or_else(|| {
            [
                "pass openshell-binary    /usr/local/bin/openshell",
                "pass ssh-binary          /usr/bin/ssh",
                "pass git-binary          /usr/bin/git",
                "pass gateway-registered  jackin-local",
                "pass gateway-health      serving",
                &format!("pass pinned-version      {pinned}"),
                "pass engine-socket       /var/run/docker.sock",
            ]
            .map(str::to_owned)
            .to_vec()
        });
        done(0, &format!("{}\n", lines.join("\n")), "")
    }

    fn build(&self, words: &[&str]) -> Result<HostOutput, HostError> {
        let first = words.contains(&"--rebuild");
        let knobs = self.shared.knobs().clone();
        self.shared.state().builds += 1;
        let id = if first {
            IMAGE_ID.to_owned()
        } else {
            knobs.second_build_id.unwrap_or_else(|| IMAGE_ID.to_owned())
        };
        let stderr = if first {
            let install = if knobs.first_build_without_install {
                ""
            } else {
                "jackin-sentinel build layer: installing jackin-sentinel-probe\njackin-sentinel build layer: installed\n"
            };
            format!(
                "image construct: building\nimage construct: built\nimage role: building\n{install}image role: built\nimage derived: building\nimage derived: built\n"
            )
        } else if let Some(line) = knobs.second_build_stage {
            format!("image construct: cached\n{line}\n")
        } else {
            "image construct: cached\nimage derived: cached\n".to_owned()
        };
        done(
            0,
            &format!(
                "{}\n",
                json!({"schema": 1, "image": "jackin.invalid/sentinel:abc", "id": id})
            ),
            &stderr,
        )
    }

    fn role_check(path: &str) -> Result<HostOutput, HostError> {
        let manifest =
            std::fs::read_to_string(Path::new(path).join("jackin.role.toml")).unwrap_or_default();
        let policy =
            std::fs::read_to_string(Path::new(path).join("policy.yaml")).unwrap_or_default();
        if manifest.contains("mystery_field") {
            return done(1, "", "jackin.role.toml: unknown field `mystery_field`\n");
        }
        if policy.contains("chatgpt.com") {
            return done(
                1,
                "",
                "policy.yaml: rule e2e-stolen reaches the provider host chatgpt.com\n",
            );
        }
        done(0, "", "")
    }

    fn inspect(&self, name: &str) -> Result<HostOutput, HostError> {
        let model = self
            .shared
            .state()
            .sandboxes
            .get(name)
            .cloned()
            .ok_or_else(|| HostError::Spawn {
                program: "jackin".to_owned(),
                message: format!("no sandbox {name}"),
            })?;
        let planned = PlannedLabels {
            workspace: WorkspaceName::parse(&model.workspace).expect("workspace"),
            role: RoleName::parse("sentinel").expect("role"),
            agents: jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
                Agent::Codex,
                AccountName::parse(&model.account).expect("account"),
            )),
            policy: PolicyLabel::of(POLICY),
            launch: LaunchId::parse(&model.launch).expect("launch"),
        };
        let docker = if self.docker_value(&model.workspace) == "off" {
            DockerLabel::Off
        } else {
            DockerLabel::Engine {
                port: std::num::NonZeroU16::new(2376).expect("port"),
                ca: CaFingerprint::parse("0123456789abcdef").expect("ca"),
            }
        };
        let supervisor = SupervisorReading::Reported(
            SupervisorState::parse(&model.supervisor).expect("supervisor"),
        );
        let view = InspectView {
            schema: 1,
            summary: SandboxSummary {
                name: SandboxName::parse(name).expect("name"),
                labels: planned.complete(&ImageId::parse(IMAGE_ID).expect("image"), docker),
                phase: Phase::Ready,
                created_at: None,
            },
            providers: Vec::new(),
            policy: PolicyStatus {
                hash: None,
                version: None,
                loaded: true,
            },
            closure_violations: Vec::new(),
            supervisor,
        };
        let mut value = serde_json::to_value(&view).expect("json");
        if let Some(leak) = self.shared.knobs().leak_in_inspect.clone() {
            value["debug"] = Value::String(leak);
        }
        done(0, &format!("{value}\n"), "")
    }

    async fn load(&self, words: &[&str]) -> Result<HostOutput, HostError> {
        let path = words.get(2).copied().unwrap_or_default();
        let launch = words
            .iter()
            .position(|word| *word == "--launch-id")
            .and_then(|at| words.get(at + 1))
            .copied()
            .unwrap_or_default();
        let failing = words.contains(&"SENTINEL_FAIL_PREFLIGHT=1");
        let found = self
            .shared
            .state()
            .workspaces
            .iter()
            .find(|(_, (dir, _))| dir.display().to_string() == path)
            .map(|(name, (_, docker))| (name.clone(), *docker));
        let Some((workspace, docker)) = found else {
            return done(1, "", "no workspace for the project directory\n");
        };
        let knobs = self.shared.knobs().clone();
        let existing = self
            .shared
            .state()
            .sandboxes
            .iter()
            .find(|(_, model)| model.launch == launch)
            .map(|(name, _)| name.clone());
        let account = self.account_for(&workspace);
        let loads = {
            let mut state = self.shared.state();
            state.loads += 1;
            state.loads
        };
        let mut name = match existing {
            Some(name) => name,
            None => self
                .make_sandbox(&workspace, launch, &account, &format!("{loads:04x}"))
                .map_err(|message| HostError::Spawn {
                    program: "jackin".to_owned(),
                    message,
                })?,
        };
        if self.shared.state().sandboxes[&name].started_once && knobs.duplicate_on_retry {
            drop(self.make_sandbox(
                &workspace,
                launch,
                &account,
                &format!("{:04x}", loads + 100),
            ));
        }
        if self.shared.state().sandboxes[&name].started_once && knobs.rename_on_retry {
            self.shared.state().sandboxes.remove(&name);
            drop(
                self.fake
                    .delete_sandbox(&SandboxName::parse(&name).expect("name"))
                    .await,
            );
            name = self
                .make_sandbox(
                    &workspace,
                    launch,
                    &account,
                    &format!("{:04x}", loads + 200),
                )
                .map_err(|message| HostError::Spawn {
                    program: "jackin".to_owned(),
                    message,
                })?;
        }
        let mut stderr = String::from("sandbox ready\n");
        if docker && !knobs.warning_missing {
            stderr.push_str(
                "warning: the Docker capability is enabled for this sandbox\n  the agent can read the engine client key\n  containers bypass the sandbox network policy\n  a bind mount names a path on the engine host, not in the sandbox\n  the engine outlives the sandbox, and sandboxes that use one engine see each other's containers; remove them yourself\n",
            );
        }
        if let Some(leak) = &knobs.leak_in_load_stderr {
            stderr.push_str(&format!("debug {leak}\n"));
        }
        let mut state = self.shared.state();
        let model = state.sandboxes.get_mut(&name).expect("sandbox");
        model.setup_runs = 1;
        model.preflight_runs += 1;
        model.started_once = true;
        if failing {
            model.preflight_failures += 1;
            model.supervisor = "waiting".to_owned();
            drop(state);
            if knobs.error_phase_after_failed_load {
                self.fake.force_phase(
                    &SandboxName::parse(&name).expect("name"),
                    Phase::Error { exit_code: None },
                );
            }
            if knobs.preflight_fail_exits_zero {
                return done(0, &format!("{name}\t{launch}\n"), &stderr);
            }
            let text = knobs.hook_error_text.unwrap_or_else(|| {
                format!("error: launch {launch} failed at step hooks: the preflight hook exited with code 17\nsentinel preflight: SENTINEL_FAIL_PREFLIGHT=1\n")
            });
            stderr.push_str(&format!("{text}\n"));
            return done(1, "", &stderr);
        }
        model.supervisor = "running 1".to_owned();
        model.codex_pid = 4242;
        drop(state);
        self.fake
            .force_phase(&SandboxName::parse(&name).expect("name"), Phase::Ready);
        done(0, &format!("{name}\t{launch}\n"), &stderr)
    }

    fn export(&self, name: &str, command: &HostCommand) -> Result<HostOutput, HostError> {
        let exports = {
            let mut state = self.shared.state();
            state.exports += 1;
            state.exports
        };
        let mode = if exports >= 2 {
            self.shared.knobs().export_mode
        } else {
            ExportMode::Normal
        };
        let dir = command.cwd.clone().unwrap_or_default();
        let reference = format!("refs/heads/jackin/{name}/main");
        match mode {
            ExportMode::NoRef => {
                git(&dir, &["update-ref", "-d", &reference]);
                return done(0, "", "nothing to export\n");
            }
            ExportMode::RoleRepo => {
                git(
                    &self.world.role,
                    &["update-ref", "refs/heads/jackin/leak/main", "HEAD"],
                );
            }
            ExportMode::MoveTip => {
                git(&dir, &["commit", "--allow-empty", "-q", "-m", "moved"]);
            }
            ExportMode::ChangeFile => {
                let readme = dir.join("untracked.txt");
                let mut text = std::fs::read_to_string(&readme).unwrap_or_default();
                text.push_str("changed\n");
                std::fs::write(readme, text).expect("write");
            }
            ExportMode::StageFile => {
                git(&dir, &["add", "untracked.txt"]);
            }
            ExportMode::Normal | ExportMode::Silent => {}
        }
        let head = git(&dir, &["rev-parse", "HEAD"]);
        git(&dir, &["update-ref", &reference, &head]);
        if mode == ExportMode::Silent {
            return done(0, "", "");
        }
        done(0, &format!("{reference}\t{head}\n"), "")
    }

    fn usage(&self) -> Result<HostOutput, HostError> {
        let call = {
            let mut state = self.shared.state();
            state.usage_calls += 1;
            state.usage_calls
        };
        let scripts = self.shared.knobs().usage.clone();
        let default = json!({"state": "windows", "windows": [
            {"kind": "weekly", "label": "Weekly", "used_percent": 12.5, "remaining": {"kind": "percent", "value": 87.5}, "resets_at": 1_800_000_000},
            {"kind": "credits", "label": "credits", "used_percent": null, "remaining": {"kind": "amount", "value": "35348.77", "unit": "credits"}, "resets_at": null},
        ]});
        let expired = json!({"state": "unavailable", "reason": {"kind": "expired_login"}});
        let mut dropped = None;
        let (a1, fixtures) = scripts.map_or((default.clone(), expired.clone()), |list| {
            let at = usize::try_from(call)
                .unwrap_or(1)
                .saturating_sub(1)
                .min(list.len().saturating_sub(1));
            dropped.clone_from(&list[at].drop_account);
            (list[at].a1.clone(), list[at].fixtures.clone())
        });
        let tag = self.shared.state().tag.clone();
        let accounts: Vec<Value> = [(format!("e2e-{tag}-a1"), a1), (format!("e2e-{tag}-fxr"), fixtures.clone()), (format!("e2e-{tag}-fxw"), fixtures)]
            .into_iter()
            .filter(|(account, _)| dropped.as_ref() != Some(account))
            .map(|(account, outcome)| json!({"account": account, "provider": "codex", "freshness": {"state": "fresh"}, "outcome": outcome}))
            .collect();
        done(
            0,
            &format!(
                "{}\n",
                json!({"schema": 1, "generated_at": 0, "accounts": accounts})
            ),
            "",
        )
    }

    async fn jackin(&self, command: &HostCommand) -> Result<HostOutput, HostError> {
        let words: Vec<&str> = command.args.iter().skip(2).map(String::as_str).collect();
        match words.as_slice() {
            ["gateway" | "role", "add", ..] | ["account", "default", ..] => {
                if let ["gateway", "add", ..] = words.as_slice() {
                    let mut state = self.shared.state();
                    state.accounts.clear();
                    state.default_account = None;
                    state.role_binding = None;
                    state.workspace_bindings.clear();
                    state.workspaces.clear();
                }
                if let ["role", "add", _, dir] = words.as_slice() {
                    self.shared.state().role_dir = Some(Path::new(dir).to_path_buf());
                }
                if let ["account", "default", name] = words.as_slice() {
                    self.shared.state().default_account = Some((*name).to_owned());
                }
                done(0, "", "")
            }
            ["account", "add", "codex" | "zai", "--name", name, ..] => {
                if self.shared.knobs().registry_write
                    && let Some(base) = command.env.get("XDG_CONFIG_HOME")
                {
                    drop(std::fs::write(
                        Path::new(base).join("openshell/leftover.txt"),
                        "x",
                    ));
                }
                self.shared.state().accounts.push((*name).to_owned());
                drop(self.gateway.put_codex_provider(name).await);
                done(0, "", "")
            }
            ["account", "bind", name, "--role", _] => {
                self.shared.state().role_binding = Some((*name).to_owned());
                done(0, "", "")
            }
            ["account", "bind", name, "--workspace", workspace] => {
                self.shared
                    .state()
                    .workspace_bindings
                    .insert((*workspace).to_owned(), (*name).to_owned());
                done(0, "", "")
            }
            ["account", "rm", name] => {
                drop(
                    self.gateway
                        .delete_provider(
                            &ProviderName::parse(&format!("jackin-{name}-codex"))
                                .expect("provider"),
                        )
                        .await,
                );
                done(0, "", "")
            }
            ["workspace", "add", name, path, rest @ ..] => {
                self.shared.state().workspaces.insert(
                    (*name).to_owned(),
                    (Path::new(path).to_path_buf(), rest.contains(&"--docker")),
                );
                done(0, "", "")
            }
            ["doctor"] => self.doctor(),
            ["build", ..] => self.build(&words),
            ["role", "check", path] => Self::role_check(path),
            ["load", "sentinel", ..] => self.load(&words).await,
            ["inspect", name, "--json"] => self.inspect(name),
            ["export", name] => self.export(name, command),
            ["stop", name] => {
                if let Some(model) = self.shared.state().sandboxes.get_mut(*name) {
                    model.stopped = true;
                }
                if self.shared.knobs().stop_changes_s
                    && let Some(first) = self.first_main()
                {
                    self.fake.force_phase(&first, Phase::Stopped);
                }
                done(0, "", "")
            }
            ["start", name] => {
                let unchanged = self.shared.knobs().boot_unchanged;
                if self.shared.knobs().start_moves_s_codex
                    && let Some(first) = self.first_main()
                    && let Some(model) = self.shared.state().sandboxes.get_mut(first.as_str())
                {
                    model.codex_pid = 9999;
                }
                if let Some(model) = self.shared.state().sandboxes.get_mut(*name) {
                    model.stopped = false;
                    if !unchanged {
                        model.boot += 1;
                    }
                }
                done(0, "", "")
            }
            ["purge", name, rest @ ..] => self.purge(name, rest.contains(&"--force")).await,
            ["usage", "--json"] => self.usage(),
            other => done(
                2,
                "",
                &format!("scenario: unhandled jackin command {other:?}\n"),
            ),
        }
    }

    async fn purge(&self, name: &str, force: bool) -> Result<HostOutput, HostError> {
        let knobs = self.shared.knobs().clone();
        let committed = self
            .shared
            .state()
            .sandboxes
            .get(name)
            .is_some_and(|model| model.committed);
        if !force && committed && !knobs.purge_unguarded {
            if knobs.purge_vanishes {
                drop(
                    self.fake
                        .delete_sandbox(&SandboxName::parse(name).expect("name"))
                        .await,
                );
                self.shared.state().sandboxes.remove(name);
            }
            return done(3, "", "refused: the sandbox holds unexported work\n");
        }
        if force && knobs.force_leaves_sandbox {
            return done(0, "", "");
        }
        drop(
            self.fake
                .delete_sandbox(&SandboxName::parse(name).expect("name"))
                .await,
        );
        self.shared.state().sandboxes.remove(name);
        done(0, "", "")
    }

    fn openshell(&self, command: &HostCommand) -> Result<HostOutput, HostError> {
        let words: Vec<&str> = command.args.iter().skip(2).map(String::as_str).collect();
        match words.as_slice() {
            ["provider", "list", ..] => {
                let providers: Vec<Value> = self
                    .fake
                    .providers()
                    .iter()
                    .map(|name| json!({"name": name.as_str()}))
                    .collect();
                done(
                    0,
                    &json!({"next_page_token": "", "providers": providers}).to_string(),
                    "",
                )
            }
            ["sandbox", "get", name, ..] => done(
                0,
                &json!({"name": name, "phase": "Ready", "labels": {}}).to_string(),
                "",
            ),
            other => done(
                2,
                "",
                &format!("scenario: unhandled openshell command {other:?}\n"),
            ),
        }
    }
}

#[async_trait]
impl HostProcess for ScenarioHost {
    async fn run(
        &self,
        command: &HostCommand,
        cancel: &CancellationToken,
    ) -> Result<HostOutput, HostError> {
        if cancel.is_cancelled() {
            return Err(HostError::Cancelled {
                program: command.program.clone(),
                group: None,
            });
        }
        if command.program == "git" {
            let broken = self.shared.knobs().git_fail.clone();
            if let Some((needle, place)) = broken
                && command.display().contains(&needle)
                && command
                    .cwd
                    .as_ref()
                    .is_some_and(|dir| dir.display().to_string().contains(&place))
            {
                return done(128, "", "fatal: the repository is broken\n");
            }
            return self.real.run(command, cancel).await;
        }
        let line = command.display();
        self.shared.state().commands.push(line.clone());
        let spawn = self.shared.knobs().host_spawn_error.clone();
        if let Some((needle, message)) = spawn
            && line.contains(&needle)
        {
            return Err(HostError::Spawn {
                program: "jackin".to_owned(),
                message,
            });
        }
        let advances = self.shared.knobs().advance.clone();
        for (needle, by) in advances {
            if line.contains(&needle) {
                self.shared.clock.advance(by);
            }
        }
        if self.shared.knobs().observe_symlink
            && line.contains("gateway add")
            && let Some(base) = command.env.get("XDG_CONFIG_HOME")
            && let Ok(target) = std::fs::read_link(Path::new(base).join("openshell"))
        {
            self.shared.state().links.push(target);
        }
        let cancel_at = self.shared.knobs().cancel_at.clone();
        if cancel_at
            .as_deref()
            .is_some_and(|needle| line.contains(needle))
        {
            self.shared.cancel.cancel();
        }
        let signals = self.shared.knobs().self_signals.clone();
        for (needle, number) in signals {
            if line.contains(&needle) {
                self.shared
                    .knobs()
                    .self_signals
                    .retain(|(other, _)| *other != needle);
                let signal = rustix::process::Signal::from_named_raw(number).expect("signal");
                rustix::process::kill_process(rustix::process::getpid(), signal)
                    .expect("self signal");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        let injection = self.shared.knobs().fail_delete_after.clone();
        if let Some((needle, times)) = injection
            && line.contains(&needle)
        {
            self.shared.knobs().fail_delete_after = None;
            for _ in 0..times {
                self.fake.inject(jackin_testkit::Fault::FailNext {
                    op: jackin_openshell::GatewayOp::Delete,
                    error: jackin_openshell::GatewayError::Cancelled {
                        op: jackin_openshell::GatewayOp::Delete,
                        group: None,
                    },
                });
            }
        }
        let rule = self.matching_rule(&line);
        if let Some(rule) = rule {
            self.shared.clock.advance(rule.advance);
            return done(rule.exit, &rule.stdout, &rule.stderr);
        }
        let result = if command.program == "openshell" {
            self.openshell(command)
        } else {
            self.jackin(command).await
        };
        self.after_command(&line).await;
        result
    }
}

impl ScenarioHost {
    fn first_main(&self) -> Option<SandboxName> {
        self.shared
            .state()
            .sandboxes
            .iter()
            .find(|(_, model)| model.kind == "main")
            .and_then(|(name, _)| SandboxName::parse(name).ok())
    }

    fn matching_rule(&self, line: &str) -> Option<Rule> {
        let rules = self.shared.knobs().host.clone();
        for (index, rule) in rules.iter().enumerate() {
            if !line.contains(&rule.needle) {
                continue;
            }
            let hits = {
                let mut state = self.shared.state();
                let slot = state
                    .rule_hits
                    .entry(format!("{index}:{}", rule.needle))
                    .or_insert(0);
                *slot += 1;
                *slot
            };
            if rule.nth.is_none_or(|nth| nth == hits) {
                return Some(rule.clone());
            }
        }
        None
    }

    async fn after_command(&self, line: &str) {
        let trigger = self.shared.knobs().foreign_run_at.clone();
        if trigger
            .as_deref()
            .is_some_and(|needle| line.contains(needle))
        {
            self.shared.knobs().foreign_run_at = None;
            drop(self.gateway.add_sandbox(
                "jackin-main-ffff",
                vec![
                    ("jackin.dev/managed".to_owned(), "true".to_owned()),
                    (
                        "jackin.dev/workspace".to_owned(),
                        "e2e-ffff-main".to_owned(),
                    ),
                ],
            ));
            drop(self.gateway.put_codex_provider("e2e-ffff-a1").await);
        }
    }
}

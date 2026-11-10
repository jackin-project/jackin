// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    Agent, GatewayName, HostPath, Labels, Phase, ProfileId, ProjectRoot, ProviderName, SandboxName,
    SandboxPath, Secret, Sha256,
};
use jackin_openshell::{
    Gateway, GatewayError, GatewayHealth, GatewayOp, Missing, PowerAction, ProfileSync,
    ProgramOutcome, ProgramRun, ProviderMaterial, ProviderProfile, ProviderSpec, ProviderState,
    SandboxCreate, SandboxDetail, SandboxFile, SandboxQuery, SandboxView, TerminalRequest,
    TerminalSession,
};
use jackin_testkit::FakeGateway;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::Shared;
use super::report;
use super::world::{ACCESS, CLIENT_ID, REFRESH, World};

pub(crate) const PROBE: &str = "/usr/local/bin/jackin-sentinel-probe";

#[derive(Debug)]
pub(crate) struct ScenarioGateway {
    pub fake: Arc<FakeGateway>,
    pub shared: Arc<Shared>,
    pub world: World,
}

fn outcome(code: i32, stdout: &str) -> ProgramOutcome {
    ProgramOutcome {
        exit_code: code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

fn selects_agent_under_jackin_start(script: &str, comm: &str) -> bool {
    script.contains("awk '$2 == \"jackin-start\" {print $1; exit}'")
        && script.contains(&format!(" -v comm=\"{comm}\" "))
}

impl ScenarioGateway {
    pub(crate) fn new(
        fake: Arc<FakeGateway>,
        shared: Arc<Shared>,
        world: &World,
    ) -> ScenarioGateway {
        ScenarioGateway {
            fake,
            shared,
            world: world.clone(),
        }
    }

    fn target(&self) -> jackin_xtask::E2eTarget {
        self.shared
            .state()
            .target
            .clone()
            .unwrap_or_else(|| jackin_xtask::E2eTarget::t01().expect("T01 target"))
    }

    fn env_text(target: &jackin_xtask::E2eTarget) -> String {
        let mut text = "PATH=/usr/bin\n".to_owned();
        for variable in &target.behavior().credential.environment {
            let value = match variable.kind {
                jackin_xtask::EnvironmentKind::Placeholder => {
                    format!("openshell:resolve:env:{}", variable.name)
                }
                jackin_xtask::EnvironmentKind::Empty => String::new(),
                jackin_xtask::EnvironmentKind::Brokered => {
                    format!("gateway-brokered-{}", target.tuple().agent.as_str())
                }
            };
            text.push_str(&format!("{}={}\n", variable.name, value));
        }
        text
    }

    pub(crate) async fn seed_foreign(&self) {
        let name = SandboxName::parse("jackin-mine-0000").expect("name");
        let labels = Labels::from_pairs([
            ("jackin.dev/managed".to_owned(), "true".to_owned()),
            ("jackin.dev/workspace".to_owned(), "mine".to_owned()),
            ("jackin.dev/role".to_owned(), "other".to_owned()),
            (
                "jackin.dev/agents".to_owned(),
                Sha256::of(b"codex:codex:mine:gpt-6.1-sol").as_str()[..32].to_owned(),
            ),
            ("jackin.dev/image".to_owned(), "0".repeat(32)),
            ("jackin.dev/launch".to_owned(), "foreignlaunch".to_owned()),
        ])
        .expect("labels");
        self.fake
            .insert_unmanaged(&name, labels, Phase::Ready)
            .expect("foreign sandbox");
        self.put_codex_provider("mine")
            .await
            .expect("foreign provider");
    }

    pub(crate) async fn put_codex_provider(&self, account: &str) -> Result<(), GatewayError> {
        let spec = Agent::Codex.spec();
        let profile =
            ProviderProfile::parse(&spec.codex.openshell.chatgpt_profile).expect("profile");
        self.fake.ensure_profile(&profile).await?;
        self.fake
            .put_provider(&ProviderSpec {
                name: ProviderName::parse(&format!("jackin-{account}-codex")).expect("provider"),
                profile: profile.id,
                keys: spec.codex.openshell.keys,
                material: ProviderMaterial::OAuth2RefreshToken {
                    access_token: Secret::new(ACCESS.to_owned()),
                    refresh_token: Secret::new(REFRESH.to_owned()),
                    client_id: CLIENT_ID.to_owned(),
                    scopes: Vec::new(),
                    expires_at: None,
                    additional_credentials: std::collections::BTreeMap::new(),
                },
            })
            .await
    }

    pub(crate) async fn put_static_provider(
        &self,
        account: &str,
        key: String,
    ) -> Result<(), GatewayError> {
        let target = self.target();
        let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(target.tuple()))
            .expect("profile");
        self.fake.ensure_profile(&profile).await?;
        let account_name = jackin_core::AccountName::parse(account).expect("account");
        self.shared
            .state()
            .static_provider_keys
            .insert(account.to_owned(), key.clone());
        let env = target.tuple().credential_env().expect("credential env");
        self.fake
            .put_provider(&ProviderSpec {
                name: target.provider_name(&account_name),
                profile: profile.id,
                keys: jackin_core::CredentialKeys {
                    access_token: env,
                    refresh_token: "",
                    account_id: "",
                    id_token: "",
                    token_url: "",
                },
                material: ProviderMaterial::StaticKey {
                    key: Secret::new(key),
                },
            })
            .await
    }

    pub(crate) fn add_sandbox(
        &self,
        name: &str,
        pairs: Vec<(String, String)>,
    ) -> Result<(), String> {
        let name = SandboxName::parse(name).map_err(|e| e.to_string())?;
        let labels = Labels::from_pairs(pairs).map_err(|e| e.to_string())?;
        self.fake
            .insert_unmanaged(&name, labels, Phase::Ready)
            .map_err(|e| e.to_string())
    }

    fn model_of(&self, sandbox: &SandboxName) -> Option<super::SandboxModel> {
        self.shared.state().sandboxes.get(sandbox.as_str()).cloned()
    }

    fn answer_probe(&self, run: &ProgramRun) -> Result<ProgramOutcome, GatewayError> {
        self.shared.state().probe_argvs.push(run.args.clone());
        let young = self.shared.knobs().codex_young_polls;
        if run.args.join(" ").contains("J04")
            && self.shared.state().age_polls <= young
            && let Some(model) = self.shared.state().sandboxes.get_mut(run.sandbox.as_str())
        {
            model.crashed = true;
            model.supervisor = "exited 1 1".to_owned();
        }
        let model = self
            .model_of(&run.sandbox)
            .ok_or_else(|| GatewayError::NotFound {
                what: Missing::Sandbox(run.sandbox.clone()),
            })?;
        let text = report::probe_output(self, &run.sandbox, &model, &run.args);
        Ok(outcome(0, &text))
    }

    fn answer_sh(&self, run: &ProgramRun) -> ProgramOutcome {
        let script = run.args.get(1).cloned().unwrap_or_default();
        let line = format!("{} {}", run.program.as_str(), run.args.join(" "));
        let rule = self
            .shared
            .knobs()
            .exec
            .iter()
            .find(|rule| line.contains(&rule.needle))
            .cloned();
        if let Some(rule) = rule {
            return ProgramOutcome {
                exit_code: rule.exit,
                stdout: rule.stdout.into_bytes(),
                stderr: rule.stderr.into_bytes(),
            };
        }
        let sandbox = run.sandbox.as_str().to_owned();
        let model = self.model_of(&run.sandbox).unwrap_or_default();
        let digest = self.world.probe_digest();
        let target = self.target();
        let mut state = self.shared.state();
        if script.contains("etimes=") {
            state.age_polls += 1;
            if !selects_agent_under_jackin_start(&script, &target.behavior().process.comm) {
                return outcome(0, "");
            }
            if let Some(mode) = self.shared.knobs().age_reply {
                let answer = match mode {
                    super::AgeReply::NoChild => "",
                    super::AgeReply::WrongComm if script.contains("$4 == comm") => "",
                    super::AgeReply::WrongParent if script.contains("$2 == main") => "",
                    super::AgeReply::Young if script.contains("$3 >= age") => "1\n",
                    _ => "999\n",
                };
                return outcome(0, answer);
            }
            let young = state.age_polls <= self.shared.knobs().codex_young_polls;
            let answer = if young { "1\n" } else { "999\n" };
            return outcome(0, answer);
        }
        if script == "/usr/bin/env" {
            return outcome(0, &Self::env_text(&target));
        }
        if script.contains("echo uid=") {
            return outcome(0, "uid=1000\nuser=sandbox\ndocker=no\nsudo=no\n");
        }
        if script.contains("ps -eo pid,ppid,comm,args") {
            let comm = &target.behavior().process.comm;
            let agent = if model.crashed {
                String::new()
            } else {
                format!(" {} 100 {comm} {comm}\n", model.agent_pid.max(4242))
            };
            return outcome(
                0,
                &format!(
                    "  PID  PPID COMMAND ARGS\n  100     1 jackin-start jackin-start\n{agent}"
                ),
            );
        }
        if script.starts_with("stat -c") {
            return outcome(0, "0 755\n");
        }
        if script.contains(": > /etc/e2e-denied") {
            return outcome(1, "");
        }
        if script.contains("sha256sum /usr/local/bin/jackin-sentinel-probe") {
            return outcome(0, &format!("{digest}  {PROBE}\n"));
        }
        if script.contains("cat /sandbox/e2e-keep.txt") {
            let lost = self.shared.knobs().file_lost;
            return if model.kept && !lost {
                outcome(0, "e2e\n")
            } else {
                outcome(1, "")
            };
        }
        if script.contains("e2e-keep.txt")
            && let Some(found) = state.sandboxes.get_mut(&sandbox)
        {
            found.kept = true;
        }
        if (script.contains("e2e-commit.txt") || script.contains("e2e-j14.txt"))
            && let Some(found) = state.sandboxes.get_mut(&sandbox)
        {
            found.committed = true;
        }
        if script.contains("docker rm -f") {
            state.containers.clear();
            return outcome(0, "");
        }
        if script.contains("codex sandbox") {
            return outcome(
                0,
                "HTTP/1.1 404 Not Found\r\ncontent-length: 9\r\n\r\nnot found",
            );
        }
        outcome(0, "")
    }
}

#[async_trait]
impl Gateway for ScenarioGateway {
    fn name(&self) -> &GatewayName {
        self.fake.name()
    }

    async fn health(&self) -> Result<GatewayHealth, GatewayError> {
        self.shared.state().health_calls += 1;
        self.fake.health().await
    }

    async fn ensure_profile(&self, profile: &ProviderProfile) -> Result<ProfileSync, GatewayError> {
        self.fake.ensure_profile(profile).await
    }

    async fn delete_profile(&self, id: &ProfileId) -> Result<(), GatewayError> {
        self.fake.delete_profile(id).await
    }

    async fn provider_state(&self, name: &ProviderName) -> Result<ProviderState, GatewayError> {
        self.fake.provider_state(name).await
    }

    async fn put_provider(&self, spec: &ProviderSpec) -> Result<(), GatewayError> {
        self.fake.put_provider(spec).await
    }

    async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError> {
        let stuck = self.shared.knobs().stuck_provider.clone();
        if stuck
            .as_deref()
            .is_some_and(|needle| name.as_str().contains(needle))
        {
            return Err(GatewayError::Cancelled {
                op: GatewayOp::Provider,
                group: None,
            });
        }
        self.fake.delete_provider(name).await
    }

    async fn create_sandbox(&self, spec: &SandboxCreate) -> Result<SandboxView, GatewayError> {
        self.fake.create_sandbox(spec).await
    }

    async fn find_sandboxes(&self, query: &SandboxQuery) -> Result<Vec<SandboxView>, GatewayError> {
        self.fake.find_sandboxes(query).await
    }

    async fn sandbox_detail(&self, name: &SandboxName) -> Result<SandboxDetail, GatewayError> {
        let mut detail = self.fake.sandbox_detail(name).await?;
        if let Some(model) = self.model_of(name) {
            let account = match self.shared.knobs().provider_account_override.clone() {
                Some((from, to)) if from == model.account => to,
                _ => model.account.clone(),
            };
            let account = jackin_core::AccountName::parse(&account).expect("account");
            let providers = vec![self.target().provider_name(&account)];
            detail.providers = providers;
        }
        Ok(detail)
    }

    async fn sandbox_environment(
        &self,
        name: &SandboxName,
    ) -> Result<std::collections::BTreeMap<jackin_core::EnvName, String>, GatewayError> {
        let knobs = self.shared.knobs().clone();
        if knobs.create_env_unreadable {
            return Err(GatewayError::Cancelled {
                op: GatewayOp::Detail,
                group: None,
            });
        }
        let model = self.model_of(name).ok_or_else(|| GatewayError::NotFound {
            what: Missing::Sandbox(name.clone()),
        })?;
        let mut env = std::collections::BTreeMap::new();
        let mut put = |key: &str, value: String| {
            env.insert(jackin_core::EnvName::parse(key).expect("name"), value);
        };
        if !knobs.create_env_drop_launch {
            put("JACKIN_LAUNCH_ID", model.launch.clone());
        }
        if let Some((key, value)) = knobs.create_env_extra {
            put(&key, value);
        }
        Ok(env)
    }

    async fn set_power(
        &self,
        name: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxView, GatewayError> {
        self.fake.set_power(name, action).await
    }

    async fn delete_sandbox(&self, name: &SandboxName) -> Result<(), GatewayError> {
        self.fake.delete_sandbox(name).await?;
        self.shared.state().sandboxes.remove(name.as_str());
        Ok(())
    }

    async fn run_program(
        &self,
        run: &ProgramRun,
        _cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        if self.fake.sandbox(&run.sandbox).is_none() {
            return Err(GatewayError::NotFound {
                what: Missing::Sandbox(run.sandbox.clone()),
            });
        }
        let line = format!("{} {}", run.program.as_str(), run.args.join(" "));
        let cancel_at = self.shared.knobs().cancel_at.clone();
        if cancel_at
            .as_deref()
            .is_some_and(|needle| line.contains(needle))
        {
            self.shared.cancel.cancel();
        }
        let raw = self.shared.knobs().probe_raw.clone();
        if let (PROBE, true, Some(raw)) = (run.program.as_str(), line.contains("J04"), raw) {
            return Ok(ProgramOutcome {
                exit_code: raw.exit,
                stdout: raw.stdout.into_bytes(),
                stderr: raw.stderr.into_bytes(),
            });
        }
        if run.program.as_str() == PROBE && self.shared.knobs().hang_probe && line.contains("J04") {
            tokio::time::sleep(run.timeout).await;
            return Err(GatewayError::Timeout {
                op: GatewayOp::Program,
                after: run.timeout,
                group: None,
            });
        }
        if let Some(message) = self.shared.knobs().panic_in_probe.clone() {
            assert!(run.program.as_str() != PROBE, "{message}");
        }
        match run.program.as_str() {
            PROBE => self.answer_probe(run),
            "/usr/bin/env" => Ok(outcome(0, &Self::env_text(&self.target()))),
            _ => Ok(self.answer_sh(run)),
        }
    }

    async fn read_files(
        &self,
        sandbox: &SandboxName,
        paths: &[SandboxPath],
    ) -> Result<Vec<Option<Vec<u8>>>, GatewayError> {
        let knobs = self.shared.knobs().clone();
        let target = self.target();
        let _ = sandbox;
        Ok(paths
            .iter()
            .map(|path| {
                let text = path.as_str();
                if text.contains("/env-") && !text.contains("env-kinds") {
                    Some(
                        knobs
                            .leak_in_env_file
                            .clone()
                            .map_or_else(|| "JACKIN_LAUNCH_ID=x\n".to_owned(), |leak| format!("LEAK={leak}\n"))
                            .into_bytes(),
                    )
                } else if text == target.behavior().credential.auth_file.path {
                    let body = match target.tuple().id {
                        "T01" => {
                            let token = knobs
                                .leak_in_auth_file
                                .clone()
                                .unwrap_or_else(|| "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN".to_owned());
                            serde_json::json!({"auth_mode":"chatgptAuthTokens","tokens":{"access_token":token,"refresh_token":"","id_token":knobs.auth_id_token.clone().unwrap_or_else(|| jackin_core::CODEX_PLACEHOLDER_ID_TOKEN.to_owned())}})
                        }
                        "T09" => serde_json::json!({"zai": {"type": "api_key", "key": "placeholder"}}),
                        "T14" => serde_json::json!({"ANTHROPIC_API_KEY": "placeholder"}),
                        _ => Value::Null,
                    };
                    Some(body.to_string().into_bytes())
                } else if let Some(config) = target
                    .behavior()
                    .credential
                    .config_files
                    .iter()
                    .find(|config| config.path == text)
                {
                    Some(config.required_text.join("\n").into_bytes())
                } else if text.ends_with("e2e-reflect.chain") {
                    let comm = &target.behavior().reflection.descendant_comm;
                    Some(if knobs.reflect_not_descendant {
                        " sh bash".to_owned()
                    } else {
                        format!(" sh {comm} jackin-start")
                    }.as_bytes().to_vec())
                } else if text.ends_with("e2e-reflect.out") {
                    Some(
                        knobs
                            .reflect_body
                            .clone()
                            .unwrap_or_else(|| "HTTP/1.1 404 Not Found\r\n\r\nnot found".to_owned())
                            .into_bytes(),
                    )
                } else {
                    Some(Self::env_text(&target).into_bytes())
                }
            })
            .collect())
    }

    async fn write_file(
        &self,
        sandbox: &SandboxName,
        file: &SandboxFile,
    ) -> Result<(), GatewayError> {
        if file.path.as_str().ends_with("/e2e/probe-spec.json") {
            self.shared
                .state()
                .probe_specs
                .push(file.content.expose().to_vec());
        }
        self.fake.write_file(sandbox, file).await
    }

    async fn upload_tree(
        &self,
        sandbox: &SandboxName,
        source: &ProjectRoot,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        self.fake.upload_tree(sandbox, source, dest, cancel).await
    }

    async fn upload_file(
        &self,
        sandbox: &SandboxName,
        source: &HostPath,
        dest: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        self.fake.upload_file(sandbox, source, dest, cancel).await
    }

    async fn download_file(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<Sha256, GatewayError> {
        self.fake
            .download_file(sandbox, remote, local, cancel)
            .await
    }

    async fn open_terminal(
        &self,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        self.fake.open_terminal(request).await
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use jackin_core::plain;
use jsonschema::Validator;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use super::redact::Redactor;
use super::target::TargetIdentity;
use super::verdict::STEP_IDS;

pub const SCHEMA_TEXT: &str = include_str!("../../../../rewrite/probe-schema.json");
const MESSAGE_MAX: usize = 160;

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("the probe exited with code {code}: {}", plain(.tail))]
    Exit { code: i32, tail: String },
    #[error("the probe was killed by signal {0}")]
    Signal(i32),
    #[error("the probe output is not exactly one JSON value: {}", plain(.0))]
    NotJson(String),
    #[error("the probe output breaks probe-schema.json: {}", plain(.0))]
    Schema(String),
    #[error("the probe report does not match the request: {}", plain(.0))]
    Coverage(String),
    #[error("the embedded probe schema does not compile: {}", plain(.0))]
    Embedded(String),
    #[error("the facts of {} do not parse: {}", plain(.step), plain(.message))]
    Facts { step: String, message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeMode {
    Sandbox,
    Host,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineExpectation {
    Absent,
    Present(String),
}

#[derive(Clone, Debug)]
pub struct ProbeRequest {
    pub target: TargetIdentity,
    pub only: Vec<String>,
    pub docker: bool,
    pub mode: ProbeMode,
    pub engine: EngineExpectation,
}

#[derive(Clone, Debug)]
pub(crate) struct ProbeInvocation {
    pub(crate) argv: Vec<String>,
    pub(crate) request: ProbeRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProbeResult {
    Pass,
    Fail,
    Skip,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProbeCheck {
    pub name: String,
    pub result: ProbeResult,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProbeStep {
    pub result: ProbeResult,
    pub reason: String,
    pub checks: Vec<ProbeCheck>,
    pub facts: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProbeReport {
    pub target: TargetIdentity,
    pub mode: String,
    pub docker: String,
    pub engine: Option<String>,
    pub steps: BTreeMap<String, ProbeStep>,
}

fn validator() -> Result<&'static Validator, ProbeError> {
    static VALIDATOR: OnceLock<Result<Validator, String>> = OnceLock::new();
    VALIDATOR
        .get_or_init(|| {
            let schema: Value = serde_json::from_str(SCHEMA_TEXT).map_err(|e| e.to_string())?;
            jsonschema::draft202012::new(&schema).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|message| ProbeError::Embedded(message.clone()))
}

fn clip(redactor: &Redactor, text: &str) -> String {
    redactor.scrub(text).chars().take(MESSAGE_MAX).collect()
}

fn deeper(instance: &Value, path: &str, redactor: &Redactor) -> Vec<String> {
    let Some(step) = path
        .strip_prefix("/steps/")
        .and_then(|rest| rest.strip_suffix("/facts"))
    else {
        return Vec::new();
    };
    let Some(facts) = instance.pointer(path).filter(|facts| !facts.is_null()) else {
        return Vec::new();
    };
    let Ok(schema) = serde_json::from_str::<Value>(SCHEMA_TEXT) else {
        return Vec::new();
    };
    let sub = serde_json::json!({
        "$ref": format!("#/$defs/Facts{step}"),
        "$defs": schema.get("$defs").cloned().unwrap_or(Value::Null),
    });
    let Ok(sub_validator) = jsonschema::draft202012::new(&sub) else {
        return Vec::new();
    };
    sub_validator
        .iter_errors(facts)
        .map(|error| {
            format!(
                "{path}{}: {}",
                error.instance_path(),
                clip(redactor, &error.to_string())
            )
        })
        .collect()
}

pub(crate) fn validate(instance: &Value, redactor: &Redactor) -> Result<(), ProbeError> {
    let mut problems: Vec<String> = Vec::new();
    for error in validator()?.iter_errors(instance) {
        let path = error.instance_path().to_string();
        let line = format!("{path}: {}", clip(redactor, &error.to_string()));
        for extra in std::iter::once(line).chain(deeper(instance, &path, redactor)) {
            if !problems.contains(&extra) {
                problems.push(extra);
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ProbeError::Schema(problems.join("; ")))
    }
}

impl ProbeReport {
    pub fn parse(
        stdout: &[u8],
        request: &ProbeRequest,
        redactor: &Redactor,
    ) -> Result<ProbeReport, ProbeError> {
        let value: Value = serde_json::from_slice(stdout)
            .map_err(|error| ProbeError::NotJson(redactor.scrub(&error.to_string())))?;
        validate(&value, redactor)?;
        let report: ProbeReport = serde_json::from_value(value)
            .map_err(|error| ProbeError::Schema(redactor.scrub(&error.to_string())))?;
        report.check_coverage(request)?;
        Ok(report)
    }

    fn check_coverage(&self, request: &ProbeRequest) -> Result<(), ProbeError> {
        let expected = &request.target;
        for (field, found, wanted) in [
            ("tuple", &self.target.tuple, &expected.tuple),
            ("agent", &self.target.agent, &expected.agent),
            ("provider", &self.target.provider, &expected.provider),
            ("model", &self.target.model, &expected.model),
            ("adapter", &self.target.adapter, &expected.adapter),
            (
                "spec_sha256",
                &self.target.spec_sha256,
                &expected.spec_sha256,
            ),
        ] {
            if found != wanted {
                return Err(ProbeError::Coverage(format!(
                    "target {field} is {found:?} but {wanted:?} was expected"
                )));
            }
        }
        let engine = match &request.engine {
            EngineExpectation::Absent => None,
            EngineExpectation::Present(endpoint) => Some(endpoint.as_str()),
        };
        if self.engine.as_deref() != engine {
            return Err(ProbeError::Coverage(format!(
                "engine is {:?} but {engine:?} was expected",
                self.engine
            )));
        }
        let mode = match request.mode {
            ProbeMode::Sandbox => "sandbox",
            ProbeMode::Host => "host",
        };
        if self.mode != mode {
            return Err(ProbeError::Coverage(format!(
                "mode is {} but {mode} was expected",
                self.mode
            )));
        }
        let docker = if request.docker { "on" } else { "off" };
        if self.docker != docker {
            return Err(ProbeError::Coverage(format!(
                "docker is {} but {docker} was passed",
                self.docker
            )));
        }
        let wanted: BTreeSet<&str> = request.only.iter().map(String::as_str).collect();
        let got: BTreeSet<&str> = self.steps.keys().map(String::as_str).collect();
        if wanted != got {
            let missing: Vec<&&str> = wanted.difference(&got).collect();
            let extra: Vec<&&str> = got.difference(&wanted).collect();
            return Err(ProbeError::Coverage(format!(
                "missing steps {missing:?}, extra steps {extra:?}"
            )));
        }
        Ok(())
    }

    pub fn facts<T: DeserializeOwned>(&self, step: &str) -> Result<T, ProbeError> {
        let value = self
            .steps
            .get(step)
            .and_then(|found| found.facts.clone())
            .ok_or_else(|| ProbeError::Facts {
                step: step.to_owned(),
                message: "the step has no facts".to_owned(),
            })?;
        serde_json::from_value(value).map_err(|error| ProbeError::Facts {
            step: step.to_owned(),
            message: error.to_string().chars().take(MESSAGE_MAX).collect(),
        })
    }

    #[must_use]
    pub fn gate(&self, step: &str) -> Option<String> {
        let found = self.steps.get(step)?;
        let mut problems = Vec::new();
        for check in &found.checks {
            match check.result {
                ProbeResult::Pass => {}
                ProbeResult::Fail => {
                    problems.push(format!("probe check {} failed: {}", check.name, check.detail));
                }
                ProbeResult::Skip if self.allowed_skip(step, &check.name) => {}
                ProbeResult::Skip => problems.push(format!(
                    "probe skipped {}: {}, but this run holds the credential and capability it needs",
                    check.name, check.detail
                )),
            }
        }
        if matches!(step, "J07" | "J09")
            && let Some(exec) = found
                .facts
                .as_ref()
                .and_then(|facts| facts.get("agent_exec"))
        {
            if exec.get("attempted").and_then(Value::as_bool) != Some(true) {
                problems.push(format!("{step} agent_exec was not attempted"));
            }
            if exec.get("credential").and_then(Value::as_str) == Some("none") {
                problems.push(format!(
                    "{step} agent_exec has credential classification none"
                ));
            }
            if exec.get("answered").and_then(Value::as_bool) != Some(true) {
                problems.push(format!("{step} agent_exec was not answered"));
            }
            if exec.get("actual_provider").and_then(Value::as_str)
                != Some(self.target.provider.as_str())
                || exec.get("actual_model").and_then(Value::as_str)
                    != Some(self.target.model.as_str())
            {
                problems.push(format!("{step} agent_exec provider or model differs"));
            }
            if exec.pointer("/tool/attempted").and_then(Value::as_bool) != Some(true)
                || exec.pointer("/tool/succeeded").and_then(Value::as_bool) != Some(true)
                || exec.pointer("/tool/exit_code").and_then(Value::as_i64) != Some(0)
            {
                problems.push(format!("{step} agent tool execution did not succeed"));
            }
        }
        (!problems.is_empty()).then(|| problems.join("; "))
    }

    fn allowed_skip(&self, step: &str, check: &str) -> bool {
        if step != "J08" || check != "environment" {
            return false;
        }
        let state = self
            .steps
            .get(step)
            .and_then(|found| found.facts.as_ref())
            .and_then(|facts| facts.pointer("/start_record/state"))
            .and_then(Value::as_str);
        matches!(state, Some("absent" | "unreadable" | "stale"))
    }
}

#[must_use]
pub fn schema_step_drift(schema: &Value) -> Option<String> {
    let known: BTreeSet<String> = schema
        .pointer("/properties/steps/properties")
        .and_then(Value::as_object)
        .map(|steps| steps.keys().cloned().collect())
        .unwrap_or_default();
    let runner: BTreeSet<String> = STEP_IDS.iter().map(|id| (*id).to_owned()).collect();
    (known != runner).then(|| {
        format!(
            "the schema has {:?} but the runner has {:?}",
            known.difference(&runner).collect::<Vec<_>>(),
            runner.difference(&known).collect::<Vec<_>>()
        )
    })
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AgentBinary {
    pub(crate) path: String,
    pub(crate) present: bool,
    pub(crate) executable: bool,
    pub(crate) version: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ00 {
    pub(crate) agent: AgentBinary,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Sudo {
    pub(crate) works: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ04 {
    pub(crate) user: String,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) sudo: Sudo,
    pub(crate) docker_binary: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FileDigest {
    pub(crate) path: String,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ05 {
    pub(crate) files: Vec<FileDigest>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct HookFile {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) uid: u32,
    pub(crate) mode: String,
    pub(crate) writable: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ06 {
    pub(crate) setup_runs: u32,
    pub(crate) preflight_runs: u32,
    pub(crate) preflight_failures: u32,
    pub(crate) hooks: Vec<HookFile>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ProcessInfo {
    pub(crate) comm: String,
    pub(crate) pid: u32,
    pub(crate) ppid: u32,
    pub(crate) start_ticks: u64,
    pub(crate) tty: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Processes {
    pub(crate) main: Option<ProcessInfo>,
    pub(crate) agent: Vec<ProcessInfo>,
    #[serde(rename = "self")]
    pub(crate) probe: ProcessInfo,
    pub(crate) self_descends_from_main: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ToolExecution {
    pub(crate) kind: String,
    pub(crate) attempted: bool,
    pub(crate) succeeded: bool,
    pub(crate) exit_code: Option<i64>,
    pub(crate) workdir: String,
    pub(crate) observation: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AgentExec {
    pub(crate) attempted: bool,
    pub(crate) credential: String,
    pub(crate) answered: bool,
    pub(crate) actual_provider: String,
    pub(crate) actual_model: String,
    pub(crate) tool: ToolExecution,
}

impl AgentExec {
    pub(crate) fn problems(
        &self,
        identity: &TargetIdentity,
        behavior: &super::adapter::CompleteBehaviorSpec,
    ) -> Vec<String> {
        let mut problems = Vec::new();
        if !self.attempted || self.credential != behavior.credential.classification.as_str() {
            problems.push("agent exec was not attempted with the selected credential".to_owned());
        }
        if !self.answered {
            problems.push("agent exec did not answer the fixed prompt".to_owned());
        }
        if self.actual_provider != identity.provider || self.actual_model != identity.model {
            problems.push(format!(
                "agent exec used provider {} and model {}, expected {} and {}",
                self.actual_provider, self.actual_model, identity.provider, identity.model
            ));
        }
        let expected = &behavior.prompt.tool;
        if !self.tool.attempted || !self.tool.succeeded || self.tool.exit_code != Some(0) {
            problems.push("agent tool execution was not successful".to_owned());
        }
        if self.tool.kind != expected.kind {
            problems.push(format!(
                "agent tool kind {} does not match {}",
                self.tool.kind, expected.kind
            ));
        }
        if self.tool.workdir != expected.workdir {
            problems.push(format!(
                "agent tool workdir {} does not match {}",
                self.tool.workdir, expected.workdir
            ));
        }
        match &expected.observation {
            super::adapter::ToolObservation::DirectoryContains { entries } => {
                for entry in entries {
                    if !self.tool.observation.contains(entry) {
                        problems.push(format!("agent tool observation lacks {entry}"));
                    }
                }
            }
        }
        problems
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ07 {
    pub(crate) processes: Processes,
    pub(crate) agent_exec: AgentExec,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct EnvVar {
    pub(crate) name: String,
    pub(crate) kind: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct StartRecord {
    pub(crate) state: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AuthField {
    pub(crate) name: String,
    pub(crate) kind: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AuthFile {
    pub(crate) state: String,
    pub(crate) path: String,
    pub(crate) mode: Option<String>,
    pub(crate) fields: Vec<AuthField>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ08 {
    pub(crate) environment: Vec<EnvVar>,
    pub(crate) start_record: StartRecord,
    pub(crate) auth_file: AuthFile,
    pub(crate) leaks: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Egress {
    pub(crate) url: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) curl_found: bool,
    pub(crate) curl_exit: Option<i64>,
    pub(crate) http_code: u32,
}

pub(crate) const CURL_COULD_NOT_RESOLVE: i64 = 6;
const CURL_COULD_NOT_CONNECT: i64 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Refused,
    Unresolved,
    CurlMissing,
    Answered(u32),
    Failed(Option<i64>),
}

impl Egress {
    pub(crate) fn outcome(&self) -> Outcome {
        if !self.curl_found {
            return Outcome::CurlMissing;
        }
        if self.http_code != 0 {
            return Outcome::Answered(self.http_code);
        }
        match self.curl_exit {
            Some(CURL_COULD_NOT_CONNECT) => Outcome::Refused,
            Some(CURL_COULD_NOT_RESOLVE) => Outcome::Unresolved,
            other => Outcome::Failed(other),
        }
    }
}

impl Outcome {
    pub(crate) fn fault(self, subject: &str) -> Option<String> {
        match self {
            Self::Refused => None,
            Self::CurlMissing => Some(format!(
                "curl is missing in the sandbox, so the {subject} was not tested"
            )),
            Self::Unresolved => Some(format!(
                "the {subject} did not resolve (curl exit {CURL_COULD_NOT_RESOLVE}): a resolver fault, not a policy denial"
            )),
            Self::Answered(code) => Some(format!("the {subject} answered HTTP {code}")),
            Self::Failed(exit) => Some(format!(
                "the {subject} gave curl exit {} and no HTTP response: not a policy denial",
                exit.map_or_else(|| "none".to_owned(), |code| code.to_string())
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ09 {
    pub(crate) outside_host: Egress,
    pub(crate) metadata_host: Egress,
    pub(crate) provider_host: Option<Egress>,
    pub(crate) agent_exec: AgentExec,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WriteProbe {
    pub(crate) path: String,
    pub(crate) expected: String,
    pub(crate) dac_writable: bool,
    pub(crate) created: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ10 {
    pub(crate) writes: Vec<WriteProbe>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsProcesses {
    pub(crate) processes: Processes,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ13 {
    pub(crate) branch: Option<String>,
    pub(crate) head: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Marker {
    pub(crate) id: String,
    pub(crate) created: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ14 {
    pub(crate) boot_id: String,
    pub(crate) marker: Marker,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DockerBinary {
    pub(crate) found: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DockerEnvironment {
    pub(crate) docker_host: Option<String>,
    pub(crate) tls_verify: Option<String>,
    pub(crate) cert_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Certificate {
    pub(crate) name: String,
    pub(crate) mode: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DockerRun {
    pub(crate) exit_code: Option<i64>,
    pub(crate) hello: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct EngineId {
    pub(crate) id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ15 {
    pub(crate) docker_binary: DockerBinary,
    pub(crate) environment: DockerEnvironment,
    pub(crate) certificates: Vec<Certificate>,
    pub(crate) run: DockerRun,
    pub(crate) engine: EngineId,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Connection {
    pub(crate) attempted: bool,
    #[serde(flatten)]
    pub(crate) reply: Egress,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FactsJ16 {
    pub(crate) docker_binary: bool,
    pub(crate) docker_variables: Vec<String>,
    pub(crate) connection: Connection,
    pub(crate) resolver_control: Option<Egress>,
}

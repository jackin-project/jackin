// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::digest::sha256_hex;
use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};

pub const SCHEMA_MODULE_NAME: &str = "Workflow.pkl";

pub const PKLR_VERSION: &str = "5.1.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalLimits {
    pub deadline: Duration,

    pub memory_bytes: u64,

    pub max_stdout_bytes: usize,

    pub max_stderr_bytes: usize,
}

impl EvalLimits {
    #[must_use]
    pub const fn defaults() -> Self {
        Self {
            deadline: Duration::from_secs(30),
            memory_bytes: 256 * 1024 * 1024,
            max_stdout_bytes: 4 * 1024 * 1024,
            max_stderr_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenModule {
    pub name: String,

    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenBundle {
    pub entry: String,

    pub modules: Vec<FrozenModule>,
}

impl FrozenBundle {
    pub fn from_texts(
        entry: impl Into<String>,
        modules: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> WorkflowResult<Self> {
        let entry = entry.into();
        let collected: Vec<FrozenModule> = modules
            .into_iter()
            .map(|(name, text)| FrozenModule {
                name: name.into(),
                bytes: text.into().into_bytes(),
            })
            .collect();
        if !collected.iter().any(|module| module.name == entry) {
            return Err(WorkflowError::single(Diagnostic::new(
                &entry,
                "",
                ErrorCode::MissingInput,
                "entry module is not in the frozen bundle",
                "include the workflow file in the frozen bundle",
            )));
        }
        Ok(Self {
            entry,
            modules: collected,
        })
    }

    #[must_use]
    pub fn digest(&self) -> String {
        let mut ordered: Vec<&FrozenModule> = self.modules.iter().collect();
        ordered.sort_by(|left, right| left.name.cmp(&right.name));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.entry.as_bytes());
        bytes.push(0);
        for module in ordered {
            bytes.extend_from_slice(module.name.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(&module.bytes);
            bytes.push(0);
        }
        sha256_hex(&bytes)
    }

    #[must_use]
    pub fn module_digests(&self) -> BTreeMap<String, String> {
        self.modules
            .iter()
            .map(|module| (module.name.clone(), sha256_hex(&module.bytes)))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedDefinition {
    pub json: serde_json::Value,

    pub module_digests: BTreeMap<String, String>,

    pub env_reads: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperRequest {
    pub entry: String,

    pub modules: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperResponse {
    pub ok: bool,

    pub value: Option<serde_json::Value>,

    pub env_reads: Option<BTreeMap<String, Option<String>>>,

    pub error: Option<String>,
}

#[must_use]
pub fn resolve_helper_binary() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let dir = current.parent()?;
    let candidate = dir.join("jackin-workflow-eval");
    candidate.is_file().then_some(candidate)
}

pub fn evaluate_bounded(
    bundle: &FrozenBundle,
    file_label: &str,
    helper: &Path,
    limits: &EvalLimits,
) -> WorkflowResult<EvaluatedDefinition> {
    let modules: BTreeMap<String, String> = bundle
        .modules
        .iter()
        .map(|module| {
            String::from_utf8(module.bytes.clone()).map(|text| (module.name.clone(), text))
        })
        .collect::<Result<BTreeMap<String, String>, std::string::FromUtf8Error>>()
        .map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                file_label,
                "",
                ErrorCode::InvalidDefinition,
                "frozen module bytes are not UTF-8",
                "save the workflow and schema files as UTF-8 and retry",
            ))
        })?;
    let request = HelperRequest {
        entry: bundle.entry.clone(),
        modules,
    };
    let stdin = serde_json::to_vec(&request).map_err(|error| {
        WorkflowError::single(Diagnostic::new(
            file_label,
            "",
            ErrorCode::EvaluationLimit,
            format!("cannot encode the frozen bundle: {error}"),
            "retry with smaller workflow files",
        ))
    })?;
    let mut spawn = jackin_process::ExecRequest::new(helper, [limits.memory_bytes.to_string()]);
    spawn.stdin = Some(stdin);
    spawn = spawn
        .timeout(limits.deadline)
        .env_clear()
        .output_limits(limits.max_stdout_bytes, limits.max_stderr_bytes);
    let outcome = match jackin_process::exec_sync(&spawn) {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(delivery) = error.downcast_ref::<jackin_process::StdinDeliveryError>() {
                let child = delivery.result();
                let delivery_detail = match delivery.operation() {
                    jackin_process::StdinDeliveryOperation::Write => "writing the request",
                    jackin_process::StdinDeliveryOperation::Shutdown => "closing request stdin",
                };
                return Err(helper_death_error(
                    file_label,
                    child.code,
                    &child.stderr,
                    Some(delivery_detail),
                    child.success,
                ));
            }
            let missing_helper = error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
            });
            let action = if missing_helper {
                "reinstall jackin so jackin-workflow-eval sits next to the CLI"
            } else {
                "inspect the helper transport error and verify the helper can run"
            };
            return Err(WorkflowError::single(Diagnostic::new(
                file_label,
                "",
                ErrorCode::EvaluationLimit,
                format!("cannot run the evaluation helper: {error}"),
                action,
            )));
        }
    };
    if outcome.timed_out {
        return Err(WorkflowError::single(Diagnostic::new(
            file_label,
            "",
            ErrorCode::EvaluationLimit,
            format!(
                "evaluation exceeded its deadline of {}s",
                limits.deadline.as_secs()
            ),
            "simplify the workflow definition and retry",
        )));
    }
    if !outcome.success {
        return Err(helper_death_error(
            file_label,
            outcome.code,
            &outcome.stderr,
            None,
            false,
        ));
    }
    let response: HelperResponse = serde_json::from_slice(&outcome.stdout).map_err(|error| {
        WorkflowError::single(Diagnostic::new(
            file_label,
            "",
            ErrorCode::EvaluationLimit,
            format!("evaluation helper returned malformed output: {error}"),
            "reinstall jackin and retry",
        ))
    })?;
    if !response.ok {
        let detail = response.error.unwrap_or_else(|| "unknown error".to_owned());
        return Err(WorkflowError::single(Diagnostic::new(
            file_label,
            "",
            ErrorCode::InvalidDefinition,
            format!("workflow definition failed to evaluate: {detail}"),
            "fix the reported Pkl error and retry",
        )));
    }
    let value = response.value.unwrap_or(serde_json::Value::Null);
    Ok(EvaluatedDefinition {
        json: value,
        module_digests: bundle.module_digests(),
        env_reads: response.env_reads.unwrap_or_default(),
    })
}

fn helper_death_error(
    file_label: &str,
    code: Option<i32>,
    stderr: &[u8],
    delivery_detail: Option<&str>,
    child_success: bool,
) -> WorkflowError {
    let stderr = stderr_excerpt(stderr);
    let delivery = delivery_detail.map_or_else(String::new, |detail| {
        format!("; stdin delivery failed while {detail}")
    });
    let message = if child_success {
        format!(
            "evaluation helper exited successfully before request delivery completed (code={code:?}{delivery}): {stderr}"
        )
    } else {
        format!("evaluation helper died (code={code:?}{delivery}): {stderr}")
    };
    let action = if stderr.contains("cannot set the memory cap") {
        "check the configured evaluation memory cap and retry"
    } else {
        "inspect the helper's reported cause; resource limits may cause helper failure"
    };
    WorkflowError::single(Diagnostic::new(
        file_label,
        "",
        ErrorCode::EvaluationLimit,
        message,
        action,
    ))
}

fn stderr_excerpt(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.chars().take(400).collect()
}

#[derive(Debug)]
pub struct FrozenCapabilities {
    modules: BTreeMap<PathBuf, Vec<u8>>,
}

impl FrozenCapabilities {
    #[must_use]
    pub fn new(modules: BTreeMap<String, String>) -> Self {
        Self {
            modules: modules
                .into_iter()
                .map(|(name, text)| (PathBuf::from(name), text.into_bytes()))
                .collect(),
        }
    }
}

fn denied(what: &str) -> pklr::Error {
    pklr::Error::Eval(format!("workflow-spec: denied {what}"))
}

impl pklr::EvalCapabilities for FrozenCapabilities {
    fn read_to_string(&mut self, path: &Path) -> pklr::Result<String> {
        match self.modules.get(path) {
            Some(bytes) => {
                String::from_utf8(bytes.clone()).map_err(|_| denied("non-UTF-8 frozen module"))
            }
            None => Err(denied(&format!("file read {}", path.display()))),
        }
    }

    fn path_exists(&mut self, path: &Path) -> pklr::Result<bool> {
        Ok(self.modules.contains_key(path))
    }

    fn canonicalize(&mut self, path: &Path) -> pklr::Result<PathBuf> {
        if self.modules.contains_key(path) {
            Ok(path.to_path_buf())
        } else {
            Err(denied(&format!("path {}", path.display())))
        }
    }

    fn read_bytes(&mut self, path: &Path) -> pklr::Result<Vec<u8>> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| denied(&format!("file read {}", path.display())))
    }

    fn create_dir_all(&mut self, path: &Path) -> pklr::Result<()> {
        Err(denied(&format!("directory creation {}", path.display())))
    }

    fn write_atomic(&mut self, path: &Path, _bytes: &[u8]) -> pklr::Result<()> {
        Err(denied(&format!("file write {}", path.display())))
    }

    fn remove_file(&mut self, path: &Path) -> pklr::Result<()> {
        Err(denied(&format!("file removal {}", path.display())))
    }

    fn read_env(&mut self, name: &str) -> pklr::Result<Option<String>> {
        let _ = name;
        Ok(None)
    }

    fn env_vars(&mut self) -> pklr::Result<Vec<(String, String)>> {
        Ok(Vec::new())
    }

    fn fetch_text(&mut self, url: &str) -> pklr::Result<String> {
        Err(denied(&format!("remote import {url}")))
    }

    fn fetch_bytes(&mut self, url: &str) -> pklr::Result<Vec<u8>> {
        Err(denied(&format!("remote import {url}")))
    }

    fn temp_dir(&mut self, prefix: &str) -> pklr::Result<PathBuf> {
        Err(denied(&format!("temporary directory {prefix}")))
    }

    fn glob(&mut self, base: &Path, pattern: &str) -> pklr::Result<Vec<PathBuf>> {
        let _ = (base, pattern);
        Err(denied("glob"))
    }
}

pub fn evaluate_request(request: &HelperRequest) -> HelperResponse {
    let capabilities = FrozenCapabilities::new(request.modules.clone());
    let mut evaluator = pklr::Evaluator::with_capabilities(capabilities);
    evaluator.set_allowed_resources(Vec::new());
    evaluator.set_offline(true);
    match evaluator.eval_file(Path::new(&request.entry)) {
        Ok(value) => {
            let env_reads = evaluator.env_reads().clone();
            HelperResponse {
                ok: true,
                value: Some(value.to_json()),
                env_reads: Some(env_reads),
                error: None,
            }
        }
        Err(error) => HelperResponse {
            ok: false,
            value: None,
            env_reads: None,
            error: Some(error.to_string()),
        },
    }
}

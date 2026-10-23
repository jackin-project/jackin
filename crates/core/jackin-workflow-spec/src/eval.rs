// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Frozen Pkl evaluation inside a killable helper process.
//!
//! The caller freezes the versioned schema module plus one workflow
//! module to bytes ([`FrozenBundle`]). Evaluation runs in the
//! `jackin-workflow-eval` helper spawned through `jackin-process` with
//! a deadline, a cleared environment, and bounded captured output. The
//! helper caps its own heap with a counting allocator (see the helper
//! binary) and evaluates with `pklr` capabilities that serve only the
//! supplied modules: ambient file reads, environment reads, remote
//! imports, temp dirs, and globs are denied by the capability
//! implementation and by an empty resource allowlist, never by a text
//! scan. A deadline breach or a helper death maps to
//! `evaluation_limit`; a Pkl-level failure maps to
//! `invalid_definition`. No Tokio timeout wraps the evaluation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::digest::sha256_hex;
use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};

/// Name the workflow schema module must have inside a bundle.
pub const SCHEMA_MODULE_NAME: &str = "Workflow.pkl";

/// pklr release this crate was verified against (crates.io, 2026-10-07).
pub const PKLR_VERSION: &str = "5.1.1";

/// Bounds for one evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalLimits {
    /// Kill the helper after this long and report `evaluation_limit`.
    pub deadline: Duration,
    /// Heap cap enforced inside the helper, in bytes.
    pub memory_bytes: u64,
    /// Maximum captured helper stdout, in bytes.
    pub max_stdout_bytes: usize,
    /// Maximum captured helper stderr, in bytes.
    pub max_stderr_bytes: usize,
}

impl EvalLimits {
    /// Production bounds: 30s deadline, 256MiB heap, 4MiB/256KiB output.
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

/// One frozen module: virtual name plus exact bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenModule {
    /// Virtual path inside the bundle (for example `/bundle/repair.pkl`).
    pub name: String,
    /// Frozen source bytes.
    pub bytes: Vec<u8>,
}

/// Supplied local modules frozen before evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenBundle {
    /// Virtual path of the entry module.
    pub entry: String,
    /// Frozen modules, including the entry and the versioned schema.
    pub modules: Vec<FrozenModule>,
}

impl FrozenBundle {
    /// Freeze the schema and workflow sources from text. The entry must
    /// be one of the modules; UTF-8 is required for Pkl sources.
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

    /// Hex SHA-256 over the entry name plus every module name and byte,
    /// in sorted name order.
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

    /// Hex digests per module, keyed by virtual name.
    #[must_use]
    pub fn module_digests(&self) -> BTreeMap<String, String> {
        self.modules
            .iter()
            .map(|module| (module.name.clone(), sha256_hex(&module.bytes)))
            .collect()
    }
}

/// Successful bounded evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedDefinition {
    /// Evaluated workflow document.
    pub json: serde_json::Value,
    /// Hex digests of the frozen modules that produced it.
    pub module_digests: BTreeMap<String, String>,
    /// Environment reads observed during evaluation (always empty: the
    /// allowlist denies them before the capability runs).
    pub env_reads: BTreeMap<String, Option<String>>,
}

/// Request bytes sent to the helper's stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperRequest {
    /// Virtual path of the entry module.
    pub entry: String,
    /// Virtual name to source text.
    pub modules: BTreeMap<String, String>,
}

/// Response JSON printed by the helper on stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperResponse {
    /// Whether evaluation completed.
    pub ok: bool,
    /// Evaluated document when `ok` is true.
    pub value: Option<serde_json::Value>,
    /// Environment reads observed (always empty).
    pub env_reads: Option<BTreeMap<String, Option<String>>>,
    /// Pkl failure message when `ok` is false.
    pub error: Option<String>,
}

/// Locate the helper next to the current executable. Returns `None`
/// when it is not installed there.
#[must_use]
pub fn resolve_helper_binary() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let dir = current.parent()?;
    let candidate = dir.join("jackin-workflow-eval");
    candidate.is_file().then_some(candidate)
}

/// Evaluate a frozen bundle with bounds. Synchronous: the helper is a
/// real OS process reaped by `jackin-process`; no blocking thread is
/// left behind on a deadline breach.
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

/// Capabilities that serve only the supplied frozen modules. Every
/// other access is denied here, and resource reads additionally fail
/// the evaluator's empty allowlist before reaching this code.
#[derive(Debug)]
pub struct FrozenCapabilities {
    modules: BTreeMap<PathBuf, Vec<u8>>,
}

impl FrozenCapabilities {
    /// Serve exactly these virtual-name to source mappings.
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

/// Evaluate one helper request. Shared by the helper binary so its
/// protocol stays in one place; the sandbox (capabilities, allowlist,
/// offline mode) is applied here.
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

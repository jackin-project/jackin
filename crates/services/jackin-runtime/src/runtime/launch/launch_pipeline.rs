// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::instance::{InstanceManifest, InstanceStatus, RoleState};
use jackin_config::AppConfig;
use jackin_config::DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_core::{CommandRunner, ContainerHandle, WorkspaceName};
use jackin_docker::docker_client::DockerApi;
use tracing::Instrument as _;

use super::launch_slot::{claim_container_name, claim_known_container_name};
use crate::runtime::attach::{
    ContainerState, start_or_hardline_agent, start_or_hardline_agent_with_container_handle,
};
use crate::runtime::naming::{image_name, image_name_for_branch};
use crate::runtime::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

mod account_admission;
mod environment_admission;

mod launch_core;
pub(super) mod launch_phases;
#[cfg(test)]
mod tests;

pub(super) struct DeferredGitPull {
    handle: tokio::task::JoinHandle<Vec<super::git_pull::GitPullResult>>,
    print_results: bool,
}

enum DeferredOperatorEnv<'a> {
    Spawned(tokio::task::JoinHandle<anyhow::Result<std::collections::BTreeMap<String, String>>>),
    Inline(InlineOperatorEnv<'a>),
    Ready(std::collections::BTreeMap<String, String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelatedRestoreExpectation {
    container_base: String,
    role_key: String,
    agent_runtime: String,
    role_source_git: String,
    role_source_ref: Option<String>,
    role_git_sha: Option<String>,
    fingerprint: String,
    admitted: Vec<(String, jackin_core::Agent, String)>,
}

impl RelatedRestoreExpectation {
    fn from_admission(admission: &super::account_identity::AdmittedAccountConfiguration) -> Self {
        let manifest = &admission.manifest;
        Self {
            container_base: manifest.container_base.clone(),
            role_key: manifest.role_key.clone(),
            agent_runtime: manifest.agent_runtime.clone(),
            role_source_git: manifest.role_source_git.clone(),
            role_source_ref: manifest.role_source_ref.clone(),
            role_git_sha: manifest.role_git_sha.clone(),
            fingerprint: admission.fingerprint.clone(),
            admitted: manifest
                .admitted_instances
                .iter()
                .map(|instance| {
                    (
                        instance.config_id.clone(),
                        instance.agent,
                        instance.account_id.clone(),
                    )
                })
                .collect(),
        }
    }

    fn verify(
        &self,
        admission: &super::account_identity::AdmittedAccountConfiguration,
    ) -> anyhow::Result<()> {
        let manifest = &admission.manifest;
        let admitted = manifest
            .admitted_instances
            .iter()
            .map(|instance| {
                (
                    instance.config_id.clone(),
                    instance.agent,
                    instance.account_id.clone(),
                )
            })
            .collect::<Vec<_>>();
        anyhow::ensure!(
            manifest.container_base == self.container_base
                && manifest.role_key == self.role_key
                && manifest.agent_runtime == self.agent_runtime
                && manifest.role_source_git == self.role_source_git
                && manifest.role_source_ref == self.role_source_ref
                && manifest.role_git_sha == self.role_git_sha
                && admission.fingerprint == self.fingerprint
                && admitted == self.admitted,
            "related restore admission changed before recursive launch"
        );
        Ok(())
    }
}

struct RestoreLaunchAdmission<'a> {
    opts: &'a super::LoadOptions,
    related_expectation: Option<RelatedRestoreExpectation>,
}

struct LaunchAdmissionRequest<'a, Trust, Branch> {
    opts: &'a super::LoadOptions,
    confirm_trust_for_test: Trust,
    confirm_branch_for_test: Branch,
    related_expectation: Option<RelatedRestoreExpectation>,
}

struct InlineOperatorEnv<'a> {
    config: &'a AppConfig,
    selector_key: String,
    workspace_key: Option<String>,
    op_runner: &'a dyn jackin_env::OpRunner,
    host_env: Option<&'a std::collections::BTreeMap<String, String>>,
}

pub(super) async fn finish_deferred_git_pull(
    deferred: DeferredGitPull,
    steps: &mut super::StepCounter,
) -> anyhow::Result<()> {
    let results = if let Some(progress) = steps.progress_mut() {
        progress
            .while_waiting(async move {
                deferred
                    .handle
                    .await
                    .map_err(|error| anyhow::anyhow!("joining git pull worker: {error}"))
            })
            .await?
    } else {
        let results = deferred
            .handle
            .await
            .map_err(|error| anyhow::anyhow!("joining git pull worker: {error}"))?;
        if deferred.print_results {
            super::print_git_pull_results(&results);
        }
        results
    };
    let (ok, failed) = super::record_git_pull_results(&results);
    let detail = git_pull_timing_detail(ok, failed);
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Workspace,
        "git_pull_on_entry",
        Some(&detail),
    );
    steps.stage_done(crate::runtime::progress::LaunchStage::Workspace, detail);
    Ok(())
}

fn git_pull_timing_detail(ok: usize, failed: usize) -> String {
    if failed == 0 {
        format!("{ok} repositories current")
    } else {
        format!("{ok} repositories current; {failed} failed")
    }
}

fn defer_operator_env<'a>(
    config: &'a AppConfig,
    selector: &RoleSelector,
    workspace_name: Option<&str>,
    opts: &'a super::LoadOptions,
) -> DeferredOperatorEnv<'a> {
    let workspace_typed = workspace_name.and_then(|n| WorkspaceName::parse(n).ok());
    if !jackin_env::has_operator_env(config, Some(&selector.key()), workspace_typed.as_ref()) {
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "operator_env",
            None,
        );
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "operator_env",
            Some("skipped"),
        );
        return DeferredOperatorEnv::Ready(std::collections::BTreeMap::new());
    }

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Credentials,
        "operator_env",
        Some("overlapped"),
    );
    let selector_key = selector.key().clone();
    let workspace_key = workspace_name.map(String::from);
    if let Some(op_runner) = opts.op_runner.as_deref() {
        return DeferredOperatorEnv::Inline(InlineOperatorEnv {
            config,
            selector_key,
            workspace_key,
            op_runner,
            host_env: opts.host_env.as_ref(),
        });
    }

    let config_clone = config.clone();
    let host_env = opts.host_env.clone();
    DeferredOperatorEnv::Spawned(jackin_telemetry::spawn::joined_blocking(move || {
        if let Some(host_env) = host_env {
            let default_runner = jackin_env::OpCli::new();
            let host_env_fn = |name: &str| -> Result<String, std::env::VarError> {
                host_env
                    .get(name)
                    .cloned()
                    .ok_or(std::env::VarError::NotPresent)
            };
            let ws = workspace_key
                .as_deref()
                .and_then(|n| WorkspaceName::parse(n).ok());
            jackin_env::resolve_operator_env_with(
                &config_clone,
                Some(&selector_key),
                ws.as_ref(),
                &default_runner,
                host_env_fn,
            )
        } else {
            let ws = workspace_key
                .as_deref()
                .and_then(|n| WorkspaceName::parse(n).ok());
            jackin_env::resolve_operator_env(&config_clone, Some(&selector_key), ws.as_ref())
        }
    }))
}

async fn await_operator_env(
    deferred: DeferredOperatorEnv<'_>,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let result = match deferred {
        DeferredOperatorEnv::Spawned(handle) => handle
            .await
            .map_err(|e| anyhow::anyhow!("env resolver panicked: {e}"))?,
        DeferredOperatorEnv::Inline(inline) => {
            let host_env_fn = |name: &str| -> Result<String, std::env::VarError> {
                inline.host_env.map_or_else(
                    || std::env::var(name),
                    |map| map.get(name).cloned().ok_or(std::env::VarError::NotPresent),
                )
            };
            let ws = inline
                .workspace_key
                .as_deref()
                .and_then(|n| WorkspaceName::parse(n).ok());
            jackin_env::resolve_operator_env_with(
                inline.config,
                Some(&inline.selector_key),
                ws.as_ref(),
                inline.op_runner,
                host_env_fn,
            )
        }
        DeferredOperatorEnv::Ready(env) => return Ok(env),
    };
    match result {
        Ok(env) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "operator_env",
                Some(&format!("{} vars overlapped", env.len())),
            );
            Ok(env)
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "operator_env",
                Some("error"),
            );
            Err(error)
        }
    }
}

pub fn load_role<'a>(
    paths: &'a JackinPaths,
    config: &'a mut AppConfig,
    selector: &'a RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    docker: &'a impl DockerApi,
    runner: &'a mut impl CommandRunner,
    opts: &'a super::LoadOptions,
) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>> {
    let target_kind = if config.workspaces.contains_key(workspace.name.as_str()) {
        jackin_telemetry::schema::enums::LaunchTargetKind::Workspace
    } else {
        jackin_telemetry::schema::enums::LaunchTargetKind::Directory
    };
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::LAUNCH_TARGET_KIND,
        value: jackin_telemetry::Value::Str(target_kind.as_str()),
    }];
    let operation =
        jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::LAUNCH, &attrs);
    let span = operation.span().clone();

    Box::pin(
        async move {
            let result = match opts.validate_programmatic(config, selector) {
                Ok(()) => {
                    let merged = with_extra_mounts(workspace, &opts.extra_mounts);
                    let workspace = merged.as_ref().unwrap_or(workspace);
                    load_role_inner(paths, config, selector, workspace, docker, runner, opts).await
                }
                Err(error) => Err(anyhow::Error::new(error)),
            };
            match &result {
                Ok(()) => {
                    operation
                        .complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
                }
                Err(error) if jackin_core::LaunchCancelled::is_cancel(error) => operation.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Cancellation,
                    None,
                ),
                Err(_) => operation.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Failure,
                    Some(jackin_telemetry::schema::enums::ErrorType::LaunchFailed),
                ),
            }
            result
        }
        .instrument(span),
    )
}

fn with_extra_mounts(
    workspace: &jackin_config::ResolvedWorkspace,
    extra: &[jackin_config::MountConfig],
) -> Option<jackin_config::ResolvedWorkspace> {
    if extra.is_empty() {
        return None;
    }
    let mut merged = workspace.clone();
    for mount in extra {
        merged.mounts.retain(|existing| existing.dst != mount.dst);
        merged.mounts.push(mount.clone());
    }
    Some(merged)
}

fn load_role_inner<'a>(
    paths: &'a JackinPaths,
    config: &'a mut AppConfig,
    selector: &'a RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    docker: &'a impl DockerApi,
    runner: &'a mut impl CommandRunner,
    opts: &'a super::LoadOptions,
) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>> {
    load_role_inner_with_admission(
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        RestoreLaunchAdmission {
            opts,
            related_expectation: None,
        },
    )
}

fn load_role_inner_with_admission<'a>(
    paths: &'a JackinPaths,
    config: &'a mut AppConfig,
    selector: &'a RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    docker: &'a impl DockerApi,
    runner: &'a mut impl CommandRunner,
    admission: RestoreLaunchAdmission<'a>,
) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>> {
    let RestoreLaunchAdmission {
        opts,
        related_expectation,
    } = admission;
    Box::pin(load_role_with_admission(
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        LaunchAdmissionRequest {
            opts,
            confirm_trust_for_test: |_: &RoleSelector, _: &jackin_config::RoleSource| {
                anyhow::bail!("role trust prompt requires the rich launch dialog")
            },
            confirm_branch_for_test: |_: &RoleSelector, _: &jackin_config::RoleSource, _: &str| {
                anyhow::bail!("branch trust prompt requires the rich launch dialog")
            },
            related_expectation,
        },
    ))
}

fn admit_explicit_restore_account(
    root: &std::path::Path,
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
    related_expectation: Option<&RelatedRestoreExpectation>,
) -> anyhow::Result<super::account_identity::AdmittedAccountConfiguration> {
    let admission = match super::account_identity::admit_account_configuration(
        root, config, workspace, role,
    )? {
        super::account_identity::AccountConfigurationAdmission::Changed => {
            if related_expectation.is_some() {
                anyhow::bail!("related restore admission changed before recursive launch");
            }
            anyhow::bail!("instance account policy changed; launch a new instance");
        }
        super::account_identity::AccountConfigurationAdmission::Matched(admission) => *admission,
    };
    if let Some(expected) = related_expectation {
        expected.verify(&admission)?;
    }
    Ok(admission)
}

fn git_pull_program(_opts: &super::LoadOptions) -> std::path::PathBuf {
    std::path::PathBuf::from("git")
}

#[expect(
    clippy::too_many_arguments,
    reason = "restore threads exact Docker ownership, account revision, and pending entry leases"
)]
async fn restore_current_role_now_with_handle(
    paths: &JackinPaths,
    container: &ContainerHandle,
    admission_lease: &super::account_identity::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    steps: &mut super::StepCounter,
    start_first: bool,
    entry_claim: Option<&crate::runtime::universe::EntryClaim>,
) -> anyhow::Result<()> {
    steps.finish_progress();
    let container_name = container.name();
    let load_result = start_or_hardline_agent_with_container_handle(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        start_first,
        container,
        entry_claim,
    )
    .await;
    super::render_exit(paths, docker).await;
    load_result
}

pub(super) async fn teardown_recreate_container(
    paths: &JackinPaths,
    container: &str,
    known_role_handle: Option<&ContainerHandle>,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let (resources, role_handle, dind_handle) =
        crate::runtime::cleanup::resolve_cleanup_handles_for_state(
            paths,
            container,
            known_role_handle,
            docker,
        )
        .await?;
    if let Some(handle) = role_handle.as_ref() {
        docker.remove_container_by_id(handle).await?;
    }
    if resources.dind_container.is_some()
        && let Some(handle) = dind_handle.as_ref()
    {
        docker.remove_container_by_id(handle).await?;
    }
    if role_handle.is_some() || dind_handle.is_some() {
        docker.remove_network(&resources.network).await?;
    }
    Ok(())
}

pub use jackin_runtime_launch_supported_agents::supported_agents::resolve_supported_agents_for_console;

pub(super) fn tag_errors<E: std::fmt::Display>(tag: &str, errors: Vec<E>) -> Vec<String> {
    errors
        .into_iter()
        .map(|error| format!("  - [{tag}] {error}"))
        .collect()
}

pub(super) fn tagged_grant_errors(
    tag: &str,
    grants: &crate::runtime::docker_profile::DockerGrants,
) -> Vec<String> {
    tag_errors(tag, crate::runtime::docker_profile::validate_grants(grants))
}

pub(super) fn bail_on_grant_errors(errors: Vec<String>) -> anyhow::Result<()> {
    if errors.is_empty() {
        return Ok(());
    }
    anyhow::bail!("docker grants validation failed:\n{}", errors.join("\n"))
}

async fn restore_explicit_container(
    paths: &JackinPaths,
    container: Option<&String>,
    admission_lease: &super::account_identity::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    steps: &mut super::StepCounter,
    opts: &super::LoadOptions,
) -> anyhow::Result<bool> {
    let Some(container) = container else {
        return Ok(false);
    };
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Restore,
        "explicit_restore_container",
        Some(container),
    );
    let inspection = docker.inspect_container_by_name(container).await;
    let docker_state = inspection.state;
    admission_lease.ensure_current(paths)?;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Restore,
        "explicit_restore_container",
        Some(docker_state.short_label().as_str()),
    );
    let start = match docker_state {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => false,
        ContainerState::Stopped { .. } | ContainerState::Created => true,
        ContainerState::InspectUnavailable(reason) => anyhow::bail!(
            "{}",
            crate::runtime::attach::docker_unavailable_msg(
                &format!("inspect explicit restore container `{container}`"),
                &reason,
            )
        ),
        ContainerState::NotFound | ContainerState::Removing | ContainerState::Dead => {
            return Ok(false);
        }
    };
    let Some(container_handle) = inspection.handle else {
        anyhow::bail!(
            "explicit restore container '{container}' inspection returned no immutable ID"
        );
    };
    super::emit_launch_plan(
        if start {
            super::LaunchPlan::StartStopped
        } else {
            super::LaunchPlan::AttachExisting
        },
        if start {
            "explicit_restore_container_startable"
        } else {
            "explicit_restore_container_running"
        },
        Some(container),
    );
    opts.record_launched_instance(container);
    restore_current_role_now_with_handle(
        paths,
        &container_handle,
        admission_lease,
        docker,
        runner,
        steps,
        start,
        opts.entry_claim.as_deref(),
    )
    .await?;
    Ok(true)
}

fn confirm_sensitive_mounts(
    workspace: &jackin_config::ResolvedWorkspace,
    steps: &mut super::StepCounter,
) -> anyhow::Result<()> {
    let normalized = jackin_config::normalize_sensitive_mount_sources(&workspace.mounts, |path| {
        std::fs::canonicalize(path).ok()
    });
    let sensitive = jackin_config::find_sensitive_mounts(&normalized);
    if sensitive.is_empty() {
        return Ok(());
    }
    let prompt = super::sensitive_mount_prompt(&sensitive);
    let Some(progress) = steps.progress_mut() else {
        anyhow::bail!("sensitive mount confirmation requires the rich launch dialog");
    };
    if !progress.confirm_prompt(prompt)? {
        anyhow::bail!("aborted — sensitive mount paths were not confirmed");
    }
    Ok(())
}

fn ensure_role_trust(
    config: &mut AppConfig,
    selector: &RoleSelector,
    source: &jackin_config::RoleSource,
    steps: &mut super::StepCounter,
    confirm: impl FnOnce(&RoleSelector, &jackin_config::RoleSource) -> anyhow::Result<()>,
) -> anyhow::Result<bool> {
    if source.trusted {
        return Ok(false);
    }
    let confirmation = if let Some(progress) = steps.progress_mut() {
        progress.confirm_role_trust(selector.key(), source.git.clone())
    } else {
        confirm(selector, source).map(|()| true)
    };
    let confirmed = match confirmation {
        Ok(confirmed) => confirmed,
        Err(error) => {
            emit_launch_trust_decision(
                jackin_telemetry::schema::enums::TrustDecision::Rejected,
                Some(jackin_telemetry::schema::enums::ErrorType::TrustError),
            );
            return Err(error);
        }
    };
    if !confirmed {
        emit_launch_trust_decision(
            jackin_telemetry::schema::enums::TrustDecision::Rejected,
            Some(jackin_telemetry::schema::enums::ErrorType::RoleSourceNotTrusted),
        );
        anyhow::bail!(
            "role source \"{selector}\" not trusted — aborting.\n\
             To trust it later, run `jackin config trust grant {selector}` or try loading again."
        );
    }
    if let Some(entry) = config.roles.get_mut(&selector.key()) {
        entry.trusted = true;
    }
    Ok(true)
}

fn emit_launch_trust_decision(
    decision: jackin_telemetry::schema::enums::TrustDecision,
    error_type: Option<jackin_telemetry::schema::enums::ErrorType>,
) {
    let outcome = match decision {
        jackin_telemetry::schema::enums::TrustDecision::Granted => {
            jackin_telemetry::schema::enums::OutcomeValue::Success
        }
        jackin_telemetry::schema::enums::TrustDecision::Rejected
        | jackin_telemetry::schema::enums::TrustDecision::Revoked => {
            jackin_telemetry::schema::enums::OutcomeValue::Failure
        }
    };
    let mut attrs = vec![
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::TRUST_DECISION,
            value: jackin_telemetry::Value::Str(decision.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::TRUST_SOURCE_TYPE,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::TrustSourceType::External.as_str(),
            ),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(outcome.as_str()),
        },
    ];
    if let Some(error_type) = error_type {
        attrs.push(jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str(error_type.as_str()),
        });
    }
    let _event = jackin_telemetry::emit_event(
        &jackin_telemetry::event::TRUST_DECISION,
        jackin_telemetry::FieldSet::new(&attrs, None),
    );
}

fn select_launch_agent(
    opts: &super::LoadOptions,
    workspace: &jackin_config::ResolvedWorkspace,
    early_restore_agent: Option<jackin_core::Agent>,
    supported_agents: &[jackin_core::Agent],
    steps: &mut super::StepCounter,
    selector: &RoleSelector,
) -> anyhow::Result<jackin_core::Agent> {
    if let Some(agent) = opts
        .agent
        .or(workspace.default_agent)
        .or(early_restore_agent)
    {
        return Ok(agent);
    }
    match supported_agents {
        [] => anyhow::bail!(
            "role \"{}\" declares no supported agents in its manifest",
            selector.key()
        ),
        [agent] => Ok(*agent),
        agents => {
            let labels = agents.iter().map(|agent| agent.slug().to_owned()).collect();
            let Some(progress) = steps.progress_mut() else {
                anyhow::bail!(
                    "role \"{}\" supports multiple agents ({:?}); load requires the rich launch dialog for agent selection, or pass --agent / set workspace `default_agent`",
                    selector.key(),
                    agents.iter().map(|agent| agent.slug()).collect::<Vec<_>>()
                );
            };
            let selection = progress.select_choice("Choose launch agent", labels)?;
            Ok(agents[selection])
        }
    }
}

fn persist_new_role_trust(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    restore_source_override: bool,
    is_new: bool,
    newly_trusted: bool,
) -> anyhow::Result<Option<AppConfig>> {
    if restore_source_override || (!is_new && !newly_trusted) {
        return Ok(None);
    }
    let mut editor = jackin_config::ConfigEditor::open(paths)?;
    if let Some(role_source) = config.roles.get(&selector.key()) {
        editor.upsert_agent_source(&selector.key(), role_source);
    }
    editor.set_agent_trust(&selector.key(), true);
    let persisted_config = editor.save()?;
    *config = persisted_config.clone();
    Ok(Some(persisted_config))
}

fn confirm_role_branch(
    branch: Option<&str>,
    selector: &RoleSelector,
    source: &jackin_config::RoleSource,
    steps: &mut super::StepCounter,
    confirm: impl FnOnce(&RoleSelector, &jackin_config::RoleSource, &str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let Some(branch) = branch else {
        return Ok(());
    };
    let prompt = format!(
        "Role `{selector}` is being loaded from unmerged branch `{branch}`.\n\
         Its Dockerfile and scripts may differ from the trusted main branch.\n\
         Have you reviewed the branch diff and verified it is safe to build?"
    );
    let confirmed = if let Some(progress) = steps.progress_mut() {
        progress.confirm_prompt(prompt)?
    } else {
        confirm(selector, source, branch)?;
        true
    };
    if !confirmed {
        anyhow::bail!(
            "branch \"{branch}\" not confirmed — aborting.\n\
             Review the Dockerfile and scripts on that branch before loading it."
        );
    }
    Ok(())
}

fn initialize_launch_progress(
    steps: &mut super::StepCounter,
    selector: &RoleSelector,
    opts: &super::LoadOptions,
    workspace: &jackin_config::ResolvedWorkspace,
    workspace_name: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(run) = jackin_diagnostics::active_run() {
        let mut progress = crate::runtime::progress::LaunchProgress::new(
            run,
            std::env::var_os("JACKIN_NO_MOTION").is_some(),
            crate::runtime::progress::host_terminal(),
            env!("JACKIN_VERSION"),
        )?;
        progress.started(crate::runtime::progress::LaunchIdentity {
            role: selector.name.clone(),
            agent: opts
                .agent
                .or(workspace.default_agent)
                .map_or_else(|| "resolving".to_owned(), |agent| agent.slug().to_owned()),
            target_kind: super::launch_target_kind(workspace_name),
            target_label: super::launch_target_label(workspace_name, workspace),
            mounts: super::launch_mount_lines(workspace),
            image: None,
            container: None,
        });
        steps.start_progress(progress);
    }
    steps.stage_done(
        crate::runtime::progress::LaunchStage::Identity,
        "resolved operator",
    );
    Ok(())
}

fn confirm_cached_repo_removal(steps: &mut super::StepCounter) -> anyhow::Result<bool> {
    let Some(progress) = steps.progress_mut() else {
        anyhow::bail!("cached repo recovery prompt requires the rich launch dialog");
    };
    progress.confirm_prompt("Remove the cached repo and re-clone from the configured source?")
}

fn mark_construct_ready(steps: &mut super::StepCounter) {
    steps.stage_started(
        crate::runtime::progress::LaunchStage::Construct,
        "verifying construct",
    );
    steps.stage_done(crate::runtime::progress::LaunchStage::Construct, "online");
}

fn breadcrumb_auth_mode(
    config: &AppConfig,
    agent: jackin_core::Agent,
    workspace: Option<&WorkspaceName>,
    role_key: &str,
) -> anyhow::Result<jackin_config::AuthForwardMode> {
    if let Ok(selected) = jackin_config::resolve_account(config, agent, workspace, role_key) {
        return Ok(selected.map_or(
            jackin_config::AuthForwardMode::Ignore,
            jackin_config::AccountConfig::auth_mode,
        ));
    }
    let instances = jackin_config::resolve_launch(config, workspace, role_key, None, Some(agent))?;
    Ok(instances
        .iter()
        .find(|instance| instance.agent == agent)
        .and_then(|instance| config.accounts.get(&instance.account_id))
        .map_or(
            jackin_config::AuthForwardMode::Ignore,
            jackin_config::AccountConfig::auth_mode,
        ))
}

#[expect(
    clippy::too_many_lines,
    reason = "Launch coordination keeps restore exits, revision checks, repository and name \
              locks, trust callbacks, and the final launch handoff in their execution order."
)]
async fn load_role_with_admission(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    workspace: &jackin_config::ResolvedWorkspace,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    request: LaunchAdmissionRequest<
        '_,
        impl FnOnce(&RoleSelector, &jackin_config::RoleSource) -> anyhow::Result<()>,
        impl FnOnce(&RoleSelector, &jackin_config::RoleSource, &str) -> anyhow::Result<()>,
    >,
) -> anyhow::Result<()> {
    let LaunchAdmissionRequest {
        opts,
        confirm_trust_for_test,
        confirm_branch_for_test,
        related_expectation,
    } = request;
    anyhow::ensure!(
        related_expectation.is_none() || opts.restore_container_base.is_some(),
        "related restore admission requires its explicit container"
    );
    let account_admission::AccountAdmission {
        initial_account_revision,
        selected_workspace,
        admission_config,
        mut account_config,
    } = account_admission::prepare(paths, config, selector, workspace, docker, opts).await?;
    let config = account_config.as_mut().unwrap_or(config);

    let ((), git) = tokio::join!(
        crate::runtime::cleanup::gc_orphaned_resources(paths, docker),
        crate::runtime::identity::load_git_identity(runner)
    );

    crate::runtime::universe::mark_start(
        paths,
        crate::runtime::universe::StartKind::ResumeExisting,
    )
    .await;

    let workspace_name = config
        .workspaces
        .contains_key(workspace.name.as_str())
        .then(|| workspace.name.clone());

    let telemetry_target_kind = if workspace_name.is_some() {
        jackin_telemetry::schema::enums::LaunchTargetKind::Workspace
    } else {
        jackin_telemetry::schema::enums::LaunchTargetKind::Directory
    };
    let mut steps = super::StepCounter::new(&selector.name, telemetry_target_kind);
    initialize_launch_progress(
        &mut steps,
        selector,
        opts,
        workspace,
        workspace_name.as_deref(),
    )?;

    confirm_sensitive_mounts(workspace, &mut steps)?;

    let role_key = selector.key();
    let selected_agent_before_role = opts.agent.or(workspace.default_agent);
    let mut early_restore_agent = None;
    let mut early_current_scan = super::EarlyCurrentRestoreScan::NotRun;
    let mut early_admitted_restore = None;
    let early_restore_container = if opts.restore_container_base.is_none()
        && current_role_reuse_is_compatible(opts)
    {
        if let Some(agent) = selected_agent_before_role {
            let candidate = super::resolve_current_restore_candidate_timed(
                paths,
                workspace_name.as_deref(),
                workspace.label.as_str(),
                &workspace.workdir,
                &role_key,
                agent,
                docker,
            )
            .await?;
            initial_account_revision.ensure_current(paths)?;
            if let Some(admitted) = candidate
                .map(|candidate| {
                    super::restore_resolve::admit_restore(
                        candidate,
                        &paths.data_dir,
                        config,
                        selected_workspace.as_ref(),
                        &role_key,
                    )
                })
                .transpose()?
            {
                match admitted.resolution.clone() {
                    super::RestoreResolution::StartCurrentRoleWithHandle(container) => {
                        let container_name = container.name().to_owned();
                        opts.record_launched_instance(&container_name);
                        return restore_current_role_now_with_handle(
                            paths,
                            &container,
                            &initial_account_revision,
                            docker,
                            runner,
                            &mut steps,
                            true,
                            opts.entry_claim.as_deref(),
                        )
                        .await;
                    }
                    super::RestoreResolution::RecreateCurrentRoleWithHandle(container) => {
                        let container_name = container.name().to_owned();
                        early_current_scan = super::EarlyCurrentRestoreScan::Scanned {
                            agent,
                            current: Some(super::RestoreResolution::RecreateCurrentRoleWithHandle(
                                container,
                            )),
                        };
                        early_admitted_restore = Some(admitted);
                        Some(container_name)
                    }
                    super::RestoreResolution::RecreateCurrentRole(container_name) => {
                        early_current_scan = super::EarlyCurrentRestoreScan::Scanned {
                            agent,
                            current: Some(super::RestoreResolution::RecreateCurrentRole(
                                container_name.clone(),
                            )),
                        };
                        early_admitted_restore = Some(admitted);
                        Some(container_name)
                    }
                    other => {
                        early_current_scan = super::EarlyCurrentRestoreScan::Scanned {
                            agent,
                            current: Some(other),
                        };
                        early_admitted_restore = Some(admitted);
                        None
                    }
                }
            } else {
                early_current_scan = super::EarlyCurrentRestoreScan::Scanned {
                    agent,
                    current: None,
                };
                None
            }
        } else {
            let candidate = super::resolve_unselected_current_restore_candidate_with_agent_timed(
                paths,
                workspace_name.as_deref(),
                workspace.label.as_str(),
                &workspace.workdir,
                &role_key,
                docker,
            )
            .await?;
            initial_account_revision.ensure_current(paths)?;
            match candidate
                .map(|mut candidate| -> anyhow::Result<_> {
                    let admitted = super::restore_resolve::admit_restore(
                        candidate.resolution,
                        &paths.data_dir,
                        config,
                        selected_workspace.as_ref(),
                        &role_key,
                    )?;
                    candidate.resolution = admitted.resolution.clone();
                    Ok((candidate, admitted))
                })
                .transpose()?
            {
                Some((
                    super::UnselectedCurrentRestoreResolution {
                        resolution: super::RestoreResolution::StartCurrentRoleWithHandle(container),
                        ..
                    },
                    _,
                )) => {
                    let container_name = container.name().to_owned();
                    opts.record_launched_instance(&container_name);
                    return restore_current_role_now_with_handle(
                        paths,
                        &container,
                        &initial_account_revision,
                        docker,
                        runner,
                        &mut steps,
                        true,
                        opts.entry_claim.as_deref(),
                    )
                    .await;
                }
                Some((
                    super::UnselectedCurrentRestoreResolution {
                        resolution:
                            super::RestoreResolution::RecreateCurrentRoleWithHandle(container),
                        agent,
                    },
                    admitted,
                )) => {
                    let container_name = container.name().to_owned();
                    early_restore_agent = Some(agent);
                    early_current_scan = super::EarlyCurrentRestoreScan::Scanned {
                        agent,
                        current: Some(super::RestoreResolution::RecreateCurrentRoleWithHandle(
                            container.clone(),
                        )),
                    };
                    early_admitted_restore = Some(admitted);
                    Some(container_name)
                }
                Some((
                    super::UnselectedCurrentRestoreResolution {
                        resolution: super::RestoreResolution::RecreateCurrentRole(container),
                        agent,
                    },
                    admitted,
                )) => {
                    early_restore_agent = Some(agent);
                    early_current_scan = super::EarlyCurrentRestoreScan::Scanned {
                        agent,
                        current: Some(super::RestoreResolution::RecreateCurrentRole(
                            container.clone(),
                        )),
                    };
                    early_admitted_restore = Some(admitted);
                    Some(container.clone())
                }
                Some((
                    super::UnselectedCurrentRestoreResolution { resolution, agent },
                    admitted,
                )) => {
                    early_current_scan = super::EarlyCurrentRestoreScan::Scanned {
                        agent,
                        current: Some(resolution),
                    };
                    early_admitted_restore = Some(admitted);
                    None
                }
                None => {
                    let role_empty = !super::restore::matching_current_role_manifests(
                        paths,
                        workspace_name.as_deref(),
                        workspace.label.as_str(),
                        &workspace.workdir,
                        &role_key,
                    )?
                    .into_iter()
                    .any(|m| m.is_restore_candidate());
                    if role_empty {
                        early_current_scan = super::EarlyCurrentRestoreScan::ScannedUnselectedEmpty;
                    }
                    None
                }
            }
        }
    } else {
        None
    };

    let mut restore_account_admission = early_admitted_restore
        .as_ref()
        .and_then(|admitted: &super::restore_resolve::AdmittedRestore| admitted.account.clone());

    if let Some(container) = opts.restore_container_base.as_ref() {
        restore_account_admission = Some(admit_explicit_restore_account(
            &paths.data_dir.join(container),
            config,
            selected_workspace.as_ref(),
            &role_key,
            related_expectation.as_ref(),
        )?);
    }
    if restore_explicit_container(
        paths,
        opts.restore_container_base.as_ref(),
        &initial_account_revision,
        docker,
        runner,
        &mut steps,
        opts,
    )
    .await?
    {
        return Ok(());
    }

    let (source, is_new, restore_source_override) = super::resolve_launch_role_source(
        config,
        selector,
        opts.restore_role_source_git.as_deref(),
    )?;

    steps.next("Resolving role identity").await?;

    let mut confirm_repo_removal = || confirm_cached_repo_removal(&mut steps);
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Role,
        "repo_refresh",
        Some(selector.key().as_str()),
    );
    let (cached_repo, validated_repo, repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        &source.git,
        runner,
        RepoResolveOptions::interactive(opts.debug)
            .with_branch(opts.role_branch.as_deref())
            .with_refresh_ttl(if opts.rebuild {
                std::time::Duration::ZERO
            } else {
                std::time::Duration::from_secs(
                    config
                        .role_repo_refresh_ttl_seconds
                        .unwrap_or(DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS),
                )
            }),
        &mut confirm_repo_removal,
    )
    .await
    .inspect(|_| {
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Role,
            "repo_refresh",
            Some("validated"),
        );
    })
    .inspect_err(|_| {
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Role,
            "repo_refresh",
            Some("error"),
        );
    })?;

    let newly_trusted = ensure_role_trust(
        config,
        selector,
        &source,
        &mut steps,
        confirm_trust_for_test,
    )?;

    initial_account_revision.ensure_current(paths)?;
    drop(initial_account_revision);
    let persisted_config = persist_new_role_trust(
        paths,
        config,
        selector,
        restore_source_override,
        is_new,
        newly_trusted,
    )?;
    if newly_trusted {
        emit_launch_trust_decision(
            jackin_telemetry::schema::enums::TrustDecision::Granted,
            None,
        );
    }
    let persisted_snapshot = persisted_config.as_ref().unwrap_or(&admission_config);
    let account_revision =
        super::account_identity::AccountConfigRevision::acquire_bound(paths, persisted_snapshot)?;

    let agent_display_name = validated_repo.manifest.display_name(&selector.name);
    steps.role_name.clone_from(&agent_display_name);

    let supported_agents = validated_repo.manifest.supported_agents();
    let agent = select_launch_agent(
        opts,
        workspace,
        early_restore_agent,
        &supported_agents,
        &mut steps,
        selector,
    )?;
    super::validate_agent_supported(selector, &validated_repo.manifest, agent)?;

    confirm_role_branch(
        opts.role_branch.as_deref(),
        selector,
        &source,
        &mut steps,
        confirm_branch_for_test,
    )?;

    let mut restore_pinned_sha = restore_account_admission
        .as_ref()
        .and_then(|admission| admission.manifest.role_git_sha.clone());
    let restore_container = if early_restore_container.is_some() {
        early_restore_container
    } else if let Some(container) = opts.restore_container_base.as_ref() {
        Some(container.clone())
    } else if !current_role_reuse_is_compatible(opts) {
        None
    } else {
        let admitted = if let Some(admitted) = early_admitted_restore.take() {
            admitted
        } else {
            let restore_candidate = super::resolve_restore_candidate_reusing_early(
                paths,
                workspace_name.as_deref(),
                workspace.label.as_str(),
                &workspace.workdir,
                &role_key,
                agent,
                docker,
                steps.progress_mut(),
                &early_current_scan,
            )
            .await?;
            account_revision.ensure_current(paths)?;
            super::restore_resolve::admit_restore(
                restore_candidate,
                &paths.data_dir,
                config,
                selected_workspace.as_ref(),
                &role_key,
            )?
        };
        restore_account_admission = admitted.account;
        restore_pinned_sha = restore_account_admission
            .as_ref()
            .and_then(|admission| admission.manifest.role_git_sha.clone());
        match admitted.resolution {
            super::RestoreResolution::StartFresh => None,
            super::RestoreResolution::StartCurrentRoleWithHandle(container) => {
                let container_name = container.name().to_owned();
                opts.record_launched_instance(&container_name);
                return restore_current_role_now_with_handle(
                    paths,
                    &container,
                    &account_revision,
                    docker,
                    runner,
                    &mut steps,
                    true,
                    opts.entry_claim.as_deref(),
                )
                .await;
            }
            super::RestoreResolution::RecreateCurrentRoleWithHandle(container_handle) => {
                let container = container_handle.name().to_owned();
                teardown_recreate_container(paths, &container, Some(&container_handle), docker)
                    .await?;
                Some(container)
            }
            super::RestoreResolution::RecreateCurrentRole(container) => {
                teardown_recreate_container(paths, &container, None, docker).await?;
                Some(container)
            }
            super::RestoreResolution::RestoreCurrentRole(container) => {
                teardown_recreate_container(paths, &container, None, docker).await?;
                Some(container)
            }
            super::RestoreResolution::RecoverRelatedRole(container) => {
                steps.finish_progress();
                let load_result = start_or_hardline_agent(
                    paths,
                    &container,
                    &account_revision,
                    docker,
                    runner,
                    false,
                    opts.entry_claim.as_deref(),
                )
                .await
                .map(|()| container);
                match load_result {
                    Ok(_) => {
                        super::render_exit(paths, docker).await;
                        return Ok(());
                    }
                    Err(error) => {
                        super::render_exit(paths, docker).await;
                        return Err(error);
                    }
                }
            }
            super::RestoreResolution::RebuildRelatedRole(manifest) => {
                steps.finish_progress();
                let related_expectation = RelatedRestoreExpectation::from_admission(
                    restore_account_admission.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("related restore has no admitted account selection")
                    })?,
                );
                let selector = RoleSelector::parse(&manifest.role_key)?;
                let related_opts = super::related_restore_load_options(opts, &manifest)?;
                let load_result = load_role_inner_with_admission(
                    paths,
                    config,
                    &selector,
                    workspace,
                    docker,
                    runner,
                    RestoreLaunchAdmission {
                        opts: &related_opts,
                        related_expectation: Some(related_expectation),
                    },
                )
                .await
                .map(|()| manifest.container_base);
                match load_result {
                    Ok(_) => {
                        super::render_exit(paths, docker).await;
                        return Ok(());
                    }
                    Err(error) => {
                        super::render_exit(paths, docker).await;
                        return Err(error);
                    }
                }
            }
            super::RestoreResolution::PurgeAndRestartFresh(container) => {
                if let Err(_error) = crate::runtime::cleanup::purge_container_state(
                    paths, &container, docker, runner,
                )
                .await
                {
                    let _warning = jackin_telemetry::record_recovered_degradation();
                }
                None
            }
        }
    };

    let mut git_pull_join: Option<DeferredGitPull> = None;
    if restore_container.is_none() && workspace.git_pull_on_entry {
        let sources = super::git_pull_sources(workspace);
        if sources.is_empty() {
            jackin_diagnostics::active_timing_started(
                jackin_diagnostics::DiagnosticStage::Workspace,
                "git_pull_on_entry",
                None,
            );
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Workspace,
                "git_pull_on_entry",
                Some("skipped_no_git_repos"),
            );
            steps.stage_skipped(
                crate::runtime::progress::LaunchStage::Workspace,
                "no mounted git repositories",
            );
        } else {
            jackin_diagnostics::active_timing_started(
                jackin_diagnostics::DiagnosticStage::Workspace,
                "git_pull_on_entry",
                Some(&format!("{} repo(s)", sources.len())),
            );
            steps.stage_started(
                crate::runtime::progress::LaunchStage::Workspace,
                format!("polling {} workspace repositories", sources.len()),
            );
            if steps.progress_mut().is_some() {
                let debug = opts.debug;
                let git_program = git_pull_program(opts);
                let handle = jackin_telemetry::spawn::joined_blocking(move || {
                    super::pull_git_sources_with_git(sources, debug, &git_program, false)
                });
                git_pull_join = Some(DeferredGitPull {
                    handle,
                    print_results: false,
                });
            } else {
                let debug = opts.debug;
                let git_program = git_pull_program(opts);
                let handle = jackin_telemetry::spawn::joined_blocking(move || {
                    super::pull_git_sources_with_git(sources, debug, &git_program, true)
                });
                git_pull_join = Some(DeferredGitPull {
                    handle,
                    print_results: true,
                });
            }
        }
    }
    let restoring = restore_container.is_some();
    let (container_name, _name_lock) = if let Some(container_name) = restore_container {
        claim_known_container_name(paths, &container_name, docker).await?
    } else {
        {
            let workspace_typed = workspace_name
                .as_deref()
                .map(WorkspaceName::parse)
                .transpose()
                .map_err(anyhow::Error::from)?;
            claim_container_name(paths, workspace_typed.as_ref(), selector, docker).await?
        }
    };
    opts.record_launched_instance(&container_name);

    let image_tag = opts.role_branch.as_deref().map_or_else(
        || image_name(selector, None),
        |b| image_name_for_branch(selector, b, None),
    );
    if let Some(progress) = steps.progress_mut() {
        progress.update_identity(crate::runtime::progress::LaunchIdentity {
            role: agent_display_name.clone(),
            agent: agent.slug().to_owned(),
            target_kind: super::launch_target_kind(workspace_name.as_deref()),
            target_label: super::launch_target_label(workspace_name.as_deref(), workspace),
            mounts: super::launch_mount_lines(workspace),
            image: Some(image_tag.clone()),
            container: Some(container_name.clone()),
        });
    }
    steps.stage_done(
        crate::runtime::progress::LaunchStage::Role,
        "trusted source",
    );

    let operator_env = defer_operator_env(config, selector, workspace_name.as_deref(), opts);

    let rebuild = opts.rebuild;
    mark_construct_ready(&mut steps);
    let repo_lock = Some(repo_lock);
    let image_decision = crate::runtime::image::decide_role_image(
        paths,
        selector,
        &cached_repo,
        &validated_repo,
        rebuild,
        opts.role_branch.as_deref(),
        restore_pinned_sha.as_deref(),
        docker,
        runner,
    )
    .await?;

    steps.stage_started(
        crate::runtime::progress::LaunchStage::Credentials,
        "resolving launch credentials",
    );

    let auth_workspace = workspace_name
        .as_deref()
        .and_then(|n| WorkspaceName::parse(n).ok());
    let auth_mode = breadcrumb_auth_mode(config, agent, auth_workspace.as_ref(), &role_key)?;
    let operator_env = await_operator_env(operator_env).await?;

    let mut merged_vars = environment_admission::resolve(
        &validated_repo.manifest,
        &operator_env,
        workspace,
        opts,
        &mut steps,
    )?;
    let exec_bindings = environment_admission::compose_exec_bindings(
        config,
        workspace_name.as_deref(),
        &role_key,
        opts,
        &mut merged_vars,
    )?;
    let resolved_env = jackin_env::ResolvedEnv { vars: merged_vars };

    if !operator_env.is_empty() {
        let diag_ws = workspace_name
            .as_deref()
            .and_then(|n| WorkspaceName::parse(n).ok());
        jackin_env::print_launch_diagnostic(
            config,
            Some(&selector.key()),
            diag_ws.as_ref(),
            &operator_env,
            opts.debug,
        );
    }
    steps.stage_done(
        crate::runtime::progress::LaunchStage::Credentials,
        "resolved",
    );

    let recipe_role_git_sha = image_decision.role_git_sha();
    let recipe_base_image_ref = image_decision.base_image_ref().map(ToOwned::to_owned);

    let selected_refresh_reason = match &image_decision {
        crate::runtime::image::ImageDecision::RefreshInBackground { reason, .. } => Some(*reason),
        crate::runtime::image::ImageDecision::Reuse { .. }
        | crate::runtime::image::ImageDecision::BuildFromPublished { .. }
        | crate::runtime::image::ImageDecision::BuildFromWorkspace { .. } => None,
    };

    let backend = super::resolve_backend(config, workspace_name.as_deref())?;

    let load_result: anyhow::Result<String> =
        launch_core::run_launch_core(launch_core::LaunchCore {
            paths,
            config,
            selector,
            workspace,
            docker,
            runner,
            opts,
            git,
            workspace_name,
            steps: &mut steps,
            role_key,
            agent_display_name,
            agent,
            supported_agents,
            cached_repo,
            validated_repo,
            source,
            auth_mode,
            backend,
            image_decision,
            repo_lock,
            restoring,
            container_name,
            exec_bindings,
            recipe_role_git_sha,
            recipe_base_image_ref,
            selected_refresh_reason,
            resolved_env,
            rebuild,
            restore_pinned_sha,
            restore_instances: restore_account_admission.map(|admission| admission.instances),
            git_pull_join,
            account_revision,
            admission_config,
        })
        .await;

    match load_result {
        Ok(_) => {
            super::render_exit(paths, docker).await;
            Ok(())
        }
        Err(error) => {
            let failed_stage = steps
                .current_stage
                .unwrap_or(crate::runtime::progress::LaunchStage::Capsule);
            let run = jackin_diagnostics::active_run();
            let final_error = super::launch_failure_cli_error(failed_stage, &error, run.as_deref());
            steps
                .stage_failed(crate::runtime::progress::LaunchFailure {
                    title: super::launch_failure_title(failed_stage, &error, run.as_deref()),
                    summary: super::short_launch_diagnosis(failed_stage, &error, run.as_deref()),
                    detail: Some(format!("{error:#}")),
                    next_step: None,
                    stage: failed_stage,
                })
                .await;
            steps.finish_progress();
            super::render_exit(paths, docker).await;
            Err(final_error)
        }
    }
}

fn current_role_reuse_is_compatible(opts: &super::LoadOptions) -> bool {
    !opts.rebuild
        && !opts.force
        && !opts.non_interactive
        && opts.role_branch.is_none()
        && opts.selection.is_none()
        && opts.model.is_none()
        && opts.effort.is_none()
        && opts.docker_profile.is_none()
        && opts.env.is_empty()
        && opts.on_demand_bindings.is_empty()
        && opts.extra_mounts.is_empty()
        && opts.op_runner.is_none()
        && opts.host_env.is_none()
        && opts.restore_role_source_git.is_none()
}

async fn absent_container_can_apply_model_and_effort(
    docker: &impl DockerApi,
    opts: &super::LoadOptions,
) -> anyhow::Result<bool> {
    let Some(container) = opts.restore_container_base.as_deref() else {
        return Ok(false);
    };
    if opts.model.is_none() && opts.effort.is_none() {
        return Ok(false);
    }
    if explicit_restore_has_non_model_override(opts) {
        return Ok(false);
    }
    let inspection = docker.inspect_container_by_name(container).await;
    Ok(matches!(inspection.state, ContainerState::NotFound))
}

fn explicit_restore_has_non_model_override(opts: &super::LoadOptions) -> bool {
    opts.rebuild
        || opts.force
        || opts.non_interactive
        || opts.selection.is_some()
        || opts.docker_profile.is_some()
        || !opts.env.is_empty()
        || !opts.on_demand_bindings.is_empty()
        || !opts.extra_mounts.is_empty()
}

fn validate_absent_rebuild_restore_options(
    paths: &JackinPaths,
    opts: &super::LoadOptions,
) -> anyhow::Result<()> {
    let Some(container) = opts.restore_container_base.as_deref() else {
        return Ok(());
    };
    anyhow::ensure!(
        !opts.rebuild
            && !opts.force
            && !opts.non_interactive
            && opts.selection.is_none()
            && opts.docker_profile.is_none()
            && opts.env.is_empty()
            && opts.on_demand_bindings.is_empty()
            && opts.extra_mounts.is_empty(),
        "an explicit restore container cannot apply rebuild, selection, profile, environment, credential, or mount overrides; its selected agent must match the stored instance; start a fresh role instance instead"
    );
    if let Some(requested_agent) = opts.agent {
        let manifest = InstanceManifest::read(&paths.data_dir.join(container))?;
        let stored_agent = manifest.agent()?;
        anyhow::ensure!(
            requested_agent == stored_agent,
            "explicit restore agent {} does not match stored instance agent {}",
            requested_agent.slug(),
            stored_agent.slug(),
        );
    }
    Ok(())
}

fn validate_explicit_restore_options(
    paths: &JackinPaths,
    opts: &super::LoadOptions,
) -> anyhow::Result<()> {
    let Some(container) = opts.restore_container_base.as_deref() else {
        return Ok(());
    };
    anyhow::ensure!(
        !opts.rebuild
            && !opts.force
            && !opts.non_interactive
            && opts.selection.is_none()
            && opts.docker_profile.is_none()
            && opts.model.is_none()
            && opts.effort.is_none()
            && opts.env.is_empty()
            && opts.on_demand_bindings.is_empty()
            && opts.extra_mounts.is_empty(),
        "an explicit restore container cannot apply rebuild, selection, model, effort, profile, environment, credential, or mount overrides; its selected agent must match the stored instance; start a fresh role instance instead"
    );
    if let Some(requested_agent) = opts.agent {
        let manifest = InstanceManifest::read(&paths.data_dir.join(container))?;
        let stored_agent = manifest.agent()?;
        anyhow::ensure!(
            requested_agent == stored_agent,
            "explicit restore agent {} does not match stored instance agent {}",
            requested_agent.slug(),
            stored_agent.slug(),
        );
    }
    Ok(())
}

pub(crate) fn emit_auth_provision_launch_plan(state: &RoleState, container: &str) {
    if state.auth_outcomes.is_empty() {
        return;
    }
    let outcomes = state
        .auth_outcomes
        .iter()
        .map(|(agent, outcome)| (agent.slug(), outcome.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let detail = serde_json::json!({
        "plan": "AuthProvision",
        "reason": "credential_outcomes",
        "container": container,
        "agents": outcomes,
    })
    .to_string();
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "launch_plan",
            jackin_diagnostics::DiagnosticStage::Credentials,
            "agent credential outcomes",
            Some(&detail),
        );
    }
}

pub(crate) fn manifest_env_timing_detail(skipped: bool, vars: usize) -> String {
    if skipped {
        "skipped".to_owned()
    } else {
        format!("{vars} vars")
    }
}

pub(super) async fn purge_or_mark_clean_exited(
    paths: &JackinPaths,
    container_name: &str,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    if let Err(_error) =
        crate::runtime::cleanup::purge_container_state(paths, container_name, docker, runner).await
    {
        let _warning = jackin_telemetry::record_recovered_degradation();
        super::write_instance_status(paths, state_dir, manifest, InstanceStatus::CleanExited)?;
    }
    Ok(())
}

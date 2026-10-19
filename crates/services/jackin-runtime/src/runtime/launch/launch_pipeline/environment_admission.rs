// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;
use jackin_core::WorkspaceName;
use jackin_manifest::RoleManifest;
use std::collections::BTreeMap;

pub(super) fn resolve(
    manifest: &RoleManifest,
    operator_env: &BTreeMap<String, String>,
    workspace: &jackin_config::ResolvedWorkspace,
    opts: &super::super::LoadOptions,
    steps: &mut super::super::StepCounter,
) -> anyhow::Result<Vec<(String, String)>> {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Credentials,
        "manifest_env",
        None,
    );
    let manifest_env: BTreeMap<_, _> = manifest
        .env
        .iter()
        .filter(|(key, _)| !jackin_env::is_account_env(key))
        .map(|(key, decl)| (key.clone(), decl.clone()))
        .collect();
    let manifest_env_skipped = manifest_env.is_empty();
    let manifest_resolved_result = if manifest_env_skipped {
        Ok(jackin_env::ResolvedEnv { vars: vec![] })
    } else {
        let prompter = super::super::LaunchEnvPrompter::new(steps.progress_mut());
        jackin_env::resolve_env_with_overrides(&manifest_env, &prompter, operator_env)
    };
    let manifest_resolved = match manifest_resolved_result {
        Ok(env) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "manifest_env",
                Some(&super::manifest_env_timing_detail(
                    manifest_env_skipped,
                    env.vars.len(),
                )),
            );
            env
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "manifest_env",
                Some("error"),
            );
            return Err(error);
        }
    };

    let mut merged_vars: Vec<(String, String)> = manifest_resolved.vars;
    for (k, v) in operator_env {
        if let Some(slot) = merged_vars.iter_mut().find(|(mk, _)| mk == k) {
            slot.1.clone_from(v);
        } else {
            merged_vars.push((k.clone(), v.clone()));
        }
    }
    for (k, v) in &opts.env {
        anyhow::ensure!(
            !jackin_env::is_account_env(k),
            "credential {k} must come from an assigned account"
        );
        if let Some(slot) = merged_vars.iter_mut().find(|(mk, _)| mk == k) {
            slot.1.clone_from(v);
        } else {
            merged_vars.push((k.clone(), v.clone()));
        }
    }
    super::super::trust::inject_workspace_mise_env(&mut merged_vars, workspace);

    Ok(merged_vars)
}

pub(super) fn compose_exec_bindings(
    config: &AppConfig,
    workspace_name: Option<&str>,
    role_key: &str,
    opts: &super::super::LoadOptions,
    merged_vars: &mut Vec<(String, String)>,
) -> anyhow::Result<Vec<jackin_protocol::ExecBinding>> {
    let exec_ws = workspace_name.and_then(|n| WorkspaceName::parse(n).ok());
    let mut exec_bindings =
        jackin_env::collect_on_demand_bindings(config, Some(role_key), exec_ws.as_ref());
    for binding in &opts.on_demand_bindings {
        anyhow::ensure!(
            !jackin_env::is_account_env(&binding.name),
            "credential {} must come from an assigned account",
            binding.name
        );
        exec_bindings.retain(|existing| existing.name != binding.name);
        exec_bindings.push(binding.clone());
    }
    exec_bindings.sort_by(|a, b| a.name.cmp(&b.name));
    if !exec_bindings.is_empty() {
        let names = super::super::exec_binding_names(&exec_bindings);
        merged_vars.retain(|(k, _)| k != "JACKIN_EXEC_BINDINGS");
        merged_vars.push(("JACKIN_EXEC_BINDINGS".to_owned(), names));
    }
    Ok(exec_bindings)
}

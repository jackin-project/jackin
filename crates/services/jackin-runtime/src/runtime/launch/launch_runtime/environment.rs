// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::auth_error::{
    NO_PROXY_LOWER, NO_PROXY_UPPER, append_no_proxy_host, is_proxy_env_name, push_env_if_present,
};
use super::profile::ProfileFacts;
use super::{
    CapsuleAuth, CapsuleEndpoint, CapsuleNetwork, LaunchContext, capsule_export_coverage,
    capsule_otlp_allowlist_host, capsule_otlp_propagation,
};

pub(super) fn append_arguments(args: &mut Vec<String>, values: &[&str]) {
    args.extend(values.iter().map(|value| (*value).to_owned()));
}

pub(super) fn arguments(
    ctx: &LaunchContext<'_, '_>,
    facts: &ProfileFacts,
    instance_id: &str,
) -> Vec<String> {
    let mut run_args = fixed_arguments(ctx, instance_id);
    let mut env_strings = resolved_arguments(ctx);
    let (container_otlp, capsule_otlp_headers) = service_arguments(ctx, &mut env_strings);
    emit_session_contract(ctx, facts);
    append_github_arguments(ctx, &mut env_strings);
    for value in env_strings {
        append_arguments(&mut run_args, &["-e", &value]);
    }
    let otlp_propagation = capsule_otlp_propagation(
        container_otlp.as_ref(),
        capsule_otlp_headers.as_deref(),
        jackin_telemetry::propagation::current_traceparent().as_deref(),
    );
    for env_str in &otlp_propagation {
        run_args.push(String::from("-e"));
        run_args.push(String::from(env_str));
    }
    if container_otlp
        .as_ref()
        .is_some_and(|otlp| otlp.needs_host_gateway)
    {
        append_arguments(
            &mut run_args,
            &["--add-host", "host.docker.internal:host-gateway"],
        );
    }

    run_args
}

pub(super) fn instance_id(ctx: &LaunchContext<'_, '_>) -> String {
    let container_name = ctx.container_name;
    let instance_id = if let Some(id) =
        crate::instance::naming::instance_id_from_container_base(container_name)
    {
        id
    } else {
        jackin_diagnostics::emit_compact_line(
            "warning",
            &format!(
                "warning: instance_id_from_container_base could not parse {container_name:?}; falling back to full container name as JACKIN_INSTANCE_ID — chrome chip will render the full name"
            ),
        );
        container_name
    };
    instance_id.to_owned()
}

fn fixed_arguments(ctx: &LaunchContext<'_, '_>, instance_id: &str) -> Vec<String> {
    let mut run_args = Vec::new();
    let dind_enabled = crate::runtime::docker_profile::dind_enabled(ctx.grants);
    let LaunchContext {
        container_name,
        dind,
        git,
        debug,
        git_coauthor_trailer,
        git_dco,
        ..
    } = ctx;
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2376");
    let docker_cert_path = format!(
        "DOCKER_CERT_PATH={}",
        jackin_core::container_paths::DIND_CERTS_CLIENT_DIR
    );
    let dind_hostname = format!("{}={dind}", jackin_core::JACKIN_DIND_HOSTNAME_ENV_NAME);
    let role_container_name_env = format!(
        "{}={container_name}",
        jackin_core::JACKIN_CONTAINER_NAME_ENV_NAME
    );
    let instance_id_env = format!("{}={instance_id}", jackin_core::JACKIN_INSTANCE_ID_ENV_NAME);
    let testcontainers_host_override = format!(
        "{}={dind}",
        jackin_core::TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME
    );
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
    append_arguments(&mut run_args, &["--user", "0:0", "-e", "HOME=/home/agent"]);

    append_arguments(
        &mut run_args,
        &[
            "-e",
            &role_container_name_env,
            "-e",
            &instance_id_env,
            "-e",
            &testcontainers_host_override,
            "-e",
            &git_author_name,
            "-e",
            &git_author_email,
        ],
    );
    if dind_enabled {
        append_arguments(
            &mut run_args,
            &[
                "-e",
                &docker_host,
                "-e",
                "DOCKER_TLS_VERIFY=1",
                "-e",
                docker_cert_path.as_str(),
                "-e",
                &dind_hostname,
            ],
        );
    }
    let run_envs = super::run_runtime_envs();
    for env in &run_envs {
        append_arguments(&mut run_args, &["-e", env.as_str()]);
    }
    let debug_envs = super::debug_runtime_envs(*debug);
    for env in &debug_envs {
        append_arguments(&mut run_args, &["-e", env.as_str()]);
    }
    let telemetry_envs = super::telemetry_runtime_envs(*debug);
    for env in &telemetry_envs {
        append_arguments(&mut run_args, &["-e", env.as_str()]);
    }
    let host_version_env = format!("JACKIN_HOST_VERSION={}", env!("JACKIN_VERSION"));
    append_arguments(&mut run_args, &["-e", host_version_env.as_str()]);

    let git_coauthor_trailer_env = git_coauthor_trailer
        .then(|| format!("{}=1", jackin_core::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME));
    if let Some(ref env) = git_coauthor_trailer_env {
        append_arguments(&mut run_args, &["-e", env.as_str()]);
    }
    let git_dco_env = git_dco.then(|| format!("{}=1", jackin_core::JACKIN_GIT_DCO_ENV_NAME));
    if let Some(ref env) = git_dco_env {
        append_arguments(&mut run_args, &["-e", env.as_str()]);
    }

    let passthrough_strings = super::host_runtime_passthrough_env(std::env::vars());
    for env_str in &passthrough_strings {
        run_args.push(String::from("-e"));
        run_args.push(String::from(env_str));
    }
    run_args
}

fn resolved_arguments(ctx: &LaunchContext<'_, '_>) -> Vec<String> {
    let dind_enabled = crate::runtime::docker_profile::dind_enabled(ctx.grants);
    let LaunchContext {
        dind,
        agent,
        resolved_env,
        ..
    } = ctx;
    let mut env_strings: Vec<String> = Vec::new();
    env_strings.push(format!(
        "{}={}",
        jackin_core::JACKIN_ENV_NAME,
        jackin_core::JACKIN_ENV_VALUE
    ));
    let proxy_seen = resolved_env.vars.iter().any(|(k, _)| is_proxy_env_name(k));
    let upper_existing = resolved_env
        .vars
        .iter()
        .find_map(|(k, v)| (k == NO_PROXY_UPPER).then_some(v.as_str()));
    let lower_existing = resolved_env
        .vars
        .iter()
        .find_map(|(k, v)| (k == NO_PROXY_LOWER).then_some(v.as_str()));
    for (key, value) in &resolved_env.vars {
        if jackin_core::is_reserved(key) {
            continue;
        }
        if key == NO_PROXY_UPPER || key == NO_PROXY_LOWER {
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }

    if *agent == jackin_core::Agent::Grok
        && let Some((_, value)) = resolved_env.vars.iter().find(|(k, _)| k == "XAI_API_KEY")
        && !resolved_env
            .vars
            .iter()
            .any(|(k, _)| k == "GROK_DEPLOYMENT_KEY")
    {
        env_strings.push(format!("GROK_DEPLOYMENT_KEY={value}"));
    }
    if dind_enabled && (proxy_seen || upper_existing.is_some() || lower_existing.is_some()) {
        let upper_value = upper_existing
            .or(lower_existing)
            .map_or_else(|| dind.to_string(), |v| append_no_proxy_host(v, dind));
        let lower_value = lower_existing
            .or(upper_existing)
            .map_or_else(|| dind.to_string(), |v| append_no_proxy_host(v, dind));
        env_strings.push(format!("{NO_PROXY_UPPER}={upper_value}"));
        env_strings.push(format!("{NO_PROXY_LOWER}={lower_value}"));
    }

    env_strings
}

fn service_arguments(
    ctx: &LaunchContext<'_, '_>,
    env_strings: &mut Vec<String>,
) -> (Option<jackin_diagnostics::ContainerOtlp>, Option<String>) {
    let LaunchContext {
        state,
        agent,
        grants,
        github_env,
        ..
    } = ctx;
    let network_disabled = crate::runtime::docker_profile::network_disabled(ctx.grants);
    let gh_token = state.gh_provision_outcome.token();
    push_env_if_present(env_strings, jackin_core::GH_TOKEN_ENV_NAME, gh_token);

    env_strings.push(format!(
        "{}={}",
        jackin_core::JACKIN_NETWORK_MODE_ENV_NAME,
        crate::runtime::docker_profile::network_grant_label(grants.network)
    ));
    if grants.sudo {
        env_strings.push(format!("{}=1", jackin_core::JACKIN_SUDO_ENV_NAME));
    }
    env_strings.extend(crate::runtime::docker_profile::readonly_home_env(grants));
    let capsule_otlp_safe = std::env::var("JACKIN_CAPSULE_OTLP_SAFE").as_deref() == Ok("1");
    let capsule_otlp_headers = std::env::var("JACKIN_CAPSULE_OTLP_HEADERS")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let capsule_export = capsule_export_coverage(
        CapsuleNetwork::classify(network_disabled),
        CapsuleEndpoint::classify(
            jackin_diagnostics::otlp_endpoint_configured(),
            capsule_otlp_safe,
        ),
        CapsuleAuth::classify(
            jackin_diagnostics::otlp_auth_configured(),
            capsule_otlp_headers.is_some(),
        ),
    );
    env_strings.push(format!(
        "{}={}",
        jackin_diagnostics::CapsuleExportCoverage::ENV_NAME,
        capsule_export.as_str()
    ));
    let container_otlp = (capsule_export == jackin_diagnostics::CapsuleExportCoverage::Enabled)
        .then(jackin_diagnostics::container_otlp)
        .flatten();
    if grants.network == crate::runtime::docker_profile::NetworkGrant::Allowlist {
        let github_hosts = if gh_token.is_some() {
            crate::runtime::docker_profile::github_allowlist_hosts(
                github_env
                    .get(jackin_core::GH_HOST_ENV_NAME)
                    .map(String::as_str),
            )
        } else {
            Vec::new()
        };
        let otlp_host = capsule_otlp_allowlist_host(container_otlp.as_ref());
        let allowlist = crate::runtime::docker_profile::allowlist_hosts(
            agent.slug(),
            grants,
            &github_hosts,
            otlp_host,
        );
        env_strings.push(format!(
            "{}={}",
            jackin_core::JACKIN_ALLOWED_HOSTS_ENV_NAME,
            allowlist.join(",")
        ));
        env_strings.push(format!(
            "{}={}",
            jackin_core::JACKIN_NETWORK_ENFORCEMENT_ENV_NAME,
            crate::runtime::docker_profile::network_enforcement_label(grants)
        ));
    }
    (container_otlp, capsule_otlp_headers)
}

fn emit_session_contract(ctx: &LaunchContext<'_, '_>, facts: &ProfileFacts) {
    let ProfileFacts {
        cgroup_version,
        apparmor_available,
        apparmor_layer,
    } = *facts;
    let LaunchContext {
        state,
        debug,
        agent,
        profile,
        profile_source,
        grants,
        ..
    } = ctx;
    if *debug {
        let agent_auth_mode = state.auth.for_agent(*agent).is_some();
        let session_contract = crate::runtime::docker_profile::format_session_contract(
            *profile,
            &profile_source.to_string(),
            grants,
            apparmor_available,
            apparmor_layer,
            cgroup_version,
            if agent_auth_mode {
                "provisioned"
            } else {
                "none"
            },
            state.gh_provision_outcome.token().is_some(),
        );
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!("session_contract\n{session_contract}"),
        );
    }
}

fn append_github_arguments(ctx: &LaunchContext<'_, '_>, env_strings: &mut Vec<String>) {
    let gh_token = ctx.state.gh_provision_outcome.token();
    let LaunchContext { github_env, .. } = ctx;
    push_env_if_present(env_strings, jackin_core::GITHUB_TOKEN_ENV_NAME, gh_token);
    push_env_if_present(
        env_strings,
        jackin_core::GH_HOST_ENV_NAME,
        github_env
            .get(jackin_core::GH_HOST_ENV_NAME)
            .map(String::as_str),
    );
    push_env_if_present(
        env_strings,
        jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME,
        github_env
            .get(jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME)
            .map(String::as_str),
    );
}

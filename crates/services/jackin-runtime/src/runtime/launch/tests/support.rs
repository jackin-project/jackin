// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Mutex;

pub(super) struct BailRunner {
    pub(super) reads: Mutex<Vec<String>>,
}

impl BailRunner {
    pub(super) fn named_reads(&self) -> Vec<String> {
        self.reads.lock().map_or_else(
            |poisoned| poisoned.into_inner().clone(),
            |reads| reads.clone(),
        )
    }
}

impl jackin_env::OpRunner for BailRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        if let Ok(mut reads) = self.reads.lock() {
            reads.push(reference.to_owned());
        }
        anyhow::bail!("op unavailable in test: {reference}")
    }
}

pub(super) fn bail_runner() -> BailRunner {
    BailRunner {
        reads: Mutex::new(Vec::new()),
    }
}

pub(super) fn host_env_fn(
    map: BTreeMap<String, String>,
) -> impl Fn(&str) -> Result<String, std::env::VarError> + Send + Sync {
    move |name: &str| map.get(name).cloned().ok_or(std::env::VarError::NotPresent)
}

pub(super) fn test_paths(root: &std::path::Path) -> anyhow::Result<jackin_core::JackinPaths> {
    let paths = jackin_core::JackinPaths::for_tests(root);
    paths.ensure_base_dirs()?;
    Ok(paths)
}

pub(super) fn find_state_file(
    root: &std::path::Path,
    file_name: &str,
) -> Option<std::path::PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).ok()?;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
                return Some(path);
            }
        }
    }
    None
}

pub(super) fn install_test_stubs(paths: &jackin_core::JackinPaths) {
    for agent in [
        jackin_core::Agent::Claude,
        jackin_core::Agent::Codex,
        jackin_core::Agent::Amp,
        jackin_core::Agent::Kimi,
        jackin_core::Agent::Opencode,
    ] {
        jackin_image::agent_binary::install_test_stub(paths, agent)
            .expect("install agent binary test stub");
    }
    jackin_image::capsule_binary::install_test_stub(paths).expect("install capsule test stub");
}

pub(super) const SINGLETON_CLAUDE_TOML: &str = r#"default_launch = ["claude-main"]

[accounts.test]
name = "Test"
provider = "anthropic"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.claude-main]
agent = "claude"
account = "test"

"#;

pub(super) fn write_singleton_claude_admission(paths: &jackin_core::JackinPaths) {
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, SINGLETON_CLAUDE_TOML).unwrap();
}

pub(super) fn repo_workspace(repo_dir: &std::path::Path) -> jackin_config::ResolvedWorkspace {
    jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: repo_dir.display().to_string(),
        workdir: "/workspace".to_owned(),
        mounts: vec![jackin_config::MountConfig {
            src: repo_dir.display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    }
}

pub(super) fn compat_dind_load_options() -> crate::runtime::launch::LoadOptions {
    crate::runtime::launch::LoadOptions {
        docker_profile: Some(crate::runtime::docker_profile::DockerSecurityProfile::Compat),
        ..Default::default()
    }
}

pub(super) fn launched_role_container_name(
    docker: &jackin_test_support::FakeDockerClient,
) -> String {
    docker
        .created_containers
        .borrow()
        .iter()
        .find(|(_, spec)| {
            spec.labels
                .get("jackin.kind")
                .is_some_and(|kind| kind == "role")
        })
        .map(|(name, _)| name.clone())
        .expect("expected role container creation")
}

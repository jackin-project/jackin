// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{PrepareResolvers, RoleState};
use jackin_instance_agents::{
    AgentRuntimeState, DEFAULT_ACCOUNT_ID, InstanceAuthBinding, ProvisionedAuth,
    capture_selected_account_sources, emit_agent_auth_provision,
    github_ignore_can_skip_state_prepare, slot_suffixes,
};
use jackin_instance_credentials::{
    AuthProvisionOutcome, GithubAuthContext, GithubProvisionOutcome, InstanceError,
};

use jackin_core::JackinPaths;
use jackin_manifest::RoleManifest;
use std::collections::BTreeMap;

use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};

#[cfg(unix)]
use anyhow::Context as _;

#[cfg(unix)]
struct PreparedContainerRoot {
    path: std::path::PathBuf,
    directory: std::fs::File,
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl PreparedContainerRoot {
    fn open(path: &Path) -> anyhow::Result<Self> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => validate_container_root(&metadata, path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = std::fs::DirBuilder::new();
                builder.recursive(true);
                builder.mode(0o700);
                builder.create(path).with_context(|| {
                    format!("cannot create private container root {}", path.display())
                })?;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot inspect container root {}", path.display()));
            }
        }

        let descriptor = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map(std::fs::File::from)
        .with_context(|| {
            format!(
                "cannot open container root {} without following its final component",
                path.display()
            )
        })?;
        let named = std::fs::symlink_metadata(path)
            .with_context(|| format!("cannot recheck container root {}", path.display()))?;
        let opened = descriptor
            .metadata()
            .context("cannot inspect opened container root")?;
        validate_container_root(&named, path)?;
        validate_container_root(&opened, path)?;
        anyhow::ensure!(
            named.dev() == opened.dev() && named.ino() == opened.ino(),
            "container root identity changed while it was opened: {}",
            path.display()
        );

        let lease = Self {
            path: path.to_path_buf(),
            directory: descriptor,
            device: opened.dev(),
            inode: opened.ino(),
        };
        lease.ensure_identity()?;
        Ok(lease)
    }

    fn ensure_identity(&self) -> anyhow::Result<()> {
        let opened = self
            .directory
            .metadata()
            .context("cannot recheck opened container root")?;
        let named = std::fs::symlink_metadata(&self.path)
            .with_context(|| format!("cannot recheck container root {}", self.path.display()))?;
        validate_container_root(&opened, &self.path)?;
        validate_container_root(&named, &self.path)?;
        anyhow::ensure!(
            opened.dev() == self.device
                && opened.ino() == self.inode
                && named.dev() == self.device
                && named.ino() == self.inode,
            "container root identity changed during preparation: {}",
            self.path.display()
        );
        Ok(())
    }
}

#[cfg(unix)]
fn validate_container_root(metadata: &std::fs::Metadata, path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        !metadata.file_type().is_symlink() && metadata.is_dir(),
        "container root must be a non-symlink directory: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.uid() == rustix::process::geteuid().as_raw(),
        "container root must be owned by the effective user: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.mode() & 0o022 == 0,
        "container root must not be group- or other-writable: {}",
        path.display()
    );
    Ok(())
}

#[cfg(not(unix))]
struct PreparedContainerRoot;

#[cfg(not(unix))]
impl PreparedContainerRoot {
    fn open(path: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(path)?;
        Ok(Self)
    }

    fn ensure_identity(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn prepare_container_directories(
    root_guard: &PreparedContainerRoot,
    home_dir: &Path,
    jackin_state_dir: &Path,
) -> anyhow::Result<()> {
    root_guard.ensure_identity()?;
    std::fs::create_dir_all(home_dir)?;
    std::fs::create_dir_all(jackin_state_dir)?;
    root_guard.ensure_identity()?;
    Ok(())
}

impl RoleState {
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        Self::prepare_for_agents(
            paths,
            container_name,
            manifest,
            resolvers,
            github,
            host_home,
            agent,
            &manifest.supported_agents(),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Per-agent prepare carries every per-agent + per-container input \
                  the role-materialize path needs: paths, container identity, \
                  role selectors, validated repo, agent list, env resolver, \
                  workspace. Bundling is a parallel-pass refactor."
    )]
    pub fn prepare_for_agents(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        resolvers: &PrepareResolvers<'_>,
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
        provision_agents: &[jackin_core::Agent],
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let supported = manifest.supported_agents();
        let bindings: Vec<InstanceAuthBinding> = provision_agents
            .iter()
            .copied()
            .filter(|provision_agent| supported.contains(provision_agent))
            .map(|provisioned| {
                InstanceAuthBinding::new(
                    DEFAULT_ACCOUNT_ID,
                    provisioned,
                    (resolvers.auth_modes)(provisioned),
                    (resolvers.sync_source_dirs)(provisioned),
                )
            })
            .collect();
        Self::prepare_for_bindings(
            paths,
            container_name,
            manifest,
            &bindings,
            github,
            host_home,
            agent,
        )
    }

    pub fn prepare_for_bindings(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        bindings: &[InstanceAuthBinding],
        github: &GithubAuthContext,
        host_home: &Path,
        agent: jackin_core::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let root_guard = PreparedContainerRoot::open(&root)?;
        let gh_config_dir = root.join(".config/gh");
        let home_dir = root.join("home");
        let jackin_state_dir = root.join("state");

        prepare_container_directories(&root_guard, &home_dir, &jackin_state_dir)?;

        let hosts_yml = gh_config_dir.join("hosts.yml");
        let github_context = github.clone();
        let bindings = capture_selected_account_sources(bindings, host_home, &root)?;
        root_guard.ensure_identity()?;

        let host_home_path = host_home.to_path_buf();
        let root_path = root.clone();
        let home_path = home_dir.clone();

        let suffixes = slot_suffixes(&bindings);
        let (gh_provision_outcome, auth_provisions) = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(bindings.len());
            for (binding, suffix) in bindings.iter().zip(suffixes) {
                let root = root_path.clone();
                let home_dir = home_path.clone();
                let host_home = host_home_path.clone();
                let provisioned = binding.agent;
                let mode = binding.mode;
                let binding = binding.clone();
                let handle = jackin_telemetry::spawn::thread_scoped_joined(scope, move || {
                    Self::provision_agent_auth_slot(
                        &root,
                        &home_dir,
                        &host_home,
                        &binding,
                        suffix.as_deref(),
                    )
                });
                handles.push((provisioned, mode, handle));
            }

            let gh_provision_outcome =
                if github_ignore_can_skip_state_prepare(&github_context, &hosts_yml)? {
                    jackin_diagnostics::active_timing_started(
                        jackin_diagnostics::DiagnosticStage::Credentials,
                        "role_state_prepare:github_auth",
                        Some(&github_context.mode.to_string()),
                    );
                    jackin_diagnostics::active_timing_done(
                        jackin_diagnostics::DiagnosticStage::Credentials,
                        "role_state_prepare:github_auth",
                        Some("skipped_no_state"),
                    );
                    GithubProvisionOutcome::Skipped
                } else {
                    let gh_handle = jackin_telemetry::spawn::thread_scoped_joined(scope, {
                        let hosts_yml = hosts_yml.clone();
                        let host_home = host_home_path.clone();
                        move || Self::provision_github_slot(&hosts_yml, &github_context, &host_home)
                    });
                    gh_handle
                        .join()
                        .map_err(|_| InstanceError::GithubAuthTaskPanicked)??
                };

            let mut auth_provisions = Vec::with_capacity(handles.len());
            for (agent, mode, handle) in handles {
                match handle.join() {
                    Ok(Ok(provision)) => {
                        emit_agent_auth_provision(agent, mode, Ok(provision.outcome));
                        auth_provisions.push(provision);
                    }
                    Ok(Err(error)) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::IoError),
                        );
                        return Err(error);
                    }
                    Err(_) => {
                        emit_agent_auth_provision(
                            agent,
                            mode,
                            Err(jackin_telemetry::schema::enums::ErrorType::Panic),
                        );
                        return Err(InstanceError::AuthProvisionTaskPanicked {
                            agent: agent.slug().to_owned(),
                        }
                        .into());
                    }
                }
            }

            anyhow::Ok((gh_provision_outcome, auth_provisions))
        })?;
        root_guard.ensure_identity()?;

        let mut auth = ProvisionedAuth::default();
        let mut auth_outcomes = BTreeMap::new();
        let mut selected_outcome = AuthProvisionOutcome::Skipped;

        for provision in auth_provisions {
            if provision.auth.agent == agent {
                selected_outcome = provision.outcome;
            }
            auth_outcomes.insert(provision.auth.agent, provision.outcome);
            auth.slots.insert(provision.key, provision.auth);
        }

        let (auth_mount_paths, auth_mount_leases) =
            jackin_instance_agents::admit_auth_mounts(&auth)?;

        let agent_runtime = AgentRuntimeState {
            agent,
            model: manifest.agent_model(agent).map(str::to_owned),
        };

        root_guard.ensure_identity()?;
        Ok((
            Self {
                root,
                gh_config_dir,
                gh_provision_outcome,
                agent_runtime,
                auth,
                auth_outcomes,
                auth_mount_paths,
                auth_mount_leases,
                provider_config_mounts: Vec::new(),
            },
            selected_outcome,
        ))
    }
}

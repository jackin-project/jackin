use std::path::{Path, PathBuf};

use crate::docker::CommandRunner;
use crate::paths::JackinPaths;

use super::attach::{ContainerState, inspect_container_state};

pub fn archive_container_state(
    paths: &JackinPaths,
    container_name: &str,
    output: Option<&Path>,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<PathBuf> {
    let state_dir = paths.data_dir.join(container_name);
    anyhow::ensure!(
        state_dir.is_dir(),
        "cannot archive `{container_name}` because its state directory does not exist at {}",
        state_dir.display()
    );
    ensure_archive_resources_absent(container_name, runner)?;

    let archive_path = output.map_or_else(
        || paths.archive_dir.join(format!("{container_name}.tar")),
        Path::to_path_buf,
    );
    anyhow::ensure!(
        !archive_path.exists(),
        "archive destination already exists: {}",
        archive_path.display()
    );
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(&archive_path)?;
    let mut builder = tar::Builder::new(file);
    builder.append_dir_all(container_name, &state_dir)?;
    builder.finish()?;
    Ok(archive_path)
}

fn ensure_archive_resources_absent(
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    ensure_container_missing(container_name, "role container", runner)?;
    let dind = format!("{container_name}-dind");
    ensure_container_missing(&dind, "DinD sidecar", runner)
}

fn ensure_container_missing(
    container_name: &str,
    label: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    match inspect_container_state(runner, container_name) {
        ContainerState::NotFound => Ok(()),
        ContainerState::Running | ContainerState::Stopped { .. } => anyhow::bail!(
            "cannot archive `{container_name}` because its {label} still exists; run `jackin eject {container_name}` first"
        ),
        ContainerState::InspectUnavailable(reason) => anyhow::bail!(
            "cannot archive `{container_name}` because Docker resource state could not be inspected: {reason}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{DockerResources, InstanceManifest, NewInstanceManifest};
    use crate::runtime::FakeRunner;
    use tempfile::tempdir;

    fn write_manifest(paths: &JackinPaths, container_name: &str) {
        let manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jackin-agent-smith",
            docker: DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        manifest
            .write(&paths.data_dir.join(container_name))
            .unwrap();
    }

    #[test]
    fn archive_container_state_writes_tar_under_archive_dir() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jackin-workspace-agentsmith-k7p9m2xq";
        write_manifest(&paths, container_name);
        let mut runner = FakeRunner::default();

        let archive = archive_container_state(&paths, container_name, None, &mut runner).unwrap();

        assert_eq!(
            archive,
            paths.archive_dir.join(format!("{container_name}.tar"))
        );
        assert!(archive.is_file());
        let file = std::fs::File::open(archive).unwrap();
        let mut archive = tar::Archive::new(file);
        let names: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(
            names
                .iter()
                .any(|name| name.ends_with(".jackin/instance.json")),
            "{names:?}"
        );
    }

    #[test]
    fn archive_container_state_refuses_existing_role_container() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container_name = "jackin-workspace-agentsmith-k7p9m2xq";
        write_manifest(&paths, container_name);
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);

        let err = archive_container_state(&paths, container_name, None, &mut runner).unwrap_err();

        assert!(err.to_string().contains("still exists"), "{err}");
        assert!(
            !paths
                .archive_dir
                .join(format!("{container_name}.tar"))
                .exists()
        );
    }
}

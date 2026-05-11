use std::path::{Path, PathBuf};

use crate::docker::CommandRunner;
use crate::paths::JackinPaths;

use super::attach::{ContainerState, inspect_container_state};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub container_name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

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

pub fn list_archives(paths: &JackinPaths) -> anyhow::Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    if !paths.archive_dir.exists() {
        return Ok(entries);
    }

    for entry in std::fs::read_dir(&paths.archive_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("tar") {
            continue;
        }
        let Some(container_name) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        entries.push(ArchiveEntry {
            container_name,
            size_bytes: entry.metadata()?.len(),
            path,
        });
    }
    entries.sort_by(|a, b| a.container_name.cmp(&b.container_name));
    Ok(entries)
}

pub fn remove_archive(paths: &JackinPaths, selector: &str) -> anyhow::Result<PathBuf> {
    let path = resolve_archive_path(paths, selector)?;
    std::fs::remove_file(&path)?;
    Ok(path)
}

fn resolve_archive_path(paths: &JackinPaths, selector: &str) -> anyhow::Result<PathBuf> {
    let entries = list_archives(paths)?;
    let matches: Vec<&ArchiveEntry> = entries
        .iter()
        .filter(|entry| {
            entry.container_name == selector
                || entry
                    .container_name
                    .rsplit_once('-')
                    .is_some_and(|(_, id)| id == selector)
        })
        .collect();
    match matches.as_slice() {
        [] => anyhow::bail!("archive not found for `{selector}`"),
        [entry] => Ok(entry.path.clone()),
        _ => {
            anyhow::bail!("archive selector `{selector}` is ambiguous; use the full container name")
        }
    }
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

    #[test]
    fn list_archives_returns_tar_files_sorted() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.archive_dir).unwrap();
        std::fs::write(paths.archive_dir.join("jackin-b-bbbbbbbb.tar"), "b").unwrap();
        std::fs::write(paths.archive_dir.join("jackin-a-aaaaaaaa.tar"), "aa").unwrap();
        std::fs::write(paths.archive_dir.join("ignore.txt"), "ignored").unwrap();

        let entries = list_archives(&paths).unwrap();

        let names: Vec<&str> = entries
            .iter()
            .map(|entry| entry.container_name.as_str())
            .collect();
        assert_eq!(names, ["jackin-a-aaaaaaaa", "jackin-b-bbbbbbbb"]);
        assert_eq!(entries[0].size_bytes, 2);
    }

    #[test]
    fn remove_archive_resolves_instance_id() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.archive_dir).unwrap();
        let archive = paths.archive_dir.join("jackin-a-aaaaaaaa.tar");
        std::fs::write(&archive, "archive").unwrap();

        let removed = remove_archive(&paths, "aaaaaaaa").unwrap();

        assert_eq!(removed, archive);
        assert!(!removed.exists());
    }

    #[test]
    fn remove_archive_rejects_ambiguous_instance_id() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.archive_dir).unwrap();
        std::fs::write(paths.archive_dir.join("jackin-a-aaaaaaaa.tar"), "a").unwrap();
        std::fs::write(paths.archive_dir.join("jackin-b-aaaaaaaa.tar"), "b").unwrap();

        let err = remove_archive(&paths, "aaaaaaaa").unwrap_err();

        assert!(err.to_string().contains("ambiguous"), "{err}");
    }
}

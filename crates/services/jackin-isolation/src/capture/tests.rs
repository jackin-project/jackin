// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, DockerApi, NetworkRow,
    RemoveImageOutcome,
};
use std::collections::HashMap;

struct PauseFake {
    id: String,
    paused: std::cell::Cell<bool>,
    running: std::cell::Cell<bool>,
    ops: std::cell::RefCell<Vec<String>>,
}

impl PauseFake {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            paused: std::cell::Cell::new(false),
            running: std::cell::Cell::new(true),
            ops: std::cell::RefCell::new(Vec::new()),
        }
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "fake backend methods mirror the async DockerApi trait"
)]
impl DockerApi for PauseFake {
    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn inspect_container_by_name(&self, _name: &str) -> ContainerInspection {
        ContainerInspection {
            handle: None,
            state: ContainerState::NotFound,
        }
    }
    async fn inspect_container_by_id(&self, _container: &ContainerHandle) -> ContainerState {
        if self.paused.get() {
            ContainerState::Paused
        } else if self.running.get() {
            ContainerState::Running
        } else {
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            }
        }
    }
    async fn remove_container_by_id(&self, _container: &ContainerHandle) -> anyhow::Result<()> {
        anyhow::bail!("not used by capture")
    }
    async fn list_containers(
        &self,
        _label_filters: &[&str],
        _all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        anyhow::bail!("not used by capture")
    }
    async fn create_container(
        &self,
        _name: &str,
        _spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle> {
        anyhow::bail!("not used by capture")
    }
    async fn start_container_by_id(&self, _container: &ContainerHandle) -> anyhow::Result<()> {
        anyhow::bail!("not used by capture")
    }
    async fn remove_volume(&self, _name: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by capture")
    }
    async fn create_network(
        &self,
        _name: &str,
        _labels: HashMap<String, String>,
        _internal: bool,
    ) -> anyhow::Result<()> {
        anyhow::bail!("not used by capture")
    }
    async fn remove_network(&self, _name: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by capture")
    }
    async fn list_networks(&self, _label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        anyhow::bail!("not used by capture")
    }
    async fn inspect_network(&self, _name: &str) -> anyhow::Result<Option<NetworkRow>> {
        anyhow::bail!("not used by capture")
    }
    async fn list_image_tags(&self, _reference_filter: &str) -> anyhow::Result<Vec<String>> {
        anyhow::bail!("not used by capture")
    }
    async fn remove_image(&self, _name: &str) -> anyhow::Result<RemoveImageOutcome> {
        anyhow::bail!("not used by capture")
    }
    async fn inspect_image_labels(&self, _image: &str) -> anyhow::Result<HashMap<String, String>> {
        anyhow::bail!("not used by capture")
    }
    async fn pull_image(&self, _image: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by capture")
    }
    async fn exec_capture_by_id(
        &self,
        _container: &ContainerHandle,
        _cmd: &[&str],
    ) -> anyhow::Result<String> {
        panic!("capture must never exec into the paused container");
    }
    async fn pause_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        assert_eq!(
            container.id(),
            self.id,
            "must pause the exact owned container"
        );
        self.ops.borrow_mut().push("pause".to_owned());
        self.paused.set(true);
        Ok(())
    }
    async fn unpause_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        assert_eq!(container.id(), self.id, "must resume the same container");
        self.ops.borrow_mut().push("unpause".to_owned());
        self.paused.set(false);
        Ok(())
    }
}

fn worker_src(dir: &Path) -> PathBuf {
    let src = dir.join("worker-src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(src.join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    src
}

fn capture_request(id: &str) -> CaptureRequest {
    CaptureRequest {
        run_id: "run-7".to_owned(),
        round: 0,
        generation: 1,
        parent: None,
        expected_container_id: id.to_owned(),
        copy_size_limit_bytes: 10_000_000,
        deadline_unix_secs: None,
    }
}

#[tokio::test]
async fn capture_round_trip_freezes_stable_source() {
    let dir = tempfile::TempDir::new().unwrap();
    let src = worker_src(dir.path());
    let state = dir.path().join("state");
    let staging_parent = dir.path().join("staging");
    let repo = dir.path().join("checkpoints");
    std::fs::create_dir_all(&state).unwrap();

    let docker = PauseFake::new("abc123");
    let handle = ContainerHandle::new("worker-1", "abc123").unwrap();
    let checkpoint = capture_checkpoint(
        &docker,
        &handle,
        &state,
        &src,
        &staging_parent,
        &repo,
        &capture_request("abc123"),
    )
    .await
    .unwrap();

    assert_eq!(checkpoint.run_id, "run-7");
    assert_eq!(checkpoint.generation, 1);
    assert!(!checkpoint.commit_id.is_empty());
    assert!(!checkpoint.tree_id.is_empty());
    assert!(!checkpoint.source_digest.is_empty());
    assert!(Path::new(&checkpoint.manifest_path).exists());
    assert_eq!(docker.ops.borrow().as_slice(), &["pause", "unpause"]);
    assert!(matches!(
        docker.inspect_container_by_id(&handle).await,
        ContainerState::Running
    ));
    assert!(!staging_parent.join("capture-run-7-0-1").exists());
    assert_eq!(
        std::fs::read_to_string(src.join("main.rs")).unwrap(),
        "fn main() {}\n"
    );
    let mut runner = HostGitRunner;
    let kind = runner
        .capture(
            "git",
            &[
                "--git-dir",
                &repo.to_string_lossy(),
                "cat-file",
                "-t",
                &checkpoint.commit_id,
            ],
            None,
        )
        .await
        .unwrap();
    assert_eq!(kind.trim(), "commit");

    let manifest: SourceManifest =
        serde_json::from_slice(&std::fs::read(&checkpoint.manifest_path).unwrap()).unwrap();
    assert!(matches!(
        compare_source(&src, &manifest, 10_000_000),
        SourceCompare::Match
    ));
    std::fs::write(src.join("main.rs"), "fn main() { todo!() }\n").unwrap();
    assert!(matches!(
        compare_source(&src, &manifest, 10_000_000),
        SourceCompare::Changed
    ));
}

#[tokio::test]
async fn capture_refuses_identity_mismatch_without_pausing() {
    let dir = tempfile::TempDir::new().unwrap();
    let src = worker_src(dir.path());
    let state = dir.path().join("state");
    let staging_parent = dir.path().join("staging");
    let repo = dir.path().join("checkpoints");
    std::fs::create_dir_all(&state).unwrap();

    let docker = PauseFake::new("abc123");
    let handle = ContainerHandle::new("worker-1", "abc123").unwrap();
    let err = capture_checkpoint(
        &docker,
        &handle,
        &state,
        &src,
        &staging_parent,
        &repo,
        &capture_request("different-id"),
    )
    .await
    .expect_err("ownership mismatch must refuse capture");
    assert!(format!("{err:#}").contains("does not match the recorded workflow identity"));
    assert!(
        docker.ops.borrow().is_empty(),
        "must not pause on identity mismatch"
    );
}

#[tokio::test]
async fn capture_rejects_unsafe_source_entries() {
    let dir = tempfile::TempDir::new().unwrap();
    let src = worker_src(dir.path());
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/passwd", src.join("evil")).unwrap();
    let err = digest_source_tree(&src, &[], 10_000_000).expect_err("escaping symlink must fail");
    assert!(format!("{err:#}").contains("outside the source root"));
    std::fs::remove_file(src.join("evil")).unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink("main.rs", src.join("link")).unwrap();
    let (digest, manifest) = digest_source_tree(&src, &[], 10_000_000).unwrap();
    assert!(!digest.is_empty());
    assert!(
        manifest
            .entries
            .iter()
            .any(|e| e.path == "link" && e.kind == "symlink")
    );

    std::fs::create_dir_all(src.join(".git")).unwrap();
    std::fs::write(src.join(".git").join("config"), "secret\n").unwrap();
    let (_, manifest) = digest_source_tree(&src, &[], 10_000_000).unwrap();
    assert!(!manifest.entries.iter().any(|e| e.path.starts_with(".git")));
}

#[tokio::test]
async fn capture_enforces_copy_size_limit() {
    let dir = tempfile::TempDir::new().unwrap();
    let src = worker_src(dir.path());
    let err = digest_source_tree(&src, &[], 1).expect_err("size limit must fail");
    assert!(format!("{err:#}").contains("copy-size limit"));
}

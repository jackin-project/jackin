// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn scope(id: &str) -> UsageMembershipScope {
    UsageMembershipScope {
        container_id: format!("{id:0<64}"),
        workspace_config_proof: "accepted-host-proof".to_owned(),
    }
}

#[tokio::test]
async fn scoped_membership_cache_preserves_unavailable_and_revoked() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = store_membership(&paths, &scope("a"), &UsageAccountMembershipV1::Unavailable)
        .await
        .unwrap();
    store_membership(&paths, &scope("b"), &UsageAccountMembershipV1::Revoked)
        .await
        .unwrap();
    let (read_path, memberships) = read_memberships(&paths).await.unwrap();
    assert_eq!(path, read_path);
    assert_eq!(memberships.len(), 2);
    assert!(memberships.iter().any(|entry| entry.scope == scope("a")
        && matches!(entry.membership, UsageAccountMembershipV1::Unavailable)));
    assert!(memberships.iter().any(|entry| entry.scope == scope("b")
        && matches!(entry.membership, UsageAccountMembershipV1::Revoked)));
}

#[tokio::test]
async fn scoped_membership_cache_retains_accepted_empty_projection() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let membership = super::super::tests::empty_membership();
    store_membership(&paths, &scope("c"), &membership)
        .await
        .unwrap();
    let (_, memberships) = read_memberships(&paths).await.unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(
        serde_json::to_value(&memberships[0].membership).unwrap(),
        serde_json::to_value(&membership).unwrap()
    );
}

#[tokio::test]
async fn missing_membership_cache_reads_without_creating_database() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let (path, memberships) = read_memberships(&paths).await.unwrap();
    assert!(memberships.is_empty());
    assert!(!path.exists());
}

struct CachedMembershipDocker {
    rows: std::result::Result<Vec<jackin_core::ContainerRow>, String>,
}

impl jackin_core::DockerApi for CachedMembershipDocker {
    fn controller_endpoint(&self) -> &jackin_core::ControllerEndpoint {
        static ENDPOINT: std::sync::OnceLock<jackin_core::ControllerEndpoint> = std::sync::OnceLock::new();
        ENDPOINT.get_or_init(|| jackin_core::ControllerEndpoint::Unix {
            socket: "/var/run/docker.sock".into(),
        })
    }
    async fn daemon_server_id(&self) -> Result<jackin_core::DaemonServerId> {
        panic!("unexpected Docker operation")
    }

    async fn ping(&self) -> Result<()> {
        panic!("unexpected Docker operation")
    }
    async fn inspect_container_by_name(&self, _name: &str) -> jackin_core::ContainerInspection {
        panic!("unexpected Docker operation")
    }
    async fn inspect_container_by_id(
        &self,
        _container: &jackin_core::ContainerHandle,
    ) -> jackin_core::ContainerState {
        panic!("unexpected Docker operation")
    }
    async fn container_init_pid_by_id(
        &self,
        _container: &jackin_core::ContainerHandle,
    ) -> Result<u32> {
        panic!("unexpected Docker operation")
    }
    async fn remove_container_by_id(
        &self,
        _container: &jackin_core::ContainerHandle,
    ) -> Result<()> {
        panic!("unexpected Docker operation")
    }
    async fn list_containers(
        &self,
        _label_filters: &[&str],
        _all: bool,
    ) -> Result<Vec<jackin_core::ContainerRow>> {
        assert!(_label_filters.is_empty());
        assert!(_all);
        self.rows.clone().map_err(anyhow::Error::msg)
    }
    async fn create_container(
        &self,
        _name: &str,
        _spec: jackin_core::ContainerSpec,
    ) -> Result<jackin_core::ContainerHandle> {
        panic!("unexpected Docker operation")
    }
    async fn start_container_by_id(&self, _container: &jackin_core::ContainerHandle) -> Result<()> {
        panic!("unexpected Docker operation")
    }
    async fn create_volume(
        &self,
        _name: &str,
        _labels: std::collections::HashMap<String, String>,
    ) -> Result<jackin_core::VolumeRow> {
        panic!("unexpected Docker operation")
    }

    async fn inspect_volume_by_name(&self, _name: &str) -> Result<Option<jackin_core::VolumeRow>> {
        panic!("unexpected Docker operation")
    }

    async fn remove_volume(&self, _name: &str) -> Result<()> {
        panic!("unexpected Docker operation")
    }
    async fn create_network(
        &self,
        _name: &str,
        _labels: std::collections::HashMap<String, String>,
        _internal: bool,
    ) -> Result<jackin_core::NetworkId> {
        panic!("unexpected Docker operation")
    }
    async fn remove_network_by_id(&self, _id: &jackin_core::NetworkId) -> Result<()> {
        panic!("unexpected Docker operation")
    }
    async fn list_networks(&self, _label_filters: &[&str]) -> Result<Vec<jackin_core::NetworkRow>> {
        panic!("unexpected Docker operation")
    }
    async fn inspect_network_by_name(
        &self,
        _name: &str,
    ) -> Result<Option<jackin_core::NetworkRow>> {
        panic!("unexpected Docker operation")
    }
    async fn inspect_network_by_id(
        &self,
        _id: &jackin_core::NetworkId,
    ) -> Result<Option<jackin_core::NetworkRow>> {
        panic!("unexpected Docker operation")
    }

    async fn list_image_tags(&self, _reference_filter: &str) -> Result<Vec<String>> {
        panic!("unexpected Docker operation")
    }
    async fn remove_image(&self, _name: &str) -> Result<jackin_core::RemoveImageOutcome> {
        panic!("unexpected Docker operation")
    }
    async fn inspect_image_labels(
        &self,
        _image: &str,
    ) -> Result<std::collections::HashMap<String, String>> {
        panic!("unexpected Docker operation")
    }
    async fn pull_image(&self, _image: &str) -> Result<()> {
        panic!("unexpected Docker operation")
    }
    async fn exec_capture_by_id(
        &self,
        _container: &jackin_core::ContainerHandle,
        _cmd: &[&str],
    ) -> Result<String> {
        panic!("unexpected Docker operation")
    }
}

#[tokio::test]
async fn actual_cache_collector_revalidates_without_writing() {
    use super::super::validate_cached_memberships_with_docker;
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let membership = super::super::tests::empty_membership();
    let path = store_membership(&paths, &scope("d"), &membership)
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    for (rows, revoked) in [
        (Err("Docker unavailable".to_owned()), false),
        (Ok(vec![]), true),
        (
            Ok(vec![jackin_core::ContainerRow {
                name: "renamed-container".to_owned(),
                id: scope("d").container_id,
                labels: std::collections::HashMap::new(),
            }]),
            false,
        ),
        (
            Ok(vec![jackin_core::ContainerRow {
                name: "original-display-name".to_owned(),
                id: scope("e").container_id,
                labels: std::collections::HashMap::new(),
            }]),
            true,
        ),
    ] {
        let (_, entries) = read_memberships(&paths).await.unwrap();
        let entries = validate_cached_memberships_with_docker(
            &paths,
            &CachedMembershipDocker { rows },
            entries,
        )
        .await
        .unwrap();
        if revoked {
            assert!(matches!(
                entries[0].membership,
                UsageAccountMembershipV1::Revoked
            ));
        } else {
            assert!(matches!(
                entries[0].membership,
                UsageAccountMembershipV1::Unavailable
            ));
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

//! Tests for `instance/manifest`.
use super::*;
use tempfile::tempdir;

fn sample_manifest() -> InstanceManifest {
    InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-workspace-agent",
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "org/agent",
        role_display_name: "Agent",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/role.git",
        role_source_ref: Some("main"),
        image_tag: "jk_org_agent",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-workspace-agent".to_owned(),
            dind_container: Some("jk-k7p9m2xq-workspace-agent-dind".to_owned()),
            network: "jk-k7p9m2xq-workspace-agent-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-workspace-agent-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

#[test]
fn writes_manifest_under_jackin_state_dir() {
    let temp = tempdir().unwrap();
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-workspace-agent",
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "org/agent",
        role_display_name: "Agent",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/role.git",
        role_source_ref: Some("main"),
        image_tag: "jk_org_agent",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-workspace-agent".to_owned(),
            dind_container: Some("jk-k7p9m2xq-workspace-agent-dind".to_owned()),
            network: "jk-k7p9m2xq-workspace-agent-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-workspace-agent-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.mark_status(InstanceStatus::Running);

    manifest.write(temp.path()).unwrap();

    let body = std::fs::read_to_string(temp.path().join(".jackin/instance.json")).unwrap();
    assert!(body.contains(r#""version": 1"#));
    assert!(body.contains(r#""status": "running""#));
    assert!(body.contains(r#""role_key": "org/agent""#));
}

#[test]
fn index_rebuilds_from_manifests_and_filters_by_query() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path();
    let manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-workspace-agent",
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "org/agent",
        role_display_name: "Agent",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/role.git",
        role_source_ref: Some("main"),
        image_tag: "jk_org_agent",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-workspace-agent".to_owned(),
            dind_container: Some("jk-k7p9m2xq-workspace-agent-dind".to_owned()),
            network: "jk-k7p9m2xq-workspace-agent-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-workspace-agent-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest
        .write(&data_dir.join("jk-k7p9m2xq-workspace-agent"))
        .unwrap();

    let matches = InstanceIndex::matching_manifests(
        data_dir,
        InstanceQuery {
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            role_key: Some("org/agent"),
            agent_runtime: Some(Agent::Claude),
        },
    )
    .unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].container_base, "jk-k7p9m2xq-workspace-agent");
    assert!(data_dir.join(INSTANCE_INDEX_FILE).exists());
}

#[test]
fn index_update_replaces_existing_entry() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path();
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-workspace-agent",
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "org/agent",
        role_display_name: "Agent",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/role.git",
        role_source_ref: Some("main"),
        image_tag: "jk_org_agent",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-workspace-agent".to_owned(),
            dind_container: Some("jk-k7p9m2xq-workspace-agent-dind".to_owned()),
            network: "jk-k7p9m2xq-workspace-agent-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-workspace-agent-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });

    InstanceIndex::update_manifest(data_dir, &manifest).unwrap();
    manifest.mark_status(InstanceStatus::Running);
    InstanceIndex::update_manifest(data_dir, &manifest).unwrap();

    let index = InstanceIndex::read(data_dir).unwrap();
    assert_eq!(index.instances.len(), 1);
    assert_eq!(index.instances[0].status, InstanceStatus::Running);
}

#[test]
fn mark_many_purged_empty_slice_is_noop() {
    let data_dir = tempdir().unwrap();
    InstanceIndex::mark_many_purged(data_dir.path(), &[]).unwrap();
    // Empty slice must not create an index file — short-circuits
    // before any read or write.
    assert!(!data_dir.path().join(INSTANCE_INDEX_FILE).exists());
}

#[test]
fn mark_many_purged_tombstones_absent_and_present_in_one_pass() {
    let data_dir = tempdir().unwrap();
    let manifest_a = sample_manifest();
    let state_a = data_dir.path().join(manifest_a.container_base.as_str());
    manifest_a.write(&state_a).unwrap();
    InstanceIndex::update_manifest(data_dir.path(), &manifest_a).unwrap();

    // Container B has a manifest on disk but no index entry —
    // simulates a manifest written before an index update.
    let manifest_b_base = "jk-orphan01-workspace-agent";
    let manifest_b = InstanceManifest {
        container_base: manifest_b_base.to_owned(),
        ..manifest_a.clone()
    };
    let state_b = data_dir.path().join(manifest_b_base);
    manifest_b.write(&state_b).unwrap();

    InstanceIndex::mark_many_purged(
        data_dir.path(),
        &[manifest_a.container_base.as_str(), manifest_b_base],
    )
    .unwrap();

    let index = InstanceIndex::read(data_dir.path()).unwrap();
    assert_eq!(index.instances.len(), 2);
    assert!(
        index
            .instances
            .iter()
            .all(|e| e.status == InstanceStatus::Purged)
    );
}

#[test]
fn mark_many_purged_is_idempotent() {
    let data_dir = tempdir().unwrap();
    let manifest = sample_manifest();
    let state_dir = data_dir.path().join(manifest.container_base.as_str());
    manifest.write(&state_dir).unwrap();
    InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

    InstanceIndex::mark_many_purged(data_dir.path(), &[manifest.container_base.as_str()]).unwrap();
    // Second call must not duplicate the entry or change the
    // status. Operator running `purge` twice (e.g. retry after a
    // partial failure) sees a stable tombstone.
    InstanceIndex::mark_many_purged(data_dir.path(), &[manifest.container_base.as_str()]).unwrap();

    let index = InstanceIndex::read(data_dir.path()).unwrap();
    assert_eq!(index.instances.len(), 1);
    assert_eq!(index.instances[0].status, InstanceStatus::Purged);
}

#[test]
fn remove_many_empty_slice_is_noop() {
    let data_dir = tempdir().unwrap();
    InstanceIndex::remove_many(data_dir.path(), &[]).unwrap();
    assert!(!data_dir.path().join("instances.json").exists());
}

#[test]
fn remove_many_deletes_only_named_entries() {
    let data_dir = tempdir().unwrap();
    let manifest = sample_manifest();
    let other_base = "jk-a1b2c3d4-other-agent";
    let mut other = manifest.clone();
    other.container_base = other_base.to_owned();
    InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();
    InstanceIndex::update_manifest(data_dir.path(), &other).unwrap();

    InstanceIndex::remove_many(data_dir.path(), &[manifest.container_base.as_str()]).unwrap();

    let index = InstanceIndex::read_or_rebuild(data_dir.path()).unwrap();
    assert_eq!(index.instances.len(), 1);
    assert_eq!(index.instances[0].container_base, other_base);
}

#[test]
fn remove_many_with_absent_name_is_noop() {
    let data_dir = tempdir().unwrap();
    let manifest = sample_manifest();
    InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

    InstanceIndex::remove_many(data_dir.path(), &["jk-nothere-agent"]).unwrap();

    let index = InstanceIndex::read_or_rebuild(data_dir.path()).unwrap();
    assert_eq!(index.instances.len(), 1);
    assert_eq!(index.instances[0].container_base, manifest.container_base);
}

#[test]
fn remove_many_with_duplicate_names_removes_once() {
    let data_dir = tempdir().unwrap();
    let manifest = sample_manifest();
    InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

    InstanceIndex::remove_many(
        data_dir.path(),
        &[
            manifest.container_base.as_str(),
            manifest.container_base.as_str(),
        ],
    )
    .unwrap();

    let index = InstanceIndex::read_or_rebuild(data_dir.path()).unwrap();
    assert!(index.instances.is_empty());
}

#[test]
fn instance_manifest_write_replaces_partial_file() {
    // Simulate a previous crash that left a half-written JSON.
    // Atomic write must replace it cleanly; a regression to a
    // direct `std::fs::write` would either preserve the truncated
    // content on a short write or interleave bytes.
    let temp = tempdir().unwrap();
    let state_dir = temp.path();
    std::fs::create_dir_all(state_dir.join(".jackin")).unwrap();
    std::fs::write(state_dir.join(".jackin/instance.json"), b"{ partial").unwrap();
    sample_manifest().write(state_dir).unwrap();
    let body = std::fs::read_to_string(state_dir.join(".jackin/instance.json")).unwrap();
    assert!(body.contains(r#""version": 1"#));
    assert!(!body.contains("partial"));
}

#[test]
fn index_write_leaves_no_temp_file_on_success() {
    // Atomic write is `tempfile + rename`; on success the temp must
    // be gone. A regression that wrote in-place would leave the
    // temp behind (or worse, never rename) — assert the data dir
    // contains exactly the canonical file.
    let temp = tempdir().unwrap();
    let data_dir = temp.path();
    InstanceIndex::update_manifest(data_dir, &sample_manifest()).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(data_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("instances"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            INSTANCE_INDEX_FILE.to_owned(),
            INSTANCE_INDEX_LOCK_FILE.to_owned(),
        ]
    );
}

#[test]
fn update_manifest_concurrent_writes_serialize_via_lock() {
    // Two threads racing `update_manifest` for *different* manifests
    // must both end up in the index. A regression that drops the
    // index flock would lose one of the two entries.
    let temp = tempdir().unwrap();
    let data_dir = temp.path().to_path_buf();
    let a = sample_manifest();
    let mut b = sample_manifest();
    b.container_base = "jackin-other-7p9m2xqk".to_owned();
    b.instance_id = "7p9m2xqk".to_owned();

    let d1 = data_dir.clone();
    let h1 = std::thread::spawn(move || InstanceIndex::update_manifest(&d1, &a).unwrap());
    let d2 = data_dir.clone();
    let h2 = std::thread::spawn(move || InstanceIndex::update_manifest(&d2, &b).unwrap());
    h1.join().unwrap();
    h2.join().unwrap();

    let index = InstanceIndex::read(&data_dir).unwrap();
    assert_eq!(index.instances.len(), 2);
}

#[test]
fn host_path_fingerprint_differs_for_distinct_canonical_paths() {
    // Two existing dirs with distinct canonical paths must yield
    // distinct fingerprints (catches a regression that hashes the
    // raw input even when canonicalize succeeds).
    let temp = tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let fa = host_path_fingerprint(&a.display().to_string());
    let fb = host_path_fingerprint(&b.display().to_string());
    assert_ne!(fa, fb);
    assert!(fa.starts_with("sha256:"));
}

#[test]
fn index_mark_purged_retains_tombstone_after_state_removal() {
    let data_dir = tempdir().unwrap();
    let manifest = sample_manifest();
    let state_dir = data_dir.path().join(manifest.container_base.as_str());
    manifest.write(&state_dir).unwrap();
    InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

    InstanceIndex::mark_purged(data_dir.path(), &manifest.container_base).unwrap();
    std::fs::remove_dir_all(&state_dir).unwrap();

    let index = InstanceIndex::read(data_dir.path()).unwrap();
    assert_eq!(index.instances.len(), 1);
    assert_eq!(index.instances[0].container_base, manifest.container_base);
    assert_eq!(index.instances[0].status, InstanceStatus::Purged);
}

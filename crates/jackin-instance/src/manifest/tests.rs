// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `instance/manifest`.
use super::*;
use tempfile::tempdir;

#[test]
fn docker_identity_roundtrips_without_inferring_ownership_from_names() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let mut manifest = sample_manifest();
    manifest.write(temp.path())?;
    assert_eq!(InstanceManifest::read(temp.path())?.docker_identity, None);
    manifest.docker_identity = Some(DockerIdentity {
        role_container_id: "original-role-id".to_owned(),
        dind_container_id: Some("original-sidecar-id".to_owned()),
        network_id: Some(jackin_core::NetworkId::parse(&"a".repeat(64)).unwrap()),
    });
    manifest.write(temp.path())?;
    assert_eq!(InstanceManifest::read(temp.path())?, manifest);
    manifest.docker_identity.as_mut().unwrap().dind_container_id = None;
    manifest.docker.dind_container = None;
    manifest.docker.certs_volume = None;
    manifest.write(temp.path())?;
    assert_eq!(InstanceManifest::read(temp.path())?, manifest);
    Ok(())
}

#[test]
fn manifest_v4_backend_roundtrips_and_omitted_backend_deserializes() {
    let manifest = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    // A v4 apple-container manifest survives a validated serialize -> deserialize round trip.
    let json = serde_json::to_string(&manifest).unwrap();
    assert_eq!(
        InstanceManifest::parse_and_validate(json.as_bytes(), Path::new("instance.json")).unwrap(),
        manifest
    );
    assert!(matches!(
        manifest.backend,
        Some(BackendResources::AppleContainer(_))
    ));

    // `backend` is optional for v4 Docker manifests.
    let mut obj = serde_json::to_value(&manifest)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    obj.remove("backend");
    let legacy: InstanceManifest = serde_json::from_value(serde_json::Value::Object(obj)).unwrap();
    assert_eq!(legacy.backend, None);
}

#[test]
fn apple_authority_allows_duplicate_names_with_distinct_process_generations() {
    let first = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let recreated = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.44444444-4444-4444-8444-444444444444",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    assert_eq!(first.container_base, recreated.container_base);
    assert_ne!(
        apple_authority(&first).resource_handle,
        apple_authority(&recreated).resource_handle
    );
    assert_eq!(
        validate_manifest(&first).unwrap().container_base,
        validate_manifest(&recreated).unwrap().container_base
    );
}

#[test]
fn apple_authority_keeps_same_name_foreign_owner_recreation_distinct() {
    let original = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let foreign_owner = sample_apple_manifest(
        "55555555-5555-4555-8555-555555555555",
        "jackin-owner-v1.66666666-6666-4666-8666-666666666666.77777777-7777-4777-8777-777777777777",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    assert_eq!(original.container_base, foreign_owner.container_base);
    assert_ne!(apple_authority(&original), apple_authority(&foreign_owner));
    assert_eq!(validate_manifest(&original).unwrap(), original);
    assert_eq!(validate_manifest(&foreign_owner).unwrap(), foreign_owner);
}

#[test]
fn apple_authority_preserves_old_owner_generation_for_exact_runtime_matching() {
    let previous_generation = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let current_generation = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.44444444-4444-4444-8444-444444444444",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    assert_eq!(
        apple_authority(&previous_generation).owner_id,
        apple_authority(&current_generation).owner_id
    );
    assert_ne!(
        apple_authority(&previous_generation).resource_handle,
        apple_authority(&current_generation).resource_handle
    );
    assert_eq!(
        validate_manifest(&previous_generation).unwrap(),
        previous_generation
    );
}

#[test]
fn apple_authority_requires_handle_and_rejects_foreign_handle_format() {
    let manifest = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let mut value = serde_json::to_value(&manifest).unwrap();
    let apple = value["backend"]["apple_container"].as_object_mut().unwrap();
    apple.remove("authority");
    let error = validate_value(value).unwrap_err();
    assert!(format!("{error:#}").contains("missing field `authority`"));

    let mut value = serde_json::to_value(&manifest).unwrap();
    let authority = value["backend"]["apple_container"]["authority"]
        .as_object_mut()
        .unwrap();
    authority.remove("resource_handle");
    let error = validate_value(value).unwrap_err();
    assert!(format!("{error:#}").contains("missing field `resource_handle`"));

    let mut foreign_handle = manifest.clone();
    apple_authority_mut(&mut foreign_handle).resource_handle =
        "docker://jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333".to_owned();
    assert!(
        validate_manifest(&foreign_handle)
            .unwrap_err()
            .to_string()
            .contains("invalid Apple authority resource handle")
    );

    let mut noncanonical_generation = manifest;
    apple_authority_mut(&mut noncanonical_generation).resource_handle =
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-33333333333A"
            .to_owned();
    assert!(
        validate_manifest(&noncanonical_generation)
            .unwrap_err()
            .to_string()
            .contains("invalid Apple authority resource handle")
    );
}

#[test]
fn apple_authority_rejects_noncanonical_owner_uuid_and_scope_fingerprint() {
    let valid = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let mut invalid_owner = valid.clone();
    apple_authority_mut(&mut invalid_owner).owner_id =
        "11111111-1111-4111-8111-11111111111A".to_owned();
    assert!(
        validate_manifest(&invalid_owner)
            .unwrap_err()
            .to_string()
            .contains("invalid Apple authority owner UUID")
    );

    for fingerprint in [
        "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0",
        "sha512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let mut invalid_scope = valid.clone();
        apple_authority_mut(&mut invalid_scope).scope_fingerprint = fingerprint.to_owned();
        assert!(
            validate_manifest(&invalid_scope)
                .unwrap_err()
                .to_string()
                .contains("invalid Apple authority scope fingerprint")
        );
    }
}

#[test]
fn apple_backend_name_must_match_manifest_container_base() {
    let mut manifest = sample_apple_manifest(
        "11111111-1111-4111-8111-111111111111",
        "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    if let Some(BackendResources::AppleContainer(apple)) = manifest.backend.as_mut() {
        apple.container_name = "jackin-recreated-with-same-label".to_owned();
    }
    assert!(
        validate_manifest(&manifest)
            .unwrap_err()
            .to_string()
            .contains("Apple backend display name does not match instance manifest container base")
    );
}

#[test]
fn admitted_instances_empty_is_explicit_and_validate_tabs() {
    let mut manifest = sample_manifest();
    assert!(manifest.admitted_instances.is_empty());
    assert!(!manifest.admits_instance("claude-work"));

    manifest.set_admitted_instances([
        AdmittedInstance::new("claude-work", Agent::Claude, "work"),
        AdmittedInstance::from(&jackin_config::ResolvedInstance {
            config_id: "claude-personal".to_owned(),
            agent: Agent::Claude,
            account_id: "personal".to_owned(),
            model: None,
            base_url: None,
            xdg_roots: None,
            label: "Claude · Personal".to_owned(),
            synthesized: true,
        }),
    ]);
    assert!(manifest.admits_instance("claude-work"));
    assert!(manifest.admits_instance("claude-personal"));
    assert!(!manifest.admits_instance("codex-work"));
    assert_eq!(manifest.account_for_instance("claude-work"), Some("work"));
    assert_eq!(
        manifest.account_for_instance("claude-personal"),
        Some("personal")
    );
    assert_eq!(manifest.account_for_instance("codex-work"), None);
    assert_eq!(
        manifest.agent_for_instance("claude-work"),
        Some(Agent::Claude)
    );
    assert_eq!(manifest.agent_for_instance("codex-work"), None);

    // Admission survives a serialize -> deserialize round trip.
    let json = serde_json::to_string(&manifest).unwrap();
    assert_eq!(
        serde_json::from_str::<InstanceManifest>(&json).unwrap(),
        manifest
    );

    // Admission is required by the v4 manifest schema; omission is malformed.
    let mut obj = serde_json::to_value(&manifest)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    obj.remove("admitted_instances");
    let error =
        serde_json::from_value::<InstanceManifest>(serde_json::Value::Object(obj)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing field `admitted_instances`")
    );
}

#[test]
fn admitted_manifest_membership_does_not_expand_from_unrelated_registration() {
    let mut manifest = sample_manifest();
    manifest.set_admitted_instances([AdmittedInstance::new("claude-work", Agent::Claude, "work")]);
    let admitted_before = manifest.admitted_instances.clone();

    // A newly registered account/configuration exists outside this immutable
    // launch admission set until an explicit recreate/update path writes a new
    // manifest.
    assert!(!manifest.admits_instance("claude-personal"));
    assert_eq!(manifest.admitted_instances, admitted_before);
}

#[test]
fn registration_state_is_visible_without_relabeling_admitted_identity() {
    let mut manifest = sample_manifest();
    manifest.set_admitted_instances([AdmittedInstance::new("claude-work", Agent::Claude, "work")]);
    assert!(manifest.mark_registration_state("claude-work", RegistrationState::Disabled));
    assert_eq!(
        manifest.registration_state_for_instance("claude-work"),
        Some(RegistrationState::Disabled)
    );
    assert_eq!(manifest.account_for_instance("claude-work"), Some("work"));
    assert!(manifest.admits_instance("claude-work"));
    assert_eq!(RegistrationState::Removed.label(), "removed");

    let restored: InstanceManifest =
        serde_json::from_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
    assert_eq!(restored.admitted_instances, manifest.admitted_instances);
}

#[test]
fn manifest_read_rejects_all_pre_v4_versions() {
    for version in [2, 3] {
        let temp = tempdir().unwrap();
        let state_dir = temp.path();
        std::fs::create_dir_all(state_dir.join(".jackin")).unwrap();
        let mut value = serde_json::to_value(sample_apple_manifest(
            "11111111-1111-4111-8111-111111111111",
            "jackin-owner-v1.22222222-2222-4222-8222-222222222222.33333333-3333-4333-8333-333333333333",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ))
        .unwrap();
        value["version"] = serde_json::json!(version);
        // A pre-v4 Apple manifest has no authority binding. Its version still
        // fails closed before any attempt to infer authority from its name.
        value["backend"]["apple_container"]
            .as_object_mut()
            .unwrap()
            .remove("authority");
        std::fs::write(
            state_dir.join(".jackin/instance.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();

        let error = InstanceManifest::read(state_dir).unwrap_err();
        assert!(
            format!("{error:#}")
                .contains(&format!("unsupported instance manifest version {version}"))
        );
    }
}

#[test]
fn manifest_read_rejects_malformed_admission_records() {
    let temp = tempdir().unwrap();
    let state_dir = temp.path();
    std::fs::create_dir_all(state_dir.join(".jackin")).unwrap();
    let mut value = serde_json::to_value(sample_manifest()).unwrap();
    value["admitted_instances"] = serde_json::json!([{
        "config_id": "claude-work",
        "account_id": "work"
    }]);
    std::fs::write(
        state_dir.join(".jackin/instance.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();

    let error = InstanceManifest::read_optional(state_dir).unwrap_err();
    assert!(format!("{error:#}").contains("missing field `agent`"));
}

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

fn sample_apple_manifest(
    owner_id: &str,
    resource_handle: &str,
    scope_fingerprint: &str,
) -> InstanceManifest {
    let mut manifest = sample_manifest();
    manifest.backend = Some(BackendResources::AppleContainer(AppleContainerResources {
        container_name: manifest.container_base.clone(),
        role_image_ref: "img".to_owned(),
        inner_docker_enabled: false,
        authority: AppleAuthorityBinding {
            owner_id: owner_id.to_owned(),
            resource_handle: resource_handle.to_owned(),
            scope_fingerprint: scope_fingerprint.to_owned(),
        },
    }));
    manifest
}

fn apple_authority(manifest: &InstanceManifest) -> &AppleAuthorityBinding {
    match manifest.backend.as_ref().unwrap() {
        BackendResources::AppleContainer(apple) => &apple.authority,
        BackendResources::Docker(_) => panic!("expected Apple backend"),
    }
}

fn apple_authority_mut(manifest: &mut InstanceManifest) -> &mut AppleAuthorityBinding {
    match manifest.backend.as_mut().unwrap() {
        BackendResources::AppleContainer(apple) => &mut apple.authority,
        BackendResources::Docker(_) => panic!("expected Apple backend"),
    }
}

fn validate_manifest(manifest: &InstanceManifest) -> anyhow::Result<InstanceManifest> {
    let bytes = serde_json::to_vec(manifest)?;
    InstanceManifest::parse_and_validate(&bytes, Path::new("instance.json"))
}

fn validate_value(value: serde_json::Value) -> anyhow::Result<InstanceManifest> {
    let bytes = serde_json::to_vec(&value)?;
    InstanceManifest::parse_and_validate(&bytes, Path::new("instance.json"))
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
    assert!(body.contains(r#""version": 4"#));
    assert!(body.contains(r#""status": "running""#));
    assert!(body.contains(r#""role_key": "org/agent""#));
}

#[cfg(unix)]
#[test]
fn manifest_write_restricts_state_directories_and_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let state_dir = temp.path().join("instance-state");
    sample_manifest().write(&state_dir).unwrap();

    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&state_dir), 0o700, "instance state dir must be 0700");
    assert_eq!(
        mode(&state_dir.join(".jackin")),
        0o700,
        "manifest dir must be 0700"
    );
    assert_eq!(
        mode(&state_dir.join(".jackin/instance.json")),
        0o600,
        "manifest file must be 0600"
    );
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
    assert!(body.contains(r#""version": 4"#));
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

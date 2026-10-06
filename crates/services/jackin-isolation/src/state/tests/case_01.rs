// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn read_records_returns_empty_when_file_missing() {
    let dir = TempDir::new().unwrap();
    assert!(read_records(dir.path()).unwrap().is_empty());
}

#[test]
fn write_then_read_roundtrip_preserves_record() {
    let dir = TempDir::new().unwrap();
    let rec = sample_record();
    write_records(dir.path(), std::slice::from_ref(&rec)).unwrap();
    let read = read_records(dir.path()).unwrap();
    assert_eq!(read, vec![rec]);
}

#[test]
fn write_emits_version_2_envelope() {
    let dir = TempDir::new().unwrap();
    write_records(dir.path(), &[sample_record()]).unwrap();
    let raw = std::fs::read_to_string(isolation_file_path(dir.path())).unwrap();
    assert!(raw.contains("\"version\": 2"));
    assert!(raw.contains("\"records\""));
}

#[test]
fn read_record_returns_none_when_missing() {
    let dir = TempDir::new().unwrap();
    write_records(dir.path(), &[sample_record()]).unwrap();
    assert!(read_record(dir.path(), "/nope").unwrap().is_none());
}

#[test]
fn read_record_returns_match() {
    let dir = TempDir::new().unwrap();
    write_records(dir.path(), &[sample_record()]).unwrap();
    let r = read_record(dir.path(), "/workspace/jackin").unwrap();
    assert!(r.is_some());
}

#[test]
fn upsert_replaces_existing_by_dst() {
    let dir = TempDir::new().unwrap();
    let mut rec = sample_record();
    write_records(dir.path(), std::slice::from_ref(&rec)).unwrap();
    rec.base_commit = "cafe".into();
    upsert_record(dir.path(), rec).unwrap();
    let all = read_records(dir.path()).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].base_commit, "cafe");
}

#[test]
fn upsert_appends_when_dst_new() {
    let dir = TempDir::new().unwrap();
    write_records(dir.path(), &[sample_record()]).unwrap();
    let mut other = sample_record();
    other.mount_dst = "/workspace/docs".into();
    upsert_record(dir.path(), other).unwrap();
    assert_eq!(read_records(dir.path()).unwrap().len(), 2);
}

#[test]
fn remove_record_drops_match_and_keeps_others() {
    let dir = TempDir::new().unwrap();
    let mut other = sample_record();
    other.mount_dst = "/workspace/docs".into();
    write_records(dir.path(), &[sample_record(), other.clone()]).unwrap();
    remove_record(dir.path(), "/workspace/jackin").unwrap();
    let all = read_records(dir.path()).unwrap();
    assert_eq!(all, vec![other]);
}

#[test]
fn remove_record_is_noop_when_missing() {
    let dir = TempDir::new().unwrap();
    write_records(dir.path(), &[sample_record()]).unwrap();
    remove_record(dir.path(), "/nope").unwrap();
    assert_eq!(read_records(dir.path()).unwrap().len(), 1);
}

#[test]
fn unsupported_version_errors_clearly() {
    let dir = TempDir::new().unwrap();
    let path = isolation_file_path(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, br#"{"version": 99, "records": []}"#).unwrap();
    let err = read_records(dir.path()).unwrap_err();
    assert!(
        err.to_string()
            .contains("unsupported isolation.json version 99")
    );
}

#[test]
fn list_records_for_workspace_walks_all_container_dirs() {
    let data = TempDir::new().unwrap();
    // Container A: workspace=jackin
    let a = data.path().join("jk-a1b2c3d4-thearchitect");
    std::fs::create_dir_all(&a).unwrap();
    let mut rec_a = sample_record();
    rec_a.container_name = "jk-a1b2c3d4-thearchitect".into();
    write_records(&a, std::slice::from_ref(&rec_a)).unwrap();
    // Container B: workspace=jackin
    let b = data.path().join("jk-k7p9m2xq-thebuilder");
    std::fs::create_dir_all(&b).unwrap();
    let mut rec_b = sample_record();
    rec_b.container_name = "jk-k7p9m2xq-thebuilder".into();
    rec_b.scratch_branch = "jackin/scratch/the-builder".into();
    write_records(&b, std::slice::from_ref(&rec_b)).unwrap();
    // Container C: workspace=docs (must be skipped when filtering by jackin)
    let c = data.path().join("jk-b2c3d4e5-docwriter");
    std::fs::create_dir_all(&c).unwrap();
    let mut rec_c = sample_record();
    rec_c.workspace_name = Some(wn("docs"));
    rec_c.container_name = "jk-b2c3d4e5-docwriter".into();
    write_records(&c, &[rec_c]).unwrap();

    let mut found = list_records_for_workspace(data.path(), &wn("jackin")).unwrap();
    found.sort_by(|x, y| x.container_name.cmp(&y.container_name));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0], rec_a);
    assert_eq!(found[1], rec_b);
}

#[test]
fn list_records_for_workspace_returns_empty_when_data_dir_missing() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("nope");
    let result = list_records_for_workspace(&missing, &wn("jackin")).unwrap();
    assert!(result.is_empty());
}

#[test]
fn list_records_for_workspace_propagates_invalid_parent_state() {
    let dir = TempDir::new().unwrap();
    let invalid_parent = dir.path().join("regular-file");
    std::fs::write(&invalid_parent, b"not a directory").unwrap();
    let error = list_records_for_workspace(&invalid_parent.join("data"), &wn("jackin"))
        .expect_err("invalid parent is unknown inventory, not absent inventory");
    assert!(error.to_string().contains("read data dir"));
}

#[test]
fn list_records_for_workspace_ignores_non_jackin_dirs() {
    let data = TempDir::new().unwrap();
    let other = data.path().join("not-a-jackin-capsule");
    std::fs::create_dir_all(&other).unwrap();
    let mut rec = sample_record();
    rec.container_name = "not-a-jackin-capsule".into();
    write_records(&other, &[rec]).unwrap();
    let result = list_records_for_workspace(data.path(), &wn("jackin")).unwrap();
    assert!(result.is_empty());
}

#[test]
fn v1_identity_migrates_from_independent_manifest_without_config() {
    for identity in [Some("saved-stem"), None] {
        let temp = TempDir::new().unwrap();
        let state = temp.path().join("jk-a1b2c3d4-role");
        std::fs::create_dir_all(&state).unwrap();
        let original = write_v1_fixture(&state, identity, "/display/label");
        let records = read_records(&state).unwrap();
        assert_eq!(
            records[0]
                .workspace_name
                .as_ref()
                .map(WorkspaceName::as_str),
            identity
        );
        assert_eq!(
            std::fs::read(isolation_file_path(&state)).unwrap(),
            original
        );
        assert_eq!(migrate_records(&state).unwrap(), records);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(isolation_file_path(&state)).unwrap()).unwrap();
        assert_eq!(raw["version"], 2);
        assert!(raw["records"][0].get("workspace").is_none());
        assert_eq!(read_records(&state).unwrap(), records);
    }
}

#[test]
fn readonly_inventory_preserves_admitted_historical_sibling_when_later_state_is_invalid() {
    let temp = TempDir::new().unwrap();
    let good = temp.path().join("jk-a1b2c3d4-role");
    let bad = temp.path().join("jk-b1b2c3d4-role");
    std::fs::create_dir_all(&good).unwrap();
    std::fs::create_dir_all(bad.join(".jackin")).unwrap();
    let original = write_v1_fixture(&good, Some("saved-stem"), "/display/label");
    let invalid = b"invalid later state";
    std::fs::write(isolation_file_path(&bad), invalid).unwrap();
    let manifest_bytes = std::fs::read(good.join(".jackin/instance.json")).unwrap();
    let names_before = std::fs::read_dir(good.join(".jackin"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        read_records(&good).unwrap()[0].workspace_name,
        Some(wn("saved-stem"))
    );
    let _error = list_records_for_workspace(temp.path(), &wn("saved-stem")).unwrap_err();
    assert_eq!(std::fs::read(isolation_file_path(&good)).unwrap(), original);
    assert_eq!(std::fs::read(isolation_file_path(&bad)).unwrap(), invalid);
    assert_eq!(
        std::fs::read(good.join(".jackin/instance.json")).unwrap(),
        manifest_bytes
    );
    let names = std::fs::read_dir(good.join(".jackin"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, names_before);
}

#[test]
fn historical_mutation_rejects_changed_manifest_before_publication() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("jk-a1b2c3d4-role");
    std::fs::create_dir_all(&state).unwrap();
    let original = write_v1_fixture(&state, Some("saved-stem"), "/display/label");
    let error = mutate_records(&state, false, |records| {
        std::fs::write(state.join(".jackin/instance.json"), b"changed manifest").unwrap();
        records.clear();
        true
    })
    .expect_err("identity witness must cover final mutation publication");
    assert!(format!("{error:#}").contains("manifest changed"));
    assert_eq!(
        std::fs::read(isolation_file_path(&state)).unwrap(),
        original
    );
}

#[test]
fn ambiguous_v1_identity_preserves_original_bytes() {
    for corruption in [
        "missing-manifest",
        "different-label",
        "different-container",
        "invalid-stem",
    ] {
        let temp = TempDir::new().unwrap();
        let state = temp.path().join("jk-a1b2c3d4-role");
        std::fs::create_dir_all(&state).unwrap();
        let original = write_v1_fixture(&state, Some("saved-stem"), "other-workspace");
        let manifest_path = state.join(".jackin/instance.json");
        if corruption == "missing-manifest" {
            std::fs::remove_file(&manifest_path).unwrap();
        } else {
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
            match corruption {
                "different-label" => manifest["workspace_label"] = "different".into(),
                "different-container" => manifest["container_base"] = "jk-other".into(),
                "invalid-stem" => manifest["workspace_name"] = "/display/label".into(),
                _ => unreachable!(),
            }
            std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        }
        let error = read_records(&state).unwrap_err();
        assert!(matches!(error.downcast_ref::<crate::IsolationError>(),
            Some(crate::IsolationError::IdentityRecoveryRequired { path }) if *path == isolation_file_path(&state)));
        assert!(
            error
                .to_string()
                .contains("preserve this state and rebuild")
        );
        assert_eq!(
            std::fs::read(isolation_file_path(&state)).unwrap(),
            original
        );
    }
}

#[test]
fn duplicate_historical_identities_and_versions_preserve_original_bytes() {
    for replacement in [
        ("\"version\":1", "\"version\":1,\"version\":2"),
        (
            "\"workspace\":\"label\"",
            "\"workspace\":\"label\",\"workspace\":\"other\"",
        ),
        (
            "\"container_name\":\"jk-a1b2c3d4-role\"",
            "\"container_name\":\"jk-a1b2c3d4-role\",\"container_name\":\"jk-other\"",
        ),
    ] {
        let temp = TempDir::new().unwrap();
        let state = temp.path().join("jk-a1b2c3d4-role");
        std::fs::create_dir_all(&state).unwrap();
        let original = write_v1_fixture(&state, Some("saved-stem"), "label");
        let original = String::from_utf8(original).unwrap();
        assert!(original.contains(replacement.0));
        let ambiguous = original.replace(replacement.0, replacement.1).into_bytes();
        std::fs::write(isolation_file_path(&state), &ambiguous).unwrap();
        let _error = read_records(&state).unwrap_err();
        assert_eq!(
            std::fs::read(isolation_file_path(&state)).unwrap(),
            ambiguous
        );
    }
}

#[test]
fn v2_missing_identity_and_path_identity_are_rejected() {
    let temp = TempDir::new().unwrap();
    for identity in [
        None,
        Some(serde_json::Value::String("/display/label".into())),
    ] {
        let mut record = serde_json::to_value(sample_record()).unwrap();
        record.as_object_mut().unwrap().remove("workspace_name");
        if let Some(identity) = identity {
            record["workspace_name"] = identity;
        }
        let bytes =
            serde_json::to_vec(&serde_json::json!({"version": 2, "records": [record]})).unwrap();
        std::fs::create_dir_all(temp.path().join(".jackin")).unwrap();
        std::fs::write(isolation_file_path(temp.path()), &bytes).unwrap();
        let _error = read_records(temp.path()).unwrap_err();
        assert_eq!(
            std::fs::read(isolation_file_path(temp.path())).unwrap(),
            bytes
        );
    }
}

#[test]
fn historical_mixed_batch_cannot_migrate_another_instances_record() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("jk-a1b2c3d4-role");
    std::fs::create_dir_all(&state).unwrap();
    let original = write_v1_fixture(&state, Some("saved-stem"), "label");
    let mut file: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let mut foreign = file["records"][0].clone();
    foreign["container_name"] = "jk-foreign".into();
    file["records"].as_array_mut().unwrap().push(foreign);
    let bytes = serde_json::to_vec(&file).unwrap();
    std::fs::write(isolation_file_path(&state), &bytes).unwrap();
    let _read_error = read_records(&state).unwrap_err();
    let _migrate_error = migrate_records(&state).unwrap_err();
    assert_eq!(std::fs::read(isolation_file_path(&state)).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn historical_symlink_source_cannot_redirect_recovery() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("jk-a1b2c3d4-role");
    std::fs::create_dir_all(&state).unwrap();
    let original = write_v1_fixture(&state, Some("saved-stem"), "label");
    let canary = temp.path().join("outside-isolation.json");
    std::fs::write(&canary, &original).unwrap();
    let source = isolation_file_path(&state);
    std::fs::remove_file(&source).unwrap();
    std::os::unix::fs::symlink(&canary, &source).unwrap();
    let _error = read_records(&state).unwrap_err();
    assert!(
        std::fs::symlink_metadata(&source)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(&canary).unwrap(), original);
}

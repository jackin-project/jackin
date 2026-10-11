// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    assert!(body.contains(r#""version": 3"#));
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

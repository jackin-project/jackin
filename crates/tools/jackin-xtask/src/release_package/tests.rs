// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use handoff::{TaskInput, TaskRequest};
use serde_json::Value;
use std::{fs, path::Path};
use velnor_actions_contract::{
    PACKAGE_RELEASE_PLAN_SCHEMA_V1, PackageReleasePlan, PackageReleasePlanProduct,
    canonical_json_bytes,
    config::RustBinaryReleaseCompanion,
    package_release::{PackageReleaseChannel, PackageReleasePayloadFormat},
    preview_production::{
        PayloadArtifactRef, RELEASE_PRODUCTION_SCHEMA_V1, ReleaseProductionContext,
        ReleaseProductionRequest, expected_release_payload_pairs, release_plan_sha256,
    },
};

fn request(assemble: bool) -> TaskRequest {
    let mut products = std::collections::BTreeMap::<String, PackageReleasePlanProduct>::new();
    for payload in crate::release_archive::release_payloads() {
        products
            .entry(payload.package.to_owned())
            .or_insert_with(|| PackageReleasePlanProduct {
                id: payload.package.to_owned(),
                package: payload.package.to_owned(),
                bins: payload
                    .binaries
                    .iter()
                    .map(|bin| (*bin).to_owned())
                    .collect(),
                targets: Vec::new(),
                format: PackageReleasePayloadFormat::TarGz,
            })
            .targets
            .push(payload.target.to_owned());
    }
    for product in products.values_mut() {
        product.targets.sort();
    }
    let plan = PackageReleasePlan {
        schema: PACKAGE_RELEASE_PLAN_SCHEMA_V1.to_owned(),
        channel: PackageReleaseChannel::Preview,
        repository: "jackin-project/jackin".to_owned(),
        default_branch: "main".to_owned(),
        base_version: env!("CARGO_PKG_VERSION").to_owned(),
        products: products.into_values().collect(),
        companions: vec![RustBinaryReleaseCompanion {
            id: "capsule-manifest".to_owned(),
            path: "capsule-manifest.json".to_owned(),
            asset_name: "capsule-manifest.json".to_owned(),
        }],
    };
    let pairs = expected_release_payload_pairs(&plan).unwrap();
    let stage = if assemble {
        ReleaseProductionStage::Assemble {
            inputs: pairs
                .iter()
                .enumerate()
                .map(|(index, pair)| PayloadArtifactRef {
                    pair: pair.clone(),
                    artifact_id: u64::try_from(index).unwrap() + 1,
                    sha256: "b".repeat(64),
                })
                .collect(),
        }
    } else {
        ReleaseProductionStage::Payload {
            pair: pairs[0].clone(),
        }
    };
    TaskRequest {
        schema: "velnor.release-task.v1".to_owned(),
        request: ReleaseProductionRequest {
            schema: RELEASE_PRODUCTION_SCHEMA_V1.to_owned(),
            policy_sha256: "c".repeat(64),
            plan_sha256: release_plan_sha256(&plan).unwrap(),
            plan,
            context: ReleaseProductionContext {
                repository_id: 1,
                head_repository_id: 1,
                source_sha: "a".repeat(40),
                source_ref: "refs/heads/main".to_owned(),
                workflow_authority_sha: "a".repeat(40),
                run_id: 2,
                run_attempt: 1,
                ci_run_id: 1,
                ci_run_attempt: 1,
                commit_count: 42,
                version: format!("{}-preview.42+aaaaaaa", env!("CARGO_PKG_VERSION")),
            },
            stage,
        },
        inputs: Vec::new(),
    }
}

fn populate(handoff: &mut TaskRequest, source: &Path) {
    for pair in expected_release_payload_pairs(&handoff.request.plan).unwrap() {
        let staging = tempfile::tempdir().unwrap();
        let archive = crate::release_sbom::write_payload_fixture(
            staging.path(),
            &pair.product_id,
            &pair.target,
            &handoff.request.context.version,
        );
        let name = format!("{}-{}.tar.gz", pair.product_id, pair.target);
        for (path, name) in [
            (archive.clone(), name.clone()),
            (
                std::path::PathBuf::from(format!("{}.sbom.json", archive.display())),
                format!("{name}.sbom.json"),
            ),
        ] {
            let destination = source.join(&name);
            fs::copy(&path, &destination).unwrap();
            handoff.inputs.push(TaskInput {
                pair: pair.clone(),
                name,
                sha256: crate::release_sbom::archive_sha256(&destination).unwrap(),
                size: fs::metadata(&destination).unwrap().len(),
                path: destination
                    .canonicalize()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            });
        }
    }
    handoff.inputs.sort_by(|a, b| a.name.cmp(&b.name));
}

#[test]
fn inventory_binds_exactly_six_payloads_sboms_and_capsule_companion() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut handoff = request(true);
    populate(&mut handoff, source.path());
    let handoff = handoff::parse(&canonical_json_bytes(&handoff).unwrap()).unwrap();
    assembly::assemble(output.path(), &handoff).unwrap();
    inventory::write_complete(output.path(), &handoff.request).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(output.path().join("package-output.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["schema"], "velnor.package-output.v1");
    assert_eq!(manifest["payloads"].as_array().unwrap().len(), 6);
    assert_eq!(manifest["sboms"].as_array().unwrap().len(), 6);
    assert_eq!(manifest["companions"].as_array().unwrap().len(), 1);
    for input in &handoff.inputs {
        assert_eq!(
            fs::read(&input.path).unwrap(),
            fs::read(output.path().join(&input.name)).unwrap()
        );
        assert_eq!(
            crate::release_sbom::archive_sha256(&output.path().join(&input.name)).unwrap(),
            input.sha256
        );
    }
    let names: Vec<_> = manifest["payloads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    let companion: Value =
        serde_json::from_slice(&fs::read(output.path().join("capsule-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(companion["targets"].as_object().unwrap().len(), 2);
    for (target, digest) in companion["targets"].as_object().unwrap() {
        assert_eq!(
            digest,
            &crate::release_sbom::archive_sha256(
                &output
                    .path()
                    .join(format!("jackin-capsule-{target}.tar.gz"))
            )
            .unwrap()
        );
    }
    assert_eq!(
        manifest["companions"][0]["sha256"],
        crate::release_sbom::archive_sha256(&output.path().join("capsule-manifest.json")).unwrap()
    );
    assert_eq!(
        crate::fs_util::read_dir_sorted(output.path())
            .unwrap()
            .len(),
        14
    );
}

#[test]
fn rejects_identity_conflicts_before_any_build() {
    let baseline = request(false);
    handoff::parse(&canonical_json_bytes(&baseline).unwrap()).unwrap();
    for mutate in [
        |request: &mut ReleaseProductionRequest| request.context.source_sha = "bad".to_owned(),
        |request: &mut ReleaseProductionRequest| {
            request.context.source_ref = "refs/heads/other".to_owned();
        },
        |request: &mut ReleaseProductionRequest| request.context.commit_count += 1,
        |request: &mut ReleaseProductionRequest| {
            request.context.version = "0.0.0-preview.42+aaaaaaa".to_owned();
        },
        |request: &mut ReleaseProductionRequest| {
            request.plan.products.pop().unwrap().targets.clear();
        },
    ] {
        let mut handoff = request(false);
        mutate(&mut handoff.request);
        handoff::parse(&canonical_json_bytes(&handoff).unwrap()).unwrap_err();
    }
}

#[test]
fn source_policy_rejects_a_self_consistent_reduced_plan() {
    let mut handoff = request(false);
    handoff.request.plan.products.pop();
    handoff.request.plan_sha256 = release_plan_sha256(&handoff.request.plan).unwrap();
    handoff.request.validate().unwrap();
    handoff::parse(&canonical_json_bytes(&handoff).unwrap()).unwrap_err();
}

#[test]
fn payload_inventory_contains_only_selected_pair_and_no_companions() {
    let temp = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let mut handoff = request(true);
    populate(&mut handoff, source.path());
    let pair = expected_release_payload_pairs(&handoff.request.plan)
        .unwrap()
        .remove(0);
    for input in handoff.inputs.iter().filter(|input| input.pair == pair) {
        fs::copy(&input.path, temp.path().join(&input.name)).unwrap();
    }
    inventory::write_leaf(temp.path(), &handoff.request, &pair).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(temp.path().join("package-output.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["schema"], "velnor.package-payload-output.v1");
    assert_eq!(manifest["payloads"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["sboms"].as_array().unwrap().len(), 1);
    assert!(manifest["companions"].as_array().unwrap().is_empty());
    assert_eq!(
        crate::fs_util::read_dir_sorted(temp.path()).unwrap().len(),
        3
    );
}

#[test]
fn task_request_rejects_duplicate_unknown_and_noncanonical_fields() {
    let bytes = canonical_json_bytes(&request(false)).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    for bad in [
        format!(" {text}"),
        text.replacen("\"commit_count\":42", "\"commit_count\":042", 1),
        text.replacen("\"inputs\":[]", "\"inputs\":[],\"inputs\":[]", 1),
        text.replacen("\"inputs\":[]", "\"inputs\":[],\"verified\":true", 1),
    ] {
        handoff::parse(bad.as_bytes()).unwrap_err();
    }
}

#[test]
fn assembly_rejects_incomplete_duplicate_unsorted_and_changed_leaf_bytes() {
    let source = tempfile::tempdir().unwrap();
    for mutation in 0..3 {
        let source = tempfile::tempdir().unwrap();
        let mut handoff = request(true);
        populate(&mut handoff, source.path());
        match mutation {
            0 => {
                handoff.inputs.pop();
            }
            1 => {
                handoff.inputs.swap(0, 1);
            }
            _ => {
                handoff.inputs[1].path = handoff.inputs[0].path.clone();
            }
        }
        handoff::parse(&canonical_json_bytes(&handoff).unwrap()).unwrap_err();
    }
    let mut handoff = request(true);
    populate(&mut handoff, source.path());
    let mut changed = fs::read(&handoff.inputs[0].path).unwrap();
    changed[0] ^= 1;
    fs::write(&handoff.inputs[0].path, changed).unwrap();
    let output = tempfile::tempdir().unwrap();
    assert!(assembly::assemble(output.path(), &handoff).is_err());
    assert!(!output.path().join("package-output.json").exists());
}

#[test]
fn assembly_refuses_linked_inputs_and_existing_output_without_overwrite() {
    let source = tempfile::tempdir().unwrap();
    let mut handoff = request(true);
    populate(&mut handoff, source.path());
    let output = tempfile::tempdir().unwrap();
    let input = &handoff.inputs[0];
    fs::write(output.path().join(&input.name), b"keep").unwrap();
    assert!(assembly::assemble(output.path(), &handoff).is_err());
    assert_eq!(fs::read(output.path().join(&input.name)).unwrap(), b"keep");
    let original = std::path::PathBuf::from(&input.path);
    let retained = original.with_extension("retained");
    fs::rename(&original, &retained).unwrap();
    std::os::unix::fs::symlink(&retained, &original).unwrap();
    let output = tempfile::tempdir().unwrap();
    assert!(assembly::assemble(output.path(), &handoff).is_err());
    assert!(
        crate::fs_util::read_dir_sorted(output.path())
            .unwrap()
            .is_empty()
    );
}

fn native_request_path(root: &Path) -> std::path::PathBuf {
    let directory = root.canonicalize().unwrap().join("velnor");
    fs::create_dir(&directory).unwrap();
    let path = directory.join("release-task-request.json");
    fs::write(&path, canonical_json_bytes(&request(false)).unwrap()).unwrap();
    path
}

#[test]
fn native_request_rejects_wrong_paths_links_fifo_and_oversize() {
    let root = tempfile::tempdir().unwrap();
    let path = native_request_path(root.path());
    handoff::load_path(&path).unwrap();
    handoff::load_path(Path::new("velnor/release-task-request.json")).unwrap_err();
    handoff::load_path(&path.parent().unwrap().join("./release-task-request.json")).unwrap_err();
    let wrong = path.with_file_name("wrong.json");
    fs::copy(&path, &wrong).unwrap();
    handoff::load_path(&wrong).unwrap_err();
    let link = path.with_file_name("retained.json");
    fs::hard_link(&path, &link).unwrap();
    handoff::load_path(&path).unwrap_err();
    fs::remove_file(&link).unwrap();
    fs::rename(&path, &link).unwrap();
    std::os::unix::fs::symlink(&link, &path).unwrap();
    handoff::load_path(&path).unwrap_err();
    fs::remove_file(&path).unwrap();
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap();
    handoff::load_path(&path).unwrap_err();
    fs::remove_file(&path).unwrap();
    fs::File::create(&path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    handoff::load_path(&path).unwrap_err();
    fs::remove_file(&path).unwrap();
    fs::rename(&link, &path).unwrap();
    let alias_root = tempfile::tempdir().unwrap();
    let alias = alias_root.path().canonicalize().unwrap().join("velnor");
    std::os::unix::fs::symlink(path.parent().unwrap(), &alias).unwrap();
    handoff::load_path(&alias.join("release-task-request.json")).unwrap_err();
}

#[test]
fn native_request_detects_replacement_and_held_file_mutation() {
    for replace in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = native_request_path(root.path());
        let (file, metadata) = handoff::open_request(&path).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        if replace {
            fs::rename(&path, path.with_file_name("retained.json")).unwrap();
        } else {
            bytes[0] ^= 1;
        }
        fs::write(&path, bytes).unwrap();
        handoff::read_request(&path, file, &metadata).unwrap_err();
    }
}

mod channels;

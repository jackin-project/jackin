// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::snapshot::verify_private_index_copy;
use super::*;

#[test]
fn staged_content_quota_counts_each_path_and_accepts_exact_limit() {
    let fixture = fixture();
    let root = fixture.path();
    fs::write(root.join("duplicate.txt"), b"staged\n").expect("write duplicate blob");
    git(root, &["add", "duplicate.txt"]);

    let live = LiveRepository::capture_at(root).expect("capture repeated-blob fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    let single_path_error = snapshot
        .check_materialized_tree_size_with_limit(&tree, 6)
        .expect_err("an oversized staged blob must exceed the smaller limit");
    assert!(format!("{single_path_error:#}").contains("exceeds its byte limit"));
    snapshot
        .check_materialized_tree_size_with_limit(&tree, 14)
        .expect("exact content quota must pass");
    let error = snapshot
        .check_materialized_tree_size_with_limit(&tree, 13)
        .expect_err("repeated paths must each count toward quota");
    assert!(format!("{error:#}").contains("exceeds its byte limit"));
}

#[test]
fn staged_symlink_blob_bytes_count_toward_content_quota() {
    let fixture = fixture();
    let root = fixture.path();
    symlink("staged.txt", root.join("staged-link")).expect("create staged symlink");
    git(root, &["add", "staged-link"]);

    let live = LiveRepository::capture_at(root).expect("capture symlink fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    let error = snapshot
        .check_materialized_tree_size_with_limit(&tree, 16)
        .expect_err("symlink target bytes must count in staged tree quota");
    assert!(format!("{error:#}").contains("exceeds its byte limit"));
}

#[test]
fn materialization_rejects_oversized_tree_before_checkout() {
    let fixture = fixture();
    let root = fixture.path();
    let oversized_path = root.join("oversized-before-checkout.txt");
    fs::write(&oversized_path, b"too large").expect("write oversized staged path");
    git(root, &["add", "oversized-before-checkout.txt"]);

    let live = LiveRepository::capture_at(root).expect("capture oversized fixture");
    let index_before = fs::read(&live.index).expect("live index bytes before materialization");
    let worktree_before = fs::read(&oversized_path).expect("live worktree bytes before");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");

    let error = snapshot
        .materialize_with_staged_content_limit(&tree, &live, 8)
        .expect_err("oversized staged path must be rejected before checkout");
    assert!(format!("{error:#}").contains("exceeds its byte limit"));
    assert!(
        !snapshot.root.join("oversized-before-checkout.txt").exists(),
        "quota rejection must happen before checkout-index creates the path"
    );
    assert_eq!(
        fs::read(&live.index).expect("live index remains"),
        index_before
    );
    assert_eq!(
        fs::read(&oversized_path).expect("live worktree remains"),
        worktree_before
    );
}

#[test]
fn rejects_private_index_destination_substitution_by_admitted_digest() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let input = live
        .index_inputs
        .iter()
        .find(|input| input.admitted_path() == live.index)
        .expect("admitted active index");
    let destination = fixture.path().join("private-index-copy");
    fs::copy(&live.index, &destination).expect("copy active index");
    verify_private_index_copy(input, &destination, "active Git index", &live.controls)
        .expect("matching private copy");

    let mut substituted = fs::read(&destination).expect("read copied index");
    substituted[0] ^= 1;
    fs::write(&destination, substituted).expect("substitute destination bytes");
    let error = verify_private_index_copy(input, &destination, "active Git index", &live.controls)
        .expect_err("destination bytes must match admitted digest");
    assert!(format!("{error:#}").contains("digest differs from admitted input"));
}

#[test]
fn rejects_shared_index_destination_substitution_by_admitted_digest() {
    let fixture = fixture();
    git(fixture.path(), &["update-index", "--split-index"]);
    let live = LiveRepository::capture_at(fixture.path()).expect("capture split-index fixture");
    let shared = live.shared_index.as_ref().expect("active shared index");
    let input = live
        .index_inputs
        .iter()
        .find(|input| input.admitted_path() == shared.as_path())
        .expect("admitted shared index");
    let destination = fixture.path().join("private-shared-index-copy");
    fs::copy(shared, &destination).expect("copy shared index");
    verify_private_index_copy(input, &destination, "shared Git index", &live.controls)
        .expect("matching private shared-index copy");

    let mut substituted = fs::read(&destination).expect("read copied shared index");
    substituted[0] ^= 1;
    fs::write(&destination, substituted).expect("substitute shared-index destination bytes");
    let error = verify_private_index_copy(input, &destination, "shared Git index", &live.controls)
        .expect_err("shared destination bytes must match admitted digest");
    assert!(format!("{error:#}").contains("digest differs from admitted input"));
}

#[test]
fn admitted_absent_index_builds_empty_private_tree() {
    let directory = tempfile::tempdir().expect("fixture directory");
    git(directory.path(), &["init", "--quiet"]);
    let live = LiveRepository::capture_at(directory.path()).expect("capture unborn empty repo");
    assert!(live.index_inputs.is_empty());
    let snapshot = OwnedSnapshot::create(&live).expect("private empty snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture empty tree");
    let mut command = Command::new("git");
    command
        .current_dir(directory.path())
        .args(["hash-object", "-t", "tree", "--stdin"])
        .stdin(std::process::Stdio::null());
    let empty = capture(&mut command).expect("derive Git empty-tree identity independently");
    assert!(empty.status.success(), "{empty:?}");
    assert_eq!(
        tree.as_os_str().as_bytes(),
        empty.stdout.strip_suffix(b"\n").expect("Git hash line")
    );
}

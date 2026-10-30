// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn clone_materialization_dissociates_objects_from_host() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(),
        "",
        "deadbeef\n",
        "https://github.com/jackin-project/jackin\n",
    ]);

    materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        Some(&WorkspaceName::parse("jackin").unwrap()),
        &ctx(),
        &mut runner,
    )
    .await
    .unwrap();

    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("repack -a -d")),
        "clone materialization must dissociate borrowed objects via repack: {:?}",
        runner.run_recorded
    );
}

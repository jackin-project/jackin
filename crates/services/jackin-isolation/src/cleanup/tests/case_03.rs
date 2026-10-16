// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn force_cleanup_is_rejected_under_preservation_hold() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    save(&rec, container.path());
    crate::holds::hold_work(container.path(), &rec.worktree_path, "K2 capture", None).unwrap();

    let mut runner = FakeRunner::default();
    let err = force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .expect_err("cleanup under a preservation hold must fail");
    assert!(format!("{err:#}").contains("preservation hold"));
    assert!(format!("{err:#}").contains("record retained"));
    // Source and record remain available.
    assert!(Path::new(&rec.worktree_path).exists());
    assert_eq!(read_records(container.path()).unwrap().len(), 1);
}

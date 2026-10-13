// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[cfg(unix)]
pub(super) async fn fixture_bare_repository(runner: &mut ShellRunner, path: &Path) -> String {
    let quiet = RunOptions {
        quiet: true,
        null_stdin: true,
        extra_env: vec![
            ("GIT_AUTHOR_DATE".into(), "2000-01-01T00:00:00Z".into()),
            ("GIT_COMMITTER_DATE".into(), "2000-01-01T00:00:00Z".into()),
        ],
        ..RunOptions::default()
    };
    runner
        .run(
            "git",
            &["init", "--bare", path.to_str().unwrap()],
            None,
            &quiet,
        )
        .await
        .unwrap();
    let tree = runner
        .capture_with_options("git", &["--git-dir=.", "mktree"], Some(path), &quiet)
        .await
        .unwrap();
    let tip = runner
        .capture_with_options(
            "git",
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "--git-dir=.",
                "commit-tree",
                &tree,
                "-m",
                "fixture",
            ],
            Some(path),
            &quiet,
        )
        .await
        .unwrap();
    runner
        .run(
            "git",
            &["--git-dir=.", "update-ref", "refs/heads/scratch", &tip],
            Some(path),
            &quiet,
        )
        .await
        .unwrap();
    tip
}

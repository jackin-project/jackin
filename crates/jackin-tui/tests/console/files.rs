// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

use jackin_testkit::{TestResult, guard};

use super::support::Lab;

#[tokio::test]
async fn a_pipe_or_a_link_to_one_is_not_offered_as_a_file() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let root = lab.code.join("picker");
        tokio::fs::create_dir_all(root.join("folder")).await?;
        tokio::fs::write(root.join("login.json"), b"{}").await?;
        tokio::fs::symlink(root.join("login.json"), root.join("link")).await?;
        let made = Command::new("mkfifo").arg(root.join("pipe")).status()?;
        assert!(made.success(), "mkfifo failed");
        tokio::fs::symlink(root.join("pipe"), root.join("to-pipe")).await?;

        let world = lab.world();
        let source = world.backend.file_source(world);
        let mut listed: Vec<(String, bool)> = source
            .list(&root.display().to_string())?
            .iter()
            .map(|entry| (entry.name().to_owned(), entry.dir))
            .collect();
        listed.sort();
        assert_eq!(
            listed,
            [
                ("folder".to_owned(), true),
                ("link".to_owned(), false),
                ("login.json".to_owned(), false),
            ]
        );
        Ok(())
    })
    .await
}

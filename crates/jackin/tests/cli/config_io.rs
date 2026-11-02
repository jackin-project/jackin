// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin::ExitClass;
use jackin_core::{FileMode, FileSystem, FsError};
use jackin_testkit::{FsOp, MemFs, TestResult, World, guard};

use super::support::{Harness, failed};

const LISTS: &[&[&str]] = &[
    &["gateway", "ls"],
    &["account", "ls", "--json"],
    &["workspace", "ls"],
    &["role", "ls", "--json"],
];

#[tokio::test]
async fn a_missing_file_reads_as_empty_without_creating_anything_and_an_edit_leaves_a_parsable_file()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        for args in LISTS {
            harness.run(args).await?.ok()?;
            assert_eq!(harness.config_bytes()?, None, "{args:?}");
        }
        harness
            .run(&["workspace", "add", "w", "proj"])
            .await?
            .ok()?;
        assert_eq!(harness.stored().await?.workspaces().count(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_bad_config_fails_every_reader_naming_the_key_and_is_never_repaired() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config("mystery_key = 1\n")?;
        let before = harness.config_bytes()?;
        for args in LISTS.iter().chain([&["ls"][..]].iter()) {
            failed(
                &harness.run(args).await?,
                ExitClass::Failure,
                &["mystery_key"],
            )?;
            assert_eq!(harness.config_bytes()?, before, "{args:?}");
        }
        harness.write_config("[gateways\n")?;
        let before = harness.config_bytes()?;
        for args in LISTS.iter().chain([&["ls"][..]].iter()) {
            let ran = harness.run(args).await?;
            failed(&ran, ExitClass::Failure, &["syntax"])?;
            assert_eq!(harness.config_bytes()?, before, "{args:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_write_surfaces_as_a_failure_with_the_old_bytes_intact() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let memory = Arc::new(MemFs::new());
        let harness = Harness::new(&world).fs(Arc::<MemFs>::clone(&memory));
        let path = harness.config_path.clone();
        let dir = path.as_path().parent().ok_or("no parent")?;
        memory.create_dir_all(&jackin_core::HostPath::parse(dir)?).await?;
        let old = "default_gateway = \"old\"\n\n[gateways.old]\nopenshell_workspace = \"default\"\nbuilder = \"default\"\nplatform = \"linux/arm64\"\n";
        memory.write_new(&path, old.as_bytes(), FileMode(0o600)).await?;
        memory.fail_at(
            FsOp::Rename,
            1,
            FsError::Io {
                path: path.clone(),
                kind: std::io::ErrorKind::PermissionDenied,
                message: "rename refused".to_owned(),
            },
        );
        let ran = harness.run(&["gateway", "add", "fresh"]).await?;
        failed(&ran, ExitClass::Failure, &["rename refused"])?;
        assert_eq!(memory.read_string(&path).as_deref(), Some(old));
        Ok(())
    })
    .await
}

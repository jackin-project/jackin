// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{Rig, TestResult, default_limits, guard};
use rustix::fs::Mode;
use rustix::process::umask;
use tokio_util::sync::CancellationToken;

use super::support::{branch_with_commit, staging_left};
use super::tap::Tap;

#[tokio::test]
async fn the_staging_directory_is_private_even_under_an_open_umask() -> TestResult {
    guard(async {
        umask(Mode::empty());
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        branch_with_commit(&handle, "work", "w.txt").await?;
        let tap = Tap::new(&rig);
        let session = tap.session(&rig, default_limits());

        let report = session
            .export(&rig.config, &name, CancellationToken::new())
            .await?;

        assert_eq!(report.branches.len(), 1);
        assert_eq!(tap.download_dir_modes(), vec![0o700]);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        Ok(())
    })
    .await
}

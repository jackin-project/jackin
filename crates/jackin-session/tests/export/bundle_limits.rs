// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::ProcessRunner;
use jackin_session::{ExportError, LaunchLimits};
use jackin_testkit::{Rig, TestResult, default_limits, guard};
use tokio_util::sync::CancellationToken;

use super::heads::crafted;
use super::support::{
    Sees, branch_with_commit, bundles_left, exports, ref_of, snapshot, staging_left,
};
use super::tap::Tap;

#[tokio::test]
async fn a_bundle_over_the_size_limit_is_refused_before_any_host_bundle_or_fetch() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let tip = branch_with_commit(&handle, "work", "w.txt").await?;
        let normal = crafted(&handle, &["refs/heads/work"]).await?;
        let limit = u64::try_from(normal.len())?;
        let limits = LaunchLimits {
            bundle_bytes: limit,
            ..default_limits()
        };
        let before = snapshot(&rig).await?;

        let tap = Tap::new(&rig);
        let sees = Arc::new(Sees::new(&rig));
        let runner: Arc<dyn ProcessRunner> = Arc::<Sees>::clone(&sees);
        let session = tap.session_over(&rig, limits, runner);
        tap.replace_next_download(vec![0; normal.len() + 1]);

        let refused = session
            .export(&rig.config, &name, CancellationToken::new())
            .await;

        assert!(
            matches!(&refused, Err(ExportError::BundleTooLarge { size, limit: found }) if *size == limit + 1 && *found == limit),
            "{refused:?}"
        );
        assert!(!sees.ran("bundle"), "a host bundle command ran");
        assert!(!sees.ran("fetch"), "a host fetch ran");
        let after = snapshot(&rig).await?;
        assert!(exports(&after).is_empty());
        assert_eq!(after.rest, before.rest);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        assert_eq!(bundles_left(&handle), Vec::<String>::new());

        tap.replace_next_download(normal);
        let report = session
            .export(&rig.config, &name, CancellationToken::new())
            .await?;
        assert_eq!(report.branches.len(), 1);
        assert!(sees.ran("bundle"), "the control export ran no host bundle command");
        assert!(sees.ran("fetch"), "the control export ran no host fetch");
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(
            landed.get(&ref_of(&name, "work")).map(String::as_str),
            Some(tip.as_str())
        );
        Ok(())
    })
    .await
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{ExportError, LaunchLimits};
use jackin_testkit::{TestResult, default_limits, guard};

use super::lab::{Check, Lab};

fn too_large(limit: u64) -> Check {
    Box::new(move |error| {
        matches!(error, ExportError::BundleTooLarge { size, limit: found }
            if *size == limit + 1 && *found == limit)
    })
}

#[tokio::test]
async fn a_bundle_over_the_size_limit_is_refused_before_any_host_import() -> TestResult {
    guard(async {
        let mut lab = Lab::new().await?;
        let normal = lab.honest().await?;
        let limit = u64::try_from(normal.len())?;
        lab.limits(LaunchLimits {
            bundle_bytes: limit,
            ..default_limits()
        });
        lab.control().await?;
        lab.refuses(
            "one byte over",
            vec![0; normal.len() + 1],
            &too_large(limit),
        )
        .await
    })
    .await
}

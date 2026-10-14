// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, Result};

use jackin_runtime_universe_claims::claims::EntryClaim;

pub async fn activate_started_entry(
    start_result: Result<()>,
    entry_claim: Option<&EntryClaim>,
) -> Result<()> {
    start_result
        .context("container run failed — required capabilities or image may be unavailable")?;
    if let Some(claim) = entry_claim {
        claim
            .activate()
            .await
            .context("activating running launch entry")?;
    }
    Ok(())
}

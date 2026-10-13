// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Started-entry activation for the apple-container launch path.
//!
//! [`activate_started_entry`] turns a `container run` outcome into
//! an active launch entry: a failed start bails with the
//! capabilities hint, and a live [`EntryClaim`](jackin_runtime_universe_claims::claims::EntryClaim)
//! is activated so the running container is tracked.

use anyhow::{Context as _, Result};

use jackin_runtime_universe_claims::claims::EntryClaim;

/// Fail on a bad `container run` result, else activate the entry claim.
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

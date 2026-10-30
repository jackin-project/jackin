// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, ensure};
use std::{env, path::Path};
use velnor_actions_contract::preview_production::ReleaseProductionStage;

mod assembly;
mod handoff;
mod inventory;
#[cfg(test)]
mod tests;

pub(crate) fn run() -> Result<()> {
    let handoff = handoff::load()?;
    let request = &handoff.request;
    handoff::validate_environment(request)?;
    let output = env::var("VELNOR_RELEASE_OUTPUT_DIRECTORY")?;
    let manifest = env::var("VELNOR_RELEASE_OUTPUT_MANIFEST")?;
    let output = Path::new(&output);
    let metadata = std::fs::symlink_metadata(output)?;
    ensure!(
        output.is_absolute() && metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Velnor output must be an existing absolute directory"
    );
    ensure!(
        Path::new(&manifest) == output.join("package-output.json"),
        "unexpected producer manifest path"
    );
    ensure!(
        crate::fs_util::read_dir_sorted(output)?.is_empty(),
        "producer output must be empty"
    );
    match &request.stage {
        ReleaseProductionStage::Payload { pair } => {
            crate::release_archive::build_release_payload(
                request.plan.channel,
                &pair.product_id,
                &pair.target,
                &request.context.version,
                &request.context.source_sha,
                output,
            )?;
            inventory::write_leaf(output, request, pair)?;
        }
        ReleaseProductionStage::Assemble { .. } => {
            assembly::assemble(output, &handoff)?;
            inventory::write_complete(output, request)?;
        }
    }
    Ok(())
}

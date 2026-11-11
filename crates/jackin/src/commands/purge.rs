// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{PurgeMode, PurgeOutcome};

use crate::app::App;
use crate::cli::PurgeArgs;
use crate::error::CommandError;

pub(crate) async fn run(app: &mut App, args: PurgeArgs) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let mode = if args.force {
        PurgeMode::Forced
    } else {
        PurgeMode::Guarded
    };
    match session.purge(&config, &args.sandbox, mode).await? {
        PurgeOutcome::Deleted { sandbox, .. } => app.io.note(&format!("deleted sandbox {sandbox}")),
        PurgeOutcome::AlreadyGone { sandbox } => {
            app.io.note(&format!("sandbox {sandbox} is already gone"));
        }
    }
    Ok(())
}

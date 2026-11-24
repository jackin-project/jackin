// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::TerminalText;
use jackin_usage::accounts::AccountRegistry;

use super::{database, failure};
use crate::app::App;
use crate::cli::UsageLsArgs;
use crate::error::CommandError;

pub(super) fn show(app: &mut App, args: &UsageLsArgs) -> Result<(), CommandError> {
    let rows = match AccountRegistry::open_read_only(&database(app)?).map_err(failure)? {
        Some(db) => db.list().map_err(failure)?,
        None => Vec::new(),
    };
    if args.json {
        return app.io.json(&rows);
    }
    for row in rows {
        app.io.line(&format!(
            "{}\t{}\t{}",
            row.id,
            row.provider.as_str(),
            TerminalText::sanitize(row.label.as_bytes())
        ))?;
    }
    Ok(())
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use rusqlite::OptionalExtension;

use super::{AccountRegistry, ConnectionError};
use crate::dashboard::DashboardSettings;

impl AccountRegistry {
    pub fn dashboard_settings(&self) -> Result<DashboardSettings, ConnectionError> {
        let raw: Option<String> = self
            .db
            .query_row("SELECT value FROM dashboard_settings WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        raw.map(|s| serde_json::from_str(&s).map_err(|_| ConnectionError::Invalid))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub fn set_dashboard_settings(
        &self,
        settings: &DashboardSettings,
    ) -> Result<(), ConnectionError> {
        let mut settings = settings.clone();
        settings.normalize();
        let value = serde_json::to_string(&settings).map_err(|_| ConnectionError::Invalid)?;
        self.db.execute("INSERT INTO dashboard_settings(id,value) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value",[value])?;
        Ok(())
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod details;
mod model;
mod render;

use std::error::Error;

use jackin_core::Timestamp;
use jackin_testkit::{ProfileAccount, seed_usage_view};
use jackin_usage::accounts::{AccountRegistry, DATABASE_PATH};
use jackin_usage::view::{UsageView, ViewFilter, build};
use tempfile::TempDir;
use uuid::Uuid;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(crate) const NOW: Timestamp = Timestamp(1_795_534_620);
pub(crate) const OFFSET: i32 = 25_200;

pub(crate) struct Seeded {
    pub(crate) dir: TempDir,
    pub(crate) accounts: Vec<ProfileAccount>,
}

impl Seeded {
    pub(crate) fn new() -> Result<Seeded> {
        let dir = tempfile::tempdir()?;
        let accounts = seed_usage_view(&dir.path().join(DATABASE_PATH))?;
        Ok(Seeded { dir, accounts })
    }

    pub(crate) fn registry(&self) -> Result<AccountRegistry> {
        AccountRegistry::open_read_only(&self.dir.path().join(DATABASE_PATH))?
            .ok_or_else(|| "the usage registry is missing".into())
    }

    pub(crate) fn view(&self, filter: &ViewFilter, now: Timestamp) -> Result<UsageView> {
        Ok(build(&self.registry()?, filter, now, OFFSET)?)
    }

    pub(crate) fn ids(&self) -> Vec<Uuid> {
        self.accounts.iter().map(|account| account.id).collect()
    }

    pub(crate) fn id_of(&self, provider: &str, label: &str, nth: usize) -> Result<Uuid> {
        self.accounts
            .iter()
            .filter(|account| account.provider.as_str() == provider && account.label == label)
            .nth(nth)
            .map(|account| account.id)
            .ok_or_else(|| format!("no {provider} account {label} #{nth}").into())
    }
}

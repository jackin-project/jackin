// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(test, feature = "test-support"))]

//! Fixture-only constructors for isolated synthetic usage-store databases.
//!
//! These helpers bypass the production local-store builder and must only open disposable test
//! paths. They must not be used for managed or persistent application stores.

use crate::{Connection, DbOperation, operation};

/// Connect to a disposable local database seeded with `WITHOUT ROWID` fixture tables.
///
/// This fixed-purpose constructor enables the pinned Turso engine's experimental `WITHOUT ROWID`
/// support for schema-validation tests. Callers remain responsible for providing an isolated
/// temporary path and for creating the fixture schema and data.
pub async fn connect_without_rowid_fixture(path: &str) -> Result<Connection, String> {
    operation(DbOperation::Connect, async {
        let database = turso::Builder::new_local(path)
            .experimental_without_rowid(true)
            .build()
            .await
            .map_err(|_| "build isolated WITHOUT ROWID fixture database failed".to_owned())?;
        database
            .connect()
            .map_err(|_| "connect isolated WITHOUT ROWID fixture database failed".to_owned())
    })
    .await
}

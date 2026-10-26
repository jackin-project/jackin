// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(test, feature = "test-support"))]

use crate::{Connection, DbOperation, operation};

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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const CURRENT_DB: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-real-current.db"
);

pub(super) const CURRENT_WAL: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-real-current.db-wal"
);

pub(super) const BIG_ENDIAN_DB: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-real-current-big-endian.db"
);

pub(super) const BIG_ENDIAN_WAL: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-real-current-big-endian.db-wal"
);

pub(super) const REUSED_STALE_DB: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-reused-stale-suffix.db"
);

pub(super) const REUSED_STALE_WAL: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-reused-stale-suffix.db-wal"
);

pub(super) const REUSED_UNCOMMITTED_STALE_DB: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-reused-uncommitted-stale-suffix.db"
);

pub(super) const REUSED_UNCOMMITTED_STALE_WAL: &[u8] = include_bytes!(
    "../../../../../services/jackin-instance/src/auth/tests/fixtures/omp-reused-uncommitted-stale-suffix.db-wal"
);

pub(super) fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}

pub(super) fn header(database: &[u8]) -> DatabaseHeader {
    validate_database(database, deadline()).expect("checked-in fixture database is valid")
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded OMP account discovery through the shared source-bound snapshot.

use std::path::Path;

use jackin_omp_store::{OmpAccount, OmpError, OmpSnapshot};

use super::StoreError;

/// Enumerate usable OMP account identities from one secure source capture.
/// Secret values stay inside the SQLite snapshot boundary and are never
/// returned to configuration discovery.
pub(crate) fn enumerate_omp_credentials(
    source_directory: &Path,
) -> Result<Vec<OmpAccount>, StoreError> {
    // A default `.omp` directory is not itself an account. Only enter the
    // secure snapshot boundary when the regular database path is present.
    let database_exists = std::fs::symlink_metadata(source_directory.join("agent/agent.db"))
        .map(|metadata| metadata.is_file())
        .unwrap_or(false);
    if !database_exists {
        return Ok(Vec::new());
    }
    let Some(snapshot) =
        OmpSnapshot::capture_from_directory(source_directory).map_err(map_omp_error)?
    else {
        return Ok(Vec::new());
    };
    Ok(snapshot.accounts().to_vec())
}

fn map_omp_error(error: OmpError) -> StoreError {
    match error {
        OmpError::LimitExceeded => StoreError::TooLarge,
        OmpError::Deadline | OmpError::Unavailable => StoreError::Malformed,
        OmpError::SelectionUnavailable => {
            StoreError::Unsupported("omp credential selector is missing or ambiguous")
        }
    }
}

#[cfg(test)]
mod tests;

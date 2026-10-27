// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_omp_store::{OmpAccount, OmpError, OmpSnapshot};

use super::StoreError;

pub(crate) fn enumerate_omp_credentials(
    source_directory: &Path,
) -> Result<Vec<OmpAccount>, StoreError> {
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

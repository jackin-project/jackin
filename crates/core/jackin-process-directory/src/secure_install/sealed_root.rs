// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Native ACL validation for an already-pinned sealed generation directory.

use super::{
    CheckedInstallRoot, File, Mode, NodeRole, fstat, io, permission_denied,
    validate_directory_descriptor, validate_stat,
};

impl CheckedInstallRoot {
    /// Validate a caller-retained generation descriptor with exact mode `0500`.
    ///
    /// The descriptor must name a directory owned by the effective user and
    /// pass the same native ACL checks as private installation roots. This
    /// checks the held object without resolving a path or changing its mode;
    /// the caller remains responsible for retaining and rechecking custody.
    /// The existing `0700` installation-root constructors are unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong ownership, type, mode, or unsafe or
    /// unsupported native ACL inspection.
    pub fn validate_pinned_sealed_directory(directory: &File) -> io::Result<()> {
        validate_directory_descriptor(directory, NodeRole::Directory)?;
        let metadata = fstat(directory)?;
        validate_stat(&metadata, NodeRole::Directory)?;
        if Mode::from_raw_mode(metadata.st_mode) != Mode::from_bits_truncate(0o500) {
            return Err(permission_denied(
                "sealed installation directory must have exact mode 0500",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

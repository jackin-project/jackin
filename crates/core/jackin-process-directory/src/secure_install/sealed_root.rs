// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Native ACL validation for an already-pinned sealed generation directory.

use super::{
    CheckedInstallRoot, File, InstallFileKind, Mode, NodeRole, ensure_confidential_acl, fstat, io,
    permission_denied, validate_directory_descriptor, validate_stat,
};

impl CheckedInstallRoot {
    /// Validate a caller-retained private data directory descriptor.
    ///
    /// This validates the held object only. The caller retains ownership of
    /// the descriptor and must establish path custody separately. The
    /// directory must be owned by the effective user, have exact mode `0700`,
    /// and have no native extended ACL entries.
    pub fn validate_confidential_directory(directory: &File) -> io::Result<()> {
        let metadata = fstat(directory)?;
        validate_stat(&metadata, NodeRole::Root)?;
        ensure_confidential_acl(directory, true)
    }

    /// Validate a caller-retained private data file descriptor.
    ///
    /// This validates the held object only. The caller retains ownership of
    /// the descriptor and must establish path custody separately. The file
    /// must be owned by the effective user, have exact mode `0600`, have one
    /// hard link, and have no native extended ACL entries.
    pub fn validate_confidential_file(file: &File) -> io::Result<()> {
        let metadata = fstat(file)?;
        validate_stat(&metadata, NodeRole::Leaf(InstallFileKind::Receipt))?;
        ensure_confidential_acl(file, false)
    }

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

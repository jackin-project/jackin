// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    CheckedInstallRoot, File, InstallFileKind, Mode, NodeRole, ensure_confidential_acl, fstat, io,
    permission_denied, validate_directory_descriptor, validate_stat,
};

impl CheckedInstallRoot {
    pub fn validate_confidential_directory(directory: &File) -> io::Result<()> {
        let metadata = fstat(directory)?;
        validate_stat(&metadata, NodeRole::Root)?;
        ensure_confidential_acl(directory, true)
    }

    pub fn validate_confidential_file(file: &File) -> io::Result<()> {
        let metadata = fstat(file)?;
        validate_stat(&metadata, NodeRole::Leaf(InstallFileKind::Receipt))?;
        ensure_confidential_acl(file, false)
    }

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

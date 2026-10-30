// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    ARTIFACT_LIMIT, ArtifactRole, OpenedFile, RECEIPT_LIMIT, ReceiptContent, read_opened_receipt,
    verify_opened_file,
};
use anyhow::{Context, Result, ensure};
use jackin_process_directory::{CheckedInstallFile, CheckedInstallRoot, InstallFileKind};
use std::path::Path;

const OWNER_NAME: &str = "pre-commit-owner";
const RECEIPT_NAME: &str = "pre-commit-owner.receipt";
const ENTRY_NAME: &str = "pre-commit-entry";
const HELPER_NAME: &str = "pre-commit-owner-common";

pub(super) struct InspectedInstall {
    _root: CheckedInstallRoot,
    owner: CheckedInstallFile,
    receipt: CheckedInstallFile,
    entry: CheckedInstallFile,
    helper: CheckedInstallFile,
    owner_contents: OpenedFile,
    receipt_contents: OpenedFile,
    entry_contents: OpenedFile,
    helper_contents: OpenedFile,
    receipt_content: ReceiptContent,
}

pub(super) fn inspect_checked_install(path: &Path) -> Result<InspectedInstall> {
    let root = CheckedInstallRoot::from_absolute_directory(path)
        .context("opening selected physical install root")?;
    let owner = root
        .open_file(Path::new(OWNER_NAME), InstallFileKind::Executable)
        .context("opening checked owner executable")?;
    let receipt = root
        .open_file(Path::new(RECEIPT_NAME), InstallFileKind::Receipt)
        .context("opening checked format3 receipt")?;
    let entry = root
        .open_file(Path::new(ENTRY_NAME), InstallFileKind::Script)
        .context("opening checked entry script")?;
    let helper = root
        .open_file(Path::new(HELPER_NAME), InstallFileKind::Script)
        .context("opening checked common helper")?;

    let receipt_contents = clone_checked_file(&receipt, "receipt", RECEIPT_LIMIT as u64)?;
    let receipt_content = read_checked_receipt(&receipt, &receipt_contents)?;
    let owner_contents = clone_checked_file(&owner, "owner", ARTIFACT_LIMIT)?;
    let entry_contents = clone_checked_file(&entry, "entry", ARTIFACT_LIMIT)?;
    let helper_contents = clone_checked_file(&helper, "helper", ARTIFACT_LIMIT)?;

    let inspected = InspectedInstall {
        _root: root,
        owner,
        receipt,
        entry,
        helper,
        owner_contents,
        receipt_contents,
        entry_contents,
        helper_contents,
        receipt_content,
    };
    inspected.recheck()?;
    Ok(inspected)
}

impl InspectedInstall {
    pub(super) fn source_hex(&self) -> String {
        self.receipt_content.source_hex()
    }

    fn recheck(&self) -> Result<()> {
        self.recheck_names()?;

        let current_receipt = read_checked_receipt(&self.receipt, &self.receipt_contents)?;
        ensure!(
            current_receipt == self.receipt_content,
            "format3 receipt fields changed during installation inspection"
        );

        verify_opened_file(
            &self.owner_contents,
            ArtifactRole::Owner,
            self.receipt_content.owner,
        )?;
        verify_opened_file(
            &self.entry_contents,
            ArtifactRole::Entry,
            self.receipt_content.entry,
        )?;
        verify_opened_file(
            &self.helper_contents,
            ArtifactRole::Helper,
            self.receipt_content.helper,
        )?;

        self.recheck_names()?;
        self.receipt_contents
            .recheck_identity("receipt")
            .context("rechecking format3 receipt descriptor")?;
        Ok(())
    }

    fn recheck_names(&self) -> Result<()> {
        self.owner
            .recheck()
            .context("rechecking owner installation name")?;
        self.receipt
            .recheck()
            .context("rechecking receipt installation name")?;
        self.entry
            .recheck()
            .context("rechecking entry installation name")?;
        self.helper
            .recheck()
            .context("rechecking helper installation name")?;
        Ok(())
    }
}

fn clone_checked_file(checked: &CheckedInstallFile, role: &str, limit: u64) -> Result<OpenedFile> {
    checked
        .recheck()
        .with_context(|| format!("rechecking {role} before descriptor inspection"))?;
    let file = checked
        .file()
        .try_clone()
        .with_context(|| format!("duplicating checked {role} descriptor"))?;
    let opened = OpenedFile::from_file(file, role, limit)?;
    checked
        .recheck()
        .with_context(|| format!("rechecking {role} after descriptor inspection"))?;
    Ok(opened)
}

fn read_checked_receipt(
    checked: &CheckedInstallFile,
    opened: &OpenedFile,
) -> Result<ReceiptContent> {
    checked
        .recheck()
        .context("rechecking receipt name before bounded read")?;
    let bytes = read_opened_receipt(opened)?;
    let parsed = ReceiptContent::parse(&bytes);
    let namespace_check = checked.recheck();
    match parsed {
        Err(error) => Err(error),
        Ok(receipt) => {
            namespace_check.context("rechecking receipt name after bounded read")?;
            Ok(receipt)
        }
    }
}

#[cfg(test)]
mod tests;

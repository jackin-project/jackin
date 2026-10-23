// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{FileIdentity, OmpError, SourcePair};

pub(super) const MAX_CAPTURE_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileSecurityIdentity {
    pub(super) device: nix::libc::dev_t,
    pub(super) inode: u64,
    pub(super) uid: u32,
    pub(super) gid: u32,
    pub(super) mode: i64,
    pub(super) links: nix::libc::nlink_t,
}

impl From<FileIdentity> for FileSecurityIdentity {
    fn from(identity: FileIdentity) -> Self {
        Self {
            device: identity.device,
            inode: identity.inode,
            uid: identity.uid,
            gid: identity.gid,
            mode: identity.mode,
            links: identity.links,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AgentDirectorySecurityIdentity {
    device: nix::libc::dev_t,
    inode: u64,
    uid: u32,
    gid: u32,
    mode: i64,
}

impl From<FileIdentity> for AgentDirectorySecurityIdentity {
    fn from(identity: FileIdentity) -> Self {
        Self {
            device: identity.device,
            inode: identity.inode,
            uid: identity.uid,
            gid: identity.gid,
            mode: identity.mode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalBinding {
    Absent,
    Present(FileSecurityIdentity),
    Removed(FileSecurityIdentity),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SourcePresence {
    #[default]
    Unobserved,
    Absent,
    Present,
}

#[derive(Debug, Default)]
pub(super) struct CaptureFence {
    source: SourcePresence,
    agent: Option<AgentDirectorySecurityIdentity>,
    database: Option<FileSecurityIdentity>,
    wal: Option<WalBinding>,
}

impl CaptureFence {
    pub(super) fn observe(&mut self, pair: Option<&SourcePair>) -> Result<(), OmpError> {
        match (self.source, pair) {
            (SourcePresence::Unobserved, None) => {
                self.source = SourcePresence::Absent;
                Ok(())
            }
            (SourcePresence::Unobserved, Some(pair)) => {
                self.observe_present(pair)?;
                self.source = SourcePresence::Present;
                Ok(())
            }
            (SourcePresence::Absent, None) => Ok(()),
            (SourcePresence::Present, Some(pair)) => self.observe_present(pair),
            (SourcePresence::Absent, Some(_)) | (SourcePresence::Present, None) => {
                Err(OmpError::Unavailable)
            }
        }
    }

    fn observe_present(&mut self, pair: &SourcePair) -> Result<(), OmpError> {
        observe_agent_identity(
            &mut self.agent,
            AgentDirectorySecurityIdentity::from(pair.agent_identity),
        )?;
        observe_identity(
            &mut self.database,
            FileSecurityIdentity::from(pair.database_identity),
        )?;
        self.observe_wal(pair.wal_identity.map(FileSecurityIdentity::from))
    }

    fn observe_wal(&mut self, actual: Option<FileSecurityIdentity>) -> Result<(), OmpError> {
        match (self.wal, actual) {
            (None, None) => self.wal = Some(WalBinding::Absent),
            (None, Some(identity)) => self.wal = Some(WalBinding::Present(identity)),
            (Some(WalBinding::Absent | WalBinding::Removed(_)), None) => {}
            (Some(WalBinding::Absent | WalBinding::Removed(_)), Some(_)) => {
                return Err(OmpError::Unavailable);
            }
            (Some(WalBinding::Present(expected)), Some(actual)) if expected == actual => {}
            (Some(WalBinding::Present(expected)), None) => {
                self.wal = Some(WalBinding::Removed(expected));
            }
            (Some(WalBinding::Present(_)), Some(_)) => return Err(OmpError::Unavailable),
        }
        Ok(())
    }
}

fn observe_agent_identity(
    expected: &mut Option<AgentDirectorySecurityIdentity>,
    actual: AgentDirectorySecurityIdentity,
) -> Result<(), OmpError> {
    match expected {
        Some(expected) if *expected != actual => Err(OmpError::Unavailable),
        Some(_) => Ok(()),
        None => {
            *expected = Some(actual);
            Ok(())
        }
    }
}

fn observe_identity(
    expected: &mut Option<FileSecurityIdentity>,
    actual: FileSecurityIdentity,
) -> Result<(), OmpError> {
    match expected {
        Some(expected) if *expected != actual => Err(OmpError::Unavailable),
        Some(_) => Ok(()),
        None => {
            *expected = Some(actual);
            Ok(())
        }
    }
}

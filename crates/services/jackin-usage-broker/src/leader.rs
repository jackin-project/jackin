// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker leader lease claim and renewal.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use std::time::{Duration, Instant};

use jackin_protocol::usage_broker::{USAGE_BROKER_PROTOCOL_VERSION, UsageCoordinationError};
use nix::fcntl::{OFlag, open};
use nix::sys::signal::kill;
use nix::sys::stat::Mode;
use nix::unistd::{Pid, UnlinkatFlags, fsync, geteuid, unlinkat};

use crate::{
    BrokerLease, BrokerLeaseOwner, CONNECT_RETRY, CONNECT_RETRY_STEP, UsageBrokerClient,
    unavailable,
};

pub(crate) fn claim_leader(
    path: &Path,
    build_id: &str,
    lease_duration: Duration,
) -> Result<Option<BrokerLeaseOwner>, UsageCoordinationError> {
    let lease = BrokerLease::new(build_id);
    loop {
        match open(path, OFlag::O_RDWR | OFlag::O_NOFOLLOW, Mode::empty()) {
            Ok(fd) => {
                let mut file = File::from(fd);
                validate_owned_file(&file, 0o600)?;
                // A live broker does not hold the lease lock continuously. A
                // contender therefore either observes the current owner or
                // takes the same descriptor lock before replacing an expired
                // payload.
                if file.try_lock().is_err() {
                    return Ok(None);
                }
                if file.metadata().map_err(|_| unavailable())?.nlink() == 0 {
                    let _ignored = file.unlock();
                    continue;
                }
                let result = claim_existing_lease(&mut file, &lease, build_id, lease_duration);
                let unlock = file.unlock();
                return match (result, unlock) {
                    (Ok(Some(())), Ok(())) => Ok(Some(BrokerLeaseOwner { lease, file })),
                    (Ok(Some(()) | None), Err(_)) => Err(unavailable()),
                    (Ok(None), Ok(())) => Ok(None),
                    (Err(error), _) => Err(error),
                };
            }
            Err(nix::errno::Errno::ENOENT) => {
                let fd = open(
                    path,
                    OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
                    Mode::from_bits_truncate(0o600),
                )
                .map_err(|_| unavailable())?;
                let mut file = Some(File::from(fd));
                let result = (|| -> Result<BrokerLeaseOwner, UsageCoordinationError> {
                    {
                        let lease_file = file.as_mut().ok_or_else(unavailable)?;
                        validate_owned_file(lease_file, 0o600)?;
                        lease_file.try_lock().map_err(|_| unavailable())?;
                        write_lease(lease_file, &lease).map_err(|_| unavailable())?;
                        lease_file.unlock().map_err(|_| unavailable())?;
                    }
                    let file = file.take().ok_or_else(unavailable)?;
                    Ok(BrokerLeaseOwner { lease, file })
                })();
                return match result {
                    Ok(owner) => Ok(Some(owner)),
                    Err(error) => {
                        if let Some(file) = file.as_mut() {
                            let _ignored = unlink_created_lease(path, file);
                        }
                        Err(error)
                    }
                };
            }
            Err(_) => return Err(unavailable()),
        }
    }
}

pub(crate) fn claim_existing_lease(
    file: &mut File,
    replacement: &BrokerLease,
    build_id: &str,
    lease_duration: Duration,
) -> Result<Option<()>, UsageCoordinationError> {
    if file.metadata().map_err(|_| unavailable())?.nlink() == 0 {
        return Ok(None);
    }
    let bytes = read_lease_bytes(file).map_err(|_| unavailable())?;
    let existing = serde_json::from_slice::<BrokerLease>(&bytes).ok();
    let replace = if let Some(existing) = existing {
        if existing.protocol_version != USAGE_BROKER_PROTOCOL_VERSION
            || existing.build_id != build_id
        {
            // A healthy incompatible endpoint is never replaced by an
            // activator; the client will receive protocol_mismatch.
            return Ok(None);
        }
        chrono::Utc::now()
            .timestamp()
            .saturating_sub(existing.renewed_at_epoch)
            >= i64::try_from(lease_duration.as_secs()).unwrap_or(i64::MAX)
    } else {
        // Preserve compatibility with pre-lease state only when its PID is
        // demonstrably gone; a malformed live lease fails closed.
        let pid = String::from_utf8_lossy(&bytes).trim().parse::<i32>().ok();
        pid.is_some_and(|pid| kill(Pid::from_raw(pid), None).is_err())
    };
    if !replace {
        return Ok(None);
    }
    write_lease(file, replacement).map_err(|_| unavailable())?;
    Ok(Some(()))
}

pub(crate) fn renew_lease(owner: &mut BrokerLeaseOwner, lease_duration: Duration) -> bool {
    if owner.file.lock().is_err() {
        return false;
    }
    let result = (|| {
        if owner.file.metadata().ok()?.nlink() == 0 {
            return Some(false);
        }
        let mut current = read_lease(&mut owner.file).ok()?;
        if current.instance_id != owner.lease.instance_id {
            return Some(false);
        }
        let now = chrono::Utc::now().timestamp();
        if now.saturating_sub(current.renewed_at_epoch)
            >= i64::try_from(lease_duration.as_secs()).unwrap_or(i64::MAX)
        {
            return Some(false);
        }
        current.renewed_at_epoch = now;
        write_lease(&mut owner.file, &current).ok()?;
        owner.lease.renewed_at_epoch = now;
        Some(true)
    })()
    .unwrap_or(false);
    let unlock = owner.file.unlock();
    result && unlock.is_ok()
}

pub(crate) fn cleanup_owned_files(
    lease_path: &Path,
    socket_path: &Path,
    owner: &mut BrokerLeaseOwner,
) -> bool {
    if owner.file.lock().is_err() {
        return false;
    }
    let result = (|| -> Result<(), ()> {
        if owner.file.metadata().map_err(|_| ())?.nlink() == 0 {
            return Err(());
        }
        let current = read_lease(&mut owner.file).map_err(|_| ())?;
        if current.instance_id != owner.lease.instance_id {
            return Err(());
        }
        // The lease descriptor remains locked across both unlinks. No valid
        // successor can bind the broker socket between the ownership check
        // and path removal.
        unlink_owned_path(socket_path)?;
        unlink_owned_path(lease_path)?;
        Ok(())
    })()
    .is_ok();
    let unlock = owner.file.unlock().is_ok();
    result && unlock
}

pub(crate) fn unlink_owned_path(path: &Path) -> Result<(), ()> {
    let parent = path.parent().ok_or(())?;
    let filename = path.file_name().and_then(|name| name.to_str()).ok_or(())?;
    let directory = open(
        parent,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| ())?;
    let directory = File::from(directory);
    match unlinkat(&directory, filename, UnlinkatFlags::NoRemoveDir) {
        Ok(()) | Err(nix::errno::Errno::ENOENT) => {}
        Err(_) => return Err(()),
    }
    fsync(&directory).map_err(|_| ())?;
    Ok(())
}

pub(crate) fn unlink_created_lease(path: &Path, file: &mut File) -> bool {
    let Ok(expected) = file.metadata() else {
        return false;
    };
    let Ok(actual) = fs::symlink_metadata(path) else {
        return false;
    };
    if actual.file_type().is_symlink()
        || actual.dev() != expected.dev()
        || actual.ino() != expected.ino()
    {
        return false;
    }
    unlink_owned_path(path).is_ok()
}

pub(crate) fn read_lease(file: &mut File) -> Result<BrokerLease, std::io::Error> {
    let bytes = read_lease_bytes(file)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

pub(crate) fn read_lease_bytes(file: &mut File) -> Result<Vec<u8>, std::io::Error> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn write_lease(file: &mut File, lease: &BrokerLease) -> Result<(), std::io::Error> {
    let bytes = serde_json::to_vec(lease)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.sync_all()
}

pub(crate) fn validate_owned_file(file: &File, mode: u32) -> Result<(), UsageCoordinationError> {
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o777 != mode {
        return Err(unavailable());
    }
    Ok(())
}

pub(crate) fn wait_for_leader(client: &UsageBrokerClient) -> Result<(), UsageCoordinationError> {
    let started = Instant::now();
    while started.elapsed() < CONNECT_RETRY {
        if connect_probe(client) {
            return Ok(());
        }
        std::thread::park_timeout(CONNECT_RETRY_STEP);
    }
    Err(unavailable())
}

pub(crate) fn connect_probe(client: &UsageBrokerClient) -> bool {
    client.probe_current_projection()
}

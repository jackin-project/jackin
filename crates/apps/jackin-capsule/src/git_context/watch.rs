// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Git HEAD change watcher.

#[cfg(target_os = "linux")]
use super::{
    git_capture_at_workdir, git_metadata_dirs, record_io_error, record_recovered_degradation,
};
#[cfg(target_os = "linux")]
use std::ffi::OsStr;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;

use crate::session::SessionEvent;
#[cfg(target_os = "linux")]
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
pub(crate) const GIT_CONTEXT_WATCH_MASK: AddWatchFlags = AddWatchFlags::IN_CLOSE_WRITE
    .union(AddWatchFlags::IN_MOVED_TO)
    .union(AddWatchFlags::IN_CREATE)
    .union(AddWatchFlags::IN_ATTRIB)
    .union(AddWatchFlags::IN_DELETE_SELF)
    .union(AddWatchFlags::IN_MOVE_SELF);

#[cfg(target_os = "linux")]
pub(crate) fn start_git_context_watcher(
    workdir: PathBuf,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
) {
    let Some(git_dir) = git_dir_for_watch(&workdir) else {
        return;
    };
    if let Err(_error) =
        jackin_telemetry::spawn::thread_stream_named("git-context-watch".to_owned(), move || {
            watch_git_head_changes(git_dir, event_tx);
        })
    {
        record_recovered_degradation();
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn start_git_context_watcher(
    _workdir: PathBuf,
    _event_tx: mpsc::UnboundedSender<SessionEvent>,
) {
}

#[cfg(target_os = "linux")]
pub(crate) fn git_dir_for_watch(workdir: &Path) -> Option<PathBuf> {
    git_metadata_dirs(workdir)
        .map(|metadata| metadata.git_dir)
        .or_else(|| {
            let raw = git_capture_at_workdir(workdir, &["rev-parse", "--git-dir"])?;
            let path = PathBuf::from(raw);
            Some(if path.is_absolute() {
                path
            } else {
                workdir.join(path)
            })
        })
}

#[cfg(target_os = "linux")]
pub(crate) fn watch_git_head_changes(
    git_dir: PathBuf,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
) {
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let instance = match Inotify::init(InitFlags::IN_CLOEXEC) {
        Ok(instance) => instance,
        Err(_error) => {
            record_io_error();
            jackin_telemetry::stream::complete_error(
                open,
                jackin_telemetry::schema::enums::ErrorType::IoError,
            );
            return;
        }
    };
    if let Err(_error) = instance.add_watch(git_dir.as_path(), GIT_CONTEXT_WATCH_MASK) {
        record_io_error();
        jackin_telemetry::stream::complete_error(
            open,
            jackin_telemetry::schema::enums::ErrorType::IoError,
        );
        return;
    }
    jackin_telemetry::stream::complete_success(open);
    loop {
        let events = match instance.read_events() {
            Ok(events) => events,
            Err(_error) => {
                record_io_error();
                jackin_telemetry::stream::complete_error(
                    jackin_telemetry::stream::phase(
                        jackin_telemetry::schema::enums::StreamOperation::Close,
                    ),
                    jackin_telemetry::schema::enums::ErrorType::IoError,
                );
                return;
            }
        };
        let changed = events.iter().any(|event| {
            event.mask.intersects(
                AddWatchFlags::IN_Q_OVERFLOW
                    | AddWatchFlags::IN_DELETE_SELF
                    | AddWatchFlags::IN_MOVE_SELF,
            ) || event.name.as_deref() == Some(OsStr::new("HEAD"))
        });
        if changed
            && event_tx
                .send(SessionEvent::GitBranchContextRefreshRequested)
                .is_err()
        {
            jackin_telemetry::stream::complete_success(jackin_telemetry::stream::phase(
                jackin_telemetry::schema::enums::StreamOperation::Close,
            ));
            return;
        }
    }
}

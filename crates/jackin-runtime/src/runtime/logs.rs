// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `jackin logs` implementation.
//!
//! Resolves the multiplexer log file for one or every active container,
//! and either lists them, prints a tail, follows the file, or copies a
//! tail into a shareable bundle.
//!
//! Path layout mirrors the host-side mount declared in
//! `runtime::launch::agent_mounts`: `<data_dir>/<container_base>/state`
//! is bind-mounted into the container at `/jackin/state`, and the
//! multiplexer writes `multiplexer.log` directly into it.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "log commands intentionally write command output to stdout/stderr"
)]

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use crate::instance::InstanceManifest;
use jackin_core::paths::JackinPaths;

const LOG_FILE_NAME: &str = "multiplexer.log";

pub fn run(
    paths: &JackinPaths,
    selector: Option<String>,
    path: bool,
    tail: usize,
    follow: bool,
    bundle: Option<PathBuf>,
) -> Result<()> {
    match selector {
        None => list_all(paths),
        Some(sel) => {
            let entry = resolve(paths, &sel)?;
            if path {
                println!("{}", entry.log_path.display());
                Ok(())
            } else if follow {
                follow_file(&entry.log_path)
            } else if let Some(dest) = bundle {
                write_bundle(&entry, tail, &dest)
            } else {
                print_tail(&entry.log_path, tail)
            }
        }
    }
}

struct LogEntry {
    container_base: String,
    role_display_name: String,
    workspace_label: String,
    status: String,
    log_path: PathBuf,
}

impl LogEntry {
    fn from_manifest(manifest: InstanceManifest, log_path: PathBuf) -> Self {
        Self {
            container_base: manifest.container_base,
            role_display_name: manifest.role_display_name,
            workspace_label: manifest.workspace_label,
            status: format!("{:?}", manifest.status),
            log_path,
        }
    }
}

fn list_all(paths: &JackinPaths) -> Result<()> {
    let entries = enumerate(paths)?;
    if entries.is_empty() {
        println!(
            "No multiplexer logs found under {}.",
            paths.data_dir.display()
        );
        println!("(Logs appear after the first `jackin load` or `jackin console` attach.)");
        return Ok(());
    }
    println!("{:<40} {:<20} {:<14} PATH", "CONTAINER", "ROLE", "STATUS");
    for entry in entries {
        println!(
            "{:<40} {:<20} {:<14} {}",
            truncate(&entry.container_base, 40),
            truncate(&entry.role_display_name, 20),
            truncate(&entry.status, 14),
            entry.log_path.display()
        );
    }
    Ok(())
}

fn enumerate(paths: &JackinPaths) -> Result<Vec<LogEntry>> {
    let mut out = Vec::new();
    if !paths.data_dir.exists() {
        return Ok(out);
    }
    for dir_entry in std::fs::read_dir(&paths.data_dir)
        .with_context(|| format!("reading {}", paths.data_dir.display()))?
    {
        let dir_entry = dir_entry?;
        if !dir_entry.file_type()?.is_dir() {
            continue;
        }
        let state_dir = dir_entry.path();
        let log_path = state_dir.join("state").join(LOG_FILE_NAME);
        if !log_path.exists() {
            continue;
        }
        // `read_optional` returns Err only on parse failure; missing
        // manifest is None. Skip parse-failed dirs but log them so an
        // operator with a corrupted manifest sees why a container they
        // expect is absent from the list.
        let manifest = match InstanceManifest::read_optional(&state_dir) {
            Ok(Some(m)) => m,
            Ok(None) => continue,
            Err(e) => {
                eprintln!(
                    "warning: skipping {} (manifest parse failed: {e:#})",
                    state_dir.display()
                );
                continue;
            }
        };
        out.push(LogEntry::from_manifest(manifest, log_path));
    }
    // Most recently updated log first. `sort_by_cached_key` evaluates
    // the key once per element (vs `sort_by_key`'s once per
    // comparison), so we don't burn O(N log N) `stat` syscalls;
    // `UNIX_EPOCH` on mtime failure keeps the order deterministic.
    out.sort_by_cached_key(|e| {
        std::cmp::Reverse(
            std::fs::metadata(&e.log_path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH),
        )
    });
    Ok(out)
}

fn resolve(paths: &JackinPaths, selector: &str) -> Result<LogEntry> {
    let entries = enumerate(paths)?;
    if entries.is_empty() {
        bail!(
            "no multiplexer logs found under {}. Launch a container with \
             `jackin load` or `jackin console` first.",
            paths.data_dir.display()
        );
    }
    let matches: Vec<LogEntry> = entries
        .into_iter()
        .filter(|e| {
            e.container_base == selector
                || e.container_base.contains(selector)
                || e.role_display_name.eq_ignore_ascii_case(selector)
                || e.workspace_label.eq_ignore_ascii_case(selector)
        })
        .collect();
    match matches.len() {
        0 => Err(anyhow!(
            "no container matched {selector:?}. Run `jackin logs` (no args) to list candidates."
        )),
        1 => matches
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("log match disappeared after length check")),
        n => {
            let names: Vec<String> = matches.iter().map(|e| e.container_base.clone()).collect();
            Err(anyhow!(
                "{n} containers matched {selector:?}: {}. Re-run with a more specific selector \
                 (full container base name).",
                names.join(", ")
            ))
        }
    }
}

fn print_tail(path: &Path, n: usize) -> Result<()> {
    let lines = read_tail(path, n)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in &lines {
        out.write_all(line.as_bytes())?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

/// Read the last `n` lines of a file using a bounded `VecDeque`. The
/// alternative — `read_to_string` then split — would balloon to file
/// size in memory for a long-lived log; a `tail`-style ring keeps
/// memory proportional to `n` regardless of file size.
pub(super) fn read_tail(path: &Path, n: usize) -> Result<Vec<String>> {
    if n == 0 {
        return Ok(Vec::new());
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "log reads run in command handlers or spawn_blocking tail paths, not frame rendering"
    )]
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut ring: VecDeque<String> = VecDeque::with_capacity(n.min(8192));
    for line in reader.lines() {
        let line = line?;
        if ring.len() == n {
            ring.pop_front();
        }
        ring.push_back(line);
    }
    Ok(ring.into_iter().collect())
}

fn write_bundle(entry: &LogEntry, n: usize, dest: &Path) -> Result<()> {
    let lines = read_tail(&entry.log_path, n)?;
    let mut file = File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    writeln!(
        file,
        "# jackin multiplexer log bundle\n\
         # container: {}\n\
         # role:      {}\n\
         # workspace: {}\n\
         # status:    {}\n\
         # source:    {}\n\
         # lines:     last {}\n",
        entry.container_base,
        entry.role_display_name,
        entry.workspace_label,
        entry.status,
        entry.log_path.display(),
        lines.len(),
    )?;
    for line in &lines {
        writeln!(file, "{line}")?;
    }
    file.flush()?;
    println!(
        "Wrote {} lines from {} to {}.",
        lines.len(),
        entry.log_path.display(),
        dest.display()
    );
    Ok(())
}

fn follow_file(path: &Path) -> Result<()> {
    // Tail-from-end: seek to current EOF, then poll for appended bytes.
    // Polling beats inotify/kqueue for one cold dependency reason — the
    // tradeoff is a ~250ms latency between writer flush and operator
    // visibility, which is below the threshold for "feels live" in a
    // human-tail use case.
    //
    // Rotation / truncation handling: the daemon may rotate
    // multiplexer.log (mv + recreate) or restart with a fresh truncated
    // file. Without explicit detection, a follower wedged on the
    // original inode (or the original offset past the new length) would
    // sit at Ok(0) forever and quietly miss every subsequent log line.
    // Two heuristics, checked on every idle tick:
    //   * inode change at `path` → file was rotated; reopen.
    //   * `metadata.len() < cursor` at the current path → file was
    //     truncated in place; reopen and tail from start.
    let (mut reader, mut watched_inode) = open_for_follow(path, /*from_end=*/ true)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => {
                if let Some((new_reader, new_inode)) =
                    detect_rotation(path, &mut reader, watched_inode)?
                {
                    reader = new_reader;
                    watched_inode = new_inode;
                    continue;
                }
                #[expect(
                    clippy::disallowed_methods,
                    reason = "log tail follow loop runs inside spawn_blocking"
                )]
                std::thread::sleep(Duration::from_millis(250));
            }
            Ok(_) => {
                out.write_all(buf.as_bytes())?;
                out.flush()?;
            }
            Err(e) => bail!("tail read failed: {e}"),
        }
    }
}

/// Open `path` for follow, optionally seeking to EOF (initial open) or
/// to the start (after a detected rotation). Returns the buffered
/// reader and the file's inode at open time so the follow loop can
/// notice an inode swap underneath the same path.
fn open_for_follow(path: &Path, from_end: bool) -> Result<(BufReader<File>, u64)> {
    #[expect(
        clippy::disallowed_methods,
        reason = "log follow opens run inside spawn_blocking"
    )]
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    if from_end {
        file.seek(SeekFrom::End(0))?;
    }
    let inode = inode_of_file(&file)?;
    Ok((BufReader::new(file), inode))
}

#[cfg(unix)]
fn inode_of_file(file: &File) -> Result<u64> {
    use std::os::unix::fs::MetadataExt as _;
    Ok(file.metadata()?.ino())
}
#[cfg(not(unix))]
fn inode_of_file(_file: &File) -> Result<u64> {
    // Non-unix hosts do not have stable inodes; fall back to "never
    // rotates" so the file-length check (still active) catches in-place
    // truncation. Rotation via mv + recreate is undetectable on these
    // platforms without a heavier file-watching dependency.
    Ok(0)
}

/// Detect whether `path` was rotated (inode change) or truncated
/// (length shrank below the reader's cursor). Returns `Some` with a
/// freshly-opened reader to swap in, or `None` if nothing changed.
fn detect_rotation(
    path: &Path,
    reader: &mut BufReader<File>,
    watched_inode: u64,
) -> Result<Option<(BufReader<File>, u64)>> {
    let path_meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    #[cfg(unix)]
    let current_inode = {
        use std::os::unix::fs::MetadataExt as _;
        path_meta.ino()
    };
    #[cfg(not(unix))]
    let current_inode = 0u64;
    let cursor = reader.get_mut().stream_position().unwrap_or(0);
    let rotated = current_inode != watched_inode;
    let truncated = path_meta.len() < cursor;
    if rotated || truncated {
        eprintln!(
            "jackin: log {} {}; reopening",
            path.display(),
            if rotated { "rotated" } else { "truncated" }
        );
        let (new_reader, new_inode) = open_for_follow(path, /*from_end=*/ false)?;
        return Ok(Some((new_reader, new_inode)));
    }
    Ok(None)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let take = max.saturating_sub(1);
        let mut t: String = s.chars().take(take).collect();
        t.push('…');
        t
    }
}

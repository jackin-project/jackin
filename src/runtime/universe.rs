//! Tracks how long the operator has been "in the construct".
//!
//! The span runs from the launch that brought the first container up to the
//! exit of the last one. A single marker file under the data dir holds the
//! start instant; the exit ritual reads and clears it to show elapsed time.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::paths::JackinPaths;

fn marker_path(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("universe-since")
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Record the construct's start instant. `fresh` is true when no containers
/// were running before this launch (the operator is entering an empty
/// construct), in which case the marker is (re)written to now; otherwise it is
/// only written if absent, so an ongoing session keeps its original start.
pub fn mark_start(paths: &JackinPaths, fresh: bool) {
    let file = marker_path(paths);
    if !fresh && file.exists() {
        return;
    }
    let _ = std::fs::write(&file, now_millis().to_string());
}

/// Read the construct's start instant, delete the marker, and return the
/// elapsed span. Returns `None` when no marker exists or it cannot be parsed
/// (the elapsed line is then simply omitted from the exit ritual).
#[must_use]
pub fn take_elapsed(paths: &JackinPaths) -> Option<Duration> {
    let file = marker_path(paths);
    let content = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    let started: u128 = content.trim().parse().ok()?;
    let elapsed_ms = now_millis().checked_sub(started)?;
    Some(Duration::from_millis(
        u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_then_take_round_trips_and_clears() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();

        mark_start(&paths, true);
        assert!(marker_path(&paths).exists(), "marker written");

        let elapsed = take_elapsed(&paths).expect("elapsed available");
        assert!(
            elapsed < Duration::from_secs(5),
            "just-started span is small"
        );
        assert!(!marker_path(&paths).exists(), "marker cleared after take");
        assert!(take_elapsed(&paths).is_none(), "second take is empty");
    }

    #[test]
    fn mark_non_fresh_preserves_existing_start() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(marker_path(&paths), "1000").unwrap();
        mark_start(&paths, false); // not fresh — must not overwrite
        let kept = std::fs::read_to_string(marker_path(&paths)).unwrap();
        assert_eq!(kept, "1000", "ongoing session keeps its original start");
    }
}

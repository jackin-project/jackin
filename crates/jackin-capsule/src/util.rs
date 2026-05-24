//! Small in-crate utilities shared by the daemon and statusbar.

use std::io::Read;
use std::path::Path;

/// Cap reads against text metadata files so a corrupt or hostile file
/// cannot pin daemon memory while parsing branch state or hostnames.
pub fn read_text_bounded(path: &Path, max_bytes: u64) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = String::new();
    file.take(max_bytes).read_to_string(&mut buf).ok()?;
    Some(buf)
}

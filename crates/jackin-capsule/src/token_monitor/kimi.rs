//! JSONL reader for Kimi token usage.
//!
//! Reads `~/.kimi/sessions/{GROUP_ID}/{SESSION_UUID}/wire.jsonl`.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use super::TokenSession;

fn find_wire_files() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let base = "/home/agent/.kimi/sessions";
    let Ok(groups) = fs::read_dir(base) else { return paths };
    for group in groups.flatten() {
        let Ok(sessions) = fs::read_dir(group.path()) else { continue };
        for session in sessions.flatten() {
            let wire = session.path().join("wire.jsonl");
            if wire.exists() {
                paths.push(wire);
            }
        }
    }
    paths
}

pub fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_wire_files();
    if files.is_empty() {
        return false;
    }
    let mut changed = false;

    for path in &files {
        let Ok(mut file) = fs::File::open(path) else { continue };
        if !super::seek_or_reset(&mut file, &mut session.file_offset, path) {
            continue;
        }
        let reader = BufReader::new(&file);
        let mut bytes_read = session.file_offset;

        for line in reader.lines() {
            let Ok(line) = line else { break };
            bytes_read += line.len() as u64 + 1;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else { continue };

            // StatusUpdate messages carry token_usage
            if let Some(usage) = val.get("token_usage") {
                let input = usage
                    .get("input_other")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
                let cache_read = usage
                    .get("input_cache_read")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_write = usage
                    .get("input_cache_creation")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                session.totals.input_tokens += input;
                session.totals.output_tokens += output;
                session.totals.cache_read_tokens += cache_read;
                session.totals.cache_write_tokens += cache_write;
                changed = true;
            }
        }
        session.file_offset = bytes_read;
    }
    changed
}

#[cfg(test)]
mod tests {
    #[test]
    fn kimi_token_reader_parses_wire_jsonl() {
        let line = r#"{"token_usage":{"input_other":500,"output":200,"input_cache_read":100,"input_cache_creation":50}}"#;
        let val: serde_json::Value = serde_json::from_str(line).unwrap();
        let usage = val.get("token_usage").unwrap();
        assert_eq!(usage.get("input_other").and_then(|v| v.as_u64()), Some(500));
        assert_eq!(usage.get("output").and_then(|v| v.as_u64()), Some(200));
        assert_eq!(
            usage.get("input_cache_read").and_then(|v| v.as_u64()),
            Some(100)
        );
        assert_eq!(
            usage.get("input_cache_creation").and_then(|v| v.as_u64()),
            Some(50)
        );
    }
}

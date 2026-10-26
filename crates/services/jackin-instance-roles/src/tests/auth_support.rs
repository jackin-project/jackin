// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn stage_host_kimi_dir(
    temp: &tempfile::TempDir,
    config_content: Option<&str>,
    cred_files: &[(&str, &str)],
    mcp_json: Option<&str>,
    device_id: Option<&str>,
) -> PathBuf {
    let host_home = temp.path().join("host_home");
    let kimi_dir = host_home.join(".kimi-code");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    if let Some(content) = config_content {
        std::fs::write(kimi_dir.join("config.toml"), content).unwrap();
    }
    if !cred_files.is_empty() {
        let creds_dir = kimi_dir.join("credentials");
        std::fs::create_dir_all(&creds_dir).unwrap();
        for (name, content) in cred_files {
            std::fs::write(creds_dir.join(name), content).unwrap();
        }
    }
    if let Some(content) = mcp_json {
        std::fs::write(kimi_dir.join("mcp.json"), content).unwrap();
    }
    if let Some(content) = device_id {
        std::fs::write(kimi_dir.join("device_id"), content).unwrap();
    }
    host_home
}

pub(super) const TEST_CREDENTIALS: &str =
    r#"{"claudeAiOauth":{"accessToken":"test","refreshToken":"test"}}"#;

#[cfg(unix)]
pub(super) const OMP_V7_SCHEMA: &str = r"
CREATE TABLE auth_schema_version (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL
);
INSERT INTO auth_schema_version (id, version) VALUES (1, 7);
CREATE TABLE auth_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    data TEXT NOT NULL,
    disabled_cause TEXT DEFAULT NULL,
    identity_key TEXT DEFAULT NULL,
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER))
);
";

#[cfg(unix)]
pub(super) fn write_v7_omp_source(source: &Path, setup: &str) -> rusqlite::Connection {
    let agent = source.join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    let connection = rusqlite::Connection::open(agent.join("agent.db")).unwrap();
    connection
        .execute_batch(&format!(
            "PRAGMA journal_mode = WAL;\nPRAGMA wal_autocheckpoint = 0;\n{setup}"
        ))
        .unwrap();
    connection
}

#[cfg(unix)]
pub(super) fn omp_row_selector(entry: &str, id: i64) -> ProfileSelector {
    ProfileSelector {
        entry: entry.to_owned(),
        profile: Some(format!("row:{id}")),
    }
}

#[cfg(unix)]
pub(super) fn contains_bytes(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

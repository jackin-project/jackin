// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const TEST_CREDENTIALS: &str =
    r#"{"claudeAiOauth":{"accessToken":"test","refreshToken":"test"}}"#;

#[cfg(unix)]
pub(super) const OMP_TEST_FILE_LIMIT: usize = 8 * 1024 * 1024;

#[cfg(unix)]
pub(super) const OMP_CURRENT_DB: &[u8] = include_bytes!("fixtures/omp-real-current.db");

#[cfg(unix)]
pub(super) const OMP_CURRENT_WAL: &[u8] = include_bytes!("fixtures/omp-real-current.db-wal");

#[cfg(unix)]
pub(super) const OMP_REUSED_STALE_SUFFIX_DB: &[u8] =
    include_bytes!("fixtures/omp-reused-stale-suffix.db");

#[cfg(unix)]
pub(super) const OMP_REUSED_STALE_SUFFIX_WAL: &[u8] =
    include_bytes!("fixtures/omp-reused-stale-suffix.db-wal");

#[cfg(unix)]
pub(super) const OMP_REUSED_UNCOMMITTED_STALE_SUFFIX_DB: &[u8] =
    include_bytes!("fixtures/omp-reused-uncommitted-stale-suffix.db");

#[cfg(unix)]
pub(super) const OMP_REUSED_UNCOMMITTED_STALE_SUFFIX_WAL: &[u8] =
    include_bytes!("fixtures/omp-reused-uncommitted-stale-suffix.db-wal");

#[cfg(unix)]
pub(super) fn omp_test_selector() -> ProfileSelector {
    ProfileSelector {
        entry: "openai".to_owned(),
        profile: Some("work".to_owned()),
    }
}

#[cfg(unix)]
pub(super) fn write_omp_source(source: &Path, database: &[u8], wal: &[u8]) {
    let agent = source.join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(agent.join("agent.db"), database).unwrap();
    std::fs::write(agent.join("agent.db-wal"), wal).unwrap();
}

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

#[cfg(unix)]
pub(super) fn read_fixture_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[cfg(unix)]
pub(super) fn omp_test_checksum(
    bytes: &[u8],
    mut checksum: (u32, u32),
    little_endian: bool,
) -> (u32, u32) {
    assert!(bytes.len().is_multiple_of(8));
    for words in bytes.as_chunks::<8>().0 {
        let first: [u8; 4] = words[..4].try_into().unwrap();
        let second: [u8; 4] = words[4..].try_into().unwrap();
        let (first, second) = if little_endian {
            (u32::from_le_bytes(first), u32::from_le_bytes(second))
        } else {
            (u32::from_be_bytes(first), u32::from_be_bytes(second))
        };
        checksum.0 = checksum.0.wrapping_add(first).wrapping_add(checksum.1);
        checksum.1 = checksum.1.wrapping_add(second).wrapping_add(checksum.0);
    }
    checksum
}

#[cfg(unix)]
pub(super) fn append_omp_uncommitted_frame(wal: &mut Vec<u8>) {
    let page_size = read_fixture_u32(wal, 8) as usize;
    let frame_size = page_size + 24;
    let frame_count = (wal.len() - 32) / frame_size;
    assert!(frame_count > 0);
    let last_at = 32 + (frame_count - 1) * frame_size;
    let checksum = (
        read_fixture_u32(wal, last_at + 16),
        read_fixture_u32(wal, last_at + 20),
    );
    let mut frame = vec![0; frame_size];
    frame[..4].copy_from_slice(&wal[last_at..last_at + 4]);
    frame[8..16].copy_from_slice(&wal[16..24]);
    frame[24..].copy_from_slice(&wal[last_at + 24..last_at + 24 + page_size]);
    let little_endian = read_fixture_u32(wal, 0) == 0x377F_0682;
    let checksum = omp_test_checksum(&frame[..8], checksum, little_endian);
    let checksum = omp_test_checksum(&frame[24..], checksum, little_endian);
    frame[16..20].copy_from_slice(&checksum.0.to_be_bytes());
    frame[20..24].copy_from_slice(&checksum.1.to_be_bytes());
    wal.extend(frame);
}

#[cfg(unix)]
pub(super) fn assert_omp_sync_rejected(source: &Path, target: &Path, database: &[u8], wal: &[u8]) {
    write_omp_source(source, database, wal);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let error = provision_omp_auth_from_source_dir(
        target,
        AuthForwardMode::Sync,
        source,
        Some(AiProvider::OpenAi),
        Some(&omp_test_selector()),
    )
    .expect_err("invalid OMP snapshots must fail closed");
    assert!(!target.exists(), "invalid source must not be provisioned");
    assert!(!format!("{error:#}").contains("fixture-"));
}

#[cfg(unix)]
pub(super) fn sync_omp_source(source: &Path, home: &Path) -> (PathBuf, Vec<u8>) {
    sync_omp_source_as(source, home, &omp_row_selector("openai", 41))
}

pub(super) fn sync_omp_source_as(
    source: &Path,
    home: &Path,
    selector: &ProfileSelector,
) -> (PathBuf, Vec<u8>) {
    let target = home.join("role/omp/agent/agent.db");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let (outcome, mounted) = provision_omp_auth_from_source_dir(
        &target,
        AuthForwardMode::Sync,
        source,
        Some(AiProvider::OpenAi),
        Some(selector),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(target.as_path()));
    (target.clone(), std::fs::read(target).unwrap())
}

#[cfg(unix)]
pub(super) fn private_snapshot_parent(temp: &tempfile::TempDir) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let parent = temp.path().join("private-snapshot-parent");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    parent
}

#[cfg(unix)]
pub(super) fn assert_kimi_snapshot_credentials(kimi_target: &Path) {
    assert_eq!(
        std::fs::read_to_string(kimi_target.join("config.toml")).unwrap(),
        "version = \"old\"\n"
    );
    assert_eq!(
        std::fs::read_to_string(kimi_target.join("credentials/token")).unwrap(),
        "old-kimi"
    );
}

pub(super) fn seed_host_auth(temp: &tempfile::TempDir) {
    std::fs::write(
        temp.path().join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
    )
    .unwrap();
    let creds_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&creds_dir).unwrap();
    std::fs::write(creds_dir.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
}

pub(super) fn stage_host_secrets(temp: &tempfile::TempDir, content: &str) -> PathBuf {
    let host_home = temp.path().join("host_home");
    let amp_dir = host_home.join(".local/share/amp");
    std::fs::create_dir_all(&amp_dir).unwrap();
    std::fs::write(amp_dir.join("secrets.json"), content).unwrap();
    host_home
}

pub(super) fn stage_host_auth_json(temp: &tempfile::TempDir, tail: &str) -> (PathBuf, String) {
    let host_home = temp.path().join("host_home");
    let codex_dir = host_home.join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let content = format!(
        "{{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{{\"id_token\":\"{tail}\"}}}}",
    );
    std::fs::write(codex_dir.join("auth.json"), &content).unwrap();
    (host_home, content)
}

pub(super) fn stage_host_hosts_yml(temp: &tempfile::TempDir, token: &str) -> PathBuf {
    let host_home = temp.path().join("host_home");
    let gh_dir = host_home.join(".config/gh");
    std::fs::create_dir_all(&gh_dir).unwrap();
    std::fs::write(
        gh_dir.join("hosts.yml"),
        format!(
            "github.com:\n    oauth_token: {token}\n    git_protocol: https\n    user: alice\n",
        ),
    )
    .unwrap();
    host_home
}

pub(super) fn ctx(mode: GithubAuthMode, token: Option<&str>) -> GithubAuthContext {
    GithubAuthContext {
        mode,
        token: token.map(str::to_owned),
    }
}

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

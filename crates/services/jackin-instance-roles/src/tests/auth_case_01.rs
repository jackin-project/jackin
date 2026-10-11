// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(target_os = "macos")]
#[test]
fn claude_keychain_service_name_matches_claude_scheme() {
    use std::path::Path;
    let home = Path::new("/Users/donbeave");

    let scope = |dir: PathBuf| {
        jackin_core::claude_keychain_scope(&dir, home, &dir)
            .expect("scope")
            .service
    };

    assert_eq!(scope(home.join(".claude")), "Claude Code-credentials");
    assert_eq!(
        scope(home.join(".claude-chainargos")),
        "Claude Code-credentials-93aecf3d"
    );
    assert_eq!(
        scope(home.join(".claude-work")),
        "Claude Code-credentials-3342f2c7"
    );
}

#[cfg(unix)]
#[test]
fn omp_selected_snapshot_and_provision_include_only_committed_wal_state() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("omp-source");
    let _writer = write_v7_omp_source(
        &source,
        &format!(
            "{OMP_V7_SCHEMA}\n\
             INSERT INTO auth_credentials (id, provider, credential_type, data)\n\
             VALUES (41, 'openai', 'api_key', '{{\"key\":\"fixture-db-stale-token\"}}');\n\
             PRAGMA wal_checkpoint(TRUNCATE);\n\
             UPDATE auth_credentials SET data = '{{\"key\":\"fixture-wal-current-token\"}}' WHERE id = 41;"
        ),
    );
    let source_agent = source.join("agent");
    let source_db_before = std::fs::read(source_agent.join("agent.db")).unwrap();
    let source_wal_before = std::fs::read(source_agent.join("agent.db-wal")).unwrap();
    assert!(contains_bytes(&source_db_before, b"fixture-db-stale-token"));
    assert!(contains_bytes(
        &source_wal_before,
        b"fixture-wal-current-token"
    ));
    let snapshot_parent = private_snapshot_parent(&temp);
    let selector = omp_row_selector("openai", 41);

    let snapshot = capture_selected_source(
        Agent::Omp,
        Some(AiProvider::OpenAi),
        Some(&selector),
        &source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap()
    .expect("selected OMP source should be captured");
    let snapshot_db = snapshot.materialized_source_dir().join("agent/agent.db");
    let snapshot_bytes = std::fs::read(&snapshot_db).unwrap();
    assert!(contains_bytes(
        &snapshot_bytes,
        b"fixture-wal-current-token"
    ));
    assert!(!contains_bytes(&snapshot_bytes, b"fixture-db-stale-token"));
    assert!(!snapshot_db.with_file_name("agent.db-wal").exists());
    assert_eq!(snapshot.descriptor().selector.as_ref(), Some(&selector));

    let target = temp.path().join("role/omp/agent/agent.db");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let (outcome, mounted) = provision_omp_auth_from_source_dir(
        &target,
        AuthForwardMode::Sync,
        &source,
        Some(AiProvider::OpenAi),
        Some(&selector),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(target.as_path()));
    let provisioned = std::fs::read(&target).unwrap();
    assert_eq!(provisioned, snapshot_bytes);
    assert_eq!(
        std::fs::read(source_agent.join("agent.db")).unwrap(),
        source_db_before,
        "snapshotting must not checkpoint or rewrite the source database"
    );
    assert_eq!(
        std::fs::read(source_agent.join("agent.db-wal")).unwrap(),
        source_wal_before,
        "snapshotting must not truncate or rewrite the source WAL"
    );
    // NOTE: no `-shm` assertion here: the fixture writer is deliberately
    // held open (dropping it would checkpoint the WAL away), and an open
    // WAL connection owns a shared-memory file by design.
}

#[cfg(unix)]
#[test]
fn omp_sync_rejects_checksum_and_joint_salt_checksum_corruption() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("omp-source");
    let frame_size = read_fixture_u32(OMP_CURRENT_WAL, 8) as usize + 24;
    let frame_count = (OMP_CURRENT_WAL.len() - 32) / frame_size;
    let last_commit = (0..frame_count)
        .rev()
        .find(|index| read_fixture_u32(OMP_CURRENT_WAL, 32 + index * frame_size + 4) > 0)
        .expect("synthetic WAL fixture has a committed frame");
    let final_frame = 32 + last_commit * frame_size;
    for (name, corrupt_salt) in [("checksum", false), ("joint", true)] {
        let mut wal = OMP_CURRENT_WAL.to_vec();
        if corrupt_salt {
            wal[final_frame + 8] ^= 1;
        }
        wal[final_frame + 16] ^= 1;
        let target = temp.path().join(format!("role-{name}/omp/agent/agent.db"));
        assert_omp_sync_rejected(&source, &target, OMP_CURRENT_DB, &wal);
    }
}

#[cfg(unix)]
#[test]
fn omp_sync_supports_little_and_big_endian_wal_checksums() {
    // Success-path provisioning through a synthetic v7 store. Big-endian
    // checksum LOGIC is covered at the validator level
    // (`jackin-omp-store` fixtures), which a little-endian host cannot
    // regenerate through SQLite itself.
    let temp = tempdir().unwrap();
    let source = temp.path().join("omp-source");
    let _writer = write_v7_omp_source(
        &source,
        &format!(
            "{OMP_V7_SCHEMA}\n\
             INSERT INTO auth_credentials (id, provider, credential_type, data)\n\
             VALUES (41, 'openai', 'api_key', '{{\"key\":\"fixture-wal-current-token\"}}');"
        ),
    );
    let (_, materialized) = sync_omp_source(&source, temp.path());
    assert!(contains_bytes(&materialized, b"fixture-wal-current-token"));
}

#[cfg(unix)]
#[test]
fn omp_sync_materializes_schema_and_credentials_from_wal_only_pages() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("omp-source");
    let _writer = write_v7_omp_source(
        &source,
        &format!(
            "{OMP_V7_SCHEMA}\n\
             INSERT INTO auth_credentials (id, provider, credential_type, data)\n\
             VALUES (41, 'openai', 'api_key', '{{\"key\":\"fixture-schema-wal-token\"}}');"
        ),
    );
    // Nothing checkpointed: the main image carries no schema or secret.
    let main = std::fs::read(source.join("agent/agent.db")).unwrap();
    assert!(!contains_bytes(&main, b"auth_credentials"));
    assert!(!contains_bytes(&main, b"fixture-schema-wal-token"));
    let (target, materialized) = sync_omp_source(&source, temp.path());
    assert!(contains_bytes(&materialized, b"fixture-schema-wal-token"));
    let discovered = jackin_config::discover_account_directory(
        Agent::Omp,
        target.parent().unwrap().parent().unwrap(),
        temp.path(),
    )
    .unwrap()
    .expect("WAL-only schema is materialized before account discovery");
    assert_eq!(discovered.provider, Some(AiProvider::OpenAi));
    assert_eq!(
        discovered.source_selector,
        Some(omp_row_selector("openai", 41))
    );
}

#[cfg(unix)]
#[test]
fn omp_sync_rejects_reused_wal_generations_instead_of_falling_back_to_old_state() {
    for (name, database, wal) in [
        (
            "stale-committed-tail",
            OMP_REUSED_STALE_SUFFIX_DB,
            OMP_REUSED_STALE_SUFFIX_WAL,
        ),
        (
            "stale-after-uncommitted-spill",
            OMP_REUSED_UNCOMMITTED_STALE_SUFFIX_DB,
            OMP_REUSED_UNCOMMITTED_STALE_SUFFIX_WAL,
        ),
    ] {
        let temp = tempdir().unwrap();
        let source = temp.path().join("omp-source");
        let target = temp.path().join(format!("role-{name}/omp/agent/agent.db"));
        assert_omp_sync_rejected(&source, &target, database, wal);
    }
}

#[cfg(unix)]
#[test]
fn omp_sync_accepts_a_complete_uncommitted_tail_but_rejects_a_partial_frame() {
    let temp = tempdir().unwrap();
    let base = temp.path().join("omp-base");
    let _writer = write_v7_omp_source(
        &base,
        &format!(
            "{OMP_V7_SCHEMA}\n\
             INSERT INTO auth_credentials (id, provider, credential_type, data)\n\
             VALUES (41, 'openai', 'api_key', '{{\"key\":\"fixture-wal-current-token\"}}');"
        ),
    );
    let database = std::fs::read(base.join("agent/agent.db")).unwrap();
    let mut complete = std::fs::read(base.join("agent/agent.db-wal")).unwrap();
    append_omp_uncommitted_frame(&mut complete);
    let source = temp.path().join("omp-source");
    write_omp_source(&source, &database, &complete);
    let (_, materialized) = sync_omp_source(&source, temp.path());
    assert!(contains_bytes(&materialized, b"fixture-wal-current-token"));

    let mut partial = complete;
    partial.extend_from_slice(&[0; 19]);
    let target = temp.path().join("partial/omp/agent/agent.db");
    assert_omp_sync_rejected(&source, &target, &database, &partial);
}

#[cfg(unix)]
#[test]
fn omp_sync_rejects_bad_page_headers_salts_and_file_size_bounds() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("omp-source");
    let mut database = OMP_CURRENT_DB.to_vec();
    database[16..18].copy_from_slice(&0_u16.to_be_bytes());
    write_omp_source(&source, &database, &[]);
    let error = provision_omp_auth_from_source_dir(
        &temp.path().join("invalid-db/omp/agent/agent.db"),
        AuthForwardMode::Sync,
        &source,
        Some(AiProvider::OpenAi),
        Some(&omp_test_selector()),
    )
    .expect_err("invalid SQLite page size must be rejected");
    assert_eq!(error.to_string(), "OMP credential source is unavailable");

    let frame_size = read_fixture_u32(OMP_CURRENT_WAL, 8) as usize + 24;
    let frame_count = (OMP_CURRENT_WAL.len() - 32) / frame_size;
    let last_commit = (0..frame_count)
        .rev()
        .find(|index| read_fixture_u32(OMP_CURRENT_WAL, 32 + index * frame_size + 4) > 0)
        .expect("synthetic WAL fixture has a committed frame");
    let last_frame = 32 + last_commit * frame_size;
    let mut bad_salt = OMP_CURRENT_WAL.to_vec();
    bad_salt[last_frame + 8] ^= 1;
    write_omp_source(&source, OMP_CURRENT_DB, &bad_salt);
    let error = provision_omp_auth_from_source_dir(
        &temp.path().join("invalid-salt/omp/agent/agent.db"),
        AuthForwardMode::Sync,
        &source,
        Some(AiProvider::OpenAi),
        Some(&omp_test_selector()),
    )
    .expect_err("a mismatched current-generation salt must fail closed");
    assert_eq!(error.to_string(), "OMP credential source is unavailable");

    for oversized in ["database", "wal"] {
        let database = if oversized == "database" {
            vec![0; OMP_TEST_FILE_LIMIT + 1]
        } else {
            OMP_CURRENT_DB.to_vec()
        };
        let wal = if oversized == "wal" {
            vec![0; OMP_TEST_FILE_LIMIT + 1]
        } else {
            OMP_CURRENT_WAL.to_vec()
        };
        write_omp_source(&source, &database, &wal);
        let error = provision_omp_auth_from_source_dir(
            &temp
                .path()
                .join(format!("oversized-{oversized}/omp/agent/agent.db")),
            AuthForwardMode::Sync,
            &source,
            Some(AiProvider::OpenAi),
            Some(&omp_test_selector()),
        )
        .expect_err("oversized OMP source files must be rejected");
        assert_eq!(
            error.to_string(),
            "OMP credential source exceeds a resource limit"
        );
    }
}

#[cfg(unix)]
#[test]
fn credential_permission_repair_fails_closed_on_injected_failures() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("credential.json");
    std::fs::write(&path, TEST_CREDENTIALS).unwrap();

    for failure in [
        PermissionRepairFailure::Stat,
        PermissionRepairFailure::Chmod,
        PermissionRepairFailure::Verify,
    ] {
        let _guard = inject_permission_repair_failure(failure);
        let error = repair_permissions(&path).expect_err("injected failure must abort repair");
        assert!(
            error
                .to_string()
                .contains("injected credential permission repair failure"),
            "unexpected error for {failure:?}: {error:#}"
        );
    }
}

#[test]
fn validate_rejects_non_directory() {
    let temp = tempdir().unwrap();
    let missing = temp.path().join("nope");
    validate_sync_source_dir(Agent::Codex, &missing, temp.path()).unwrap_err();
}

#[test]
fn validate_claude_accepts_file_credentials_rejects_bare_folder() {
    let temp = tempdir().unwrap();
    let good = temp.path().join("claude-good");
    std::fs::create_dir_all(&good).unwrap();
    std::fs::write(good.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
    validate_sync_source_dir(Agent::Claude, &good, temp.path()).unwrap();

    // No .credentials.json file; host_home is a temp dir so the macOS
    // Keychain probe is skipped — must be rejected, not accepted.
    let bare = temp.path().join("claude-bare");
    std::fs::create_dir_all(&bare).unwrap();
    let err = validate_sync_source_dir(Agent::Claude, &bare, temp.path()).unwrap_err();
    assert!(
        err.to_string().contains("Claude"),
        "msg should name the agent: {err}"
    );
}

#[cfg(unix)]
#[test]
fn selected_snapshot_pins_claude_bytes_and_descriptor_revision() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("claude");
    let target = temp.path().join("role/claude");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
    std::fs::write(source.join(".claude.json"), r#"{"account":"old"}"#).unwrap();
    let snapshot_parent = private_snapshot_parent(&temp);

    let snapshot = capture_selected_source(
        Agent::Claude,
        None,
        None,
        &source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap()
    .expect("source snapshot");
    assert_eq!(snapshot.descriptor().agent, Agent::Claude);
    assert_eq!(snapshot.descriptor().source_dir, source);
    assert!(!snapshot.content_revision().is_empty());

    std::fs::write(
        source.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"new"}}"#,
    )
    .unwrap();
    std::fs::write(source.join(".claude.json"), r#"{"account":"new"}"#).unwrap();
    std::fs::create_dir_all(&target).unwrap();

    let (outcome, mounted) = provision_claude_auth_from_config_dir(
        &target.join("account.json"),
        &target.join("credentials.json"),
        AuthForwardMode::Sync,
        temp.path(),
        snapshot.materialized_source_dir(),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(mounted);
    assert_eq!(
        std::fs::read_to_string(target.join("credentials.json")).unwrap(),
        TEST_CREDENTIALS
    );
    assert_eq!(
        std::fs::read_to_string(target.join("account.json")).unwrap(),
        r#"{"account":"old"}"#
    );
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

use rusqlite::Connection;
use std::ffi::OsString;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[derive(Clone, Copy)]
struct OmpRowFixture<'a> {
    id: i64,
    provider: &'a str,
    credential_type: &'a str,
    data: &'a str,
    disabled_cause: Option<&'a str>,
    identity_key: Option<&'a str>,
    created_at: i64,
    updated_at: i64,
}

fn fixture_json(marker: &str) -> String {
    serde_json::json!({"key": marker}).to_string()
}

fn valid_row<'a>(
    id: i64,
    provider: &'static str,
    data: &'a str,
    identity_key: &'static str,
) -> OmpRowFixture<'a> {
    OmpRowFixture {
        id,
        provider,
        credential_type: "api_key",
        data,
        disabled_cause: None,
        identity_key: Some(identity_key),
        created_at: 1_700_000_000 + id,
        updated_at: 1_700_000_100 + id,
    }
}

fn insert_row(writer: &Connection, row: OmpRowFixture<'_>) {
    writer
        .execute(
            "INSERT INTO auth_credentials
                (id, provider, credential_type, data, disabled_cause, identity_key, \
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                row.id,
                row.provider,
                row.credential_type,
                row.data,
                row.disabled_cause,
                row.identity_key,
                row.created_at,
                row.updated_at,
            ],
        )
        .expect("fixture credential row is inserted");
}

fn write_omp_rows(source: &Path, rows: &[OmpRowFixture<'_>]) -> Connection {
    let writer = write_v7_omp_source(source, OMP_V7_SCHEMA);
    for row in rows {
        insert_row(&writer, *row);
    }
    writer
}

fn close_omp_writer(writer: Connection) {
    let _: (i64, i64, i64) = writer
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .expect("fixture WAL checkpoints before the writer closes");
    drop(writer);
}

fn row_selector(entry: &str, profile: Option<&str>) -> ProfileSelector {
    ProfileSelector {
        entry: entry.to_owned(),
        profile: profile.map(str::to_owned),
    }
}

fn sync_omp(
    source: &Path,
    target: &Path,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_omp_auth_from_source_dir(target, AuthForwardMode::Sync, source, provider, selector)
}

fn write_private_target(target: &Path, contents: &[u8]) {
    std::fs::create_dir_all(target.parent().expect("target has a parent"))
        .expect("target parent is created");
    std::fs::write(target, contents).expect("existing target is seeded");
    std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o600))
        .expect("existing target is private");
}

fn assert_private_target(target: &Path) {
    let mode = std::fs::metadata(target)
        .expect("target metadata is readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "target remains private");
}

fn assert_no_target_sidecars(target: &Path) {
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = target.with_file_name(format!("agent.db{suffix}"));
        assert!(!sidecar.exists(), "unexpected target sidecar: {sidecar:?}");
    }
}

fn assert_target_parent_entries(target: &Path, target_exists: bool) {
    let parent = target.parent().expect("target has a parent");
    let mut entries = std::fs::read_dir(parent)
        .expect("target parent can be inspected")
        .map(|entry| entry.expect("target parent entry can be read").file_name())
        .collect::<Vec<OsString>>();
    entries.sort();
    let expected = if target_exists {
        vec![OsString::from("agent.db")]
    } else {
        Vec::new()
    };
    assert_eq!(
        entries, expected,
        "no provisional files remain in target parent"
    );
}

fn assert_rejected_target(
    source: &Path,
    target: &Path,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    existing_target: bool,
) -> String {
    let parent = target.parent().expect("target has a parent");
    std::fs::create_dir_all(parent).expect("target parent is created");
    let previous = b"fixture-existing-target-must-survive-selection-failure";
    if existing_target {
        write_private_target(target, previous);
    }

    let error = sync_omp(source, target, provider, selector)
        .expect_err("invalid OMP selection must fail closed");
    if existing_target {
        let contents = std::fs::read(target).expect("existing target remains readable");
        assert_eq!(contents.as_slice(), previous);
        assert_private_target(target);
    } else {
        assert!(
            !target.exists(),
            "failed selection leaves an absent target absent"
        );
    }
    assert_no_target_sidecars(target);
    assert_target_parent_entries(target, existing_target);
    format!("{error:#}")
}

fn assert_rejected_both_target_states(
    base: &Path,
    name: &str,
    rows: &[OmpRowFixture<'_>],
    provider: Option<AiProvider>,
    selector: &ProfileSelector,
) {
    let source = base.join(format!("source-{name}/.omp"));
    let writer = write_omp_rows(&source, rows);
    let existing_target = base.join(format!("target-{name}-existing/agent.db"));
    let existing_error =
        assert_rejected_target(&source, &existing_target, provider, Some(selector), true);
    assert!(!existing_error.contains("fixture-"));
    let absent_target = base.join(format!("target-{name}-absent/agent.db"));
    let absent_error =
        assert_rejected_target(&source, &absent_target, provider, Some(selector), false);
    assert!(!absent_error.contains("fixture-"));
    drop(writer);
}

type ExportedRow = (
    i64,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    i64,
);

fn read_exported_rows(target: &Path) -> Vec<ExportedRow> {
    let connection = Connection::open(target).expect("exported database opens");
    let mut statement = connection
        .prepare(
            "SELECT id, provider, credential_type, data, disabled_cause, identity_key,
                    created_at, updated_at
             FROM auth_credentials ORDER BY id",
        )
        .expect("exported credential query prepares");
    let rows: Vec<ExportedRow> = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })
        .expect("exported credential query runs")
        .collect::<Result<Vec<_>, _>>()
        .expect("exported credential rows decode");
    rows
}

fn assert_standalone_export(target: &Path, expected: OmpRowFixture<'_>) {
    let connection = Connection::open(target).expect("exported database opens for schema checks");
    let (version_count, version_min, version_max): (i64, i64, i64) = connection
        .query_row(
            "SELECT COUNT(*), MIN(version), MAX(version) FROM auth_schema_version",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("exported schema version is readable");
    assert_eq!((version_count, version_min, version_max), (1, 7, 7));

    let mut columns_statement = connection
        .prepare("PRAGMA table_info(auth_credentials)")
        .expect("exported table columns query prepares");
    let columns = columns_statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("exported table columns query runs")
        .collect::<Result<Vec<_>, _>>()
        .expect("exported table column names decode");
    assert_eq!(
        columns,
        vec![
            "id",
            "provider",
            "credential_type",
            "data",
            "disabled_cause",
            "identity_key",
            "created_at",
            "updated_at",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    );

    let mut schema_statement = connection
        .prepare(
            "SELECT type, name FROM main.sqlite_master
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .expect("exported schema object query prepares");
    let schema_objects = schema_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("exported schema object query runs")
        .collect::<Result<Vec<_>, _>>()
        .expect("exported schema object names decode");
    assert_eq!(
        schema_objects,
        vec![
            ("index".to_owned(), "idx_auth_provider".to_owned()),
            ("index".to_owned(), "idx_auth_provider_identity".to_owned()),
            ("table".to_owned(), "auth_credentials".to_owned()),
            ("table".to_owned(), "auth_schema_version".to_owned()),
        ]
    );
    drop(schema_statement);
    drop(columns_statement);
    drop(connection);

    assert_eq!(
        read_exported_rows(target),
        vec![(
            expected.id,
            expected.provider.to_owned(),
            expected.credential_type.to_owned(),
            expected.data.to_owned(),
            expected.disabled_cause.map(str::to_owned),
            expected.identity_key.map(str::to_owned),
            expected.created_at,
            expected.updated_at,
        )]
    );
    assert_private_target(target);
    assert_no_target_sidecars(target);
}

#[test]
fn omp_explicit_multirow_sync_exports_only_the_selected_current_row() {
    let temp = tempdir().expect("fixture directory is created");
    let source = temp.path().join("source/.omp");
    let deleted = valid_row(
        99,
        "openai",
        r#"{"key":"fixture-deleted-freelist-canary"}"#,
        "fixture-deleted-identity",
    );
    let first_data = fixture_json("fixture-sibling-row-41");
    let first = valid_row(41, "openai", &first_data, "fixture-identity-41");
    let selected = valid_row(
        42,
        "openai",
        r#"{"key":"fixture-selected-row-42"}"#,
        "fixture-identity-42",
    );
    let writer = write_v7_omp_source(&source, OMP_V7_SCHEMA);
    writer
        .pragma_update(None, "secure_delete", "OFF")
        .expect("deleted-row canary remains a fixture-only freelist value");
    insert_row(&writer, deleted);
    writer
        .execute("DELETE FROM auth_credentials WHERE id = 99", [])
        .expect("deleted-row canary is removed from the logical source");
    insert_row(&writer, first);
    insert_row(&writer, selected);

    let source_db = source.join("agent/agent.db");
    let source_wal = source.join("agent/agent.db-wal");
    let source_db_before = std::fs::read(&source_db).expect("source database is readable");
    let source_wal_before = std::fs::read(&source_wal).expect("source WAL is readable");
    assert!(contains_bytes(
        &source_wal_before,
        b"fixture-deleted-freelist-canary"
    ));
    let target = temp.path().join("role/omp/agent.db");
    let selector = omp_row_selector("openai", 42);
    std::fs::create_dir_all(target.parent().expect("target has a parent"))
        .expect("role target parent is created");

    let (outcome, mounted) = sync_omp(&source, &target, Some(AiProvider::OpenAi), Some(&selector))
        .expect("explicit row selection provisions successfully");

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(target.as_path()));
    assert_standalone_export(&target, selected);
    let target_bytes = std::fs::read(&target).expect("selected export is readable");
    assert!(contains_bytes(&target_bytes, b"fixture-selected-row-42"));
    assert!(!contains_bytes(&target_bytes, b"fixture-sibling-row-41"));
    assert!(!contains_bytes(
        &target_bytes,
        b"fixture-deleted-freelist-canary"
    ));
    assert_eq!(
        std::fs::read(&source_db).expect("source database remains readable"),
        source_db_before,
        "selected export must not rewrite the source database"
    );
    assert_eq!(
        std::fs::read(&source_wal).expect("source WAL remains readable"),
        source_wal_before,
        "selected export must not checkpoint or rewrite the source WAL"
    );
    drop(writer);
}

#[test]
fn omp_account_binding_captures_and_provisions_the_pinned_selected_row() {
    let temp = tempdir().expect("fixture directory is created");
    let source = temp.path().join("source/.omp");
    let first_data = fixture_json("fixture-pinned-sibling-41");
    let first = valid_row(41, "openai", &first_data, "fixture-pinned-identity-41");
    let selected = valid_row(
        42,
        "openai",
        r#"{"key":"fixture-pinned-selected-42"}"#,
        "fixture-pinned-identity-42",
    );
    let writer = write_omp_rows(&source, &[first, selected]);
    let selector = omp_row_selector("openai", 42);
    let mut binding = InstanceAuthBinding::new(
        "selected-omp-account",
        Agent::Omp,
        AuthForwardMode::Sync,
        Some(source.clone()),
    );
    binding.source_provider = Some(AiProvider::OpenAi);
    binding.source_selector = Some(selector.clone());
    let role_root = temp.path().join("capture-role");
    let captured = capture_selected_account_sources(&[binding], temp.path(), &role_root)
        .expect("configured OMP source is captured before provisioning");
    let admitted = captured.first().expect("one OMP binding was captured");
    let snapshot = admitted
        .selected_source
        .as_ref()
        .expect("OMP binding carries its immutable selected snapshot");
    assert_eq!(snapshot.descriptor().agent, Agent::Omp);
    assert_eq!(snapshot.descriptor().provider, Some(AiProvider::OpenAi));
    assert_eq!(snapshot.descriptor().selector.as_ref(), Some(&selector));
    assert_eq!(snapshot.descriptor().source_dir, source);
    assert_eq!(admitted.source_selector.as_ref(), Some(&selector));

    let snapshot_db = snapshot.materialized_source_dir().join("agent/agent.db");
    let snapshot_rows = read_exported_rows(&snapshot_db);
    assert_eq!(snapshot_rows.len(), 1);
    assert_eq!(snapshot_rows[0].0, 42);
    assert_eq!(
        snapshot_rows[0].3.as_str(),
        selected.data,
        "captured source contains the exact selected row"
    );
    assert!(!snapshot_db.with_file_name("agent.db-wal").exists());

    writer
        .execute(
            "UPDATE auth_credentials SET data = ?1 WHERE id = 42",
            [r#"{"key":"fixture-after-capture-replacement"}"#],
        )
        .expect("source can change after the admitted capture");
    let role = temp.path().join("worker-role");
    let home = role.join("home");
    std::fs::create_dir_all(&home).expect("worker home is created");
    let (slot, outcome) =
        jackin_instance_agents::provision_omp_slot(&role, &home, temp.path(), admitted, None)
            .expect("worker provisions from the admitted binding snapshot");
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(slot.account_id, "selected-omp-account");
    assert_eq!(slot.credential_paths.len(), 1);
    let target = slot
        .credential_paths
        .first()
        .expect("OMP slot exposes its selected database")
        .clone();
    assert_standalone_export(&target, selected);
    let target_bytes = std::fs::read(&target).expect("pinned selected export is readable");
    assert!(contains_bytes(&target_bytes, b"fixture-pinned-selected-42"));
    assert!(!contains_bytes(
        &target_bytes,
        b"fixture-after-capture-replacement"
    ));
    drop(writer);
}

#[test]
fn omp_captured_binding_descriptor_mutations_fail_before_slot_publication() {
    let temp = tempdir().expect("fixture directory is created");
    let source = temp.path().join("source/.omp");
    let first = valid_row(
        41,
        "openai",
        r#"{"key":"fixture-binding-sibling"}"#,
        "fixture-binding-identity-41",
    );
    let selected = valid_row(
        42,
        "openai",
        r#"{"key":"fixture-binding-selected"}"#,
        "fixture-binding-identity-42",
    );
    let writer = write_omp_rows(&source, &[first, selected]);
    let selector = omp_row_selector("openai", 42);
    let mut binding = InstanceAuthBinding::new(
        "bound-omp-account",
        Agent::Omp,
        AuthForwardMode::Sync,
        Some(source.clone()),
    );
    binding.source_provider = Some(AiProvider::OpenAi);
    binding.source_selector = Some(selector);
    let captured = capture_selected_account_sources(
        &[binding],
        temp.path(),
        &temp.path().join("initial-role"),
    )
    .expect("source binding is captured");
    let admitted = captured.first().expect("one binding was captured");
    let manifest = simple_manifest(&temp);
    let paths = JackinPaths::for_tests(temp.path());
    let alternate_source = temp.path().join("changed-source/.omp");

    let mut provider_changed = admitted.clone();
    provider_changed.source_provider = Some(AiProvider::Anthropic);
    let mut selector_changed = admitted.clone();
    selector_changed.source_selector = Some(omp_row_selector("openai", 41));
    let mut source_changed = admitted.clone();
    source_changed.sync_source_dir = Some(alternate_source);
    for (mutation, changed) in [
        ("provider", provider_changed),
        ("selector", selector_changed),
        ("source", source_changed),
    ] {
        let container = format!("mutated-omp-{mutation}");
        let error = RoleState::prepare_for_bindings(
            &paths,
            &container,
            &manifest,
            &[changed],
            &GithubAuthContext::default(),
            temp.path(),
            Agent::Omp,
        )
        .expect_err("changed selected-source descriptor is rejected before workers start");
        assert!(format!("{error:#}").contains("descriptor changed after credential capture"));
        assert!(
            !paths.data_dir.join(container).join("omp").exists(),
            "descriptor mutation {mutation} must fail before OMP slot publication"
        );
    }
    drop(writer);
}

#[test]
fn omp_missing_selector_rejects_ambiguous_rows_before_provider_filtering() {
    let temp = tempdir().expect("fixture directory is created");
    let first = valid_row(
        41,
        "openai",
        r#"{"key":"fixture-openai-first"}"#,
        "fixture-no-selector-identity-41",
    );
    let second_same_provider = valid_row(
        42,
        "openai",
        r#"{"key":"fixture-openai-second"}"#,
        "fixture-no-selector-identity-42",
    );
    let same_source = temp.path().join("same-provider/.omp");
    let same_writer = write_omp_rows(&same_source, &[first, second_same_provider]);
    let same_target = temp.path().join("same-provider-role/agent.db");
    let same_error = assert_rejected_target(
        &same_source,
        &same_target,
        Some(AiProvider::OpenAi),
        None,
        true,
    );
    assert!(same_error.contains("OMP account selection is missing or ambiguous"));

    let second_other_provider = valid_row(
        42,
        "anthropic",
        r#"{"key":"fixture-anthropic-second"}"#,
        "fixture-no-selector-identity-anthropic",
    );
    let different_source = temp.path().join("different-provider/.omp");
    let different_writer = write_omp_rows(&different_source, &[first, second_other_provider]);
    let different_target = temp.path().join("different-provider-role/agent.db");
    let different_error = assert_rejected_target(
        &different_source,
        &different_target,
        Some(AiProvider::OpenAi),
        None,
        false,
    );
    assert!(different_error.contains("OMP account selection is missing or ambiguous"));
    drop(same_writer);
    drop(different_writer);
}

#[test]
fn omp_single_row_without_selector_remains_supported_and_provider_mismatch_fails() {
    let temp = tempdir().expect("fixture directory is created");
    let source = temp.path().join("single-row/.omp");
    let selected = valid_row(
        41,
        "openai",
        r#"{"key":"fixture-single-row-no-selector"}"#,
        "fixture-single-row-identity",
    );
    let writer = write_omp_rows(&source, &[selected]);
    let target = temp.path().join("single-row-role/agent.db");
    std::fs::create_dir_all(target.parent().expect("target has a parent"))
        .expect("role target parent is created");
    let (outcome, mounted) = sync_omp(&source, &target, None, None)
        .expect("one unambiguous row remains selectable without a selector or provider");
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(target.as_path()));
    assert_standalone_export(&target, selected);

    let mismatch_target = temp.path().join("provider-mismatch-role/agent.db");
    let mismatch = row_selector("openai", Some("row:41"));
    let error = assert_rejected_target(
        &source,
        &mismatch_target,
        Some(AiProvider::Anthropic),
        Some(&mismatch),
        true,
    );
    assert!(!error.contains("fixture-"));
    drop(writer);
}

#[test]
fn omp_invalid_explicit_rows_fail_without_fallback_or_target_changes() {
    let temp = tempdir().expect("fixture directory is created");
    let first_data = fixture_json("fixture-valid-sibling-41");
    let first = valid_row(
        41,
        "openai",
        &first_data,
        "fixture-invalid-matrix-identity-41",
    );
    let second = valid_row(
        42,
        "openai",
        r#"{"key":"fixture-valid-selected-42"}"#,
        "fixture-invalid-matrix-identity-42",
    );
    let provider = Some(AiProvider::OpenAi);

    assert_rejected_both_target_states(
        temp.path(),
        "stale-id",
        &[first, second],
        provider,
        &omp_row_selector("openai", 99),
    );
    let mut disabled_selected = second;
    disabled_selected.disabled_cause = Some("fixture-disabled");
    assert_rejected_both_target_states(
        temp.path(),
        "disabled-selected",
        &[first, disabled_selected],
        provider,
        &omp_row_selector("openai", 42),
    );
    assert_rejected_both_target_states(
        temp.path(),
        "wrong-entry",
        &[first, second],
        provider,
        &omp_row_selector("anthropic", 42),
    );
    assert_rejected_both_target_states(
        temp.path(),
        "wrong-provider",
        &[first, second],
        Some(AiProvider::Anthropic),
        &omp_row_selector("openai", 42),
    );
    assert_rejected_both_target_states(
        temp.path(),
        "missing-profile",
        &[first, second],
        provider,
        &row_selector("openai", None),
    );

    for (name, profile) in [
        ("noncanonical-leading-zero", "row:041"),
        ("noncanonical-zero", "row:0"),
        ("invalid-profile-text", "fixture-not-a-row"),
    ] {
        assert_rejected_both_target_states(
            temp.path(),
            name,
            &[first, second],
            provider,
            &row_selector("openai", Some(profile)),
        );
    }

    let mut unsupported_selected = second;
    unsupported_selected.credential_type = "password";
    assert_rejected_both_target_states(
        temp.path(),
        "unsupported-selected-credential",
        &[first, unsupported_selected],
        provider,
        &omp_row_selector("openai", 42),
    );
    let mut malformed_selected = second;
    malformed_selected.data = r#"{"key":42}"#;
    assert_rejected_both_target_states(
        temp.path(),
        "invalid-selected-credential",
        &[first, malformed_selected],
        provider,
        &omp_row_selector("openai", 42),
    );
    let mut malformed_sibling = first;
    malformed_sibling.data = r#"{"key":42}"#;
    assert_rejected_both_target_states(
        temp.path(),
        "invalid-active-sibling",
        &[malformed_sibling, second],
        provider,
        &omp_row_selector("openai", 42),
    );
}

#[test]
fn omp_unsupported_schema_fails_without_target_changes_or_fallback() {
    let temp = tempdir().expect("fixture directory is created");
    let source = temp.path().join("unsupported-schema/.omp");
    let writer = write_v7_omp_source(
        &source,
        r#"
        CREATE TABLE auth_schema_version (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL
        );
        INSERT INTO auth_schema_version (id, version) VALUES (1, 7);
        CREATE TABLE auth_credentials (
            id INTEGER PRIMARY KEY,
            provider TEXT NOT NULL,
            credential_type TEXT NOT NULL,
            data TEXT NOT NULL
        );
        INSERT INTO auth_credentials (id, provider, credential_type, data)
            VALUES (41, 'openai', 'api_key', '{"key":"fixture-schema-sibling"}');
        INSERT INTO auth_credentials (id, provider, credential_type, data)
            VALUES (42, 'openai', 'api_key', '{"key":"fixture-schema-selected"}');
        "#,
    );
    let selector = omp_row_selector("openai", 42);
    let existing_target = temp.path().join("unsupported-schema-existing/agent.db");
    let error = assert_rejected_target(
        &source,
        &existing_target,
        Some(AiProvider::OpenAi),
        Some(&selector),
        true,
    );
    assert!(!error.contains("fixture-"));
    let absent_target = temp.path().join("unsupported-schema-absent/agent.db");
    let error = assert_rejected_target(
        &source,
        &absent_target,
        Some(AiProvider::OpenAi),
        Some(&selector),
        false,
    );
    assert!(!error.contains("fixture-"));
    drop(writer);
}

#[test]
fn omp_multirow_database_leaf_swap_fails_before_publishing() {
    use jackin_omp_store::{CapturePhase, set_capture_hook};
    use std::cell::Cell;
    use std::rc::Rc;

    let temp = tempdir().expect("fixture directory is created");
    let source = temp.path().join("source/.omp");
    let source_sibling_data = fixture_json("fixture-race-sibling-41");
    let source_rows = [
        valid_row(
            41,
            "openai",
            &source_sibling_data,
            "fixture-race-identity-41",
        ),
        valid_row(
            42,
            "openai",
            r#"{"key":"fixture-race-selected-42"}"#,
            "fixture-race-identity-42",
        ),
    ];
    close_omp_writer(write_omp_rows(&source, &source_rows));
    let replacement_source = temp.path().join("replacement/.omp");
    let replacement_sibling_data = fixture_json("fixture-race-replacement-41");
    let replacement_selected_data = fixture_json("fixture-race-replacement-42");
    let replacement_rows = [
        valid_row(
            41,
            "openai",
            &replacement_sibling_data,
            "fixture-race-replacement-identity-41",
        ),
        valid_row(
            42,
            "openai",
            &replacement_selected_data,
            "fixture-race-replacement-identity-42",
        ),
    ];
    close_omp_writer(write_omp_rows(&replacement_source, &replacement_rows));

    let database = source.join("agent/agent.db");
    let replacement = replacement_source.join("agent/agent.db");
    let original_inode = std::fs::metadata(&database)
        .expect("original source database metadata is readable")
        .ino();
    let replacement_inode = std::fs::metadata(&replacement)
        .expect("replacement database metadata is readable")
        .ino();
    assert_ne!(original_inode, replacement_inode);
    let displaced = source.join("agent/agent.db.displaced");
    let database_hook = database.clone();
    let replacement_hook = replacement.clone();
    let displaced_hook = displaced.clone();
    let hook_calls = Rc::new(Cell::new(0));
    let hook_count = Rc::clone(&hook_calls);
    let hook = set_capture_hook(move |phase| {
        if matches!(
            phase,
            CapturePhase::BeforeDatabaseOpen {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            hook_count.set(hook_count.get() + 1);
            std::fs::rename(&database_hook, &displaced_hook)
                .expect("original source database is displaced at the capture race point");
            std::fs::rename(&replacement_hook, &database_hook)
                .expect("replacement database is installed at the capture race point");
        }
    });

    let target = temp.path().join("role/agent.db");
    let previous = b"fixture-target-must-survive-multirow-leaf-swap";
    write_private_target(&target, previous);
    let selector = omp_row_selector("openai", 42);
    let result = sync_omp(&source, &target, Some(AiProvider::OpenAi), Some(&selector));
    drop(hook);

    let error = result.expect_err("multirow selected-source leaf swap fails closed");
    assert_eq!(
        hook_calls.get(),
        1,
        "selection reached the database-open fence"
    );
    assert!(format!("{error:#}").contains("OMP credential source is unavailable"));
    let contents = std::fs::read(&target).expect("existing target remains readable");
    assert_eq!(contents.as_slice(), previous);
    assert_private_target(&target);
    assert_no_target_sidecars(&target);
    assert_target_parent_entries(&target, true);
    assert_eq!(
        std::fs::metadata(&database)
            .expect("replacement source database remains present")
            .ino(),
        replacement_inode
    );
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const DB_WIRE_CHILD: &str = "JACKIN_DB_WIRE_CHILD";

#[tokio::test]
async fn connection_owner_exports_outcome_without_database_path() {
    let directory = tempfile::tempdir().unwrap();
    let success_path = directory.path().join("sqlite-secret-success.db");
    let failure_path = directory.path().join("sqlite-secret-directory");
    std::fs::create_dir(&failure_path).unwrap();
    let success_path = success_path.to_string_lossy();
    let failure_path = failure_path.to_string_lossy();

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    connect_local(&success_path).await.unwrap();
    connect_local(&failure_path).await.unwrap_err();

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 2);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("connect"));
    assert!(export.contains_span_text("db_error"));
    for prohibited in [
        success_path.as_ref(),
        failure_path.as_ref(),
        "sqlite-secret",
    ] {
        assert!(!export.contains_span_text(prohibited));
    }
}

#[test]
fn conformance_wire_sql_operations_are_bounded_and_private() -> anyhow::Result<()> {
    if std::env::var_os(DB_WIRE_CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("store_backend::tests::conformance_wire_sql_operations_are_bounded_and_private")
            .arg("--nocapture")
            .env(DB_WIRE_CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "isolated DB wire test failed");
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("wire-private-usage.db");
    let path = path.to_string_lossy();
    let conn = runtime
        .block_on(connect_local(&path))
        .map_err(anyhow::Error::msg)?;
    runtime.block_on(operation(
            DbOperation::Update,
            conn.execute(
                "CREATE TABLE wire_private_accounts (private_identity TEXT PRIMARY KEY, private_value TEXT)",
                (),
            ),
        ))?;
    runtime.block_on(operation(
            DbOperation::Upsert,
            conn.execute(
                "INSERT INTO wire_private_accounts VALUES ('wire-private-account', 'wire-private-value')",
                (),
            ),
        ))?;
    let mut rows = runtime.block_on(operation(
            DbOperation::Select,
            conn.query(
                "SELECT private_value FROM wire_private_accounts WHERE private_identity = 'wire-private-account'",
                (),
            ),
        ))?;
    assert!(runtime.block_on(rows.next())?.is_some());
    let failure = runtime.block_on(operation(
        DbOperation::Select,
        conn.query("SELECT wire_private_secret FROM missing_private_table", ()),
    ));
    assert!(failure.is_err());
    jackin_diagnostics::flush_wire_test_export()?;
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    runtime.block_on(async {
        loop {
            let db_spans = testbed
                .spans()
                .iter()
                .filter(|span| span.name == "db.client")
                .count();
            let has_duration = testbed
                .metric_names()
                .iter()
                .any(|name| name == "db.client.operation.duration");
            if db_spans == 5 && has_duration {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "DB wire spans and duration metric did not arrive"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    });

    let spans = testbed
        .spans()
        .into_iter()
        .filter(|span| span.name == "db.client")
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 5);
    let wire = format!("{spans:?}");
    for expected in [
        "sqlite", "connect", "update", "upsert", "select", "success", "failure", "db_error",
    ] {
        assert!(wire.contains(expected), "missing {expected}: {wire}");
    }
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.status.as_ref().is_some_and(|status| status.code == 2))
            .count(),
        1
    );
    assert!(
        testbed
            .metric_names()
            .iter()
            .any(|name| name == "db.client.operation.duration")
    );
    let prohibited = [
        path.as_ref(),
        "wire-private-usage.db",
        "wire_private_accounts",
        "wire-private-account",
        "wire-private-value",
        "wire_private_secret",
        "missing_private_table",
        "CREATE TABLE",
        "INSERT INTO",
        "SELECT private_value",
    ];
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

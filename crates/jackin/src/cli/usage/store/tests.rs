// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn account(window: &str, used: i64) -> AccountUsageSnapshotView {
    AccountUsageSnapshotView {
        provider: "codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        source: "codex-rpc".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: window.to_owned(),
        used_amount: Some(used),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: Some(1_781_200_000),
        fetched_at: 1_781_190_000,
        expires_at: Some(1_781_190_300),
        status: "fresh".to_owned(),
        last_error: None,
    }
}

#[tokio::test]
async fn host_account_cache_upserts_rows() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let rows = [account("session", 37), account("weekly", 10)];

    let path = upsert_accounts(&paths, &rows).await.unwrap();
    assert_eq!(path, paths.data_dir.join("daemon").join("accounts.db"));
    assert_eq!(count_account_rows(path.clone()).await.unwrap(), 2);

    upsert_accounts(&paths, &[account("session", 38)])
        .await
        .unwrap();
    assert_eq!(count_account_rows(path).await.unwrap(), 2);
}

#[tokio::test]
async fn host_account_cache_reads_seeded_rows_without_provider_poll() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let seeded = [account("session", 37), account("weekly", 10)];

    let path = upsert_accounts(&paths, &seeded).await.unwrap();
    let (read_path, rows) = read_accounts(&paths).await.unwrap();

    assert_eq!(read_path, path);
    assert_eq!(rows, seeded);
}

#[tokio::test]
async fn host_account_cache_missing_file_reads_empty_without_creating_db() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = paths.data_dir.join("daemon").join("accounts.db");

    let (read_path, rows) = read_accounts(&paths).await.unwrap();

    assert_eq!(read_path, path);
    assert!(rows.is_empty());
    assert!(!read_path.exists());
}

#[test]
fn account_hash_is_stable_and_namespaced() {
    assert_eq!(
        account_key_hash("codex", "alexey@example.com"),
        account_key_hash("codex", "alexey@example.com")
    );
    assert_ne!(
        account_key_hash("codex", "alexey@example.com"),
        account_key_hash("claude", "alexey@example.com")
    );
}

#[tokio::test]
async fn host_account_cache_exports_owned_operations_without_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("host-cache-secret-path");
    let paths = JackinPaths::for_tests(&root);
    let mut sensitive_account = account("host-cache-secret-window", 37);
    sensitive_account.account_label = "host-cache-secret@example.com".to_owned();

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    upsert_accounts(&paths, &[sensitive_account]).await.unwrap();
    read_accounts(&paths).await.unwrap();

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 7);
    assert_eq!(export.error_span_count(), 0);
    for expected in ["connect", "update", "upsert", "select"] {
        assert!(export.contains_span_text(expected), "missing {expected}");
    }
    for prohibited in [
        "host-cache-secret-path",
        "host-cache-secret-window",
        "host-cache-secret@example.com",
        "account_usage_snapshots",
        "CREATE TABLE",
    ] {
        assert!(!export.contains_span_text(prohibited));
    }
}

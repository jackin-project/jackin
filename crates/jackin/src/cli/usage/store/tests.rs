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

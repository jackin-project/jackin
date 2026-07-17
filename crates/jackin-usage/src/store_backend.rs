//! Single import chokepoint for the workspace `turso` `SQLite` client.
//!
//! All production and test code in this crate — and the host CLI usage
//! cache under `crates/jackin` — reaches turso through this module so a
//! version bump or backend swap is one-file work.

use std::future::Future;
use std::time::Instant;

pub use turso::{Connection, Row, params};

pub use jackin_telemetry::schema::enums::DbOperationName as DbOperation;

pub async fn operation<T, E>(
    kind: DbOperation,
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, E> {
    let operation_name = kind.as_str();
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::DB_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("sqlite"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::DB_OPERATION_NAME,
            value: jackin_telemetry::Value::Str(operation_name),
        },
    ];
    let span =
        jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::DB_CLIENT, &attrs);
    let started = Instant::now();
    let result = future.await;
    let outcome = if result.is_ok() {
        jackin_telemetry::schema::enums::OutcomeValue::Success
    } else {
        jackin_telemetry::schema::enums::OutcomeValue::Failure
    };
    span.complete(
        outcome,
        result
            .as_ref()
            .err()
            .map(|_| jackin_telemetry::schema::enums::ErrorType::DbError),
    );
    let metric_attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::std_attrs::DB_OPERATION_NAME,
        value: jackin_telemetry::Value::Str(operation_name),
    }];
    let _duration =
        jackin_telemetry::histogram(&jackin_telemetry::metric::DB_CLIENT_OPERATION_DURATION)
            .record(started.elapsed().as_secs_f64(), &metric_attrs);
    result
}

/// Open a local `SQLite` database at `path` and return a connection.
pub async fn connect_local(path: &str) -> Result<Connection, String> {
    operation(DbOperation::Connect, async {
        let db = turso::Builder::new_local(path)
            .build()
            .await
            .map_err(|_| "open local store failed".to_owned())?;
        db.connect()
            .map_err(|_| "connect local store failed".to_owned())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

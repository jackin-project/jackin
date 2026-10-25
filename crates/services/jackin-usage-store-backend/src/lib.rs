use std::future::Future;
use std::time::Instant;

pub use turso::{Connection, Row, params};

pub use jackin_telemetry::schema::enums::DbOperationName as DbOperation;

mod local_store;
pub use local_store::{
    BackupReceipt, CommittedMigrationValidation, LocalStore, LogicalDigest, ManagedStoreKind,
    MigrationReceipt, PreopenStatus, StoreError, StoreOperationGuard, acquire_store_operation,
};

mod fixtures;

#[cfg(feature = "test-support")]
pub use fixtures::connect_without_rowid_fixture;

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
        jackin_telemetry::schema::enums::OutcomeValue::Error
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

pub async fn connect_local(path: &str) -> Result<Connection, String> {
    operation(DbOperation::Connect, async {
        let db = local_store::build_legacy_database(path).await?;
        db.connect()
            .map_err(|_| "connect local store failed".to_owned())
    })
    .await
}

pub async fn connect_local_with_required_file_lock(path: &str) -> Result<Connection, String> {
    require_default_file_lock()?;
    connect_local(path).await
}

pub fn require_default_file_lock() -> Result<(), String> {
    if std::env::var_os("LIMBO_DISABLE_FILE_LOCK").is_some() {
        return Err("local usage store requires Turso's default file lock".to_owned());
    }
    Ok(())
}

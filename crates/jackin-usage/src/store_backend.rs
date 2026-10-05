//! Single import chokepoint for the workspace `turso` `SQLite` client.
//!
//! All production and test code in this crate — and the host CLI usage
//! cache under `crates/jackin` — reaches turso through this backend owner.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) use turso::{Connection, Row, params};

pub(crate) use jackin_telemetry::schema::enums::DbOperationName as DbOperation;

mod physical;
mod readonly;

pub(crate) use readonly::{ReadOnlyConnection, ReadOnlyRow};

#[cfg(test)]
pub(crate) use physical::source_bytes as source_bytes_for_test;

pub(crate) fn with_store_custody<T>(
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    static CUSTODY: Mutex<()> = Mutex::new(());
    let _custody = CUSTODY
        .lock()
        .map_err(|_| "usage store custody unavailable".to_owned())?;
    operation()
}

pub(crate) fn read_local<T>(
    path: &str,
    read: impl FnOnce(&ReadOnlyConnection) -> Result<T, String>,
) -> Result<Option<T>, String> {
    operation_sync(DbOperation::Connect, || {
        let canonical = match std::fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("resolve local reader failed".to_owned()),
        };
        let canonical = canonical
            .to_str()
            .ok_or_else(|| "invalid local reader path".to_owned())?;
        readonly::read_admitted(canonical, read)
    })
}

pub(crate) async fn operation<T, E>(
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

fn operation_sync<T, E>(
    kind: DbOperation,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::DB_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("sqlite"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::DB_OPERATION_NAME,
            value: jackin_telemetry::Value::Str(kind.as_str()),
        },
    ];
    let span =
        jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::DB_CLIENT, &attrs);
    let started = Instant::now();
    let result = operation();
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
        value: jackin_telemetry::Value::Str(kind.as_str()),
    }];
    let _duration =
        jackin_telemetry::histogram(&jackin_telemetry::metric::DB_CLIENT_OPERATION_DURATION)
            .record(started.elapsed().as_secs_f64(), &metric_attrs);
    result
}

/// Open a local `SQLite` database at `path` and return a connection.
pub(crate) async fn connect_local(path: &str) -> Result<Connection, String> {
    operation(DbOperation::Connect, async {
        let input = std::path::Path::new(path);
        let path = match std::fs::canonicalize(input) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = input
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new("."));
                let parent = std::fs::canonicalize(parent)
                    .map_err(|_| "resolve local store failed".to_owned())?;
                parent.join(
                    input
                        .file_name()
                        .ok_or_else(|| "invalid local store path".to_owned())?,
                )
            }
            Err(_) => return Err("resolve local store failed".to_owned()),
        };
        let path = path
            .to_str()
            .ok_or_else(|| "invalid local store path".to_owned())?;
        let db = turso::Builder::new_local(path)
            .with_io_impl(Arc::new(
                physical::PhysicalIO::new(path)
                    .map_err(|_| "create local store IO failed".to_owned())?,
            ))
            .build()
            .await
            .map_err(|_| "open local store failed".to_owned())?;
        db.connect()
            .map_err(|_| "connect local store failed".to_owned())
    })
    .await
}

#[cfg(test)]
mod tests;

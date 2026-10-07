//! jackin-usage-store-backend: turso `SQLite` import chokepoint.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`connect_local`] — open a local store connection.
//!
//! All production and test usage-store code — and the host CLI usage
//! cache under `crates/apps/jackin` — reaches turso through this crate so a
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
mod tests;

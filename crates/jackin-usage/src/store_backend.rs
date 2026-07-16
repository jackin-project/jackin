//! Single import chokepoint for the workspace `turso` `SQLite` client.
//!
//! All production and test code in this crate — and the host CLI usage
//! cache under `crates/jackin` — reaches turso through this module so a
//! version bump or backend swap is one-file work.

use std::future::Future;
use std::time::Instant;

pub use turso::{Connection, Row, params};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DbOperation {
    Begin,
    Select,
    Insert,
    Upsert,
    Update,
    Delete,
}

impl DbOperation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::Select => "select",
            Self::Insert => "insert",
            Self::Upsert => "upsert",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

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
    span.complete(outcome, result.as_ref().err().map(|_| "db_error"));
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
    let db = turso::Builder::new_local(path)
        .build()
        .await
        .map_err(|err| format!("open local store failed: {err}"))?;
    db.connect()
        .map_err(|err| format!("connect local store failed: {err}"))
}

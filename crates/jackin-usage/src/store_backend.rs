//! Single import chokepoint for the workspace `turso` `SQLite` client.
//!
//! All production and test code in this crate — and the host CLI usage
//! cache under `crates/jackin` — reaches turso through this module so a
//! version bump or backend swap is one-file work.

pub use turso::{Connection, Row, params};

/// Open a local `SQLite` database at `path` and return a connection.
pub async fn connect_local(path: &str) -> Result<Connection, String> {
    let db = turso::Builder::new_local(path)
        .build()
        .await
        .map_err(|err| format!("open local store failed: {err}"))?;
    db.connect()
        .map_err(|err| format!("connect local store failed: {err}"))
}

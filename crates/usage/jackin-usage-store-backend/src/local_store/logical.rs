use std::path::Path;

use sha2::{Digest, Sha256};
use turso::{Connection, Value};

use crate::{DbOperation, operation};

use super::manifest::sync_file_on_worker;
use super::path::ensure_managed_sql_path;
use super::{LogicalDigest, StoreError};

pub(super) async fn create_and_validate_backup(
    source: &Connection,
    backup_connection: &Connection,
    output: &Path,
) -> Result<LogicalDigest, StoreError> {
    ensure_backup_destination_unused(output)?;
    ensure_managed_sql_path(output)?;
    let output_text = output.to_str().ok_or(StoreError::UnsupportedSourcePath)?;
    vacuum_into(backup_connection, output_text).await?;
    prepare_backup_file(output).await?;
    validate_no_backup_sidecars(output)?;

    super::manifest::require_private_regular_file(output)?;
    let _identity_lease = super::registry::reserve_artifact_path(output)?;
    let (source_digest, backup_digest) = validate_backup_database(source, output_text).await?;
    validate_no_backup_sidecars(output)?;
    if source_digest != backup_digest {
        return Err(StoreError::LogicalBackupMismatch);
    }
    Ok(source_digest)
}

fn ensure_backup_destination_unused(output: &Path) -> Result<(), StoreError> {
    match std::fs::symlink_metadata(output) {
        Ok(_) => return Err(StoreError::BackupCollision),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(StoreError::io("inspect logical backup destination", source)),
    }
    Ok(())
}

async fn vacuum_into(backup_connection: &Connection, output_text: &str) -> Result<(), StoreError> {
    let sql = format!("VACUUM main INTO '{output_text}'");
    operation(DbOperation::Update, backup_connection.execute(&sql, ()))
        .await
        .map_err(|_| StoreError::Backend("VACUUM INTO failed"))?;
    Ok(())
}

async fn prepare_backup_file(output: &Path) -> Result<(), StoreError> {
    let output_meta = std::fs::symlink_metadata(output)
        .map_err(|source| StoreError::io("inspect logical backup", source))?;
    if output_meta.file_type().is_symlink() || !output_meta.file_type().is_file() {
        return Err(StoreError::BackupIncomplete);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o600))
            .map_err(|source| StoreError::io("restrict logical backup permissions", source))?;
    }
    sync_file_on_worker(output).await?;
    Ok(())
}

async fn validate_backup_database(
    source: &Connection,
    output_text: &str,
) -> Result<(LogicalDigest, LogicalDigest), StoreError> {
    let database = turso::Builder::new_local(output_text)
        .build()
        .await
        .map_err(|_| StoreError::Backend("open logical backup for validation failed"))?;
    let connection = database
        .connect()
        .map_err(|_| StoreError::Backend("connect logical backup for validation failed"))?;
    let source_digest = logical_digest(source).await?;
    let backup_digest = logical_digest(&connection).await?;
    drop(connection);
    drop(database);
    Ok((source_digest, backup_digest))
}

fn validate_no_backup_sidecars(output: &Path) -> Result<(), StoreError> {
    for suffix in ["-wal", "-journal"] {
        let sidecar = super::path::sidecar_path(output, suffix);
        match std::fs::symlink_metadata(&sidecar) {
            Ok(metadata) if metadata.file_type().is_file() && metadata.len() == 0 => {}
            Ok(_) => return Err(StoreError::BackupIncomplete),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(StoreError::io("inspect logical backup sidecar", source));
            }
        }
    }
    Ok(())
}

pub(super) async fn logical_digest(connection: &Connection) -> Result<LogicalDigest, StoreError> {
    let (schema_sha256, schema_objects, tables) = digest_schema(connection).await?;
    let (data_sha256, rows) = digest_tables(connection, &tables).await?;
    let metadata_sha256 = digest_metadata(connection).await?;
    Ok(LogicalDigest {
        schema_sha256,
        data_sha256,
        metadata_sha256,
        schema_objects,
        tables: u64::try_from(tables.len()).map_err(|_| StoreError::CountOverflow)?,
        rows,
    })
}

pub(super) async fn is_fresh_empty_database(connection: &Connection) -> Result<bool, StoreError> {
    let digest = logical_digest(connection).await?;
    if digest.schema_objects != 0 || digest.tables != 0 || digest.rows != 0 {
        return Ok(false);
    }
    for (pragma, expected) in [
        ("application_id", Value::Integer(0)),
        ("user_version", Value::Integer(0)),
        ("encoding", Value::Text("UTF-8".to_owned())),
        ("page_size", Value::Integer(4096)),
        ("auto_vacuum", Value::Integer(0)),
    ] {
        let mut rows = connection
            .query(&format!("PRAGMA main.{pragma}"), ())
            .await
            .map_err(|_| StoreError::Backend("read fresh database metadata failed"))?;
        let row = rows
            .next()
            .await
            .map_err(|_| StoreError::Backend("read fresh database metadata row failed"))?
            .ok_or(StoreError::UnsupportedDatabaseMetadata)?;
        let value = row
            .get_value(0)
            .map_err(|_| StoreError::UnsupportedDatabaseMetadata)?;
        if value != expected
            || rows
                .next()
                .await
                .map_err(|_| StoreError::Backend("read fresh database metadata row failed"))?
                .is_some()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

struct TableDefinition {
    name: String,
    sql: Option<String>,
}

struct ColumnDefinition {
    name: String,
    declared_type: String,
    primary_key_order: i64,
    hidden: i64,
}

async fn digest_schema(
    connection: &Connection,
) -> Result<(String, u64, Vec<TableDefinition>), StoreError> {
    let mut hasher = Sha256::new();
    let mut tables = Vec::new();
    let mut count = 0_u64;
    let mut rows = connection
        .query(
            "SELECT type, name, tbl_name, sql FROM main.sqlite_schema ORDER BY type, name, tbl_name",
            (),
        )
        .await
        .map_err(|_| StoreError::Backend("read logical schema failed"))?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| StoreError::Backend("read logical schema row failed"))?
    {
        let (object_type, name) = schema_object_identity(&row)?;
        feed_schema_row(&row, &mut hasher)?;
        count = count.checked_add(1).ok_or(StoreError::CountOverflow)?;
        if let Some(table) = table_definition(&row, object_type, name)? {
            tables.push(table);
        }
    }
    drop(rows);
    Ok((hex::encode(hasher.finalize()), count, tables))
}

fn schema_object_identity(row: &crate::Row) -> Result<(String, String), StoreError> {
    let object_type = text_value(
        &row.get_value(0)
            .map_err(|_| StoreError::UnsupportedSchemaObject)?,
    )?;
    let name = text_value(
        &row.get_value(1)
            .map_err(|_| StoreError::UnsupportedSchemaObject)?,
    )?;
    Ok((object_type, name))
}

fn feed_schema_row(row: &crate::Row, hasher: &mut Sha256) -> Result<(), StoreError> {
    for column in 0..4 {
        let value = row
            .get_value(column)
            .map_err(|_| StoreError::UnsupportedSchemaObject)?;
        feed_value(hasher, &value)?;
    }
    Ok(())
}

fn table_definition(
    row: &crate::Row,
    object_type: String,
    name: String,
) -> Result<Option<TableDefinition>, StoreError> {
    if object_type == "table" && name != "sqlite_schema" && name != "sqlite_master" {
        return table_definition_from_sql(row, name).map(Some);
    }
    if !matches!(object_type.as_str(), "table" | "index" | "view" | "trigger") {
        return Err(StoreError::UnsupportedSchemaObject);
    }
    Ok(None)
}

fn table_definition_from_sql(
    row: &crate::Row,
    name: String,
) -> Result<TableDefinition, StoreError> {
    let sql = match row
        .get_value(3)
        .map_err(|_| StoreError::UnsupportedSchemaObject)?
    {
        Value::Null => None,
        Value::Text(sql) => Some(sql),
        _ => return Err(StoreError::UnsupportedSchemaObject),
    };
    if sql
        .as_deref()
        .is_some_and(|sql| sql.to_ascii_uppercase().contains("VIRTUAL TABLE"))
    {
        return Err(StoreError::UnsupportedSchemaObject);
    }
    Ok(TableDefinition { name, sql })
}

async fn digest_tables(
    connection: &Connection,
    tables: &[TableDefinition],
) -> Result<(String, u64), StoreError> {
    let mut hasher = Sha256::new();
    let mut total_rows = 0_u64;
    for table in tables {
        feed(&mut hasher, table.name.as_bytes());
        let table_rows = digest_table(connection, table, &mut hasher, &mut total_rows).await?;
        feed(&mut hasher, &table_rows.to_be_bytes());
    }
    Ok((hex::encode(hasher.finalize()), total_rows))
}

async fn digest_table(
    connection: &Connection,
    table: &TableDefinition,
    hasher: &mut Sha256,
    total_rows: &mut u64,
) -> Result<u64, StoreError> {
    let columns = table_columns(connection, &table.name).await?;
    validate_table_columns(&columns)?;
    feed_column_definitions(hasher, &columns);
    let rowid = rowid_column(table, &columns)?;
    let order = table_order(&columns, rowid)?;
    let query = data_query(&table.name, &columns, rowid, &order);
    digest_table_rows(connection, &query, &columns, rowid, hasher, total_rows).await
}

fn validate_table_columns(columns: &[ColumnDefinition]) -> Result<(), StoreError> {
    if columns.is_empty() || columns.iter().any(|column| column.hidden != 0) {
        return Err(StoreError::UnsupportedTableShape);
    }
    Ok(())
}

fn feed_column_definitions(hasher: &mut Sha256, columns: &[ColumnDefinition]) {
    for column in columns {
        feed(hasher, column.name.as_bytes());
        feed(hasher, column.declared_type.as_bytes());
        feed(hasher, &column.primary_key_order.to_be_bytes());
        feed(hasher, &column.hidden.to_be_bytes());
    }
}

fn rowid_column(
    table: &TableDefinition,
    columns: &[ColumnDefinition],
) -> Result<Option<&'static str>, StoreError> {
    let has_rowid = table
        .sql
        .as_deref()
        .is_none_or(|sql| !sql.to_ascii_uppercase().contains("WITHOUT ROWID"));
    if !has_rowid {
        return Ok(None);
    }
    ["rowid", "_rowid_", "oid"]
        .into_iter()
        .find(|candidate| {
            !columns
                .iter()
                .any(|column| column.name.eq_ignore_ascii_case(candidate))
        })
        .map(Some)
        .ok_or(StoreError::UnsupportedTableShape)
}

fn table_order(columns: &[ColumnDefinition], rowid: Option<&str>) -> Result<String, StoreError> {
    if let Some(rowid) = rowid {
        return Ok(rowid.to_owned());
    }
    let mut primary = columns
        .iter()
        .filter(|column| column.primary_key_order > 0)
        .collect::<Vec<_>>();
    primary.sort_by_key(|column| column.primary_key_order);
    if primary.is_empty() {
        return Err(StoreError::UnsupportedTableShape);
    }
    Ok(primary
        .into_iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", "))
}

async fn digest_table_rows(
    connection: &Connection,
    query: &str,
    columns: &[ColumnDefinition],
    rowid: Option<&str>,
    hasher: &mut Sha256,
    total_rows: &mut u64,
) -> Result<u64, StoreError> {
    let mut rows = connection
        .query(query, ())
        .await
        .map_err(|_| StoreError::Backend("read logical table failed"))?;
    let mut table_rows = 0_u64;
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| StoreError::Backend("read logical table row failed"))?
    {
        digest_table_row(&row, columns, rowid.is_some(), hasher)?;
        table_rows = table_rows.checked_add(1).ok_or(StoreError::CountOverflow)?;
        *total_rows = total_rows.checked_add(1).ok_or(StoreError::CountOverflow)?;
    }
    drop(rows);
    Ok(table_rows)
}

fn digest_table_row(
    row: &crate::Row,
    columns: &[ColumnDefinition],
    has_rowid: bool,
    hasher: &mut Sha256,
) -> Result<(), StoreError> {
    if has_rowid {
        let value = row
            .get_value(0)
            .map_err(|_| StoreError::UnsupportedTableShape)?;
        feed_value(hasher, &value)?;
    }
    let offset = usize::from(has_rowid);
    for column_index in 0..columns.len() {
        digest_table_cell(row, column_index, offset, hasher)?;
    }
    Ok(())
}

fn digest_table_cell(
    row: &crate::Row,
    column_index: usize,
    offset: usize,
    hasher: &mut Sha256,
) -> Result<(), StoreError> {
    let type_value = row
        .get_value(offset + column_index * 3)
        .map_err(|_| StoreError::UnsupportedTableShape)?;
    let kind = text_value(&type_value)?;
    feed(hasher, kind.as_bytes());
    let numeric = row
        .get_value(offset + column_index * 3 + 1)
        .map_err(|_| StoreError::UnsupportedTableShape)?;
    let bytes = row
        .get_value(offset + column_index * 3 + 2)
        .map_err(|_| StoreError::UnsupportedTableShape)?;
    feed_cell(hasher, &kind, &numeric, &bytes)
}

fn feed_cell(
    hasher: &mut Sha256,
    kind: &str,
    numeric: &Value,
    bytes: &Value,
) -> Result<(), StoreError> {
    match kind {
        "null" if numeric.is_null() && bytes.is_null() => {}
        "integer" | "real" if bytes.is_null() => feed_value(hasher, numeric)?,
        "text" | "blob" if numeric.is_null() => {
            let encoded = text_value(bytes)?;
            let raw = hex::decode(encoded).map_err(|_| StoreError::UnsupportedTableShape)?;
            feed(hasher, &raw);
        }
        _ => return Err(StoreError::UnsupportedTableShape),
    }
    Ok(())
}

async fn table_columns(
    connection: &Connection,
    table: &str,
) -> Result<Vec<ColumnDefinition>, StoreError> {
    let sql = format!("PRAGMA main.table_xinfo({})", quote_literal(table));
    let mut rows = connection
        .query(&sql, ())
        .await
        .map_err(|_| StoreError::UnsupportedTableShape)?;
    let mut columns = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| StoreError::UnsupportedTableShape)?
    {
        columns.push(ColumnDefinition {
            name: row.get(1).map_err(|_| StoreError::UnsupportedTableShape)?,
            declared_type: row
                .get::<Option<String>>(2)
                .map_err(|_| StoreError::UnsupportedTableShape)?
                .unwrap_or_default(),
            primary_key_order: row.get(5).map_err(|_| StoreError::UnsupportedTableShape)?,
            hidden: row.get(6).map_err(|_| StoreError::UnsupportedTableShape)?,
        });
    }
    drop(rows);
    Ok(columns)
}

fn data_query(
    table: &str,
    columns: &[ColumnDefinition],
    rowid: Option<&str>,
    order: &str,
) -> String {
    let mut selected = Vec::new();
    if let Some(rowid) = rowid {
        selected.push(rowid.to_owned());
    }
    for column in columns {
        let name = quote_identifier(&column.name);
        selected.push(format!("typeof({name})"));
        selected.push(format!(
            "CASE WHEN typeof({name}) IN ('integer', 'real') THEN {name} ELSE NULL END"
        ));
        selected.push(format!(
            "CASE WHEN typeof({name}) IN ('text', 'blob') THEN hex(CAST({name} AS BLOB)) ELSE NULL END"
        ));
    }
    format!(
        "SELECT {} FROM main.{} ORDER BY {order}",
        selected.join(", "),
        quote_identifier(table)
    )
}

async fn digest_metadata(connection: &Connection) -> Result<String, StoreError> {
    let mut hasher = Sha256::new();
    for pragma in [
        "application_id",
        "user_version",
        "encoding",
        "page_size",
        "auto_vacuum",
    ] {
        let mut rows = connection
            .query(&format!("PRAGMA main.{pragma}"), ())
            .await
            .map_err(|_| StoreError::Backend("read logical metadata failed"))?;
        let row = rows
            .next()
            .await
            .map_err(|_| StoreError::Backend("read logical metadata row failed"))?
            .ok_or(StoreError::UnsupportedDatabaseMetadata)?;
        feed(&mut hasher, pragma.as_bytes());
        feed_value(
            &mut hasher,
            &row.get_value(0)
                .map_err(|_| StoreError::UnsupportedDatabaseMetadata)?,
        )?;
        if rows
            .next()
            .await
            .map_err(|_| StoreError::Backend("read logical metadata row failed"))?
            .is_some()
        {
            return Err(StoreError::UnsupportedDatabaseMetadata);
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn text_value(value: &Value) -> Result<String, StoreError> {
    match value {
        Value::Text(value) => Ok(value.clone()),
        _ => Err(StoreError::UnsupportedSchemaObject),
    }
}

fn feed_value(hasher: &mut Sha256, value: &Value) -> Result<(), StoreError> {
    match value {
        Value::Null => feed(hasher, &[0]),
        Value::Integer(value) => {
            feed(hasher, &[1]);
            feed(hasher, &value.to_be_bytes());
        }
        Value::Real(value) => {
            feed(hasher, &[2]);
            feed(hasher, &value.to_bits().to_be_bytes());
        }
        Value::Text(value) => {
            feed(hasher, &[3]);
            feed(hasher, value.as_bytes());
        }
        Value::Blob(value) => {
            feed(hasher, &[4]);
            feed(hasher, value);
        }
    }
    Ok(())
}

fn feed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

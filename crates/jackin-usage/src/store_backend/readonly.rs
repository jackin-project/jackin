//! Physical read capability: no create, write, recovery rewrite, or WAL checkpoint.

use std::io::ErrorKind;
use std::sync::Arc;

use super::physical::PhysicalIO;
use turso::core::io::FileSyncType;
use turso::core::{self, Clock, Completion, File, IO, OpenFlags};

pub(crate) struct ReadOnlyConnection {
    connection: Arc<core::Connection>,
}

pub(crate) struct ReadOnlyRow(Vec<core::Value>);

impl ReadOnlyRow {
    pub(crate) fn i64(&self, index: usize, name: &str) -> Result<i64, String> {
        match self.0.get(index) {
            Some(core::Value::Numeric(core::Numeric::Integer(value))) => Ok(*value),
            _ => Err(format!("invalid stored {name}")),
        }
    }

    pub(crate) fn optional_i64(&self, index: usize, name: &str) -> Result<Option<i64>, String> {
        match self.0.get(index) {
            Some(core::Value::Null) => Ok(None),
            Some(core::Value::Numeric(core::Numeric::Integer(value))) => Ok(Some(*value)),
            _ => Err(format!("invalid stored {name}")),
        }
    }

    pub(crate) fn string(&self, index: usize, name: &str) -> Result<String, String> {
        match self.0.get(index) {
            Some(core::Value::Text(value)) => Ok(value.as_str().to_owned()),
            _ => Err(format!("invalid stored {name}")),
        }
    }

    pub(crate) fn optional_string(
        &self,
        index: usize,
        name: &str,
    ) -> Result<Option<String>, String> {
        match self.0.get(index) {
            Some(core::Value::Null) => Ok(None),
            Some(core::Value::Text(value)) => Ok(Some(value.as_str().to_owned())),
            _ => Err(format!("invalid stored {name}")),
        }
    }

    pub(crate) fn numeric_f64(&self, index: usize, name: &str) -> Result<f64, String> {
        match self.0.get(index) {
            Some(core::Value::Numeric(core::Numeric::Integer(value))) => Ok(*value as f64),
            Some(core::Value::Numeric(core::Numeric::Float(value))) => Ok(f64::from(*value)),
            _ => Err(format!("invalid stored {name}")),
        }
    }
}

impl ReadOnlyConnection {
    pub(crate) fn query(&self, sql: &str) -> Result<Vec<ReadOnlyRow>, String> {
        super::operation_sync(super::DbOperation::Select, || {
            self.connection
                .prepare(sql)
                .and_then(|mut statement| statement.run_collect_rows())
                .map(|rows| rows.into_iter().map(ReadOnlyRow).collect())
                .map_err(|_| "read local store failed".to_owned())
        })
    }

    pub(crate) fn query_i64(&self, sql: &str, value: i64) -> Result<Vec<ReadOnlyRow>, String> {
        super::operation_sync(super::DbOperation::Select, || {
            let mut statement = self
                .connection
                .prepare(sql)
                .map_err(|_| "read local store failed".to_owned())?;
            statement
                .bind_at(
                    std::num::NonZeroUsize::new(1).expect("one is nonzero"),
                    core::Value::Numeric(core::Numeric::Integer(value)),
                )
                .map_err(|_| "bind local reader failed".to_owned())?;
            statement
                .run_collect_rows()
                .map(|rows| rows.into_iter().map(ReadOnlyRow).collect())
                .map_err(|_| "read local store failed".to_owned())
        })
    }
}

/// Caller owns process admission for this physical database throughout this call.
pub(super) fn read_admitted<T>(
    path: &str,
    read: impl FnOnce(&ReadOnlyConnection) -> Result<T, String>,
) -> Result<Option<T>, String> {
    let io: Arc<dyn IO> = Arc::new(ReadOnlyIO {
        inner: PhysicalIO::new(path).map_err(|_| "create local reader failed".to_owned())?,
        path: path.to_owned(),
        wal_path: format!("{path}-wal"),
    });
    let file = match io.open_file(path, OpenFlags::ReadOnly, true) {
        Ok(file) => file,
        Err(core::LimboError::CompletionError(core::CompletionError::IOError(
            ErrorKind::NotFound,
            _,
        ))) => return Ok(None),
        Err(_) => return Err("open local reader failed".to_owned()),
    };
    // Shared legacy DB lock permits readers together and excludes other-process
    // writers. Admission excludes same-process writers before taking this lock.
    file.lock_file(false)
        .map_err(|_| "local store busy".to_owned())?;
    match std::fs::metadata(format!("{path}-tshm")) {
        Ok(_) => return Err("multiprocess usage cache is unsupported".to_owned()),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(_) => return Err("inspect local reader coordination failed".to_owned()),
    }
    let storage = Arc::new(core::storage::database::DatabaseFile::new(file));
    let mut state = core::OpenDbAsyncState::new();
    let database = loop {
        match core::Database::open_with_flags_bypass_registry_async(
            &mut state,
            io.clone(),
            path,
            None,
            storage.clone(),
            OpenFlags::ReadOnly,
            core::DatabaseOpts::new(),
            None,
            None,
        )
        .map_err(|_| "open local reader failed".to_owned())?
        {
            core::IOResult::Done(database) => break database,
            core::IOResult::IO(completion) => completion
                .wait(io.as_ref())
                .map_err(|_| "open local reader failed".to_owned())?,
        }
    };
    let connection = database
        .connect()
        .map_err(|_| "connect local reader failed".to_owned())?;
    connection.set_temp_store(core::TempStore::Memory);
    let reader = ReadOnlyConnection { connection };
    read(&reader).map(Some)
}

struct ReadOnlyIO {
    inner: PhysicalIO,
    path: String,
    wal_path: String,
}

impl Clock for ReadOnlyIO {
    fn current_time_monotonic(&self) -> core::MonotonicInstant {
        self.inner.current_time_monotonic()
    }

    fn current_time_wall_clock(&self) -> core::WallClockInstant {
        self.inner.current_time_wall_clock()
    }
}

impl IO for ReadOnlyIO {
    fn open_file(
        &self,
        path: &str,
        _flags: OpenFlags,
        direct: bool,
    ) -> core::Result<Arc<dyn File>> {
        if path != self.path && path != self.wal_path {
            return Err(core::LimboError::ReadOnly);
        }
        self.inner
            .open_file(path, OpenFlags::ReadOnly, direct)
            .map(|inner| -> Arc<dyn File> { Arc::new(ReadOnlyFile { inner }) })
    }

    fn remove_file(&self, _path: &str) -> core::Result<()> {
        Err(core::LimboError::ReadOnly)
    }

    fn step(&self) -> core::Result<()> {
        self.inner.step()
    }
}

struct ReadOnlyFile {
    inner: Arc<dyn File>,
}

impl File for ReadOnlyFile {
    fn lock_file(&self, exclusive: bool) -> core::Result<()> {
        if exclusive {
            return Err(core::LimboError::ReadOnly);
        }
        self.inner.lock_file(false)
    }

    fn unlock_file(&self) -> core::Result<()> {
        self.inner.unlock_file()
    }

    fn pread(&self, position: u64, completion: Completion) -> core::Result<Completion> {
        self.inner.pread(position, completion)
    }

    fn pwrite(
        &self,
        _position: u64,
        _buffer: Arc<core::Buffer>,
        _completion: Completion,
    ) -> core::Result<Completion> {
        Err(core::LimboError::ReadOnly)
    }

    fn pwritev(
        &self,
        _position: u64,
        _buffers: Vec<Arc<core::Buffer>>,
        _completion: Completion,
    ) -> core::Result<Completion> {
        Err(core::LimboError::ReadOnly)
    }

    fn sync(&self, _completion: Completion, _kind: FileSyncType) -> core::Result<Completion> {
        Err(core::LimboError::ReadOnly)
    }

    fn size(&self) -> core::Result<u64> {
        self.inner.size()
    }

    fn truncate(&self, _length: u64, _completion: Completion) -> core::Result<Completion> {
        Err(core::LimboError::ReadOnly)
    }

    fn punch_hole(&self, _position: usize, _length: usize) -> core::Result<()> {
        Err(core::LimboError::ReadOnly)
    }
}

#[cfg(all(test, unix))]
mod tests;

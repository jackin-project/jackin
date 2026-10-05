//! Process-scoped native locks require custody of every descriptor for an inode.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Weak};

use turso::core::io::{FileId, FileSyncType};
use turso::core::{self, Clock, Completion, File, IO, OpenFlags};

const MAX_PROBES: usize = 512;
static PROBE_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct Registry {
    files: HashMap<FileId, Weak<PhysicalFile>>,
    unidentified: Vec<Probe>,
}

impl Registry {
    fn retain_probe(&mut self, identity: FileId, probe: Probe) -> Option<Arc<PhysicalFile>> {
        if let Some(existing) = self.files.get(&identity).and_then(Weak::upgrade) {
            existing
                .probes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(probe);
            Some(existing)
        } else {
            if self.files.contains_key(&identity) {
                self.unidentified.push(probe);
            }
            None
        }
    }
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

struct ProbeBudget(&'static AtomicUsize);

impl ProbeBudget {
    fn reserve(counter: &'static AtomicUsize, limit: usize) -> core::Result<Self> {
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < limit).then_some(count + 1)
            })
            .map(|_| Self(counter))
            .map_err(|_| core::LimboError::Busy)
    }
}

impl Drop for ProbeBudget {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Probe {
    file: std::fs::File,
    _budget: ProbeBudget,
}

/// Field order is custody: every early return closes unregistered native and
/// probe descriptors before unlocking the admission registry.
struct PendingAdmission {
    native: Option<Arc<dyn File>>,
    probe: Option<Probe>,
    registry: MutexGuard<'static, Registry>,
}

impl PendingAdmission {
    fn probe(&self) -> &Probe {
        self.probe.as_ref().expect("probe has not transferred")
    }
    fn take_probe(&mut self) -> Probe {
        self.probe.take().expect("probe transfers once")
    }
    fn retain_probe(&mut self, identity: FileId) -> Option<Arc<PhysicalFile>> {
        let probe = self.take_probe();
        self.registry.retain_probe(identity, probe)
    }
}

pub(super) struct PhysicalIO {
    inner: core::PlatformIO,
    main_path: String,
    main_identity: Mutex<Option<FileId>>,
    probe_counter: &'static AtomicUsize,
    probe_limit: usize,
}

impl PhysicalIO {
    pub(super) fn new(path: &str) -> core::Result<Self> {
        #[cfg(not(unix))]
        return Err(core::LimboError::InvalidArgument(
            "pinned local database IO requires Unix".to_owned(),
        ));
        #[cfg(unix)]
        Ok(Self {
            inner: core::PlatformIO::new()?,
            main_path: path.to_owned(),
            main_identity: Mutex::new(None),
            probe_counter: &PROBE_COUNT,
            probe_limit: MAX_PROBES,
        })
    }
}

impl Clock for PhysicalIO {
    fn current_time_monotonic(&self) -> core::MonotonicInstant {
        self.inner.current_time_monotonic()
    }

    fn current_time_wall_clock(&self) -> core::WallClockInstant {
        self.inner.current_time_wall_clock()
    }
}

impl IO for PhysicalIO {
    fn open_file(&self, path: &str, flags: OpenFlags, direct: bool) -> core::Result<Arc<dyn File>> {
        #[cfg(not(unix))]
        {
            let _ = (path, flags, direct);
            Err(core::LimboError::ReadOnly)
        }
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            use std::os::unix::fs::MetadataExt;

            if flags.contains(OpenFlags::NoLock) {
                return Err(core::LimboError::InvalidArgument(
                    "multiprocess local database IO is unsupported".to_owned(),
                ));
            }
            if path == self.main_path {
                match std::fs::metadata(format!("{path}-tshm")) {
                    Ok(_) => {
                        return Err(core::LimboError::InvalidArgument(
                            "multiprocess local database IO is unsupported".to_owned(),
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(core::io_error(error, "coordination metadata")),
                }
            }
            // Reserve before opening: even rejected descriptors must stay alive
            // when their inode has a process-scoped native lock.
            let budget = ProbeBudget::reserve(self.probe_counter, self.probe_limit)?;
            let readonly = flags.contains(OpenFlags::ReadOnly);
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(!readonly)
                .create(!readonly && flags.contains(OpenFlags::Create))
                .open(path)
                .map_err(|error| core::io_error(error, "open"))?;
            let probe = Probe {
                file,
                _budget: budget,
            };
            let mut admission = PendingAdmission {
                native: None,
                probe: Some(probe),
                registry: REGISTRY
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            };
            let metadata = match admission.probe().file.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    // Without an identity, conservatively retain until all
                    // native leases have finished; never release a live lock.
                    let probe = admission.take_probe();
                    admission.registry.unidentified.push(probe);
                    if admission.registry.files.is_empty() {
                        admission.registry.unidentified.clear();
                    }
                    return Err(core::io_error(error, "metadata"));
                }
            };
            let identity = FileId {
                dev: metadata.dev(),
                ino: metadata.ino(),
            };
            let previous_identity = *self
                .main_identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let owner_identity = if path == self.main_path {
                identity
            } else {
                let Some(owner) = previous_identity else {
                    let retained = admission.retain_probe(identity);
                    drop(admission);
                    drop(retained);
                    return Err(core::LimboError::InvalidArgument(
                        "local database sidecar opened before main authority".to_owned(),
                    ));
                };
                owner
            };
            let slot = if path == self.main_path {
                "main".to_owned()
            } else {
                format!("sidecar:{path}")
            };
            if path != self.main_path && identity == owner_identity {
                let retained = admission.retain_probe(identity);
                drop(admission);
                drop(retained);
                return Err(core::LimboError::InvalidArgument(
                    "local database sidecar aliases main authority".to_owned(),
                ));
            }
            if path == self.main_path
                && (metadata.nlink() != 1
                    || previous_identity.is_some_and(|previous| previous != identity))
            {
                let retained = admission.retain_probe(identity);
                drop(admission);
                drop(retained);
                return Err(core::LimboError::InvalidArgument(
                    "local database has ambiguous or replaced physical authority".to_owned(),
                ));
            }
            if let Some(existing) = admission
                .registry
                .files
                .get(&identity)
                .and_then(Weak::upgrade)
            {
                existing
                    .probes
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(admission.take_probe());
                if existing.readonly != readonly
                    || existing.owner_identity != owner_identity
                    || existing.slot != slot
                {
                    drop(admission);
                    return Err(core::LimboError::Busy);
                }
                if path == self.main_path {
                    *self
                        .main_identity
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(identity);
                }
                return Ok(existing);
            }
            // An expired entry can be a destructor waiting for REGISTRY. It
            // still owns native descriptors: do not reopen or drop this probe.
            if admission.registry.files.contains_key(&identity) {
                let probe = admission.take_probe();
                admission.registry.unidentified.push(probe);
                return Err(core::LimboError::Busy);
            }
            let pinned_path = format!("/dev/fd/{}", admission.probe().file.as_raw_fd());
            admission.native = Some(self.inner.open_file(
                &pinned_path,
                flags & !OpenFlags::Create,
                direct,
            )?);
            if !readonly {
                // UnixIO's automatic lock can be disabled by an inherited
                // dependency environment flag. Custody never inherits that
                // escape hatch; acquiring the same lock again is idempotent.
                admission
                    .native
                    .as_ref()
                    .expect("native file admitted")
                    .lock_file(true)?;
            }
            let physical = Arc::new(PhysicalFile {
                inner: admission.native.take(),
                probes: Mutex::new(vec![admission.take_probe()]),
                identity,
                owner_identity,
                slot,
                readonly,
            });
            admission
                .registry
                .files
                .insert(identity, Arc::downgrade(&physical));
            if path == self.main_path {
                *self
                    .main_identity
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(identity);
            }
            Ok(physical)
        }
    }

    fn remove_file(&self, _path: &str) -> core::Result<()> {
        Err(core::LimboError::ReadOnly)
    }

    fn step(&self) -> core::Result<()> {
        self.inner.step()
    }

    fn file_id(&self, path: &str) -> core::Result<FileId> {
        if path == self.main_path {
            if let Some(identity) = *self
                .main_identity
                .lock()
                .map_err(|_| core::LimboError::Busy)?
            {
                return Ok(identity);
            }
        }
        self.inner.file_id(path)
    }
}

struct PhysicalFile {
    inner: Option<Arc<dyn File>>,
    probes: Mutex<Vec<Probe>>,
    identity: FileId,
    owner_identity: FileId,
    slot: String,
    readonly: bool,
}

impl PhysicalFile {
    fn inner(&self) -> &dyn File {
        self.inner
            .as_deref()
            .expect("native file remains until final lease drop")
    }
}

impl Drop for PhysicalFile {
    fn drop(&mut self) {
        // Keep the registry locked while releasing native descriptors. A Weak
        // can expire before this destructor starts; another open must not treat
        // expiration as proof that native locks have already disappeared.
        let mut registry = REGISTRY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(self.inner.take());
        self.probes
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        registry.files.remove(&self.identity);
        if registry.files.is_empty() {
            registry.unidentified.clear();
        }
    }
}

impl File for PhysicalFile {
    fn lock_file(&self, exclusive: bool) -> core::Result<()> {
        self.inner().lock_file(exclusive)
    }
    fn unlock_file(&self) -> core::Result<()> {
        self.inner().unlock_file()
    }
    fn pread(&self, position: u64, completion: Completion) -> core::Result<Completion> {
        self.inner().pread(position, completion)
    }
    fn pwrite(
        &self,
        position: u64,
        buffer: Arc<core::Buffer>,
        completion: Completion,
    ) -> core::Result<Completion> {
        self.inner().pwrite(position, buffer, completion)
    }
    fn pwritev(
        &self,
        position: u64,
        buffers: Vec<Arc<core::Buffer>>,
        completion: Completion,
    ) -> core::Result<Completion> {
        self.inner().pwritev(position, buffers, completion)
    }
    fn sync(&self, completion: Completion, kind: FileSyncType) -> core::Result<Completion> {
        self.inner().sync(completion, kind)
    }
    fn size(&self) -> core::Result<u64> {
        self.inner().size()
    }
    fn truncate(&self, length: u64, completion: Completion) -> core::Result<Completion> {
        self.inner().truncate(length, completion)
    }
    fn has_hole(&self, position: usize, length: usize) -> core::Result<bool> {
        self.inner().has_hole(position, length)
    }
    fn punch_hole(&self, position: usize, length: usize) -> core::Result<()> {
        self.inner().punch_hole(position, length)
    }
}

#[cfg(test)]
pub(super) fn file_bytes(file: &dyn File) -> Vec<u8> {
    let buffer = Arc::new(core::Buffer::new_temporary(
        usize::try_from(file.size().unwrap()).unwrap(),
    ));
    let completion = file
        .pread(0, Completion::new_read(buffer.clone(), |_| None))
        .unwrap();
    assert!(completion.finished());
    buffer.as_slice().to_vec()
}

#[cfg(test)]
pub(crate) fn source_bytes(path: &str) -> (Vec<u8>, Option<Vec<u8>>) {
    let io = PhysicalIO::new(path).unwrap();
    let main = io.open_file(path, OpenFlags::None, false).unwrap();
    let database = file_bytes(main.as_ref());
    let wal = match io.open_file(&format!("{path}-wal"), OpenFlags::None, false) {
        Ok(file) => Some(file_bytes(file.as_ref())),
        Err(core::LimboError::CompletionError(core::CompletionError::IOError(
            std::io::ErrorKind::NotFound,
            _,
        ))) => None,
        Err(error) => panic!("source fixture bytes failed: {error}"),
    };
    (database, wal)
}

#[cfg(all(test, unix))]
mod tests;

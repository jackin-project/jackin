use super::*;

const LOCK_CHILD_PATH: &str = "JACKIN_NATIVE_IO_LOCK_CHILD_PATH";
const LOCK_CHILD_BUSY: &str = "JACKIN_NATIVE_IO_LOCK_CHILD_BUSY";

struct NativeDropWitness {
    inner: Option<Arc<dyn File>>,
    path: std::path::PathBuf,
    observed: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for NativeDropWitness {
    fn drop(&mut self) {
        assert!(
            REGISTRY.try_lock().is_err(),
            "native drop escaped registry custody"
        );
        // The probe must still be alive: closing it first would clear this
        // process's native fcntl lock before native completion/drop finishes.
        assert_native_lock(&self.path, true);
        drop(self.inner.take());
        self.observed.store(true, Ordering::Release);
    }
}

impl File for NativeDropWitness {
    fn lock_file(&self, exclusive: bool) -> core::Result<()> {
        self.inner.as_ref().unwrap().lock_file(exclusive)
    }
    fn unlock_file(&self) -> core::Result<()> {
        self.inner.as_ref().unwrap().unlock_file()
    }
    fn pread(&self, position: u64, completion: Completion) -> core::Result<Completion> {
        self.inner.as_ref().unwrap().pread(position, completion)
    }
    fn pwrite(
        &self,
        position: u64,
        buffer: Arc<core::Buffer>,
        completion: Completion,
    ) -> core::Result<Completion> {
        self.inner
            .as_ref()
            .unwrap()
            .pwrite(position, buffer, completion)
    }
    fn sync(&self, completion: Completion, kind: FileSyncType) -> core::Result<Completion> {
        self.inner.as_ref().unwrap().sync(completion, kind)
    }
    fn size(&self) -> core::Result<u64> {
        self.inner.as_ref().unwrap().size()
    }
    fn truncate(&self, length: u64, completion: Completion) -> core::Result<Completion> {
        self.inner.as_ref().unwrap().truncate(length, completion)
    }
}

#[test]
fn unregistered_admission_drops_native_then_probe_before_registry_unlock() {
    use std::os::fd::AsRawFd;
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("failed-admission.db");
    std::fs::write(&path, b"committed").unwrap();
    let probe = Probe {
        file: std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap(),
        _budget: ProbeBudget::reserve(&COUNTER, 1).unwrap(),
    };
    let registry = REGISTRY.lock().unwrap();
    let native = core::PlatformIO::new()
        .unwrap()
        .open_file(
            &format!("/dev/fd/{}", probe.file.as_raw_fd()),
            OpenFlags::None,
            false,
        )
        .unwrap();
    native.lock_file(true).unwrap();
    let observed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let admission = PendingAdmission {
        native: Some(Arc::new(NativeDropWitness {
            inner: Some(native),
            path: path.clone(),
            observed: observed.clone(),
        })),
        probe: Some(probe),
        registry,
    };
    drop(admission);
    assert!(observed.load(Ordering::Acquire));
    assert_eq!(COUNTER.load(Ordering::Acquire), 0);
    assert_native_lock(&path, false);
}

#[test]
fn exhausted_probe_budget_rejects_before_create_open() {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("never-created.db");
    let mut io = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    io.probe_counter = &COUNTER;
    io.probe_limit = 0;
    assert!(matches!(
        io.open_file(path.to_str().unwrap(), OpenFlags::Create, false),
        Err(core::LimboError::Busy)
    ));
    assert!(!path.exists());
    assert_eq!(COUNTER.load(Ordering::Acquire), 0);
}

#[test]
fn native_lock_probe_child() {
    let Some(path) = std::env::var_os(LOCK_CHILD_PATH) else {
        return;
    };
    let io = core::PlatformIO::new().unwrap();
    let result = io.open_file(path.to_str().unwrap(), OpenFlags::None, false);
    assert_eq!(result.is_err(), std::env::var_os(LOCK_CHILD_BUSY).is_some());
}

fn assert_native_lock(path: &std::path::Path, busy: bool) {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "store_backend::physical::tests::native_lock_probe_child",
            "--nocapture",
        ])
        .env(LOCK_CHILD_PATH, path)
        .env_remove(LOCK_CHILD_BUSY)
        .env_remove("LIMBO_DISABLE_FILE_LOCK");
    if busy {
        child.env(LOCK_CHILD_BUSY, "1");
    }
    assert!(child.status().unwrap().success());
}

#[test]
fn adapter_locks_with_dependency_lock_environment_disabled_child() {
    let Some(path) = std::env::var_os(LOCK_CHILD_PATH) else {
        return;
    };
    let io = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    let file = io
        .open_file(path.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    assert_native_lock(std::path::Path::new(&path), true);
    drop(file);
    assert_native_lock(std::path::Path::new(&path), false);
}

#[test]
fn inherited_dependency_lock_disable_cannot_bypass_adapter_custody() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("environment.db");
    std::fs::write(&path, b"committed").unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "store_backend::physical::tests::adapter_locks_with_dependency_lock_environment_disabled_child", "--nocapture"])
        .env(LOCK_CHILD_PATH, &path)
        .env("LIMBO_DISABLE_FILE_LOCK", "1")
        .status().unwrap();
    assert!(status.success());
}

#[test]
fn hardlink_denial_retains_probe_without_releasing_live_native_lock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.db");
    let alias = directory.path().join("hardlink.db");
    std::fs::write(&path, b"committed").unwrap();
    let io = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    let file = io
        .open_file(path.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    std::fs::hard_link(&path, &alias).unwrap();
    let alias_io = PhysicalIO::new(alias.to_str().unwrap()).unwrap();
    assert!(
        alias_io
            .open_file(alias.to_str().unwrap(), OpenFlags::ReadOnly, false)
            .is_err()
    );
    drop(alias_io);
    assert_native_lock(&path, true);
    assert_eq!(file_bytes(file.as_ref()), b"committed");
    std::fs::remove_file(&alias).unwrap();
    drop(file);
    assert_native_lock(&path, false);
}

#[test]
fn rejected_reader_probe_preserves_writer_native_lock_until_last_file_drop() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lease.db");
    std::fs::write(&path, b"committed").unwrap();
    let path_string = path.to_str().unwrap();
    let writer_io = PhysicalIO::new(path_string).unwrap();
    let writer = writer_io
        .open_file(path_string, OpenFlags::None, false)
        .unwrap();
    let reader_io = PhysicalIO::new(path_string).unwrap();
    assert!(matches!(
        reader_io.open_file(path_string, OpenFlags::ReadOnly, false),
        Err(core::LimboError::Busy)
    ));
    drop(reader_io);
    assert_native_lock(&path, true);
    assert_eq!(file_bytes(writer.as_ref()), b"committed");
    drop(writer_io);
    assert_native_lock(&path, true);
    drop(writer);
    assert_native_lock(&path, false);
}

#[test]
fn symlink_alias_writer_reuses_exact_native_file_and_retains_all_probes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.db");
    let alias = directory.path().join("alias.db");
    std::fs::write(&path, b"committed").unwrap();
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    let writer = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    let file = writer
        .open_file(path.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    let alias_io = PhysicalIO::new(alias.to_str().unwrap()).unwrap();
    let alias_file = alias_io
        .open_file(alias.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    assert!(Arc::ptr_eq(&file, &alias_file));
    drop(alias_file);
    drop(alias_io);
    assert_native_lock(&path, true);
    drop(file);
    assert_native_lock(&path, false);
}

#[test]
fn pinned_descriptor_survives_path_replacement_without_identity_rebinding() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("replace.db");
    let moved = directory.path().join("old.db");
    std::fs::write(&path, b"old generation").unwrap();
    let path_string = path.to_str().unwrap();
    let io = PhysicalIO::new(path_string).unwrap();
    let old = io.open_file(path_string, OpenFlags::None, false).unwrap();
    let identity = io.file_id(path_string).unwrap();
    std::fs::rename(&path, &moved).unwrap();
    std::fs::write(&path, b"new").unwrap();
    assert_eq!(old.size().unwrap(), 14);
    assert_eq!(io.file_id(path_string).unwrap(), identity);
    assert!(io.open_file(path_string, OpenFlags::None, false).is_err());
    assert_native_lock(&moved, true);
    assert_native_lock(&path, false);
    drop(old);
    assert_native_lock(&moved, false);
}

#[test]
fn replacement_database_cannot_reuse_old_live_wal_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.db");
    let wal_path = directory.path().join("main.db-wal");
    std::fs::write(&path, b"old").unwrap();
    std::fs::write(&wal_path, b"old wal").unwrap();
    let old_io = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    let old_db = old_io
        .open_file(path.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    let old_wal = old_io
        .open_file(wal_path.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    std::fs::rename(&path, directory.path().join("old.db")).unwrap();
    std::fs::write(&path, b"new").unwrap();
    let new_io = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    let new_db = new_io
        .open_file(path.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    assert!(matches!(
        new_io.open_file(wal_path.to_str().unwrap(), OpenFlags::None, false),
        Err(core::LimboError::Busy)
    ));
    assert_native_lock(&wal_path, true);
    assert_eq!(file_bytes(old_wal.as_ref()), b"old wal");
    drop(new_db);
    drop(old_db);
    assert_native_lock(&wal_path, true);
    drop(old_wal);
    assert_native_lock(&wal_path, false);
}

#[test]
fn wal_symlink_to_main_is_rejected_without_releasing_lock_or_changing_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.db");
    let wal_path = directory.path().join("main.db-wal");
    std::fs::write(&path, b"main authority").unwrap();
    let io = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    let main = io
        .open_file(path.to_str().unwrap(), OpenFlags::None, false)
        .unwrap();
    std::os::unix::fs::symlink(&path, &wal_path).unwrap();
    assert!(
        io.open_file(wal_path.to_str().unwrap(), OpenFlags::Create, false)
            .is_err()
    );
    assert_native_lock(&path, true);
    assert_eq!(file_bytes(main.as_ref()), b"main authority");
    drop(main);
    assert_native_lock(&path, false);
}

#[test]
fn synchronous_read_completion_finishes_before_file_custody_can_drop() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("completion.db");
    std::fs::write(&path, b"committed").unwrap();
    let io = PhysicalIO::new(path.to_str().unwrap()).unwrap();
    let file = io
        .open_file(path.to_str().unwrap(), OpenFlags::ReadOnly, false)
        .unwrap();
    file.lock_file(false).unwrap();
    let completion = Completion::new_read(Arc::new(core::Buffer::new_temporary(9)), |_| None);
    let completed = file.pread(0, completion).unwrap();
    assert!(completed.finished());
    io.cancel(std::slice::from_ref(&completed)).unwrap();
    assert_native_lock(&path, true);
    drop(file);
    assert_native_lock(&path, false);
}

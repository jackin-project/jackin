use super::*;

fn reader(path: &str) -> ReadOnlyIO {
    ReadOnlyIO {
        inner: PhysicalIO::new(path).unwrap(),
        path: path.to_owned(),
        wal_path: format!("{path}-wal"),
    }
}

#[test]
fn create_flags_cannot_create_missing_database_or_wal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.db");
    let path = path.to_str().unwrap();
    let io = reader(path);
    for file in [path.to_owned(), format!("{path}-wal")] {
        assert!(io.open_file(&file, OpenFlags::Create, false).is_err());
        assert!(!std::path::Path::new(&file).exists());
    }
    assert!(read_admitted(path, |_| Ok(())).unwrap().is_none());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn deleted_database_is_missing_and_never_recreated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deleted.db");
    std::fs::write(&path, b"original bytes").unwrap();
    let io = reader(path.to_str().unwrap());
    let file = io
        .open_file(path.to_str().unwrap(), OpenFlags::Create, false)
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    // The capability remains pinned to the removed inode; reopening the path
    // reports missing rather than silently creating a replacement database.
    assert_eq!(file.size().unwrap(), 14);
    assert!(
        read_admitted(path.to_str().unwrap(), |_| Ok(()))
            .unwrap()
            .is_none()
    );
    assert!(!path.exists());
}

#[test]
fn database_and_wal_capabilities_reject_every_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("protected.db");
    let path = path.to_str().unwrap();
    let io = reader(path);
    for file_path in [path.to_owned(), format!("{path}-wal")] {
        std::fs::write(&file_path, b"unchanged").unwrap();
        let file = io.open_file(&file_path, OpenFlags::Create, false).unwrap();
        let buffer = Arc::new(core::Buffer::new_temporary(1));
        assert!(
            file.pwrite(0, buffer.clone(), Completion::new_write(|_| {}))
                .is_err()
        );
        assert!(
            file.pwritev(0, vec![buffer], Completion::new_write(|_| {}))
                .is_err()
        );
        assert!(file.truncate(0, Completion::new_trunc(|_| {})).is_err());
        assert!(
            file.sync(Completion::new_sync(|_| {}), FileSyncType::Fsync)
                .is_err()
        );
        assert!(file.punch_hole(0, 1).is_err());
        assert!(file.shared_wal_set_len(0).is_err());
        assert!(file.shared_wal_map(0, 1).is_err());
        assert!(file.lock_file(true).is_err());
        assert!(io.remove_file(&file_path).is_err());
        assert_eq!(std::fs::read(&file_path).unwrap(), b"unchanged");
    }
    assert!(!io.supports_shared_wal_coordination());
    assert!(io.open_shared_wal_file(&format!("{path}-tshm")).is_err());
    assert!(!std::path::Path::new(&format!("{path}-tshm")).exists());
}

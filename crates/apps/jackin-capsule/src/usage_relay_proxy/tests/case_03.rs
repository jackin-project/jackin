// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::ffi::CString;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::UnixListener as StdUnixListener;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};
use tempfile::TempDir;
use tokio::io::AsyncWrite;

type V2SetupResult = (
    SupervisorIdentity,
    UsageRelayAuthorization,
    UnixListener,
    V2SocketCleanup,
);

fn open_directory_path(path: &Path) -> Result<File> {
    Ok(File::from(open(
        path,
        SECURE_DIRECTORY_OPEN_FLAGS,
        Mode::empty(),
    )?))
}

#[cfg(target_os = "macos")]
fn test_mode(mode: libc::mode_t) -> u32 {
    u32::from(mode)
}

#[cfg(not(target_os = "macos"))]
fn test_mode(mode: libc::mode_t) -> u32 {
    mode
}

struct OwnerFixture {
    temp: TempDir,
    root_name: CString,
    owner_uid: u32,
    owner_gid: u32,
}

impl OwnerFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let metadata = fs::metadata(temp.path()).unwrap();
        let root_name = CString::new(temp.path().file_name().unwrap().as_bytes()).unwrap();
        let leaf_path = temp.path().join("usage-relay-v2");
        fs::create_dir(&leaf_path).unwrap();
        fs::set_permissions(
            &leaf_path,
            fs::Permissions::from_mode(test_mode(OWNER_DIRECTORY_MODE)),
        )
        .unwrap();
        let leaf = open_directory_path(&leaf_path).unwrap();
        let lock = openat(
            &leaf,
            OWNER_LOCK_NAME,
            OWNER_LOCK_OPEN_FLAGS
                .union(OFlag::O_CREAT)
                .union(OFlag::O_EXCL),
            Mode::from_bits_truncate(OWNER_LOCK_MODE),
        )
        .unwrap();
        drop(File::from(lock));
        Self {
            temp,
            root_name,
            owner_uid: metadata.uid(),
            owner_gid: fs::metadata(leaf_path).unwrap().gid(),
        }
    }

    fn leaf_path(&self) -> PathBuf {
        self.temp.path().join("usage-relay-v2")
    }

    fn socket_path(&self) -> PathBuf {
        self.leaf_path().join("usage.sock")
    }

    fn owner(&self, proxy_uid: u32) -> Result<V2SocketOwner> {
        acquire_v2_owner(
            open_directory_path(self.temp.path())?,
            open_directory_path(self.temp.path().parent().unwrap())?,
            &self.root_name,
            self.owner_uid,
            self.owner_uid,
            self.owner_gid,
            proxy_uid,
        )
    }

    fn bind(&self) -> Result<(UnixListener, V2SocketCleanup)> {
        let proxy_uid = nix::unistd::geteuid().as_raw();
        bind_v2_socket_at(
            self.owner(proxy_uid)?,
            &self.socket_path(),
            OWNER_SOCKET_NAME,
            proxy_uid,
            self.owner_gid,
        )
    }

    fn setup(&self) -> impl FnOnce() -> Result<V2SetupResult> + Send + 'static {
        let root = self.temp.path().to_owned();
        let parent = self.temp.path().parent().unwrap().to_owned();
        let root_name = self.root_name.clone();
        let owner_uid = self.owner_uid;
        let owner_gid = self.owner_gid;
        move || {
            let proxy_uid = nix::unistd::geteuid().as_raw();
            let owner = acquire_v2_owner(
                open_directory_path(&root)?,
                open_directory_path(&parent)?,
                &root_name,
                owner_uid,
                owner_uid,
                owner_gid,
                proxy_uid,
            )?;
            let socket_path = root.join("usage-relay-v2").join("usage.sock");
            let (listener, cleanup) =
                bind_v2_socket_at(owner, &socket_path, OWNER_SOCKET_NAME, proxy_uid, owner_gid)?;
            let (authorization, _) = single_session_authorization(1, 1, "session-a");
            Ok((supervisor(123), authorization, listener, cleanup))
        }
    }
}

#[test]
fn owner_identity_allows_only_image_agent_or_root() {
    validate_owner_policy(1_000, 0, 1_000, 0, 1_000, 0, 1_000)
        .expect("image-agent proxy UID is allowed");
    validate_owner_policy(1_000, 0, 1_000, 0, 1_000, 0, 0).expect("root proxy UID is allowed");
    assert!(validate_owner_policy(1_000, 0, 1_000, 0, 1_000, 0, 2_000).is_err());
    assert!(validate_owner_policy(1_000, 0, 2_000, 0, 1_000, 0, 0).is_err());
    assert!(validate_owner_policy(1_000, 9, 1_000, 9, 1_000, 0, 0).is_err());

    let fixture = OwnerFixture::new();
    let directory = open_directory_path(&fixture.leaf_path()).unwrap();
    let mut stat = fstat(&directory).unwrap();
    for links in [2, 3] {
        stat.st_nlink = links;
        validate_directory_stat(
            &stat,
            fixture.owner_uid,
            Some((fixture.owner_gid, OWNER_DIRECTORY_MODE)),
            "test usage relay directory",
        )
        .expect("directory with an allowed link count is accepted");
    }

    stat.st_nlink = 1;
    assert!(
        validate_directory_stat(
            &stat,
            fixture.owner_uid,
            Some((fixture.owner_gid, OWNER_DIRECTORY_MODE)),
            "test usage relay directory",
        )
        .is_err()
    );

    stat.st_nlink = 2;
    assert!(
        validate_directory_stat(
            &stat,
            fixture.owner_uid.wrapping_add(1),
            Some((fixture.owner_gid, OWNER_DIRECTORY_MODE)),
            "test usage relay directory",
        )
        .is_err()
    );
    assert!(
        validate_directory_stat(
            &stat,
            fixture.owner_uid,
            Some((fixture.owner_gid.wrapping_add(1), OWNER_DIRECTORY_MODE)),
            "test usage relay directory",
        )
        .is_err()
    );
    assert!(
        validate_directory_stat(
            &stat,
            fixture.owner_uid,
            Some((fixture.owner_gid, OWNER_DIRECTORY_MODE ^ 0o100)),
            "test usage relay directory",
        )
        .is_err()
    );
}

#[tokio::test]
async fn preprovisioned_fixture_is_accepted_without_repair() {
    let fixture = OwnerFixture::new();
    let (listener, cleanup) = fixture.bind().unwrap();
    assert!(v2_path_is_socket(&listener, &fixture.socket_path()));
    drop(listener);
    drop(cleanup);
    assert!(!fixture.socket_path().exists());
}

#[test]
fn permanent_owner_lock_requires_a_private_single_link_regular_file() {
    let hardlinked = OwnerFixture::new();
    let lock_path = hardlinked.leaf_path().join("owner.lock");
    let alias_path = hardlinked.leaf_path().join("lock-alias");
    fs::hard_link(&lock_path, &alias_path).unwrap();
    let error = hardlinked
        .owner(nix::unistd::geteuid().as_raw())
        .err()
        .expect("a hardlinked lock must be rejected");
    assert!(
        error
            .to_string()
            .contains("unsafe owner, type, size, or link count")
    );
    assert_eq!(fs::symlink_metadata(&lock_path).unwrap().nlink(), 2);
    assert!(alias_path.exists());

    let loose = OwnerFixture::new();
    let loose_path = loose.leaf_path().join("owner.lock");
    fs::set_permissions(&loose_path, fs::Permissions::from_mode(test_mode(0o644))).unwrap();
    let error = loose
        .owner(nix::unistd::geteuid().as_raw())
        .err()
        .expect("a loose lock mode must be rejected");
    assert!(error.to_string().contains("preprovisioned owner policy"));
    assert_eq!(
        fs::symlink_metadata(&loose_path).unwrap().mode() & 0o7777,
        0o644
    );

    let fifo = OwnerFixture::new();
    let fifo_path = fifo.leaf_path().join("owner.lock");
    fs::remove_file(&fifo_path).unwrap();
    nix::unistd::mkfifo(&fifo_path, Mode::from_bits_truncate(OWNER_LOCK_MODE)).unwrap();
    assert!(
        fs::symlink_metadata(&fifo_path)
            .unwrap()
            .file_type()
            .is_fifo()
    );
    let error = fifo
        .owner(nix::unistd::geteuid().as_raw())
        .err()
        .expect("a FIFO lock must be rejected");
    assert!(
        error
            .to_string()
            .contains("unsafe owner, type, size, or link count")
    );
    assert!(
        fs::symlink_metadata(&fifo_path)
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[tokio::test]
async fn busy_v2_owner_preserves_live_socket_and_v1_endpoint() {
    let fixture = OwnerFixture::new();
    let v1_path = fixture.temp.path().join("usage-v1.sock");
    let v1_listener = UnixListener::bind(&v1_path).unwrap();
    let (v2_listener, v2_cleanup) = fixture.bind().unwrap();
    let v1_client = UnixStream::connect(&v1_path).await.unwrap();
    let (_v1_server, _) = v1_listener.accept().await.unwrap();
    let socket_before = fstatat(
        &v2_cleanup.owner.leaf,
        OWNER_SOCKET_NAME,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .unwrap();

    let second_owner = fixture.owner(nix::unistd::geteuid().as_raw());
    assert!(
        second_owner.is_err(),
        "the independent lock open must be busy"
    );
    let socket_after = fstatat(
        &v2_cleanup.owner.leaf,
        OWNER_SOCKET_NAME,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .unwrap();
    assert_eq!(
        FsIdentity::from_stat(&socket_before),
        FsIdentity::from_stat(&socket_after)
    );
    assert!(v1_path.exists(), "the v1 endpoint remains untouched");
    assert!(v2_path_is_socket(&v2_listener, &fixture.socket_path()));

    drop(v1_client);
    drop(v2_listener);
    drop(v2_cleanup);
    drop(v1_listener);
}

#[tokio::test]
async fn stale_v2_socket_is_replaced_only_after_lock_acquisition() {
    let fixture = OwnerFixture::new();
    let stale = StdUnixListener::bind(fixture.socket_path()).unwrap();
    drop(stale);
    fs::set_permissions(
        fixture.socket_path(),
        fs::Permissions::from_mode(test_mode(OWNER_SOCKET_MODE)),
    )
    .unwrap();
    let old = fstatat(
        open_directory_path(&fixture.leaf_path()).unwrap(),
        OWNER_SOCKET_NAME,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .unwrap();

    let held_owner = fixture.owner(nix::unistd::geteuid().as_raw()).unwrap();
    let busy = match fixture.bind() {
        Ok((listener, cleanup)) => {
            drop(listener);
            drop(cleanup);
            panic!("second owner must stop before inspecting or removing a stale socket")
        }
        Err(error) => error,
    };
    assert!(busy.to_string().contains("owner lock is busy"));
    assert_eq!(
        FsIdentity::from_stat(&old),
        FsIdentity::from_stat(
            &fstatat(
                open_directory_path(&fixture.leaf_path()).unwrap(),
                OWNER_SOCKET_NAME,
                AtFlags::AT_SYMLINK_NOFOLLOW,
            )
            .unwrap()
        )
    );
    drop(held_owner);

    let (listener, cleanup) = fixture.bind().unwrap();
    let current = fstatat(
        &cleanup.owner.leaf,
        OWNER_SOCKET_NAME,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .unwrap();
    assert_ne!(FsIdentity::from_stat(&old), FsIdentity::from_stat(&current));
    assert!(v2_path_is_socket(&listener, &fixture.socket_path()));
    drop(listener);
    drop(cleanup);
    assert!(!fixture.socket_path().exists());
}

#[tokio::test]
async fn cleanup_does_not_unlink_a_replacement_socket_path() {
    let fixture = OwnerFixture::new();
    let (listener, cleanup) = fixture.bind().unwrap();
    let old_path = fixture.leaf_path().join("old.sock");
    fs::rename(fixture.socket_path(), &old_path).unwrap();
    let replacement = StdUnixListener::bind(fixture.socket_path()).unwrap();
    let replacement_stat = fstatat(
        &cleanup.owner.leaf,
        OWNER_SOCKET_NAME,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .unwrap();
    assert_ne!(
        cleanup.identity,
        FsIdentity::from_stat(&replacement_stat),
        "the retained old name prevents inode reuse in this fixture"
    );

    drop(listener);
    drop(cleanup);
    assert!(fixture.socket_path().exists());
    drop(replacement);
    drop(fs::remove_file(old_path));
}

#[cfg(unix)]
#[tokio::test]
async fn unsafe_socket_entry_is_refused_without_unlinking_it() {
    let fixture = OwnerFixture::new();
    let target = fixture.temp.path().join("target");
    fs::write(&target, b"preserve").unwrap();
    std::os::unix::fs::symlink(&target, fixture.socket_path()).unwrap();
    let error = match fixture.bind() {
        Ok((listener, cleanup)) => {
            drop(listener);
            drop(cleanup);
            panic!("unsafe socket path must not be replaced")
        }
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("unsafe owner, type, mode, or link count")
    );
    assert!(
        fs::symlink_metadata(fixture.socket_path())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(target).unwrap(), b"preserve");
}

#[test]
fn unsafe_parent_leaf_and_lock_are_rejected_without_repair() {
    let unsafe_parent = OwnerFixture::new();
    fs::set_permissions(unsafe_parent.temp.path(), fs::Permissions::from_mode(0o720)).unwrap();
    assert!(
        unsafe_parent
            .owner(nix::unistd::geteuid().as_raw())
            .is_err()
    );
    assert_eq!(
        fs::metadata(unsafe_parent.temp.path()).unwrap().mode() & 0o7777,
        0o720,
        "the runtime refuses to repair the parent"
    );

    let unsafe_leaf = OwnerFixture::new();
    let leaf = unsafe_leaf.leaf_path();
    let moved_leaf = unsafe_leaf.temp.path().join("saved-leaf");
    fs::rename(&leaf, &moved_leaf).unwrap();
    std::os::unix::fs::symlink(&moved_leaf, &leaf).unwrap();
    assert!(unsafe_leaf.owner(nix::unistd::geteuid().as_raw()).is_err());
    assert!(
        fs::symlink_metadata(&leaf)
            .unwrap()
            .file_type()
            .is_symlink()
    );

    let unsafe_lock = OwnerFixture::new();
    let lock = unsafe_lock.leaf_path().join("owner.lock");
    let moved_lock = unsafe_lock.leaf_path().join("saved-lock");
    fs::rename(&lock, &moved_lock).unwrap();
    std::os::unix::fs::symlink(&moved_lock, &lock).unwrap();
    assert!(unsafe_lock.owner(nix::unistd::geteuid().as_raw()).is_err());
    assert!(
        fs::symlink_metadata(&lock)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[tokio::test]
async fn extra_v2_arguments_are_rejected_before_startup_or_filesystem_access() {
    let fixture = OwnerFixture::new();
    fs::write(fixture.socket_path(), b"sentinel").unwrap();
    let before = fs::symlink_metadata(fixture.socket_path()).unwrap();
    let setup_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let setup_called_by_closure = Arc::clone(&setup_called);
    let actual_setup = fixture.setup();
    let setup = move || {
        setup_called_by_closure.store(true, Ordering::SeqCst);
        actual_setup()
    };
    let (output, mut peer) = tokio::io::duplex(1024);
    let error = run_v2_with(
        &["unexpected".to_owned()],
        setup,
        tokio::io::empty(),
        output,
    )
    .await
    .unwrap_err();
    let mut ready = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut peer, &mut ready)
        .await
        .unwrap();
    assert!(error.to_string().contains("does not accept arguments"));
    assert!(!setup_called.load(Ordering::SeqCst));
    assert!(ready.is_empty());
    assert_eq!(fs::read(fixture.socket_path()).unwrap(), b"sentinel");
    let after = fs::symlink_metadata(fixture.socket_path()).unwrap();
    assert_eq!(before.ino(), after.ino());
}

#[tokio::test]
async fn setup_lock_failure_sends_no_ready_and_preserves_existing_endpoints() {
    let fixture = OwnerFixture::new();
    let stale = StdUnixListener::bind(fixture.socket_path()).unwrap();
    drop(stale);
    let stale_stat = fstatat(
        open_directory_path(&fixture.leaf_path()).unwrap(),
        OWNER_SOCKET_NAME,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .unwrap();
    let v1_path = fixture.temp.path().join("usage-v1.sock");
    let v1_listener = StdUnixListener::bind(&v1_path).unwrap();
    let held_owner = fixture.owner(nix::unistd::geteuid().as_raw()).unwrap();
    let (output, mut peer) = tokio::io::duplex(1024);
    let error = run_v2_with(&[], fixture.setup(), tokio::io::empty(), output)
        .await
        .unwrap_err();
    let mut ready = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut peer, &mut ready)
        .await
        .unwrap();
    assert!(error.to_string().contains("owner lock is busy"));
    assert!(ready.is_empty());
    assert!(v1_path.exists());
    let stale_after = fstatat(
        open_directory_path(&fixture.leaf_path()).unwrap(),
        OWNER_SOCKET_NAME,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .unwrap();
    assert_eq!(
        FsIdentity::from_stat(&stale_stat),
        FsIdentity::from_stat(&stale_after),
        "setup must fail before inspecting or replacing the v2 endpoint"
    );
    drop(held_owner);
    drop(v1_listener);
}

#[tokio::test]
async fn ready_is_written_while_owner_lock_is_held_then_scoped_cleanup_runs() {
    let fixture = OwnerFixture::new();
    let bytes = Arc::new(StdMutex::new(Vec::new()));
    let lock_observation = Arc::new(StdMutex::new(None));
    let writer = LockCheckingWriter {
        leaf_path: fixture.leaf_path(),
        bytes: Arc::clone(&bytes),
        lock_observation: Arc::clone(&lock_observation),
    };
    let error = run_v2_with(&[], fixture.setup(), tokio::io::empty(), writer)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("usage relay frame is invalid"));
    assert_eq!(
        *bytes.lock().unwrap(),
        UsageRelayReadyV1::canonical().encode_frame().unwrap()
    );
    assert_eq!(*lock_observation.lock().unwrap(), Some(true));
    assert!(!fixture.socket_path().exists());
    let owner = fixture.owner(nix::unistd::geteuid().as_raw()).unwrap();
    drop(owner);
}

#[tokio::test]
async fn partial_ready_write_has_unknown_visibility_and_cleans_up_under_lock() {
    let fixture = OwnerFixture::new();
    let (writer, bytes) = FailAfterPrefix::new();
    let error = run_v2_with(&[], fixture.setup(), tokio::io::empty(), writer)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("peer visibility is unknown"));
    let full = UsageRelayReadyV1::canonical().encode_frame().unwrap();
    let written = bytes.lock().unwrap();
    assert!(!written.is_empty());
    assert_ne!(*written, full, "the failed write may expose a prefix");
    drop(written);
    assert!(!fixture.socket_path().exists());
    let owner = fixture.owner(nix::unistd::geteuid().as_raw()).unwrap();
    drop(owner);
}

#[tokio::test]
async fn ready_flush_failure_is_not_misreported_as_zero_visibility() {
    let fixture = OwnerFixture::new();
    let (writer, bytes) = FailOnFlush::new();
    let error = run_v2_with(&[], fixture.setup(), tokio::io::empty(), writer)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("peer visibility is unknown"));
    let complete = UsageRelayReadyV1::canonical().encode_frame().unwrap();
    assert_eq!(
        *bytes.lock().unwrap(),
        complete,
        "flush can fail after the complete frame is written"
    );
    assert!(!fixture.socket_path().exists());
    let owner = fixture.owner(nix::unistd::geteuid().as_raw()).unwrap();
    drop(owner);
}

struct LockCheckingWriter {
    leaf_path: PathBuf,
    bytes: Arc<StdMutex<Vec<u8>>>,
    lock_observation: Arc<StdMutex<Option<bool>>>,
}

impl AsyncWrite for LockCheckingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let mut observation = this.lock_observation.lock().unwrap();
        if observation.is_none() {
            *observation = Some(owner_lock_is_busy(&this.leaf_path));
        }
        this.bytes.lock().unwrap().extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn owner_lock_is_busy(leaf_path: &Path) -> bool {
    let Ok(leaf) = open_directory_path(leaf_path) else {
        return false;
    };
    let Ok(lock_file) = open_owner_lock_at(&leaf, OWNER_LOCK_NAME) else {
        return false;
    };
    match Flock::lock(lock_file, FlockArg::LockExclusiveNonblock) {
        Ok(lock) => {
            drop(lock);
            false
        }
        Err((_, error)) => error == Errno::EWOULDBLOCK,
    }
}

struct FailAfterPrefix {
    bytes: Arc<StdMutex<Vec<u8>>>,
    failed: bool,
}

impl FailAfterPrefix {
    fn new() -> (Self, Arc<StdMutex<Vec<u8>>>) {
        let bytes = Arc::new(StdMutex::new(Vec::new()));
        (
            Self {
                bytes: Arc::clone(&bytes),
                failed: false,
            },
            bytes,
        )
    }
}

impl AsyncWrite for FailAfterPrefix {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.as_mut().get_mut();
        if this.failed {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "injected readiness write failure",
            )));
        }
        let written = (bytes.len() / 2).max(1);
        this.bytes
            .lock()
            .unwrap()
            .extend_from_slice(&bytes[..written]);
        this.failed = true;
        Poll::Ready(Ok(written))
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

struct FailOnFlush {
    bytes: Arc<StdMutex<Vec<u8>>>,
}

impl FailOnFlush {
    fn new() -> (Self, Arc<StdMutex<Vec<u8>>>) {
        let bytes = Arc::new(StdMutex::new(Vec::new()));
        (
            Self {
                bytes: Arc::clone(&bytes),
            },
            bytes,
        )
    }
}

impl AsyncWrite for FailOnFlush {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "injected readiness flush failure",
        )))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_acl_entry_checks_allow_only_an_empty_acl() {
    classify_darwin_acl_entry(-1, Some(libc::EINVAL), false, "test")
        .expect("empty ACL is accepted");
    assert!(classify_darwin_acl_entry(0, None, true, "test").is_err());
    assert!(classify_darwin_acl_entry(-1, Some(libc::EIO), false, "test").is_err());
    assert!(classify_darwin_acl_entry(0, None, false, "test").is_err());
}

fn v2_path_is_socket(_listener: &UnixListener, path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
}

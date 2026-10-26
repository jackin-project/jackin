// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::Read as _;
use std::os::fd::AsRawFd as _;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError,
    UsageCoordinationErrorKind, UsageRelayReadyV1, UsageRelayTunnelRequest,
    UsageRelayTunnelResponse,
};
use nix::errno::Errno;
use nix::fcntl::{AtFlags, Flock, FlockArg, OFlag, open, openat};
use nix::sys::stat::{FchmodatFlags, Mode, fchmodat, fstat, fstatat};
use nix::unistd::{UnlinkatFlags, unlinkat};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWrite,
    AsyncWriteExt as _, BufReader,
};
use tokio::net::UnixListener;

use tokio::sync::{Mutex, mpsc, oneshot};

mod authorization;
mod identity;

pub(crate) use authorization::UsageRelayAuthorization;

pub(crate) use identity::{
    PeerIdentity, SupervisorIdentity, load_supervisor_identity, peer_identity,
    supervisor_peer_allows,
};

const TUNNEL_CAPACITY: usize = 128;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(35);
const OWNER_DIRECTORY_NAME: &CStr = c"usage-relay-v2";
const OWNER_LOCK_NAME: &CStr = c"owner.lock";
const OWNER_SOCKET_NAME: &CStr = c"usage.sock";
const OWNER_DIRECTORY_MODE: libc::mode_t = 0o700;
const OWNER_LOCK_MODE: libc::mode_t = 0o600;
const OWNER_SOCKET_MODE: libc::mode_t = 0o600;
const ROOT_UID: libc::uid_t = 0;
const OWNER_GID: libc::gid_t = 0;

const SECURE_DIRECTORY_OPEN_FLAGS: OFlag = OFlag::O_RDONLY
    .union(OFlag::O_DIRECTORY)
    .union(OFlag::O_NOFOLLOW)
    .union(OFlag::O_CLOEXEC);
const SECURE_FILE_OPEN_FLAGS: OFlag = OFlag::O_RDONLY
    .union(OFlag::O_NOFOLLOW)
    .union(OFlag::O_CLOEXEC)
    .union(OFlag::O_NONBLOCK);
const OWNER_LOCK_OPEN_FLAGS: OFlag = OFlag::O_RDWR
    .union(OFlag::O_NOFOLLOW)
    .union(OFlag::O_CLOEXEC)
    .union(OFlag::O_NONBLOCK);

type Pending = Arc<Mutex<BTreeMap<u64, oneshot::Sender<UsageBrokerResponse>>>>;
type SupervisorLiveness = Arc<dyn Fn(SupervisorIdentity) -> bool + Send + Sync>;

#[derive(Clone)]
struct SupervisorCheck {
    identity: SupervisorIdentity,
    is_current: SupervisorLiveness,
}

pub(crate) async fn run() -> Result<()> {
    let config = crate::config::load().context("loading Capsule config for usage relay")?;
    let authorization = UsageRelayAuthorization::from_config(&config)
        .context("building usage relay session authorization")?;
    run_at(
        Path::new(jackin_core::container_paths::USAGE_SOCK),
        load_supervisor_identity()?,
        authorization,
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await
}

pub(crate) async fn run_v2(args: &[String]) -> Result<()> {
    run_v2_with(
        args,
        || {
            let config = crate::config::load().context("loading Capsule config for usage relay")?;
            let authorization = UsageRelayAuthorization::from_config(&config)
                .context("building usage relay session authorization")?;
            let supervisor = load_supervisor_identity()?;
            let (listener, cleanup) = bind_owned_v2_socket()?;
            Ok((supervisor, authorization, listener, cleanup))
        },
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await
}

async fn run_v2_with<R, W, S, C>(args: &[String], setup: S, input: R, mut output: W) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    S: FnOnce() -> Result<(SupervisorIdentity, UsageRelayAuthorization, UnixListener, C)>
        + Send
        + 'static,
    C: Send + 'static,
{
    validate_v2_args(args)?;
    let (supervisor, authorization, listener, cleanup) =
        jackin_telemetry::spawn::joined_blocking(setup)
            .await
            .context("usage relay setup task failed")??;
    write_ready_frame(&mut output)
        .await
        .context("usage relay readiness delivery failed; peer visibility is unknown")?;
    run_on_listener(
        listener,
        cleanup,
        SupervisorCheck {
            identity: supervisor,
            is_current: Arc::new(SupervisorIdentity::is_current),
        },
        authorization,
        None,
        input,
        output,
    )
    .await
}

pub(crate) fn validate_v2_args(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        bail!("usage-relay-proxy-v2 does not accept arguments");
    }
    Ok(())
}

fn bind_owned_v2_socket() -> Result<(UnixListener, V2SocketCleanup)> {
    let (root, jackin, agent_uid) = open_image_identity_roots()?;
    let proxy_uid = nix::unistd::geteuid().as_raw();
    let owner = acquire_v2_owner(
        jackin, root, c"jackin", ROOT_UID, agent_uid, OWNER_GID, proxy_uid,
    )?;
    bind_v2_socket_at(
        owner,
        Path::new(jackin_core::container_paths::USAGE_RELAY_V2_SOCK),
        OWNER_SOCKET_NAME,
        proxy_uid,
        OWNER_GID,
    )
}

fn open_image_identity_roots() -> Result<(File, File, libc::uid_t)> {
    let root = File::from(
        open("/", SECURE_DIRECTORY_OPEN_FLAGS, Mode::empty())
            .context("opening filesystem root without following links")?,
    );
    validate_directory_stat(&fstat(&root)?, ROOT_UID, None, "filesystem root")?;

    let jackin = open_directory_at(&root, c"jackin")?;
    let jackin_stat = fstat(&jackin)?;
    let jackin_label = jackin_core::container_paths::JACKIN_ROOT;
    validate_directory_stat(&jackin_stat, ROOT_UID, None, jackin_label)?;
    ensure_no_acl_fd(&jackin).context("checking /jackin ACL")?;
    ensure_same_named_object(&root, c"jackin", &jackin_stat, "checking /jackin identity")?;

    let etc = open_directory_at(&root, c"etc")?;
    let etc_stat = fstat(&etc)?;
    let etc_label = jackin_core::container_paths::SYSTEM_ETC_DIR;
    validate_directory_stat(&etc_stat, ROOT_UID, None, etc_label)?;
    ensure_no_acl_fd(&etc).context("checking /etc ACL")?;
    let passwd = open_file_at(&etc, c"passwd")?;
    let passwd_stat = fstat(&passwd)?;
    let passwd_label = format!("{etc_label}/passwd");
    validate_regular_stat(&passwd_stat, ROOT_UID, None, None, &passwd_label)?;
    if mode_bits(&passwd_stat) & 0o022 != 0 {
        bail!("/etc/passwd is writable by group or other");
    }
    ensure_no_acl_fd(&passwd).context("checking /etc/passwd ACL")?;
    ensure_same_named_object(
        &etc,
        c"passwd",
        &passwd_stat,
        "checking /etc/passwd identity",
    )?;

    let mut contents = String::new();
    (&passwd)
        .take(1024 * 1024 + 1)
        .read_to_string(&mut contents)
        .context("reading bounded /etc/passwd")?;
    if contents.len() > 1024 * 1024 {
        bail!("/etc/passwd exceeds the bounded image identity size");
    }
    let mut agent_uids = contents.lines().filter_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _password = fields.next()?;
        let uid = fields.next()?.parse::<u32>().ok()?;
        (name == "agent").then_some(uid)
    });
    let agent_uid = agent_uids
        .next()
        .context(format!("{passwd_label} does not define the image agent"))?;
    if agent_uid == ROOT_UID || agent_uids.next().is_some() {
        bail!("/etc/passwd has an invalid image agent identity");
    }
    Ok((root, jackin, agent_uid))
}

fn acquire_v2_owner(
    jackin: File,
    root: File,
    root_name: &CStr,
    root_owner_uid: libc::uid_t,
    agent_uid: libc::uid_t,
    expected_gid: libc::gid_t,
    proxy_uid: libc::uid_t,
) -> Result<V2SocketOwner> {
    let jackin_stat = fstat(&jackin)?;
    validate_directory_stat(&jackin_stat, root_owner_uid, None, "usage relay parent")?;
    ensure_no_acl_fd(&jackin).context("checking /jackin ACL")?;
    ensure_same_named_object(
        &root,
        root_name,
        &jackin_stat,
        "checking usage relay parent identity",
    )?;

    let leaf = open_directory_at(&jackin, OWNER_DIRECTORY_NAME)?;
    let leaf_stat = fstat(&leaf)?;
    validate_directory_stat(
        &leaf_stat,
        agent_uid,
        Some((expected_gid, OWNER_DIRECTORY_MODE)),
        "usage relay v2 directory",
    )?;
    ensure_no_acl_fd(&leaf).context("checking usage relay v2 directory ACL")?;
    ensure_same_named_object(
        &jackin,
        OWNER_DIRECTORY_NAME,
        &leaf_stat,
        "checking usage relay v2 directory identity",
    )?;

    let lock_file = open_owner_lock_at(&leaf, OWNER_LOCK_NAME)?;
    let lock_stat = fstat(&lock_file)?;
    validate_regular_stat(
        &lock_stat,
        agent_uid,
        Some((expected_gid, OWNER_LOCK_MODE, 1)),
        Some(0),
        "usage relay v2 owner lock",
    )?;
    ensure_no_acl_fd(&lock_file).context("checking usage relay v2 owner lock ACL")?;
    validate_owner_policy(
        uid(&leaf_stat),
        gid(&leaf_stat),
        uid(&lock_stat),
        gid(&lock_stat),
        agent_uid,
        expected_gid,
        proxy_uid,
    )?;

    let lock = Flock::lock(lock_file, FlockArg::LockExclusiveNonblock).map_err(|(_, error)| {
        anyhow::anyhow!("usage relay v2 owner lock is busy or unavailable: {error}")
    })?;

    let locked_stat = fstat(&*lock)?;
    validate_regular_stat(
        &locked_stat,
        agent_uid,
        Some((expected_gid, OWNER_LOCK_MODE, 1)),
        Some(0),
        "usage relay v2 owner lock",
    )?;
    ensure_no_acl_fd(&lock).context("rechecking usage relay v2 owner lock ACL")?;
    ensure_same_named_object(
        &jackin,
        OWNER_DIRECTORY_NAME,
        &leaf_stat,
        "rechecking usage relay v2 directory identity",
    )?;
    ensure_same_named_object(
        &leaf,
        OWNER_LOCK_NAME,
        &locked_stat,
        "rechecking usage relay v2 owner lock identity",
    )?;
    ensure_same_named_object(
        &root,
        root_name,
        &jackin_stat,
        "rechecking usage relay parent identity",
    )?;
    let current_jackin_stat = fstat(&jackin)?;
    validate_directory_stat(
        &current_jackin_stat,
        root_owner_uid,
        None,
        "usage relay parent",
    )?;
    ensure_no_acl_fd(&jackin).context("rechecking /jackin ACL")?;
    ensure_same_identity(
        &FsIdentity::from_stat(&jackin_stat),
        &FsIdentity::from_stat(&current_jackin_stat),
        "rechecking usage relay parent identity",
    )?;
    let current_leaf_stat = fstat(&leaf)?;
    validate_directory_stat(
        &current_leaf_stat,
        agent_uid,
        Some((expected_gid, OWNER_DIRECTORY_MODE)),
        "usage relay v2 directory",
    )?;
    ensure_no_acl_fd(&leaf).context("rechecking usage relay v2 directory ACL")?;
    ensure_same_identity(
        &FsIdentity::from_stat(&leaf_stat),
        &FsIdentity::from_stat(&current_leaf_stat),
        "rechecking usage relay v2 directory identity",
    )?;

    Ok(V2SocketOwner {
        root,
        root_name: root_name.to_owned(),
        root_owner_uid,
        jackin,
        leaf,
        lock,
        lock_identity: FsIdentity::from_stat(&locked_stat),
        agent_uid,
        expected_gid,
        proxy_uid,
    })
}

fn bind_v2_socket_at(
    owner: V2SocketOwner,
    socket_path: &Path,
    socket_name: &CStr,
    proxy_uid: libc::uid_t,
    expected_gid: libc::gid_t,
) -> Result<(UnixListener, V2SocketCleanup)> {
    if proxy_uid != owner.proxy_uid || expected_gid != owner.expected_gid {
        bail!("usage relay v2 owner policy changed during setup");
    }
    let leaf_stat = fstat(&owner.leaf)?;
    let lock_stat = fstat(&*owner.lock)?;
    let jackin_stat = fstat(&owner.jackin)?;
    validate_directory_stat(
        &jackin_stat,
        owner.root_owner_uid,
        None,
        "usage relay parent",
    )?;
    ensure_no_acl_fd(&owner.jackin).context("rechecking usage relay parent ACL")?;
    ensure_same_named_object(
        &owner.root,
        &owner.root_name,
        &jackin_stat,
        "rechecking usage relay parent name",
    )?;
    validate_directory_stat(
        &leaf_stat,
        owner.agent_uid,
        Some((expected_gid, OWNER_DIRECTORY_MODE)),
        "usage relay v2 directory",
    )?;
    validate_regular_stat(
        &lock_stat,
        owner.agent_uid,
        Some((expected_gid, OWNER_LOCK_MODE, 1)),
        Some(0),
        "usage relay v2 owner lock",
    )?;
    validate_owner_policy(
        uid(&leaf_stat),
        gid(&leaf_stat),
        uid(&lock_stat),
        gid(&lock_stat),
        owner.agent_uid,
        expected_gid,
        proxy_uid,
    )?;
    ensure_no_acl_fd(&owner.leaf).context("rechecking usage relay v2 directory ACL")?;
    ensure_no_acl_fd(&owner.lock).context("rechecking usage relay v2 owner lock ACL")?;
    ensure_same_named_object(
        &owner.jackin,
        OWNER_DIRECTORY_NAME,
        &leaf_stat,
        "rechecking usage relay v2 directory name",
    )?;
    ensure_same_named_object(
        &owner.leaf,
        OWNER_LOCK_NAME,
        &lock_stat,
        "rechecking usage relay v2 owner lock name",
    )?;
    if let Some(stale) = stat_at_nofollow(&owner.leaf, socket_name)? {
        validate_socket_stat(
            &stale,
            proxy_uid,
            expected_gid,
            "stale usage relay v2 socket",
        )?;
        ensure_no_acl_path(socket_path).context("checking stale usage relay v2 socket ACL")?;
        ensure_same_named_object(
            &owner.leaf,
            socket_name,
            &stale,
            "rechecking stale usage relay v2 socket",
        )?;
        unlinkat(&owner.leaf, socket_name, UnlinkatFlags::NoRemoveDir)
            .context("removing eligible stale usage relay v2 socket")?;
    }

    let listener = UnixListener::bind(socket_path).with_context(|| {
        format!(
            "binding owned usage relay v2 socket at {}",
            socket_path.display()
        )
    })?;
    let bound_stat = stat_at_nofollow(&owner.leaf, socket_name)?
        .context("bound usage relay v2 socket disappeared")?;
    let mut cleanup = V2SocketCleanup {
        owner,
        socket_name: socket_name.to_owned(),
        identity: FsIdentity::from_stat(&bound_stat),
    };

    fchmodat(
        &cleanup.owner.leaf,
        socket_name,
        Mode::from_bits_truncate(OWNER_SOCKET_MODE),
        FchmodatFlags::NoFollowSymlink,
    )
    .context("setting usage relay v2 socket mode")?;
    let bound_stat = stat_at_nofollow(&cleanup.owner.leaf, socket_name)?
        .context("usage relay v2 socket disappeared after mode setup")?;
    validate_socket_stat(
        &bound_stat,
        proxy_uid,
        expected_gid,
        "bound usage relay v2 socket",
    )?;
    ensure_same_identity(
        &cleanup.identity,
        &FsIdentity::from_stat(&bound_stat),
        "usage relay v2 socket changed during setup",
    )?;
    ensure_no_acl_path(socket_path).context("checking bound usage relay v2 socket ACL")?;
    cleanup.identity = FsIdentity::from_stat(&bound_stat);
    Ok((listener, cleanup))
}

async fn write_ready_frame<W>(output: &mut W) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let frame = UsageRelayReadyV1::canonical()
        .encode_frame()
        .map_err(anyhow::Error::msg)?;
    output.write_all(&frame).await?;
    output.flush().await?;
    Ok(())
}

struct V2SocketOwner {
    root: File,
    root_name: CString,
    root_owner_uid: libc::uid_t,
    jackin: File,
    leaf: File,
    lock: Flock<File>,
    lock_identity: FsIdentity,
    agent_uid: libc::uid_t,
    expected_gid: libc::gid_t,
    proxy_uid: libc::uid_t,
}

struct V2SocketCleanup {
    owner: V2SocketOwner,
    socket_name: CString,
    identity: FsIdentity,
}

impl Drop for V2SocketCleanup {
    fn drop(&mut self) {
        let Ok(Some(lock_stat)) = stat_at_nofollow(&self.owner.leaf, OWNER_LOCK_NAME) else {
            return;
        };
        if !self.owner.lock_identity.matches_stat(&lock_stat) {
            return;
        }
        let Ok(Some(socket_stat)) = stat_at_nofollow(&self.owner.leaf, &self.socket_name) else {
            return;
        };
        if self.identity.matches_stat(&socket_stat) {
            let _unlink_result = unlinkat(
                &self.owner.leaf,
                self.socket_name.as_c_str(),
                UnlinkatFlags::NoRemoveDir,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FsIdentity {
    device: libc::dev_t,
    inode: libc::ino_t,
    file_type: libc::mode_t,
    owner: libc::uid_t,
    group: libc::gid_t,
    links: libc::nlink_t,
}

impl FsIdentity {
    fn from_stat(stat: &libc::stat) -> Self {
        Self {
            device: stat.st_dev,
            inode: stat.st_ino,
            file_type: stat.st_mode & libc::S_IFMT,
            owner: uid(stat),
            group: gid(stat),
            links: stat.st_nlink,
        }
    }

    fn matches_stat(self, stat: &libc::stat) -> bool {
        self == Self::from_stat(stat)
    }
}

fn open_directory_at(parent: &File, name: &CStr) -> Result<File> {
    let fd = openat(parent, name, SECURE_DIRECTORY_OPEN_FLAGS, Mode::empty())
        .with_context(|| format!("opening directory {name:?} without following links"))?;
    Ok(File::from(fd))
}

fn open_file_at(parent: &File, name: &CStr) -> Result<File> {
    let fd = openat(parent, name, SECURE_FILE_OPEN_FLAGS, Mode::empty())
        .with_context(|| format!("opening file {name:?} without following links"))?;
    Ok(File::from(fd))
}

fn open_owner_lock_at(parent: &File, name: &CStr) -> Result<File> {
    let fd = openat(parent, name, OWNER_LOCK_OPEN_FLAGS, Mode::empty())
        .with_context(|| format!("opening owner lock {name:?} without following links"))?;
    Ok(File::from(fd))
}

fn stat_at_nofollow(parent: &File, name: &CStr) -> Result<Option<libc::stat>> {
    match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(Some(stat)),
        Err(Errno::ENOENT) => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("inspecting {name:?} without following links"))
        }
    }
}

fn ensure_same_named_object(
    parent: &File,
    name: &CStr,
    expected: &libc::stat,
    context: &str,
) -> Result<()> {
    let actual = stat_at_nofollow(parent, name)?.context(context.to_owned())?;
    ensure_same_identity(
        &FsIdentity::from_stat(expected),
        &FsIdentity::from_stat(&actual),
        context,
    )
}

fn ensure_same_identity(expected: &FsIdentity, actual: &FsIdentity, context: &str) -> Result<()> {
    if expected != actual {
        bail!("{context}: filesystem object identity changed");
    }
    Ok(())
}

fn validate_directory_stat(
    stat: &libc::stat,
    expected_uid: libc::uid_t,
    exact: Option<(libc::gid_t, libc::mode_t)>,
    label: &str,
) -> Result<()> {
    if stat.st_mode & libc::S_IFMT != libc::S_IFDIR
        || uid(stat) != expected_uid
        || mode_bits(stat) & 0o022 != 0
        || stat.st_nlink < 2
    {
        bail!("{label} has an unsafe owner, type, mode, or link count");
    }
    if let Some((expected_gid, expected_mode)) = exact
        && (gid(stat) != expected_gid || mode_bits(stat) != expected_mode)
    {
        bail!("{label} does not match the preprovisioned owner policy");
    }
    Ok(())
}

fn validate_regular_stat(
    stat: &libc::stat,
    expected_uid: libc::uid_t,
    exact: Option<(libc::gid_t, libc::mode_t, libc::nlink_t)>,
    expected_size: Option<i64>,
    label: &str,
) -> Result<()> {
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG
        || uid(stat) != expected_uid
        || stat.st_nlink != 1
        || expected_size.is_some_and(|size| stat.st_size != size)
    {
        bail!("{label} has an unsafe owner, type, size, or link count");
    }
    if let Some((expected_gid, expected_mode, expected_links)) = exact
        && (gid(stat) != expected_gid
            || mode_bits(stat) != expected_mode
            || stat.st_nlink != expected_links)
    {
        bail!("{label} does not match the preprovisioned owner policy");
    }
    Ok(())
}

fn validate_socket_stat(
    stat: &libc::stat,
    expected_uid: libc::uid_t,
    expected_gid: libc::gid_t,
    label: &str,
) -> Result<()> {
    if stat.st_mode & libc::S_IFMT != libc::S_IFSOCK
        || uid(stat) != expected_uid
        || gid(stat) != expected_gid
        || mode_bits(stat) != OWNER_SOCKET_MODE
        || stat.st_nlink != 1
    {
        bail!("{label} has an unsafe owner, type, mode, or link count");
    }
    Ok(())
}

fn validate_owner_policy(
    leaf_uid: libc::uid_t,
    leaf_gid: libc::gid_t,
    lock_uid: libc::uid_t,
    lock_gid: libc::gid_t,
    agent_uid: libc::uid_t,
    expected_gid: libc::gid_t,
    proxy_uid: libc::uid_t,
) -> Result<()> {
    if leaf_uid != agent_uid
        || lock_uid != agent_uid
        || leaf_gid != expected_gid
        || lock_gid != expected_gid
        || (proxy_uid != agent_uid && proxy_uid != ROOT_UID)
    {
        bail!("usage relay v2 owner identity does not match the image policy");
    }
    Ok(())
}

fn uid(stat: &libc::stat) -> libc::uid_t {
    stat.st_uid
}

fn gid(stat: &libc::stat) -> libc::gid_t {
    stat.st_gid
}

fn mode_bits(stat: &libc::stat) -> libc::mode_t {
    stat.st_mode & 0o7777
}

#[cfg(target_os = "linux")]
#[expect(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    reason = "inspect Linux POSIX ACL xattrs on validated descriptors: `file` remains live and its descriptor is borrowed for this call; `name` is NUL-terminated, and a null value pointer is valid with size zero"
)]
fn ensure_no_acl_fd(file: &File) -> Result<()> {
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        let result =
            unsafe { libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
        classify_linux_acl_probe(result, name)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[expect(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    reason = "inspect Linux POSIX ACL xattrs without following the socket path: `path` and `name` are live NUL-terminated strings for this call and a null value pointer is valid with size zero"
)]
fn ensure_no_acl_path(path: &Path) -> Result<()> {
    let path = CString::new(path.as_os_str().as_bytes())?;
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        let result =
            unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
        classify_linux_acl_probe(result, name)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn classify_linux_acl_probe(result: libc::ssize_t, name: &CStr) -> Result<()> {
    if result >= 0 {
        bail!(
            "extended ACL attribute {} is present",
            name.to_string_lossy()
        );
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENODATA) => Ok(()),
        Some(libc::ERANGE) => bail!(
            "extended ACL attribute {} is present",
            name.to_string_lossy()
        ),
        _ => Err(error)
            .with_context(|| format!("inspecting ACL attribute {}", name.to_string_lossy())),
    }
}

#[cfg(target_os = "macos")]
const ACL_TYPE_EXTENDED: libc::c_int = 0x100;
#[cfg(target_os = "macos")]
const ACL_FIRST_ENTRY: libc::c_int = 0;

#[cfg(target_os = "macos")]
#[expect(
    unsafe_code,
    reason = "audited opaque Darwin ACL inspection for owned relay paths"
)]
#[link(name = "System")]
unsafe extern "C" {
    fn acl_get_fd_np(fd: libc::c_int, acl_type: libc::c_int) -> *mut libc::c_void;
    fn acl_get_link_np(path: *const libc::c_char, acl_type: libc::c_int) -> *mut libc::c_void;
    fn acl_get_entry(
        acl: *mut libc::c_void,
        entry_id: libc::c_int,
        entry: *mut *mut libc::c_void,
    ) -> libc::c_int;
    fn acl_valid(acl: *mut libc::c_void) -> libc::c_int;
    fn acl_free(object: *mut libc::c_void) -> libc::c_int;
}

#[cfg(target_os = "macos")]
#[expect(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    reason = "inspect Darwin ACLs through the validated descriptor ABI: `file` remains live for this call, the descriptor is borrowed, and the returned pointer is treated only as an opaque owned ACL allocation"
)]
fn ensure_no_acl_fd(file: &File) -> Result<()> {
    inspect_darwin_acl(
        unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) },
        "descriptor",
    )
}

#[cfg(target_os = "macos")]
#[expect(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    reason = "inspect Darwin ACL on a no-follow stat-validated socket path: `path` is NUL-terminated for this call and the named filesystem object is inspected without following a replaced symlink"
)]
fn ensure_no_acl_path(path: &Path) -> Result<()> {
    let path = CString::new(path.as_os_str().as_bytes())?;
    inspect_darwin_acl(
        unsafe { acl_get_link_np(path.as_ptr(), ACL_TYPE_EXTENDED) },
        "path",
    )
}

#[cfg(target_os = "macos")]
struct OwnedDarwinAcl(std::ptr::NonNull<libc::c_void>);

#[cfg(target_os = "macos")]
impl Drop for OwnedDarwinAcl {
    #[expect(
        unsafe_code,
        clippy::undocumented_unsafe_blocks,
        reason = "free the opaque result returned by Darwin ACL inspection: this unique owner is created only from a successful non-null ACL result and releases that allocation exactly once"
    )]
    fn drop(&mut self) {
        let _ = unsafe { acl_free(self.0.as_ptr()) };
    }
}

#[cfg(target_os = "macos")]
#[expect(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    reason = "inspect the validated opaque Darwin ACL without dereferencing it: `acl` owns a live opaque allocation returned by the Darwin getter and `entry` is initialized output storage borrowed from `acl`"
)]
fn inspect_darwin_acl(acl: *mut libc::c_void, label: &str) -> Result<()> {
    let Some(acl) = std::ptr::NonNull::new(acl) else {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(());
        }
        return Err(error).with_context(|| format!("inspecting extended ACL on {label}"));
    };
    let acl = OwnedDarwinAcl(acl);
    if unsafe { acl_valid(acl.0.as_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("validating extended ACL on {label}"));
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(acl.0.as_ptr(), ACL_FIRST_ENTRY, &raw mut entry) };
    let errno = (result == -1)
        .then(|| std::io::Error::last_os_error().raw_os_error())
        .flatten();
    classify_darwin_acl_entry(result, errno, !entry.is_null(), label)
}

#[cfg(target_os = "macos")]
fn classify_darwin_acl_entry(
    result: libc::c_int,
    errno: Option<i32>,
    entry_present: bool,
    label: &str,
) -> Result<()> {
    match (result, entry_present, errno) {
        (0, true, _) => bail!("extended ACL is present on {label}"),
        (-1, false, Some(error)) if error == libc::EINVAL => Ok(()),
        (0, false, _) => bail!("Darwin ACL inspection returned an empty successful entry"),
        (-1, _, Some(error)) => Err(std::io::Error::from_raw_os_error(error))
            .with_context(|| format!("inspecting extended ACL entries on {label}")),
        (-1, _, None) => bail!("Darwin ACL inspection failed without an error code on {label}"),
        _ => bail!("Darwin ACL inspection returned an invalid result on {label}"),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn ensure_no_acl_fd(_file: &File) -> Result<()> {
    bail!("usage relay v2 ACL inspection is unsupported on this platform")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn ensure_no_acl_path(_path: &Path) -> Result<()> {
    bail!("usage relay v2 ACL inspection is unsupported on this platform")
}

async fn run_at<R, W>(
    socket_path: &Path,
    supervisor: SupervisorIdentity,
    authorization: UsageRelayAuthorization,
    input: R,
    output: W,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    run_at_with_peer(
        socket_path,
        supervisor,
        authorization,
        None,
        Arc::new(SupervisorIdentity::is_current),
        input,
        output,
    )
    .await
}

async fn run_at_with_peer<R, W>(
    socket_path: &Path,
    supervisor: SupervisorIdentity,
    authorization: UsageRelayAuthorization,
    forced_peer: Option<PeerIdentity>,
    supervisor_liveness: SupervisorLiveness,
    input: R,
    output: W,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    drop(std::fs::remove_file(socket_path));
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "creating scoped usage socket directory {}",
                parent.display()
            )
        })?;
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("binding scoped usage socket at {}", socket_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    }
    let cleanup = SocketCleanup(socket_path.to_path_buf());
    run_on_listener(
        listener,
        cleanup,
        SupervisorCheck {
            identity: supervisor,
            is_current: supervisor_liveness,
        },
        authorization,
        forced_peer,
        input,
        output,
    )
    .await
}

async fn run_on_listener<R, W, C>(
    listener: UnixListener,
    _cleanup: C,
    supervisor_check: SupervisorCheck,
    authorization: UsageRelayAuthorization,
    forced_peer: Option<PeerIdentity>,
    input: R,
    output: W,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let authorization = Arc::new(authorization);
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let request_ids = Arc::new(AtomicU64::new(1));
    let (requests, mut request_rx) = mpsc::channel::<UsageRelayTunnelRequest>(TUNNEL_CAPACITY);

    let mut writer = jackin_telemetry::spawn::spawn_stream("usage_relay.writer", async move {
        let mut output = output;
        while let Some(request) = request_rx.recv().await {
            write_frame(&mut output, &request).await?;
        }
        Ok::<(), anyhow::Error>(())
    });
    let response_pending = Arc::clone(&pending);
    let mut reader = jackin_telemetry::spawn::spawn_stream("usage_relay.reader", async move {
        let mut input = BufReader::new(input);
        loop {
            let response = read_frame::<_, UsageRelayTunnelResponse>(&mut input).await?;
            if let Some(waiter) = response_pending.lock().await.remove(&response.request_id) {
                drop(waiter.send(response.response));
            }
        }
    });

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let requests = requests.clone();
                let pending = Arc::clone(&pending);
                let authorization = Arc::clone(&authorization);
                let supervisor_check = supervisor_check.clone();
                let peer = forced_peer.or_else(|| peer_identity(&stream));
                let request_id = request_ids.fetch_add(1, Ordering::Relaxed);
                drop(jackin_telemetry::spawn::spawn_stream(
                    "usage_relay.local_request",
                    handle_local(
                        stream,
                        request_id,
                        requests,
                        pending,
                        authorization,
                        peer,
                        supervisor_check,
                    ),
                ));
            }
            result = &mut reader => {
                fail_pending(&pending).await;
                return result.context("usage relay response task panicked")?;
            }
            result = &mut writer => {
                fail_pending(&pending).await;
                return result.context("usage relay request task panicked")?;
            }
        }
    }
}

async fn handle_local<S>(
    mut stream: S,
    request_id: u64,
    requests: mpsc::Sender<UsageRelayTunnelRequest>,
    pending: Pending,
    authorization: Arc<UsageRelayAuthorization>,
    peer: Option<PeerIdentity>,
    supervisor_check: SupervisorCheck,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let request = {
        let mut reader = BufReader::new(&mut stream);
        read_frame::<_, UsageBrokerRequest>(&mut reader).await
    };
    let supervisor_ok = supervisor_peer_allows(supervisor_check.identity, peer);
    let supervisor_is_current = if request.is_ok() {
        (supervisor_check.is_current)(supervisor_check.identity)
    } else {
        false
    };
    let response = match request {
        Ok(_) if !supervisor_is_current => supervisor_unauthorized_response(),
        Ok(_) if !supervisor_ok => supervisor_unauthorized_response(),
        Ok(request)
            if authorization.authorizes(supervisor_check.identity, peer, &request.operation) =>
        {
            let (response_tx, response_rx) = oneshot::channel();
            pending.lock().await.insert(request_id, response_tx);
            let tunneled = UsageRelayTunnelRequest {
                request_id,
                request,
            };
            if requests.send(tunneled).await.is_err() {
                pending.lock().await.remove(&request_id);
                unavailable_response()
            } else if let Ok(Ok(response)) =
                tokio::time::timeout(RESPONSE_TIMEOUT, response_rx).await
            {
                response
            } else {
                pending.lock().await.remove(&request_id);
                unavailable_response()
            }
        }
        Ok(_) => capability_unauthorized_response(),
        Err(_) => protocol_response(),
    };
    drop(write_frame(&mut stream, &response).await);
}

fn capability_unauthorized_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "usage account capability is not authorized".to_owned(),
        },
    }
}

async fn fail_pending(pending: &Pending) {
    let waiters = std::mem::take(&mut *pending.lock().await);
    for (_, waiter) in waiters {
        drop(waiter.send(unavailable_response()));
    }
}

async fn read_frame<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncBufRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut bytes = Vec::new();
    let read = reader
        .take(u64::try_from(USAGE_BROKER_MAX_FRAME_BYTES).unwrap_or(u64::MAX) + 1)
        .read_until(b'\n', &mut bytes)
        .await?;
    if read == 0 || read > USAGE_BROKER_MAX_FRAME_BYTES || bytes.last() != Some(&b'\n') {
        bail!("usage relay frame is invalid");
    }
    bytes.pop();
    serde_json::from_slice(&bytes).context("decoding usage relay frame")
}

async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
        bail!("usage relay frame is too large");
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

fn unavailable_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: "usage broker is unavailable".to_owned(),
        },
    }
}

fn protocol_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::ProtocolMismatch,
            message: "usage relay protocol mismatch".to_owned(),
        },
    }
}

fn supervisor_unauthorized_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "usage relay peer is not the Capsule supervisor".to_owned(),
        },
    }
}

struct SocketCleanup(std::path::PathBuf);

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.0));
    }
}

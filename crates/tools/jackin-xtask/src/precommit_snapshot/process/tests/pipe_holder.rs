use std::{
    fs,
    io::Read,
    mem::MaybeUninit,
    os::{
        fd::{AsFd, OwnedFd},
        unix::{
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, OnceLock, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use rustix::{
    cmsg_space,
    io::{IoSlice, IoSliceMut},
    net::{
        RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer,
        SendAncillaryMessage, SendFlags, recvmsg, sendmsg,
    },
};

use super::super::{
    CaptureFailureKind, CommandBudget, EnvironmentMode, SetupControls, capture_strict,
};

const PIPE_HOLDER_SOCKET_PATH: &str = "JACKIN_STRICT_PIPE_HOLDER_SOCKET_PATH";
const PIPE_HOLDER_LIVE_MODE: &str = "JACKIN_STRICT_PIPE_HOLDER_LIVE_MODE";
const PIPE_HOLDER_TEST_NAME: &str = "precommit_snapshot::process::tests::pipe_holder::strict_capture_rejects_surviving_pipe_holder_after_zero_exit";

struct PipeHolder {
    child: Option<Child>,
}

impl PipeHolder {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child
            .as_mut()
            .expect("holder child remains owned")
            .try_wait()
    }

    fn stop_and_wait(&mut self) -> std::io::Result<()> {
        if let Some(mut child) = self.child.take() {
            drop(child.kill());
            child.wait()?;
        }
        Ok(())
    }
}

impl Drop for PipeHolder {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.kill());
            drop(child.wait());
        }
    }
}

fn send_pipe_descriptors(socket_path: &std::ffi::OsStr) -> Result<UnixStream> {
    let socket =
        UnixStream::connect(socket_path).context("connecting pipe-holder descriptor socket")?;
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let descriptors = [stdout.as_fd(), stderr.as_fd()];
    let mut space = [MaybeUninit::uninit(); cmsg_space!(ScmRights(2))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    ensure!(
        control.push(SendAncillaryMessage::ScmRights(&descriptors)),
        "building pipe-holder descriptor message"
    );
    sendmsg(
        &socket,
        &[IoSlice::new(b"pipe descriptors")],
        &mut control,
        SendFlags::empty(),
    )
    .context("sending captured output descriptors")?;
    Ok(socket)
}

fn receive_pipe_descriptors(socket: &UnixStream, deadline: Instant) -> Result<[OwnedFd; 2]> {
    let mut payload = [0_u8; 32];
    let mut buffers = [IoSliceMut::new(&mut payload)];
    let mut space = [MaybeUninit::uninit(); cmsg_space!(ScmRights(2))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let received = loop {
        match recvmsg(socket, &mut buffers, &mut control, RecvFlags::DONTWAIT) {
            Ok(received) => break received,
            Err(error) if error == rustix::io::Errno::AGAIN => {
                ensure!(Instant::now() < deadline, "descriptor receive timed out");
                thread::park_timeout(Duration::from_millis(10));
            }
            Err(error) => return Err(error).context("receiving captured output descriptors"),
        }
    };
    ensure!(
        received.bytes > 0,
        "pipe-holder sender sent no descriptor payload"
    );
    let mut descriptors = control
        .drain()
        .filter_map(|message| match message {
            RecvAncillaryMessage::ScmRights(fds) => Some(fds.collect::<Vec<_>>()),
            _ => None,
        })
        .flatten();
    let stdout = descriptors
        .next()
        .context("missing captured stdout descriptor")?;
    let stderr = descriptors
        .next()
        .context("missing captured stderr descriptor")?;
    ensure!(
        descriptors.next().is_none(),
        "too many captured output descriptors"
    );
    Ok([stdout, stderr])
}

fn wait_for_sender_exit(socket: &mut UnixStream, deadline: Instant) -> Result<()> {
    let mut bytes = [0_u8; 256];
    loop {
        match socket.read(&mut bytes) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(Instant::now() < deadline, "descriptor sender did not exit");
                thread::park_timeout(Duration::from_millis(10));
            }
            Err(error) => return Err(error).context("waiting for descriptor sender exit"),
        }
    }
}

fn receive_pipe_holder(
    listener: UnixListener,
    tick_path: std::path::PathBuf,
    cancellation: Option<Arc<AtomicBool>>,
    holder_sender: std::sync::mpsc::Sender<PipeHolder>,
) -> Result<()> {
    listener
        .set_nonblocking(true)
        .context("setting descriptor listener nonblocking")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let (socket, _) = loop {
        match listener.accept() {
            Ok(connection) => break connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(
                    Instant::now() < deadline,
                    "pipe-holder sender did not connect"
                );
                thread::park_timeout(Duration::from_millis(10));
            }
            Err(error) => return Err(error).context("accepting pipe-holder descriptor socket"),
        }
    };
    socket
        .set_nonblocking(true)
        .context("setting descriptor socket nonblocking")?;
    let sender_deadline = Instant::now() + Duration::from_secs(30);
    let [stdout, stderr] = receive_pipe_descriptors(&socket, sender_deadline)?;
    let mut holder =
        Command::new(std::env::current_exe().context("resolving pipe-holder test executable")?);
    holder
        .args(["--exact", PIPE_HOLDER_TEST_NAME, "--nocapture"])
        .env(PIPE_HOLDER_LIVE_MODE, tick_path)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .process_group(0);
    let holder = PipeHolder {
        child: Some(holder.spawn().context("spawning outer-owned pipe holder")?),
    };
    holder_sender
        .send(holder)
        .map_err(|_| anyhow::anyhow!("outer test dropped pipe-holder ownership"))?;
    let mut socket = socket;
    wait_for_sender_exit(&mut socket, sender_deadline)
        .context("waiting for zero-exit pipe-holder sender")?;
    if let Some(cancellation) = cancellation {
        cancellation.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    Ok(())
}

struct PipeHolderCapture {
    error: anyhow::Error,
    holder: PipeHolder,
    ticks: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

fn capture_with_pipe_holder(cancel_after_target_exit: bool) -> PipeHolderCapture {
    let directory = tempfile::tempdir().expect("creating capture fixture");
    let socket_path = directory.path().join("pipe-holder.sock");
    let listener = UnixListener::bind(&socket_path).expect("binding descriptor socket");
    let tick_path = directory.path().join("pipe-holder.ticks");
    let cancellation = Arc::new(AtomicBool::new(false));
    let receiver_cancellation = cancel_after_target_exit.then(|| Arc::clone(&cancellation));
    let (holder_sender, holder_receiver) = std::sync::mpsc::channel();
    let receiver = thread::spawn(move || {
        receive_pipe_holder(
            listener,
            tick_path.clone(),
            receiver_cancellation,
            holder_sender,
        )
    });
    let mut child = Command::new(std::env::current_exe().expect("test executable"));
    child
        .args(["--exact", PIPE_HOLDER_TEST_NAME, "--nocapture"])
        .env(PIPE_HOLDER_SOCKET_PATH, &socket_path);
    let controls = if cancel_after_target_exit {
        SetupControls::new(Arc::clone(&cancellation), Duration::from_secs(10))
    } else {
        SetupControls::for_test(Duration::from_millis(750))
    };
    let result = capture_strict(
        &mut child,
        &controls,
        EnvironmentMode::Inherit,
        1024 * 1024,
        1024 * 1024,
        CommandBudget::PhaseRemainder,
    );
    receiver
        .join()
        .expect("pipe-holder receiver thread joined")
        .expect("outer receiver owns a live pipe holder");
    let holder = holder_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("outer test received holder child for cleanup");
    let error =
        result.expect_err("captured target exits zero while external holder keeps pipes open");
    PipeHolderCapture {
        error,
        holder,
        ticks: directory.path().join("pipe-holder.ticks"),
        _directory: directory,
    }
}

fn run_pipe_holder_test_helper() -> bool {
    if let Some(tick_path) = std::env::var_os(PIPE_HOLDER_LIVE_MODE) {
        let mut count = 0_u64;
        let tick_path = Path::new(&tick_path);
        let tick_temp_path = tick_path.with_extension("tmp");
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            fs::write(&tick_temp_path, count.to_string())
                .and_then(|()| fs::rename(&tick_temp_path, tick_path))
                .expect("atomically publish external-holder heartbeat");
            count += 1;
            thread::park_timeout(Duration::from_millis(10));
        }
        return true;
    }
    if let Some(socket_path) = std::env::var_os(PIPE_HOLDER_SOCKET_PATH) {
        static SENDER_SOCKET: OnceLock<UnixStream> = OnceLock::new();
        SENDER_SOCKET.get_or_init(|| {
            send_pipe_descriptors(&socket_path).expect("send actual captured output descriptors")
        });
        return true;
    }
    false
}

fn assert_pipe_holder_remains_live(capture: &mut PipeHolderCapture) {
    assert!(
        capture
            .holder
            .try_wait()
            .expect("poll outer-owned holder")
            .is_none()
    );
    let before = fs::read_to_string(&capture.ticks)
        .expect("holder tick file")
        .parse::<u64>()
        .expect("numeric holder tick");
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        thread::park_timeout(Duration::from_millis(20));
        let after = fs::read_to_string(&capture.ticks)
            .expect("holder remains active")
            .parse::<u64>()
            .expect("numeric holder tick");
        if after > before {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "external pipe holder remains live"
        );
    }
}

#[test]
fn strict_capture_rejects_surviving_pipe_holder_after_zero_exit() {
    if run_pipe_holder_test_helper() {
        return;
    }
    let mut capture = capture_with_pipe_holder(false);
    let failure = capture
        .error
        .downcast_ref::<super::super::CaptureFailure>()
        .expect("incomplete strict capture remains typed");
    assert_eq!(failure.kind, CaptureFailureKind::Capture);
    let detail = capture.error.to_string();
    assert!(detail.contains("code=Some(0)"), "{detail}");
    assert!(detail.contains("capture_complete=false"), "{detail}");
    assert!(detail.contains("group_drained=true"), "{detail}");
    assert!(detail.contains("timed_out=false"), "{detail}");
    assert!(detail.contains("cancelled=false"), "{detail}");
    assert!(
        detail.contains("stdout_eof=false") || detail.contains("stderr_eof=false"),
        "{detail}"
    );
    assert_pipe_holder_remains_live(&mut capture);
    let diagnostics = detail
        .split_once("diagnostic spools retained at ")
        .expect("failure retains diagnostic spool")
        .1
        .trim();
    let diagnostics = Path::new(diagnostics);
    assert!(
        diagnostics.starts_with(std::env::temp_dir()),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics.file_name().is_some_and(|name| name
            .as_encoded_bytes()
            .starts_with(b"jackin-native-capture-")),
        "{diagnostics:?}"
    );
    capture
        .holder
        .stop_and_wait()
        .expect("stop and reap external holder");
    fs::remove_dir_all(diagnostics).expect("remove retained incomplete-capture diagnostics");
}

#[test]
fn strict_capture_does_not_treat_incomplete_pipes_as_clean_cancellation() {
    if run_pipe_holder_test_helper() {
        return;
    }
    let mut capture = capture_with_pipe_holder(true);
    let failure = capture
        .error
        .downcast_ref::<super::super::CaptureFailure>()
        .expect("incomplete cancelled capture remains typed");
    assert_eq!(failure.kind, CaptureFailureKind::Capture);
    assert!(!super::super::is_clean_cancellation(&capture.error));
    let detail = capture.error.to_string();
    assert!(detail.contains("code=Some(0)"), "{detail}");
    assert!(detail.contains("capture_complete=false"), "{detail}");
    assert!(detail.contains("cancelled=true"), "{detail}");
    assert!(detail.contains("group_drained=true"), "{detail}");
    assert!(
        detail.contains("stdout_eof=false") || detail.contains("stderr_eof=false"),
        "{detail}"
    );
    assert!(detail.contains("did not reach EOF"), "{detail}");
    assert_pipe_holder_remains_live(&mut capture);
    let diagnostics = detail
        .split_once("diagnostic spools retained at ")
        .expect("failure retains diagnostic spool")
        .1
        .trim();
    let diagnostics = Path::new(diagnostics);
    assert!(
        diagnostics.starts_with(std::env::temp_dir()),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics.file_name().is_some_and(|name| name
            .as_encoded_bytes()
            .starts_with(b"jackin-native-capture-")),
        "{diagnostics:?}"
    );
    capture
        .holder
        .stop_and_wait()
        .expect("stop and reap external holder");
    fs::remove_dir_all(diagnostics).expect("removing retained incomplete-capture diagnostics");
}

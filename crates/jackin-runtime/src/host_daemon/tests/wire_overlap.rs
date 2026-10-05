use super::*;
use std::time::{Duration, Instant};

const ENABLE: &str = "JACKIN_DAEMON_WIRE_OVERLAP";
pub(super) const ENDPOINT: &str = "JACKIN_DAEMON_WIRE_OVERLAP_ENDPOINT";
const WIRE_TEST: &str =
    "host_daemon::tests::conformance_wire_real_daemon_socket_exports_bounded_parented_rpc";
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(super) struct ParentOverlap {
    _directory: tempfile::TempDir,
    endpoint: PathBuf,
    worker: Option<std::thread::JoinHandle<Result<()>>>,
}

impl ParentOverlap {
    pub(super) fn start_if_requested() -> Result<Option<Self>> {
        if std::env::var_os(ENABLE).is_none() || std::env::var_os(ENDPOINT).is_some() {
            return Ok(None);
        }
        // Keep the socket path short enough for macOS sockaddr_un.
        let directory = tempfile::Builder::new()
            .prefix("jk-wire-")
            .tempdir_in("/tmp")?;
        let endpoint = directory.path().join("overlap.sock");
        let listener = UnixListener::bind(&endpoint)?;
        listener.set_nonblocking(true)?;
        let worker = std::thread::spawn(move || {
            let deadline = Instant::now() + TIMEOUT;
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        anyhow::ensure!(Instant::now() < deadline, "collector never became ready");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            stream.set_read_timeout(Some(TIMEOUT))?;
            stream.set_write_timeout(Some(TIMEOUT))?;
            let mut ready = [0];
            stream.read_exact(&mut ready)?;
            anyhow::ensure!(ready == [1], "invalid collector readiness signal");

            let (_temp, _paths, layout) = layout();
            let mut attention = AttentionAdapter::new(RecordingNotifier::default());
            let request = DaemonRequest {
                id: "controlled-overlap".to_owned(),
                protocol_version: DAEMON_PROTOCOL_VERSION,
                build_id: "overlap-private-build".to_owned(),
                ctx: TelemetryContext {
                    traceparent: Some(
                        "00-5bf92f3577b34da6a3ce929d0e0e4736-10f067aa0ba902b7-01".to_owned(),
                    ),
                    ..TelemetryContext::v1()
                },
                kind: DaemonRequestKind::Status,
            };
            // This thread uses its process's default subscriber. ACK follows
            // the real request handler's synchronous operation completion.
            let response = handle_request_line(
                &serde_json::to_string(&request)?,
                &layout,
                "overlap-private-build",
                &CoredumpPolicy::Disabled,
                &mut attention,
            );
            anyhow::ensure!(
                matches!(response.kind, DaemonResponseKind::Status(_)),
                "unrelated production Status request failed"
            );
            writeln!(
                std::io::stdout(),
                "wire_overlap_request_completed pid={}",
                std::process::id()
            )?;
            stream.write_all(&[1])?;
            Ok(())
        });
        Ok(Some(Self {
            _directory: directory,
            endpoint,
            worker: Some(worker),
        }))
    }

    pub(super) fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    pub(super) fn finish(mut self) -> Result<()> {
        self.worker
            .take()
            .context("overlap worker already joined")?
            .join()
            .map_err(|_| anyhow::anyhow!("overlap worker panicked"))?
    }
}

impl Drop for ParentOverlap {
    fn drop(&mut self) {
        // The worker's accept/read/write bounds also bound cleanup on errors.
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

pub(super) fn collector_ready(overlap: Option<&ParentOverlap>) -> Result<()> {
    let endpoint = overlap
        .map(|overlap| overlap.endpoint().to_path_buf())
        .or_else(|| std::env::var_os(ENDPOINT).map(PathBuf::from));
    let Some(endpoint) = endpoint else {
        return Ok(());
    };
    let mut stream = UnixStream::connect(endpoint)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    writeln!(
        std::io::stdout(),
        "wire_overlap_collector_ready pid={}",
        std::process::id()
    )?;
    stream.write_all(&[1])?;
    let mut ack = [0];
    stream.read_exact(&mut ack)?;
    anyhow::ensure!(ack == [1], "invalid production request acknowledgment");
    writeln!(
        std::io::stdout(),
        "wire_overlap_collector_ack pid={}",
        std::process::id()
    )?;
    Ok(())
}

fn receipt_pid(output: &str, prefix: &str) -> Result<u32> {
    let receipts = output
        .lines()
        .filter_map(|line| line.split_once(prefix).map(|(_, pid)| pid))
        .collect::<Vec<_>>();
    anyhow::ensure!(receipts.len() == 1, "expected one {prefix} receipt");
    Ok(receipts[0].parse()?)
}

pub(super) fn assert_isolated_collector() -> Result<()> {
    let output = std::process::Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg(WIRE_TEST)
        .arg("--nocapture")
        .env(ENABLE, "1")
        .env_remove(ENDPOINT)
        .env_remove("JACKIN_DAEMON_WIRE_TEST_CHILD")
        .output()?;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    let stdout = String::from_utf8(output.stdout)?;
    anyhow::ensure!(
        stdout.contains("running 1 test")
            && stdout
                .lines()
                .any(|line| line.starts_with(&format!("test {WIRE_TEST} ..."))),
        "controlled wire test did not execute exactly the named scenario"
    );
    anyhow::ensure!(
        output.status.success(),
        "controlled daemon wire test failed"
    );
    let request_pid = receipt_pid(&stdout, "wire_overlap_request_completed pid=")?;
    let collector_pid = receipt_pid(&stdout, "wire_overlap_collector_ready pid=")?;
    let ack_pid = receipt_pid(&stdout, "wire_overlap_collector_ack pid=")?;
    anyhow::ensure!(
        collector_pid == ack_pid,
        "collector did not acknowledge the request"
    );
    anyhow::ensure!(
        request_pid != collector_pid,
        "unrelated request shares the collector's global subscriber process"
    );
    Ok(())
}

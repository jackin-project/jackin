//! PTY-backed host-console frame timing for scheduled health measurement.

use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Args;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Serialize;

const SCHEMA: u8 = 1;
const FRAME_MIN_BYTES: usize = 256;

#[derive(Args, Debug)]
pub(crate) struct FrameTimingArgs {
    /// Built jackin binary to execute.
    #[arg(long, default_value = "target/debug/jackin")]
    binary: PathBuf,
    /// JSON artifact path.
    #[arg(long, default_value = "target/frame-timing.json")]
    output: PathBuf,
    /// Independent PTY samples.
    #[arg(long, default_value_t = 3)]
    samples: usize,
    /// Per-frame timeout.
    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
}

#[derive(Debug, Serialize)]
struct FrameTimingArtifact {
    schema: u8,
    terminal: TerminalShape,
    samples: Vec<FrameSample>,
    first_frame_max_ms: u128,
    input_to_frame_max_ms: u128,
}

#[derive(Debug, Serialize)]
struct TerminalShape {
    cols: u16,
    rows: u16,
}

#[derive(Debug, Serialize)]
struct FrameSample {
    first_frame_ms: u128,
    input_to_frame_ms: u128,
    first_frame_bytes: usize,
    repaint_bytes: usize,
}

pub(crate) fn run(args: FrameTimingArgs) -> Result<()> {
    if args.samples == 0 {
        bail!("--samples must be at least 1");
    }
    let timeout = Duration::from_millis(args.timeout_ms);
    let root = crate::docs::repo_root()?;
    let binary = absolute_from(&root, &args.binary);
    if !binary.is_file() {
        bail!(
            "frame timing binary {} is missing; run `cargo build -p jackin --bin jackin` first",
            binary.display()
        );
    }
    let state_root = root.join("target/frame-timing-state");
    if state_root.exists() {
        fs::remove_dir_all(&state_root)
            .with_context(|| format!("removing {}", state_root.display()))?;
    }
    fs::create_dir_all(&state_root)
        .with_context(|| format!("creating {}", state_root.display()))?;

    let mut samples = Vec::with_capacity(args.samples);
    for index in 0..args.samples {
        samples.push(measure_sample(&binary, &state_root, index, timeout)?);
    }
    let artifact = FrameTimingArtifact {
        schema: SCHEMA,
        terminal: TerminalShape {
            cols: 120,
            rows: 36,
        },
        first_frame_max_ms: samples
            .iter()
            .map(|sample| sample.first_frame_ms)
            .max()
            .unwrap_or(0),
        input_to_frame_max_ms: samples
            .iter()
            .map(|sample| sample.input_to_frame_ms)
            .max()
            .unwrap_or(0),
        samples,
    };
    let output = absolute_from(&root, &args.output);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&artifact)?)
        .with_context(|| format!("writing {}", output.display()))?;
    writeln!(
        std::io::stdout().lock(),
        "frame timing OK - first-frame max={}ms, input-to-frame max={}ms, samples={}, artifact={}",
        artifact.first_frame_max_ms,
        artifact.input_to_frame_max_ms,
        artifact.samples.len(),
        output.display()
    )
    .context("writing frame timing summary")?;
    Ok(())
}

fn measure_sample(
    binary: &Path,
    state_root: &Path,
    index: usize,
    timeout: Duration,
) -> Result<FrameSample> {
    let sample_root = state_root.join(index.to_string());
    let config = sample_root.join("config");
    let home = sample_root.join("home");
    fs::create_dir_all(&config)?;
    fs::create_dir_all(&home)?;

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 36,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("allocating frame timing PTY")?;
    let mut command = CommandBuilder::new(binary);
    command.arg("console");
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("JACKIN_CONFIG_DIR", &config);
    command.env("JACKIN_HOME_DIR", &home);

    let started = Instant::now();
    let mut child = pty
        .slave
        .spawn_command(command)
        .context("spawning jackin console in frame timing PTY")?;
    drop(pty.slave);
    let mut reader = pty
        .master
        .try_clone_reader()
        .context("cloning PTY reader")?;
    let mut writer = pty.master.take_writer().context("taking PTY writer")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let first = receive_frame(&rx, timeout, true).inspect_err(|_| {
        drop(child.kill());
    })?;
    let first_frame_ms = started.elapsed().as_millis();

    let input_started = Instant::now();
    writer
        .write_all(b"\x1b[B")
        .context("writing navigation input")?;
    writer.flush().context("flushing navigation input")?;
    let repaint = receive_frame(&rx, timeout, false).inspect_err(|_| {
        drop(child.kill());
    })?;
    let input_to_frame_ms = input_started.elapsed().as_millis();
    drop(child.kill());
    drop(child.wait());

    Ok(FrameSample {
        first_frame_ms,
        input_to_frame_ms,
        first_frame_bytes: first,
        repaint_bytes: repaint,
    })
}

fn receive_frame(rx: &mpsc::Receiver<Vec<u8>>, timeout: Duration, initial: bool) -> Result<usize> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!(
                "timed out waiting for console {}frame",
                if initial { "first " } else { "re" }
            );
        }
        let chunk = rx
            .recv_timeout(remaining)
            .context("receiving console PTY output")?;
        bytes.extend_from_slice(&chunk);
        let entered_alt_screen = !initial || bytes.windows(8).any(|w| w == b"\x1b[?1049h");
        if entered_alt_screen && bytes.len() >= FRAME_MIN_BYTES {
            return Ok(bytes.len());
        }
    }
}

fn absolute_from(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests;

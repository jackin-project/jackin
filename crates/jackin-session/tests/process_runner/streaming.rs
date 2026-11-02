// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jackin_core::{
    OutputLine, OutputStream, ProcessError, ProcessRunner, ProcessSpec, ProcessStatus,
};
use jackin_session::TokioProcessRunner;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::support::{
    LOOP_DETACHED, LOOP_HOLDING, Scratch, TestResult, WAIT_FOR, assert_no_leftover, guarded,
    guarded_heavy, next_line, same_bytes,
};

const MARK: &str = " [truncated]";
const LINE_LIMIT: usize = 65536;
const WHOLE_LIMIT: usize = 16_777_216;
const SLOW_LINES: usize = 200;

fn numbers(last: u32) -> String {
    let mut text = String::new();
    for n in 1..=last {
        text.push_str(&n.to_string());
        text.push('\n');
    }
    text
}

async fn release_gates(
    rx: &mut mpsc::Receiver<OutputLine>,
    root: &Path,
) -> Result<Vec<(OutputStream, String)>, Box<dyn Error>> {
    let mut seen = Vec::new();
    for (index, gate) in ["g1", "g2", "", ""].into_iter().enumerate() {
        let what = format!("line {index} while the child is gated");
        let line = next_line(rx, &what).await?;
        seen.push((line.stream, line.text));
        if !gate.is_empty() {
            fs::write(root.join(gate), b"")?;
        }
    }
    Ok(seen)
}

#[tokio::test]
async fn lines_reach_the_sink_in_arrival_order_while_the_child_is_still_running() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let body = format!(
            "{WAIT_FOR}echo out-1\nwait_for \"$1/g1\"\necho err-1 >&2\nwait_for \"$1/g2\"\necho out-2\nprintf tail\n"
        );
        let spec = scratch.spec(&body, &[])?;
        let (tx, mut rx) = mpsc::channel::<OutputLine>(8);
        let cancel = CancellationToken::new();
        let run = TokioProcessRunner.run(&spec, Some(&tx), &cancel);
        let driver = release_gates(&mut rx, scratch.root());
        let (output, seen) = tokio::join!(run, driver);
        let seen = seen?;
        let output = output?;
        assert_eq!(
            seen,
            [
                (OutputStream::Stdout, "out-1".to_owned()),
                (OutputStream::Stderr, "err-1".to_owned()),
                (OutputStream::Stdout, "out-2".to_owned()),
                (OutputStream::Stdout, "tail".to_owned()),
            ]
        );
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert_eq!(output.stdout, b"out-1\nout-2\ntail");
        assert_eq!(output.stderr, b"err-1\n");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unstreamed_output_is_returned_whole_from_both_streams_without_a_pipe_deadlock()
-> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let spec = scratch.spec("seq 1 50000 >&2\nseq 1 50000\nprintf last\n", &[])?;
        let output = TokioProcessRunner
            .run(&spec, None, &CancellationToken::new())
            .await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        let expected = numbers(50000);
        same_bytes(
            "stdout",
            &output.stdout,
            format!("{expected}last").as_bytes(),
        )?;
        same_bytes("stderr", &output.stderr, expected.as_bytes())
    })
    .await
}

#[tokio::test]
async fn streamed_output_is_lossless_ordered_per_stream_lossy_decoded_and_tailed_to_64_kib()
-> TestResult {
    guarded_heavy(async {
        let scratch = Scratch::heavy()?;
        let body = "seq 1 30000\nprintf '\\377\\376bad\\n'\nseq 1 5 >&2\necho done\n";
        let spec = scratch.spec(body, &[])?;
        let (tx, mut rx) = mpsc::channel::<OutputLine>(1);
        let cancel = CancellationToken::new();
        let run = async {
            let result = TokioProcessRunner.run(&spec, Some(&tx), &cancel).await;
            drop(tx);
            result
        };
        let collect = async {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            while let Some(line) = rx.recv().await {
                match line.stream {
                    OutputStream::Stdout => stdout.push(line.text),
                    OutputStream::Stderr => stderr.push(line.text),
                }
            }
            (stdout, stderr)
        };
        let (output, (stdout, stderr)) = tokio::join!(run, collect);
        let output = output?;
        let mut expected_out: Vec<String> = (1..=30000).map(|n| n.to_string()).collect();
        expected_out.push("\u{FFFD}\u{FFFD}bad".to_owned());
        expected_out.push("done".to_owned());
        assert_eq!(stdout.len(), expected_out.len(), "stdout line count");
        if let Some(index) = stdout.iter().zip(&expected_out).position(|(a, b)| a != b) {
            return Err(format!("stdout line {index} differs: {:?}", stdout[index]).into());
        }
        assert_eq!(stderr, ["1", "2", "3", "4", "5"]);
        assert_eq!(stdout.len() + stderr.len(), 30007);
        let mut full = numbers(30000).into_bytes();
        full.extend_from_slice(b"\xff\xfebad\ndone\n");
        assert!(
            (4096..=65536).contains(&output.stdout.len()),
            "stdout tail length {}",
            output.stdout.len()
        );
        assert!(
            full.ends_with(&output.stdout),
            "the tail is not a suffix of stdout"
        );
        assert!(output.stdout.ends_with(b"\xff\xfebad\ndone\n"));
        assert_eq!(output.stderr, b"1\n2\n3\n4\n5\n");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gone_line_receiver_never_fails_or_stalls_the_run() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let spec = scratch.spec("seq 1 50000\nseq 1 50000 >&2\nexit 0\n", &[])?;
        let (tx, rx) = mpsc::channel::<OutputLine>(1);
        drop(rx);
        let output = TokioProcessRunner
            .run(&spec, Some(&tx), &CancellationToken::new())
            .await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        for (label, bytes) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
            assert!(!bytes.is_empty(), "{label} tail is empty");
            assert!(bytes.ends_with(b"50000\n"), "{label} tail end");
        }
        Ok(())
    })
    .await
}

async fn streamed(
    spec: &ProcessSpec,
) -> Result<(jackin_core::ProcessOutput, Vec<OutputLine>), Box<dyn Error>> {
    let (tx, mut rx) = mpsc::channel::<OutputLine>(4);
    let cancel = CancellationToken::new();
    let run = async {
        let result = TokioProcessRunner.run(spec, Some(&tx), &cancel).await;
        drop(tx);
        result
    };
    let collect = async {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    };
    let (output, lines) = tokio::join!(run, collect);
    Ok((output?, lines))
}

fn texts(lines: &[OutputLine], stream: OutputStream) -> Vec<&str> {
    lines
        .iter()
        .filter(|line| line.stream == stream)
        .map(|line| line.text.as_str())
        .collect()
}

#[tokio::test]
async fn a_single_output_line_is_cut_at_64_kib_with_a_mark_and_the_next_line_arrives_intact()
-> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let body = "for n in 65535 65536; do head -c $n /dev/zero | tr '\\0' a; echo; done\nhead -c 65536 /dev/zero | tr '\\0' a; echo z\necho next\nprintf last\nhead -c 1048576 /dev/zero | tr '\\0' e >&2\necho >&2\necho after-err >&2\n";
        let (output, lines) = streamed(&scratch.spec(body, &[])?).await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        let full = "a".repeat(LINE_LIMIT);
        let cut = format!("{full}{MARK}");
        let short = "a".repeat(LINE_LIMIT - 1);
        assert_eq!(
            texts(&lines, OutputStream::Stdout),
            [short.as_str(), full.as_str(), cut.as_str(), "next", "last"]
        );
        let err_cut = format!("{}{MARK}", "e".repeat(LINE_LIMIT));
        assert_eq!(
            texts(&lines, OutputStream::Stderr),
            [err_cut.as_str(), "after-err"]
        );
        let mut raw_out = Vec::new();
        for n in [LINE_LIMIT - 1, LINE_LIMIT] {
            raw_out.extend(std::iter::repeat_n(b'a', n));
            raw_out.push(b'\n');
        }
        raw_out.extend(std::iter::repeat_n(b'a', LINE_LIMIT));
        raw_out.extend_from_slice(b"z\nnext\nlast");
        assert_eq!(output.stdout.len(), LINE_LIMIT, "stdout tail length");
        same_bytes(
            "stdout tail is the raw suffix",
            &output.stdout,
            &raw_out[raw_out.len() - LINE_LIMIT..],
        )?;
        assert!(
            output.stdout.ends_with(b"az\nnext\nlast"),
            "stdout tail ends az, next, last"
        );
        assert_eq!(output.stderr.len(), LINE_LIMIT, "stderr tail length");
        assert!(output.stderr.ends_with(b"\nafter-err\n"));

        let multibyte = Scratch::new()?;
        let body = "head -c 65535 /dev/zero | tr '\\0' a\nprintf '\\303\\251\\n'\n";
        let (_, lines) = streamed(&multibyte.spec(body, &[])?).await?;
        let expected = format!("{}\u{FFFD}{MARK}", "a".repeat(LINE_LIMIT - 1));
        assert_eq!(texts(&lines, OutputStream::Stdout), [expected.as_str()]);
        Ok(())
    })
    .await
}

async fn resident_kib() -> Result<u64, Box<dyn Error + Send + Sync>> {
    let pid = std::process::id().to_string();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .stdout(Stdio::piped())
            .spawn()?
            .wait_with_output()
    })
    .await??;
    Ok(String::from_utf8(output.stdout)?.trim().parse()?)
}

#[tokio::test]
async fn a_128_mib_line_does_not_grow_process_memory() -> TestResult {
    guarded_heavy(async {
        let scratch = Scratch::heavy()?;
        let spec = scratch.spec("head -c 134217728 /dev/zero | tr '\\0' a\n", &[])?;
        let baseline = resident_kib().await.map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let watching = Arc::clone(&stop);
        let sampler = tokio::spawn(async move {
            let mut peak = 0;
            while !watching.load(Ordering::SeqCst) {
                peak = peak.max(resident_kib().await?);
                sleep(Duration::from_millis(10)).await;
            }
            Ok::<u64, Box<dyn Error + Send + Sync>>(peak)
        });
        let (output, lines) = streamed(&spec).await?;
        stop.store(true, Ordering::SeqCst);
        let peak = sampler.await?.map_err(|error| error.to_string())?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert_eq!(lines.len(), 1, "one cut line");
        let limit = baseline + 48 * 1024;
        assert!(
            peak <= limit,
            "peak {peak} KiB exceeds baseline {baseline} KiB plus 48 MiB"
        );
        Ok(())
    })
    .await
}

fn flood(stream: OutputStream, bytes: usize, other: &str) -> String {
    let big = format!("head -c {bytes} /dev/zero");
    match stream {
        OutputStream::Stdout => format!("{other}{big}\n"),
        OutputStream::Stderr => format!("{other}{big} >&2\n"),
    }
}

async fn too_large(
    scratch: &Scratch,
    body: &str,
    secret: bool,
) -> Result<ProcessError, Box<dyn Error>> {
    let spec = scratch.spec(body, &[])?;
    let cancel = CancellationToken::new();
    let result = if secret {
        TokioProcessRunner
            .run_secret(&spec, &cancel)
            .await
            .map(|_| ())
    } else {
        TokioProcessRunner
            .run(&spec, None, &cancel)
            .await
            .map(|_| ())
    };
    match result {
        Err(error) => Ok(error),
        Ok(()) => Err("the run succeeded".into()),
    }
}

fn names(error: &ProcessError, expected: OutputStream) -> TestResult {
    match error {
        ProcessError::OutputTooLarge { stream, limit, .. } => {
            assert_eq!(*stream, expected);
            assert_eq!(*limit, WHOLE_LIMIT);
            Ok(())
        }
        other => Err(format!("expected OutputTooLarge, got {other:?}").into()),
    }
}

#[tokio::test]
async fn unstreamed_output_beyond_16_mib_per_stream_fails_with_output_too_large_and_stops_the_child()
-> TestResult {
    guarded_heavy(async {
        let exact = Scratch::heavy()?;
        let spec = exact.spec(&flood(OutputStream::Stdout, WHOLE_LIMIT, ""), &[])?;
        let output = TokioProcessRunner
            .run(&spec, None, &CancellationToken::new())
            .await?;
        assert_eq!(output.stdout.len(), WHOLE_LIMIT);
        assert!(output.stdout.iter().all(|byte| *byte == 0));

        let rows = [
            (
                OutputStream::Stdout,
                flood(OutputStream::Stdout, WHOLE_LIMIT + 1, ""),
                false,
            ),
            (
                OutputStream::Stderr,
                flood(OutputStream::Stderr, WHOLE_LIMIT + 1, "echo small\n"),
                false,
            ),
            (
                OutputStream::Stderr,
                flood(OutputStream::Stderr, WHOLE_LIMIT + 1, ""),
                true,
            ),
            (
                OutputStream::Stdout,
                flood(OutputStream::Stdout, WHOLE_LIMIT + 1, ""),
                true,
            ),
        ];
        for (stream, body, secret) in rows {
            let scratch = Scratch::heavy()?;
            names(&too_large(&scratch, &body, secret).await?, stream)?;
        }

        let both = Scratch::heavy()?;
        let body = "head -c 10485760 /dev/zero\nhead -c 10485760 /dev/zero >&2\n";
        let output = TokioProcessRunner
            .run(&both.spec(body, &[])?, None, &CancellationToken::new())
            .await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert_eq!(output.stdout.len(), 10 * 1024 * 1024, "stdout length");
        assert_eq!(output.stderr.len(), 10 * 1024 * 1024, "stderr length");

        let endless = Scratch::heavy()?;
        let body = format!("{LOOP_DETACHED}{WAIT_FOR}wait_for \"$d/beat\"\nyes\n");
        let spec = endless.spec(&body, &[])?;
        match TokioProcessRunner
            .run(&spec, None, &CancellationToken::new())
            .await
        {
            Err(error) => names(&error, OutputStream::Stdout)?,
            Ok(output) => return Err(format!("yes ended with {:?}", output.status).into()),
        }
        assert_no_leftover(endless.root()).await?;

        let sinked = Scratch::heavy()?;
        let body = "yes \"$(printf '%0999d' 0)\" | head -c 20971000\n";
        let (output, lines) = streamed(&sinked.spec(body, &[])?).await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert_eq!(lines.len(), 20971);
        assert!(lines.iter().all(|line| line.text.len() == 999));
        assert_eq!(output.stdout.len(), LINE_LIMIT);
        Ok(())
    })
    .await
}

async fn stalled_consumer(
    mut rx: mpsc::Receiver<OutputLine>,
    root: &Path,
) -> Result<Vec<String>, Box<dyn Error>> {
    let done = root.join("done");
    let mut polls = 0;
    while !done.exists() {
        polls += 1;
        if polls > 500 {
            return Err("the child never wrote its done marker".into());
        }
        sleep(Duration::from_millis(20)).await;
    }
    sleep(Duration::from_secs(3)).await;
    let mut lines = Vec::new();
    while let Some(line) = rx.recv().await {
        lines.push(line.text);
    }
    Ok(lines)
}

fn same_lines(actual: &[String], expected: &[String]) -> TestResult {
    if actual == expected {
        return Ok(());
    }
    let at = actual
        .iter()
        .zip(expected)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    Err(format!(
        "{} of {} lines; first difference at index {at}",
        actual.len(),
        expected.len()
    )
    .into())
}

async fn slow_sink_run(holding: bool) -> TestResult {
    let scratch = Scratch::new()?;
    let holder = if holding { LOOP_HOLDING } else { "" };
    let body = format!(
        "{holder}i=1\nwhile [ \"$i\" -le {SLOW_LINES} ]; do printf 'line-%03d-%050d\\n' \"$i\" 0; i=$((i+1)); done\n: > \"$1/done\"\nexit 0\n"
    );
    let mut spec = scratch.spec(&body, &[])?;
    spec.timeout = Duration::from_secs(15);
    let expected: Vec<String> = (1..=SLOW_LINES)
        .map(|n| format!("line-{n:03}-{:050}", 0))
        .collect();
    let written: usize = expected.iter().map(|line| line.len() + 1).sum();
    assert!(
        (8193..16384).contains(&written),
        "payload of {written} bytes"
    );
    let (tx, rx) = mpsc::channel::<OutputLine>(1);
    let cancel = CancellationToken::new();
    let run = async {
        let result = TokioProcessRunner.run(&spec, Some(&tx), &cancel).await;
        drop(tx);
        result
    };
    let (output, lines) = tokio::join!(run, stalled_consumer(rx, scratch.root()));
    let output = output?;
    assert_eq!(output.status, ProcessStatus::Exited(0));
    same_lines(&lines?, &expected)?;
    let bytes = format!("{}\n", expected.join("\n"));
    assert!(
        bytes.as_bytes().ends_with(&output.stdout),
        "the tail is not a suffix"
    );
    if holding {
        assert_no_leftover(scratch.root()).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_slow_sink_after_the_child_exits_loses_no_line_and_the_idle_rule_still_ends_the_run()
-> TestResult {
    guarded(async {
        let (plain, holding) = tokio::join!(slow_sink_run(false), slow_sink_run(true));
        plain?;
        holding
    })
    .await
}

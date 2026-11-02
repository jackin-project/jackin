// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::path::Path;

use jackin_core::{OutputLine, OutputStream, ProcessRunner, ProcessStatus};
use jackin_session::TokioProcessRunner;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{Scratch, TestResult, guarded, next_line, same_bytes};

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
            "{}echo out-1\nwait_for \"$1/g1\"\necho err-1 >&2\nwait_for \"$1/g2\"\necho out-2\nprintf tail\n",
            crate::support::WAIT_FOR
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
    guarded(async {
        let scratch = Scratch::new()?;
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

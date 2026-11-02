// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    HostPath, ProcessError, ProcessRunner, ProcessStatus, ProcessStdin, SecretBytes,
};
use jackin_session::TokioProcessRunner;
use tokio_util::sync::CancellationToken;

use crate::support::{Scratch, TestResult, guarded, same_bytes};

#[tokio::test]
async fn stdin_is_dev_null_or_the_given_bytes_followed_by_end_of_input() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let cancel = CancellationToken::new();
        let null = scratch.spec(
            "[ /dev/stdin -ef /dev/null ] && echo devnull || echo other\ncat\n",
            &[],
        )?;
        let output = TokioProcessRunner.run(&null, None, &cancel).await?;
        assert_eq!(output.stdout, b"devnull\n", "Null stdin");

        let input: Vec<u8> = (0..1_usize << 20)
            .map(|index| (index % 256) as u8)
            .collect();
        let mut echo = scratch.spec("cat\n", &[])?;
        echo.stdin = ProcessStdin::Bytes(SecretBytes::new(input.clone()));
        let output = TokioProcessRunner.run(&echo, None, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        same_bytes("echoed 1 MiB", &output.stdout, &input)?;

        let mut empty = scratch.spec("cat\n", &[])?;
        empty.stdin = ProcessStdin::Bytes(SecretBytes::new(Vec::new()));
        let output = TokioProcessRunner.run(&empty, None, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert!(output.stdout.is_empty(), "empty input gave output");

        let mut unread = scratch.spec("exit 3\n", &[])?;
        unread.stdin = ProcessStdin::Bytes(SecretBytes::new(input));
        let output = TokioProcessRunner.run(&unread, None, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(3), "unread stdin");
        Ok(())
    })
    .await
}

fn file_stdin(
    path: &std::path::Path,
    offset: u64,
) -> Result<ProcessStdin, Box<dyn std::error::Error>> {
    Ok(ProcessStdin::File {
        path: HostPath::parse(path)?,
        offset,
    })
}

#[tokio::test]
async fn file_stdin_feeds_the_file_from_the_offset_to_end_of_input() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let cancel = CancellationToken::new();
        let content: Vec<u8> = (0..1_usize << 20)
            .map(|index| (index % 251) as u8)
            .collect();
        let file = scratch.root().join("pack.bin");
        std::fs::write(&file, &content)?;
        let size = content.len() as u64;
        for (label, offset, expected) in [
            ("offset 0", 0, &content[..]),
            ("offset 1000", 1000, &content[1000..]),
            ("offset at the size", size, &[][..]),
            ("offset past the size", size + 10, &[][..]),
        ] {
            let mut echo = scratch.spec("cat\n", &[])?;
            echo.stdin = file_stdin(&file, offset)?;
            let output = TokioProcessRunner.run(&echo, None, &cancel).await?;
            assert_eq!(output.status, ProcessStatus::Exited(0), "{label}");
            same_bytes(label, &output.stdout, expected)?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn file_stdin_of_8_mib_arrives_intact_and_an_early_exit_is_still_the_exit() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let cancel = CancellationToken::new();
        let content: Vec<u8> = (0..8_usize << 20)
            .map(|index| (index % 253) as u8)
            .collect();
        let file = scratch.root().join("big.bin");
        std::fs::write(&file, &content)?;
        let mut echo = scratch.spec("cat\n", &[])?;
        echo.stdin = file_stdin(&file, 0)?;
        let output = TokioProcessRunner.run(&echo, None, &cancel).await?;
        same_bytes("echoed 8 MiB", &output.stdout, &content)?;
        let mut unread = scratch.spec("exit 3\n", &[])?;
        unread.stdin = file_stdin(&file, 0)?;
        let output = TokioProcessRunner.run(&unread, None, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(3), "unread file stdin");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn file_stdin_of_a_missing_file_or_a_directory_never_starts_the_child() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let cancel = CancellationToken::new();
        for (label, path) in [
            ("missing", scratch.root().join("gone.bin")),
            ("directory", scratch.work().to_path_buf()),
        ] {
            let mut touched = scratch.spec("touch \"$1/started\"\n", &[])?;
            touched.stdin = file_stdin(&path, 0)?;
            let refused = TokioProcessRunner.run(&touched, None, &cancel).await;
            assert!(
                matches!(refused, Err(ProcessError::Spawn { .. })),
                "{label}: {refused:?}"
            );
            assert!(
                !scratch.root().join("started").exists(),
                "{label}: the child started"
            );
        }
        Ok(())
    })
    .await
}
